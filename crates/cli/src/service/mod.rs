//! `tamias service`: register `tamias collect` with the OS scheduler.
//!
//! [`Scheduler`] is implemented per OS (`cfg(target_os)`): a launchd agent on macOS, a systemd
//! user timer on Linux, a Task Scheduler task on Windows. External commands such as `launchctl`
//! go through a [`Runner`], which tests replace with a fake.

// The Linux module is pure file and command logic, so it compiles (and its tests run) on every
// OS; only Linux selects it in `platform()`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
// Like Linux: pure file and command logic that compiles and is tested everywhere.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub mod windows;

use std::fmt::{self, Write as _};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Subcommand;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_core::snapshot::Provider;
use tamias_core::store::{SourceStatus, Store};

/// Action of `tamias service`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum Action {
    /// Run `tamias collect` every 5 minutes in the background (no admin rights needed).
    Install,
    /// Remove the background collector (keeps the data and the log).
    Uninstall,
    /// Show whether the collector is installed and its last run per provider.
    Status,
}

/// What the OS scheduler knows about the collector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerState {
    /// Whether a definition (file or registration) of the collector exists.
    pub installed: bool,
    /// Whether the scheduler has the collector registered (loaded).
    pub registered: bool,
    /// Executable the definition runs, when it can be read.
    pub exe: Option<PathBuf>,
    /// Where the definition lives (for example the plist).
    pub definition: Option<PathBuf>,
}

/// Registers `tamias collect` with the scheduler of the OS.
pub trait Scheduler {
    /// Registers `exe collect` to run periodically, replacing an existing registration.
    ///
    /// # Errors
    ///
    /// I/O errors or a failing scheduler command; no half-written definition is left behind.
    fn install(&self, exe: &Path) -> Result<()>;

    /// Removes the registration and its definition; a no-op when not installed.
    ///
    /// # Errors
    ///
    /// I/O errors or a failing scheduler command.
    fn uninstall(&self) -> Result<()>;

    /// Current state of the registration.
    ///
    /// # Errors
    ///
    /// I/O errors or a failing scheduler command.
    fn status(&self) -> Result<SchedulerState>;
}

/// Error of `tamias service`.
#[derive(Debug)]
pub enum Error {
    /// No scheduler for this OS yet.
    #[cfg_attr(
        any(target_os = "macos", target_os = "linux", target_os = "windows"),
        allow(dead_code)
    )]
    Unsupported(&'static str),
    /// A file operation failed.
    Io { context: String, source: io::Error },
    /// An external command exited with an error.
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
        allow(dead_code)
    )]
    Command {
        command: String,
        status: String,
        stderr: String,
    },
    /// Other problem (for example a path that is not valid UTF-8).
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
        allow(dead_code)
    )]
    Other(String),
}

impl Error {
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Error::Io {
            context: context.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported(os) => write!(
                f,
                "tamias service is not supported yet on this platform ({os})"
            ),
            Error::Io { context, source } => write!(f, "{context}: {source}"),
            Error::Command {
                command,
                status,
                stderr,
            } => {
                write!(f, "`{command}` failed ({status})")?;
                if stderr.trim().is_empty() {
                    Ok(())
                } else {
                    write!(f, ": {}", stderr.trim())
                }
            }
            Error::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Result of `tamias service`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Output of an external command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
    allow(dead_code)
)]
pub struct Output {
    /// Exit status 0.
    pub success: bool,
    /// Exit status for messages, for example `exit status: 5`.
    pub status: String,
    pub stdout: String,
    pub stderr: String,
}

/// Runs external commands.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
    allow(dead_code)
)]
pub trait Runner {
    /// Runs `program` with `args` and waits for it.
    ///
    /// # Errors
    ///
    /// The program could not be started.
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output>;
}

