//! Collection setup from the app: the bundled `tamias` CLI (a Tauri sidecar, `bundle.externalBin`)
//! runs `tamias service status|install|uninstall`. The app stays a reader; the scheduled
//! `tamias collect` of the CLI collects (ADR 0001).
//!
//! - Every call has a timeout ([`TIMEOUT`]); a call that does not answer in time is killed and
//!   reported as an error, so the UI never hangs.
//! - [`Sidecar::status`] maps the first line of `tamias service status` to a [`State`]: `service:
//!   installed` is [`State::Installed`] (also a CLI from the release archives: the app only shows
//!   it); `service: not installed` and a broken installation (not loaded, or its exe is gone) are
//!   [`State::NotInstalled`], with the CLI's line as the message; a non-zero exit, a timeout or
//!   unexpected output is [`State::Error`].
//! - `tamias service install` registers the executable it runs from. On macOS and Windows that is
//!   the sidecar inside the installed app (`.app/Contents/MacOS`, the install directory with
//!   `tamiasw.exe` next to it). On Linux the `AppImage` mount path changes on every start, so
//!   [`Sidecar::install`] first copies the sidecar to `~/.local/bin/tamias` and runs the install of
//!   that copy; the systemd unit points there.
//! - [`ServiceLock`] keeps install and uninstall from running concurrently.

use std::io::{self, Read};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Longest time a sidecar call may take before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// File name of the sidecar next to the app's executable (Tauri strips the target triple).
pub const SIDECAR: &str = if cfg!(windows) {
    "tamias.exe"
} else {
    "tamias"
};

/// How often a running sidecar is checked for its exit.
const POLL: Duration = Duration::from_millis(10);

/// Attempts to start a file that is still open for writing elsewhere (`ETXTBSY`, a fresh copy).
const SPAWN_ATTEMPTS: u32 = 5;

/// Whether collection is set up, as far as `tamias service status` tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The scheduler has the collector registered and its exe exists.
    Installed,
    /// No collector registered, or a broken registration that `install` repairs.
    NotInstalled,
    /// The status could not be read (sidecar missing, non-zero exit, timeout, unexpected output).
    Error,
}

/// The status the frontend shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceStatus {
    /// Installed, not installed or error.
    pub state: State,
    /// The error, or the CLI's note on a broken installation; absent otherwise.
    pub message: Option<String>,
    /// The executable the scheduler runs, when the CLI names it.
    pub exe: Option<String>,
}

impl ServiceStatus {
    fn error(message: impl Into<String>) -> Self {
        Self {
            state: State::Error,
            message: Some(message.into()),
            exe: None,
        }
    }
}

/// Why a sidecar call failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The bundled CLI could not be located (no path of the app's own executable).
    #[error("cannot locate the bundled tamias: {0}")]
    Locate(#[source] io::Error),
    /// The sidecar could not be started.
    #[error("cannot run {path}: {source}")]
    Start {
        /// The sidecar.
        path: PathBuf,
        /// What the OS said.
        #[source]
        source: io::Error,
    },
    /// The sidecar did not exit within the timeout and was killed.
    #[error("`tamias {command}` did not answer within {} s", .after.as_secs_f64())]
    Timeout {
        /// The arguments, for example `service status`.
        command: String,
        /// The timeout.
        after: Duration,
    },
    /// The sidecar exited with an error.
    #[error("`tamias {command}` failed ({status}): {message}")]
    Failed {
        /// The arguments, for example `service install`.
        command: String,
        /// The exit status, for example `exit status: 1`.
        status: String,
        /// Its stderr (or stdout when stderr is empty).
        message: String,
    },
    /// The sidecar could not be copied to `~/.local/bin` (Linux).
    #[error("cannot copy {from} to {to}: {source}")]
    Copy {
        /// The sidecar.
        from: PathBuf,
        /// The copy.
        to: PathBuf,
        /// What the OS said.
        #[source]
        source: io::Error,
    },
    /// No home directory to copy the sidecar to (Linux).
    #[error("no home directory found for the current user")]
    NoHome,
    /// An install or uninstall is already running.
    #[error("collection setup is already running")]
    Busy,
}

/// Output of a sidecar call that exited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Exit status 0.
    pub success: bool,
    /// The exit status for messages, for example `exit status: 1`.
    pub status: String,
    /// Its stdout.
    pub stdout: String,
    /// Its stderr.
    pub stderr: String,
}

