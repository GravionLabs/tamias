//! The live refresh on a temp data dir: debounced store writes, ignored files, a data dir that
//! appears or disappears after the start, the fallback timer and watcher errors.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use notify::{Config, EventHandler, RecursiveMode, Watcher, WatcherKind};
use tamias_app_lib::watch::{Cause, DEBOUNCE, FALLBACK, Options, Watch};

/// Bound for an expected event; generous for slow CI file systems.
const ARRIVES_WITHIN: Duration = Duration::from_secs(10);

/// Wait for an event that must not come: the debounce window and a margin, no longer.
const QUIET_FOR: Duration = Duration::from_millis(1500);

/// Long enough that the timer never fires within a test.
const NO_TIMER: Duration = Duration::from_secs(3600);

fn options() -> Options {
    Options {
        debounce: DEBOUNCE,
        fallback: NO_TIMER,
    }
}

/// Starts a watch on `data_dir` with `W`; its events arrive on the receiver.
fn start_with<W: Watcher + 'static>(data_dir: &Path, options: Options) -> (Watch, Receiver<Cause>) {
    let (tx, rx) = mpsc::channel();
    let watch = Watch::start_with::<W>(data_dir.to_path_buf(), options, move |cause| {
        let _ = tx.send(cause);
    })
    .unwrap();
    (watch, rx)
}

fn start(data_dir: &Path, options: Options) -> (Watch, Receiver<Cause>) {
    start_with::<notify::RecommendedWatcher>(data_dir, options)
}

fn write(path: PathBuf, content: &str) {
    std::fs::write(path, content).unwrap();
}

/// A data dir holding a migrated database, made before any watch starts.
///
/// The database is created in a sibling directory and renamed into place: `FSEvents` (macOS)
/// still delivers the creation's writes when a watch starts right after them, which would be
/// taken for a collect run. The rename leaves those events under the old path.
fn seeded_data_dir() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let seed = root.path().join("seed");
    tamias_core::store::Store::open(seed.join("tamias.db")).unwrap();
    let data_dir = root.path().join("tamias");
    std::fs::rename(&seed, &data_dir).unwrap();
    (root, data_dir)
}

fn next(rx: &Receiver<Cause>) -> Cause {
    rx.recv_timeout(ARRIVES_WITHIN)
        .expect("no event within the timeout")
}

fn assert_quiet(rx: &Receiver<Cause>) {
    assert_quiet_for(rx, QUIET_FOR);
}

fn assert_quiet_for(rx: &Receiver<Cause>, window: Duration) {
    match rx.recv_timeout(window) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("expected no event, got {other:?}"),
    }
}

#[test]
fn two_wal_writes_within_the_debounce_give_one_event() {
    // Longer than the default, so a slow CI file system still delivers both writes within it.
    let debounce = Duration::from_secs(2);
    let dir = tempfile::tempdir().unwrap();
    let (_watch, rx) = start(
        dir.path(),
        Options {
            debounce,
            fallback: NO_TIMER,
        },
    );

    write(dir.path().join("tamias.db-wal"), "first");
    write(dir.path().join("tamias.db-wal"), "second");

    assert_eq!(next(&rx), Cause::Write);
    assert_quiet_for(&rx, debounce + Duration::from_secs(1));
}

#[test]
fn a_database_write_gives_an_event() {
    let dir = tempfile::tempdir().unwrap();
    let (_watch, rx) = start(dir.path(), options());

    write(dir.path().join("tamias.db"), "db");

    assert_eq!(next(&rx), Cause::Write);
}

#[test]
fn writes_to_other_files_give_none() {
    let dir = tempfile::tempdir().unwrap();
    let (_watch, rx) = start(dir.path(), options());

    // The reader's shared memory, the collect log and lookalike names.
    write(dir.path().join("tamias.db-shm"), "shm");
    write(dir.path().join("collect.log"), "log");
    write(dir.path().join("tamias.db-journal"), "journal");
    write(dir.path().join("other.db-wal"), "other");

    assert_quiet(&rx);
}

#[test]
fn a_collect_run_gives_one_event_and_the_reload_after_it_none() {
    let (_root, data_dir) = seeded_data_dir();
    let database = data_dir.join("tamias.db");
    let (_watch, rx) = start(&data_dir, options());

    // The collector: open, write a run, close (the checkpoint removes the WAL).
    let mut store = tamias_core::store::Store::open(&database).unwrap();
    store
        .record_run(
            tamias_core::snapshot::Provider::Claude,
            &[],
            jiff::Timestamp::now(),
        )
        .unwrap();
    drop(store);
    assert_eq!(next(&rx), Cause::Write);
    assert_quiet(&rx);

    // The app's reload after the event.
    let store = tamias_core::store::Store::open_read_only(&database).unwrap();
    store
        .status(tamias_core::snapshot::Provider::Claude)
        .unwrap();
    drop(store);
    assert_quiet(&rx);
}

#[test]
fn reading_the_database_gives_none() {
    let (_root, data_dir) = seeded_data_dir();
    let database = data_dir.join("tamias.db");
    let (_watch, rx) = start(&data_dir, options());

    // What the app does after every event: a read-only open of the store.
    let store = tamias_core::store::Store::open_read_only(&database).unwrap();
    store
        .status(tamias_core::snapshot::Provider::Claude)
        .unwrap();
    drop(store);

    assert_quiet(&rx);
}