/// [`Runner`] of the real system.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
    allow(dead_code)
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        let output = Command::new(program).args(args).output()?;
        Ok(Output {
            success: output.status.success(),
            status: output.status.to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Runs `program args` and fails unless it exits with 0.
///
/// # Errors
///
/// [`Error::Io`] when it cannot start, [`Error::Command`] on a non-zero exit.
// Linux inspects the exit status of `systemctl` itself.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn run_checked(runner: &dyn Runner, program: &str, args: &[&str]) -> Result<Output> {
    let command = std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    let output = runner
        .run(program, args)
        .map_err(|e| Error::io(format!("cannot run `{command}`"), e))?;
    if output.success {
        Ok(output)
    } else {
        Err(Error::Command {
            command,
            status: output.status,
            stderr: output.stderr,
        })
    }
}

/// The scheduler of this OS.
///
/// # Errors
///
/// [`Error::Other`] without a home
/// directory.
#[cfg(target_os = "macos")]
pub fn platform() -> Result<Box<dyn Scheduler>> {
    let home = std::env::home_dir()
        .filter(|h| !h.as_os_str().is_empty())
        .ok_or_else(|| Error::Other("no home directory found for the current user".to_owned()))?;
    Ok(Box::new(macos::LaunchAgent::new(
        macos::agents_dir(&home),
        Box::new(SystemRunner),
    )))
}

/// The scheduler of this OS.
///
/// # Errors
///
/// [`Error::Other`] without a home directory.
#[cfg(target_os = "linux")]
pub fn platform() -> Result<Box<dyn Scheduler>> {
    let home = std::env::home_dir().filter(|h| !h.as_os_str().is_empty());
    let dir = linux::units_dir(std::env::var_os("XDG_CONFIG_HOME"), home.as_deref())
        .ok_or_else(|| Error::Other("no home directory found for the current user".to_owned()))?;
    Ok(Box::new(linux::SystemdTimer::new(
        dir,
        Box::new(SystemRunner),
    )))
}

/// The scheduler of this OS.
///
/// # Errors
///
/// [`Error::Other`] without a temp directory.
#[cfg(target_os = "windows")]
#[allow(clippy::unnecessary_wraps)] // same signature as the other platforms
pub fn platform() -> Result<Box<dyn Scheduler>> {
    Ok(Box::new(windows::TaskScheduler::new(
        std::env::temp_dir(),
        Box::new(SystemRunner),
    )))
}

/// The scheduler of this OS.
///
/// # Errors
///
/// [`Error::Unsupported`] on an OS without a scheduler.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn platform() -> Result<Box<dyn Scheduler>> {
    Err(Error::Unsupported(std::env::consts::OS))
}

/// Path of the running executable, canonicalised (the path the scheduler registers).
///
/// # Errors
///
/// [`Error::Io`] when it cannot be determined.
pub fn current_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::io("cannot determine the tamias executable", e))?;
    exe.canonicalize()
        .map_err(|e| Error::io(format!("cannot resolve {}", exe.display()), e))
}

/// Runs `action` against `scheduler`; returns the text for stdout.
///
/// `open_store` is called only by `status`: install and uninstall work without a database.
///
/// # Errors
///
/// Errors of the scheduler, of `open_store` or of the store.
pub fn execute(
    action: Action,
    scheduler: &dyn Scheduler,
    open_store: impl FnOnce() -> std::result::Result<Store, Box<dyn std::error::Error>>,
    log_path: &Path,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    match action {
        Action::Install => {
            let exe = current_exe()?;
            scheduler.install(&exe)?;
            Ok(format!(
                "installed: tamias collect runs every 5 minutes ({})\nlog: {}\n",
                exe.display(),
                log_path.display()
            ))
        }
        Action::Uninstall => {
            let before = scheduler.status()?;
            scheduler.uninstall()?;
            Ok(if before.installed {
                "uninstalled; the data and the log are kept\n".to_owned()
            } else {
                "not installed, nothing to remove\n".to_owned()
            })
        }
        Action::Status => {
            let state = scheduler.status()?;
            let store = open_store()?;
            let mut statuses = Vec::new();
            for provider in [Provider::Claude, Provider::Copilot] {
                statuses.push((provider, store.status(provider)?));
            }
            Ok(render_status(
                &state,
                &statuses,
                log_path,
                &TimeZone::system(),
            ))
        }
    }
}

/// Text of `tamias service status`.
#[must_use]
pub fn render_status(
    state: &SchedulerState,
    statuses: &[(Provider, Option<SourceStatus>)],
    log_path: &Path,
    tz: &TimeZone,
) -> String {
    let mut out = String::new();
    if state.installed {
        out.push_str(if state.registered {
            "service: installed\n"
        } else {
            "service: installed but not loaded; run `tamias service install` again\n"
        });
    } else {
        out.push_str("service: not installed (run `tamias service install`)\n");
    }
    if let Some(definition) = &state.definition {
        let _ = writeln!(out, "definition: {}", definition.display());
    }
    if let Some(exe) = &state.exe {
        let _ = writeln!(out, "exe: {}", exe.display());
        if !exe.exists() {
            out.push_str(
                "warning: the registered exe no longer exists; run `tamias service install` again\n",
            );
        }
    }
    for (provider, status) in statuses {
        let _ = writeln!(out, "{provider}: {}", last_run(status.as_ref(), tz));
    }
    let _ = writeln!(out, "log: {}", log_path.display());
    out
}

fn last_run(status: Option<&SourceStatus>, tz: &TimeZone) -> String {
    let time = |at: Timestamp| {
        at.to_zoned(tz.clone())
            .strftime("%Y-%m-%d %H:%M")
            .to_string()
    };
    let Some(status) = status else {
        return "no run yet".to_owned();
    };
    match (status.last_ok_at, status.last_error_at) {
        (Some(ok), Some(err)) if err > ok => error_text(&time(err), status),
        (Some(ok), _) => format!("last run {} ok", time(ok)),
        (None, Some(err)) => error_text(&time(err), status),
        (None, None) => "no run yet".to_owned(),
    }
}

fn error_text(time: &str, status: &SourceStatus) -> String {
    format!(
        "last run {time} error ({})",
        status.last_error.as_deref().unwrap_or("unknown")
    )
}

#[cfg(test)]
mod tests;