/// The `tamias` CLI the app runs, with the timeout of every call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    path: PathBuf,
    timeout: Duration,
}

impl Sidecar {
    /// The CLI at `path`, each call limited to `timeout`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            path: path.into(),
            timeout,
        }
    }

    /// The sidecar Tauri placed next to the app's executable (bundle and `tauri dev`), with
    /// [`TIMEOUT`].
    ///
    /// # Errors
    ///
    /// [`Error::Locate`] when the app's own executable is unknown.
    pub fn bundled() -> Result<Self, Error> {
        let exe = std::env::current_exe().map_err(Error::Locate)?;
        let dir = exe.parent().ok_or_else(|| {
            Error::Locate(io::Error::other(format!(
                "{} has no parent directory",
                exe.display()
            )))
        })?;
        Ok(Self::new(dir.join(SIDECAR), TIMEOUT))
    }

    /// Runs the CLI with `args` and waits at most the timeout; a call that takes longer is
    /// killed.
    ///
    /// # Errors
    ///
    /// [`Error::Start`] or [`Error::Timeout`]; a non-zero exit is an [`Output`].
    pub fn run(&self, args: &[&str]) -> Result<Output, Error> {
        let command = args.join(" ");
        let deadline = Instant::now() + self.timeout;
        let mut child = self.spawn(args)?;
        let stdout = read_in_background(child.stdout.take());
        let stderr = read_in_background(child.stderr.take());
        let Some(status) = wait_until(&mut child, deadline).map_err(|source| Error::Start {
            path: self.path.clone(),
            source,
        })?
        else {
            return Err(Error::Timeout {
                command,
                after: self.timeout,
            });
        };
        // A grandchild that inherited a pipe may keep it open after the exit: never wait for
        // the output past the deadline either.
        Ok(Output {
            success: status.success(),
            status: status.to_string(),
            stdout: collect(&stdout, deadline),
            stderr: collect(&stderr, deadline),
        })
    }

    fn spawn(&self, args: &[&str]) -> Result<Child, Error> {
        let mut attempt = 1;
        loop {
            let mut command = tamias_core::process::command(&self.path);
            command
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            match command.spawn() {
                Ok(child) => return Ok(child),
                Err(e)
                    if e.kind() == io::ErrorKind::ExecutableFileBusy
                        && attempt < SPAWN_ATTEMPTS =>
                {
                    attempt += 1;
                    thread::sleep(Duration::from_millis(20));
                }
                Err(source) => {
                    return Err(Error::Start {
                        path: self.path.clone(),
                        source,
                    });
                }
            }
        }
    }

    /// Runs `args` and returns the stdout of a zero exit.
    fn run_ok(&self, args: &[&str]) -> Result<String, Error> {
        let output = self.run(args)?;
        if output.success {
            return Ok(output.stdout);
        }
        let message = if output.stderr.trim().is_empty() {
            output.stdout.trim()
        } else {
            output.stderr.trim()
        };
        Err(Error::Failed {
            command: args.join(" "),
            status: output.status,
            message: message
                .strip_prefix("error: ")
                .unwrap_or(message)
                .to_owned(),
        })
    }

    /// `tamias service status`, mapped to a [`ServiceStatus`]; every failure is
    /// [`State::Error`] with its message.
    #[must_use]
    pub fn status(&self) -> ServiceStatus {
        match self.run_ok(&["service", "status"]) {
            Ok(stdout) => parse_status(&stdout),
            Err(error) => ServiceStatus::error(error.to_string()),
        }
    }

    /// `tamias service install` of the copy in `~/.local/bin` (see [`Sidecar::install_copied`]).
    /// Returns the CLI's output.
    ///
    /// # Errors
    ///
    /// [`Error::NoHome`], or [`Error`] of the copy or of the call.
    #[cfg(target_os = "linux")]
    pub fn install(&self) -> Result<String, Error> {
        let home = std::env::home_dir()
            .filter(|h| !h.as_os_str().is_empty())
            .ok_or(Error::NoHome)?;
        self.install_copied(&home)
    }

    /// `tamias service install`, with the sidecar inside the app as the scheduled executable.
    /// Returns the CLI's output.
    ///
    /// # Errors
    ///
    /// [`Error`] of the call.
    #[cfg(not(target_os = "linux"))]
    pub fn install(&self) -> Result<String, Error> {
        self.run_ok(&["service", "install"])
    }

    /// Copies the sidecar to `home/.local/bin/tamias` (see [`copy_to_local_bin`]) and runs
    /// `tamias service install` of that copy, so the scheduler runs a path that outlives the
    /// `AppImage` mount. Used on Linux.
    ///
    /// # Errors
    ///
    /// [`Error::Copy`], or [`Error`] of the call.
    #[cfg(unix)]
    pub fn install_copied(&self, home: &Path) -> Result<String, Error> {
        let copy = copy_to_local_bin(&self.path, home)?;
        Self::new(copy, self.timeout).run_ok(&["service", "install"])
    }

    /// `tamias service uninstall`; the data and the log are kept. Returns the CLI's output.
    ///
    /// # Errors
    ///
    /// [`Error`] of the call.
    pub fn uninstall(&self) -> Result<String, Error> {
        self.run_ok(&["service", "uninstall"])
    }
}