#[test]
fn a_data_dir_created_after_the_start_is_picked_up() {
    let parent = tempfile::tempdir().unwrap();
    let data_dir = parent.path().join("tamias");
    let (_watch, rx) = start(&data_dir, options());

    std::fs::create_dir(&data_dir).unwrap();
    // The collector may have written before the watch moved: the appearance is an event itself.
    assert_eq!(next(&rx), Cause::Write);

    write(data_dir.join("tamias.db-wal"), "wal");
    assert_eq!(next(&rx), Cause::Write);
}

#[test]
fn a_data_dir_below_missing_ancestors_is_picked_up() {
    let root = tempfile::tempdir().unwrap();
    let data_dir = root.path().join("share").join("tamias");
    let (_watch, rx) = start(&data_dir, options());

    std::fs::create_dir_all(&data_dir).unwrap();
    assert_eq!(next(&rx), Cause::Write);

    write(data_dir.join("tamias.db"), "db");
    assert_eq!(next(&rx), Cause::Write);
}

#[test]
fn the_fallback_timer_emits_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let (_watch, rx) = start(
        dir.path(),
        Options {
            debounce: DEBOUNCE,
            fallback: Duration::from_millis(100),
        },
    );

    assert_eq!(next(&rx), Cause::Timer);
    assert_eq!(next(&rx), Cause::Timer);
}

#[test]
fn the_default_timings_are_the_specified_ones() {
    assert_eq!(DEBOUNCE, Duration::from_millis(500));
    assert_eq!(FALLBACK, Duration::from_secs(60));
    assert_eq!(
        Options::default(),
        Options {
            debounce: DEBOUNCE,
            fallback: FALLBACK
        }
    );
}

#[test]
fn a_removed_data_dir_falls_back_to_the_timer_and_is_picked_up_again() {
    let parent = tempfile::tempdir().unwrap();
    let data_dir = parent.path().join("tamias");
    std::fs::create_dir(&data_dir).unwrap();
    let (_watch, rx) = start(
        &data_dir,
        Options {
            debounce: DEBOUNCE,
            fallback: Duration::from_millis(200),
        },
    );

    std::fs::remove_dir(&data_dir).unwrap();
    // Still refreshing, whatever the watcher made of the removal. A tick also moves the watch to
    // the parent, which releases the old directory (Windows keeps a watched one delete-pending).
    while next(&rx) != Cause::Timer {}

    std::fs::create_dir(&data_dir).unwrap();
    write(data_dir.join("tamias.db-wal"), "wal");
    // The timer keeps firing, so wait for a write event among the ticks.
    let deadline = std::time::Instant::now() + ARRIVES_WITHIN;
    while next(&rx) != Cause::Write {
        assert!(std::time::Instant::now() < deadline, "no write event");
    }
}

#[test]
fn stop_ends_the_thread() {
    let dir = tempfile::tempdir().unwrap();
    let (watch, rx) = start(
        dir.path(),
        Options {
            debounce: DEBOUNCE,
            fallback: Duration::from_millis(50),
        },
    );

    watch.stop();

    // The emitter (and with it the sender) went away with the thread.
    while let Ok(cause) = rx.recv_timeout(ARRIVES_WITHIN) {
        assert_eq!(cause, Cause::Timer);
    }
    assert!(matches!(
        rx.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}

/// A watcher that cannot be created, as when the system has no watches left.
#[derive(Debug)]
struct Unavailable;

impl Watcher for Unavailable {
    fn new<F: EventHandler>(_: F, _: Config) -> notify::Result<Self> {
        Err(notify::Error::generic("no inotify watches left"))
    }

    fn watch(&mut self, _: &Path, _: RecursiveMode) -> notify::Result<()> {
        unreachable!()
    }

    fn unwatch(&mut self, _: &Path) -> notify::Result<()> {
        unreachable!()
    }

    fn kind() -> WatcherKind {
        WatcherKind::NullWatcher
    }
}

/// A watcher that is refused every directory, as on a permission error.
#[derive(Debug)]
struct Refused;

impl Watcher for Refused {
    fn new<F: EventHandler>(_: F, _: Config) -> notify::Result<Self> {
        Ok(Self)
    }

    fn watch(&mut self, path: &Path, _: RecursiveMode) -> notify::Result<()> {
        Err(
            notify::Error::new(notify::ErrorKind::Io(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied,
            )))
            .add_path(path.to_path_buf()),
        )
    }

    fn unwatch(&mut self, _: &Path) -> notify::Result<()> {
        Ok(())
    }

    fn kind() -> WatcherKind {
        WatcherKind::NullWatcher
    }
}

fn assert_timer_keeps_refreshing<W: Watcher + 'static>() {
    let dir = tempfile::tempdir().unwrap();
    let (_watch, rx) = start_with::<W>(
        dir.path(),
        Options {
            debounce: DEBOUNCE,
            fallback: Duration::from_millis(100),
        },
    );

    // Each tick retries the watch and fails again; the timer still fires every time.
    assert_eq!(next(&rx), Cause::Timer);
    assert_eq!(next(&rx), Cause::Timer);
    assert_eq!(next(&rx), Cause::Timer);
}

#[test]
fn a_watcher_that_cannot_start_falls_back_to_the_timer() {
    assert_timer_keeps_refreshing::<Unavailable>();
}

#[test]
fn a_refused_watch_falls_back_to_the_timer() {
    assert_timer_keeps_refreshing::<Refused>();
}
