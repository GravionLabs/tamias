//! Live refresh: tells the frontend when the collector wrote new snapshots. The collector stays
//! independent (ADR 0001: no push); the app watches the `SQLite` files instead.
//!
//! - A thread watches the data directory ([`tamias_core::paths::data_dir`]) for writes to
//!   `tamias.db` and `tamias.db-wal`. Other files (the reader's `tamias.db-shm`, the collect log),
//!   reads and file creations without content are ignored, so reloading after an event never
//!   triggers the next one.
//! - Writes are debounced ([`DEBOUNCE`]): one collect run gives one event.
//! - A timer emits every [`FALLBACK`] as well: watchers miss events on some file systems, and a
//!   provider must turn stale without a write.
//! - A data directory that does not exist yet: its nearest existing ancestor is watched until it
//!   appears; a removed one falls back to its ancestor the same way.
//! - A watcher error (directory removed, permission, no watcher available) is logged; the timer
//!   keeps refreshing and the watch is retried on the next tick.
//!
//! The emitter is injected ([`Emit`]), so tests run the watch on a temp directory without Tauri.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::event::{EventKind, ModifyKind};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tamias_core::paths::DATABASE_FILE;

/// Name of the Tauri event the app emits when the store may have changed.
pub const EVENT: &str = "store-changed";

/// Quiet time after the last write before the event is emitted.
pub const DEBOUNCE: Duration = Duration::from_millis(500);

/// Interval of the fallback timer.
pub const FALLBACK: Duration = Duration::from_secs(60);

/// Deepest ancestor chain walked while the data directory does not exist.
const MAX_SWITCHES: usize = 32;

/// Why an event was emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// `tamias.db` or `tamias.db-wal` was written, or the data directory appeared.
    Write,
    /// The fallback timer.
    Timer,
}

/// Receives the events of a [`Watch`]; the app emits [`EVENT`], tests send into a channel.
pub trait Emit: Send + 'static {
    /// Called on the watch thread for every event.
    fn emit(&self, cause: Cause);
}

impl<F: Fn(Cause) + Send + 'static> Emit for F {
    fn emit(&self, cause: Cause) {
        self(cause);
    }
}

/// Timings of a [`Watch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Quiet time after the last write before the event is emitted.
    pub debounce: Duration,
    /// Interval of the fallback timer.
    pub fallback: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            debounce: DEBOUNCE,
            fallback: FALLBACK,
        }
    }
}

/// The running watch thread. Dropping it (or [`Watch::stop`]) stops the thread and waits for it.
#[derive(Debug)]
pub struct Watch {
    messages: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Starts watching `data_dir` with the platform's watcher. Returns once the first watch is
    /// set up (or failed and was logged), so a write after the return is seen.
    ///
    /// # Errors
    ///
    /// The watch thread could not be spawned.
    pub fn start(data_dir: PathBuf, options: Options, emit: impl Emit) -> std::io::Result<Self> {
        Self::start_with::<RecommendedWatcher>(data_dir, options, emit)
    }

    /// As [`Watch::start`], with the watcher implementation `W`.
    ///
    /// # Errors
    ///
    /// The watch thread could not be spawned.
    pub fn start_with<W: Watcher + 'static>(
        data_dir: PathBuf,
        options: Options,
        emit: impl Emit,
    ) -> std::io::Result<Self> {
        let (messages, inbox) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let sender = messages.clone();
        let thread = std::thread::Builder::new()
            .name("tamias-watch".into())
            .spawn(move || {
                let mut state = State::<W, _>::new(data_dir, options, emit, sender);
                state.reconcile();
                // The caller may have gone away already; the thread then stops on its `Stop`.
                let _ = ready.send(());
                state.run(&inbox);
            })?;
        // An error means the thread ended early; `Drop` joins it.
        let _ = started.recv();
        Ok(Self {
            messages,
            thread: Some(thread),
        })
    }

    /// Stops the watch thread and waits for it.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.messages.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            // A panic of the emitter ended the thread already; nothing left to stop.
            let _ = thread.join();
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum Message {
    Fs(notify::Result<Event>),
    Stop,
}

/// The watch thread: the watcher, what it watches, and the pending and timer deadlines.
struct State<W, E> {
    data_dir: PathBuf,
    options: Options,
    emit: E,
    sender: Sender<Message>,
    watcher: Option<W>,
    /// The directory being watched: the data directory or, while it is missing, an ancestor.
    watched: Option<PathBuf>,
    /// Whether the first watch was set up; only later appearances of the data dir emit.
    started: bool,
    pending: Option<Instant>,
    next_tick: Instant,
}