/// Copies `sidecar` to `home/.local/bin/tamias` with mode 0755, replacing an older copy, and
/// returns the copy's path. The copy is written next to the target and renamed over it, so a
/// copy the timer is running right now is replaced without `ETXTBSY`.
///
/// # Errors
///
/// [`Error::Copy`].
#[cfg(unix)]
pub fn copy_to_local_bin(sidecar: &Path, home: &Path) -> Result<PathBuf, Error> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let dir = home.join(".local").join("bin");
    let target = dir.join("tamias");
    let partial = dir.join(format!(".tamias.{}.partial", std::process::id()));
    let copy = || -> io::Result<()> {
        fs::create_dir_all(&dir)?;
        fs::copy(sidecar, &partial)?;
        fs::set_permissions(&partial, fs::Permissions::from_mode(0o755))?;
        fs::rename(&partial, &target)
    };
    copy().map_err(|source| {
        let _ = fs::remove_file(&partial);
        Error::Copy {
            from: sidecar.to_path_buf(),
            to: target.clone(),
            source,
        }
    })?;
    Ok(target)
}

/// Maps the output of a successful `tamias service status`.
fn parse_status(stdout: &str) -> ServiceStatus {
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(prefix))
            .map(|rest| rest.trim().to_owned())
    };
    let exe = line("exe:");
    let first = stdout.lines().next().unwrap_or("").trim();
    let (state, message) = if first == "service: installed" {
        match line("warning:") {
            Some(warning) => (State::NotInstalled, Some(warning)),
            None => (State::Installed, None),
        }
    } else if let Some(rest) = first.strip_prefix("service: installed but not loaded") {
        let note = rest.trim_start_matches(';').trim();
        (
            State::NotInstalled,
            Some(format!("installed but not loaded; {note}")),
        )
    } else if first.starts_with("service: not installed") {
        (State::NotInstalled, None)
    } else {
        return ServiceStatus::error(format!(
            "unexpected output of `tamias service status`: {first}"
        ));
    };
    ServiceStatus {
        state,
        message,
        exe,
    }
}

/// Waits for `child` until `deadline`; kills it and returns `None` when it is still running then.
fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

fn read_in_background(stream: Option<impl Read + Send + 'static>) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    if let Some(mut stream) = stream {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    rx
}

fn collect(rx: &Receiver<Vec<u8>>, deadline: Instant) -> String {
    let left = deadline.saturating_duration_since(Instant::now());
    rx.recv_timeout(left.max(POLL))
        .map(|buf| String::from_utf8_lossy(&buf).into_owned())
        .unwrap_or_default()
}

/// Keeps install and uninstall from running concurrently; managed by the app.
#[derive(Debug, Clone, Default)]
pub struct ServiceLock(Arc<AtomicBool>);

/// Held while an install or uninstall runs; dropping it releases the [`ServiceLock`].
#[derive(Debug)]
pub struct ServiceGuard(Arc<AtomicBool>);

impl ServiceLock {
    /// Takes the lock, or [`Error::Busy`] while another call holds it.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`].
    pub fn try_lock(&self) -> Result<ServiceGuard, Error> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ServiceGuard(Arc::clone(&self.0)))
            .map_err(|_| Error::Busy)
    }
}

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