impl<W: Watcher, E: Emit> State<W, E> {
    fn new(data_dir: PathBuf, options: Options, emit: E, sender: Sender<Message>) -> Self {
        Self {
            data_dir,
            options,
            emit,
            sender,
            watcher: None,
            watched: None,
            started: false,
            pending: None,
            next_tick: Instant::now() + options.fallback,
        }
    }

    fn run(&mut self, inbox: &Receiver<Message>) {
        loop {
            let deadline = self
                .pending
                .map_or(self.next_tick, |pending| pending.min(self.next_tick));
            match inbox.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Message::Fs(result)) => self.on_fs(result),
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            let tick = self.next_tick <= now;
            if tick {
                self.next_tick = now + self.options.fallback;
                self.reconcile();
            }
            if self.pending.is_some_and(|pending| pending <= now) {
                self.pending = None;
                self.emit.emit(Cause::Write);
            } else if tick {
                self.emit.emit(Cause::Timer);
            }
        }
    }

    fn on_fs(&mut self, result: notify::Result<Event>) {
        match result {
            Ok(event) => {
                let on_data_dir = self.watched.as_deref() == Some(self.data_dir.as_path());
                if on_data_dir && (event.need_rescan() || is_store_write(&event)) {
                    self.pending = Some(Instant::now() + self.options.debounce);
                }
                if on_data_dir && self.is_data_dir_removal(&event) {
                    // Its watch is gone even when a new directory of that name exists already.
                    self.forget_watch();
                }
            }
            Err(error) => log(&format!("watch error: {error}")),
        }
        // The data dir may have appeared below the watched ancestor, or been removed.
        self.reconcile();
    }

    fn is_data_dir_removal(&self, event: &Event) -> bool {
        matches!(event.kind, EventKind::Remove(_))
            && event
                .paths
                .iter()
                .any(|path| path.file_name() == self.data_dir.file_name())
    }

    fn forget_watch(&mut self) {
        if let (Some(watcher), Some(old)) = (self.watcher.as_mut(), self.watched.take()) {
            // Fails when the directory was removed; the watch is gone then anyway.
            let _ = watcher.unwatch(&old);
        }
    }

    /// Points the watcher at the data dir, or at its nearest existing ancestor while it is
    /// missing. Errors are logged and retried on the next event or tick.
    fn reconcile(&mut self) {
        for _ in 0..MAX_SWITCHES {
            let target = watch_target(&self.data_dir);
            if target == self.watched {
                break;
            }
            if !self.switch(target) {
                break;
            }
        }
        self.started = true;
    }

    /// Watches `target` instead of the current directory; `false` when that failed.
    fn switch(&mut self, target: Option<PathBuf>) -> bool {
        self.create_watcher();
        let Some(watcher) = self.watcher.as_mut() else {
            return false;
        };
        if let Some(old) = self.watched.take() {
            // Fails when the directory was removed; the watch is gone then anyway.
            let _ = watcher.unwatch(&old);
        }
        let Some(target) = target else {
            return false;
        };
        if let Err(error) = watcher.watch(&target, RecursiveMode::NonRecursive) {
            log(&format!("cannot watch {}: {error}", target.display()));
            return false;
        }
        if self.started && target == self.data_dir {
            // Created after the start: the collector may have written before the watch was set.
            self.pending = Some(Instant::now() + self.options.debounce);
        }
        self.watched = Some(target);
        true
    }

    /// Creates the watcher on first use (and again after a failed creation).
    fn create_watcher(&mut self) {
        if self.watcher.is_none() {
            let sender = self.sender.clone();
            let handler = move |result| {
                // The thread has stopped; nothing left to notify.
                let _ = sender.send(Message::Fs(result));
            };
            match W::new(handler, notify::Config::default()) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(error) => log(&format!("cannot start the file watcher: {error}")),
            }
        }
    }
}

/// The data dir when it exists, else its nearest existing ancestor; `None` when none exists.
fn watch_target(data_dir: &Path) -> Option<PathBuf> {
    data_dir
        .ancestors()
        .find(|dir| !dir.as_os_str().is_empty() && dir.is_dir())
        .map(Path::to_path_buf)
}

/// A change (not a read) of `tamias.db` or `tamias.db-wal`.
fn is_store_write(event: &Event) -> bool {
    // Content changes and removals only. The app's read-only open creates an empty WAL when the
    // collector's checkpoint removed it, and closes it again (an access event on inotify); taking
    // those for writes would make every reload trigger the next one.
    let write = matches!(
        event.kind,
        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any)
            | EventKind::Remove(_)
            | EventKind::Any
    );
    write
        && event
            .paths
            .iter()
            .filter_map(|path| path.file_name())
            .any(is_store_file)
}

fn is_store_file(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.strip_prefix(DATABASE_FILE)
        .is_some_and(|rest| rest.is_empty() || rest == "-wal")
}

fn log(message: &str) {
    eprintln!("tamias-app: live refresh: {message}");
}
