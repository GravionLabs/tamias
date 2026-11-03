//! Linux: a systemd user timer (no root).
//!
//! `$XDG_CONFIG_HOME/systemd/user/tamias-collect.service` (a oneshot running `<exe> collect`)
//! and `tamias-collect.timer` (1 minute after boot, then 5 minutes after each run) are enabled
//! with `systemctl --user enable --now`. `tamias collect` exits 3 when a poll failed, which the
//! service accepts (`SuccessExitStatus=3`); any other non-zero exit is a failed unit.

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{Error, Output, Result, Runner, Scheduler, SchedulerState};

/// Name of the timer unit.
pub const TIMER: &str = "tamias-collect.timer";
/// Name of the service unit.
pub const SERVICE: &str = "tamias-collect.service";

const SYSTEMCTL: &str = "systemctl";

/// `$XDG_CONFIG_HOME/systemd/user`, or `~/.config/systemd/user` when the variable is unset,
/// empty or relative (the XDG spec says to ignore those). `None` without both.
#[must_use]
pub fn units_dir(xdg_config_home: Option<OsString>, home: Option<&Path>) -> Option<PathBuf> {
    let base = xdg_config_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| h.join(".config")))?;
    Some(base.join("systemd").join("user"))
}

/// The systemd user timer of the collector in one unit directory.
pub struct SystemdTimer {
    units_dir: PathBuf,
    runner: Box<dyn Runner>,
}

impl std::fmt::Debug for SystemdTimer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemdTimer")
            .field("units_dir", &self.units_dir)
            .finish_non_exhaustive()
    }
}

/// Properties of `systemctl --user show tamias-collect.timer` that tamias reads.
struct TimerProperties {
    load: String,
    active: String,
    file: String,
}

impl SystemdTimer {
    /// Timer with its units in `units_dir`, running commands with `runner`.
    #[must_use]
    pub fn new(units_dir: PathBuf, runner: Box<dyn Runner>) -> Self {
        Self { units_dir, runner }
    }

    /// Path of the service unit.
    #[must_use]
    pub fn service_path(&self) -> PathBuf {
        self.units_dir.join(SERVICE)
    }

    /// Path of the timer unit.
    #[must_use]
    pub fn timer_path(&self) -> PathBuf {
        self.units_dir.join(TIMER)
    }

    fn temp_path(&self, unit: &str) -> PathBuf {
        self.units_dir.join(format!(".{unit}.tmp"))
    }

    /// `systemctl --user args`; a missing systemd user instance is a clear error.
    fn systemctl(&self, args: &[&str]) -> Result<Output> {
        let mut full = vec!["--user"];
        full.extend_from_slice(args);
        let command = format!("{SYSTEMCTL} {}", full.join(" "));
        let output = match self.runner.run(SYSTEMCTL, &full) {
            Ok(output) => output,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(no_user_manager("systemctl was not found"));
            }
            Err(e) => return Err(Error::io(format!("cannot run `{command}`"), e)),
        };
        if output.success {
            Ok(output)
        } else if no_manager_in(&output.stderr) {
            Err(no_user_manager(output.stderr.trim()))
        } else {
            Err(Error::Command {
                command,
                status: output.status,
                stderr: output.stderr,
            })
        }
    }

    fn show(&self) -> Result<Option<TimerProperties>> {
        let output = self.systemctl(&[
            "show",
            TIMER,
            "--property=LoadState,ActiveState,UnitFileState",
        ])?;
        let get = |key: &str| {
            output
                .stdout
                .lines()
                .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        let props = TimerProperties {
            load: get("LoadState"),
            active: get("ActiveState"),
            file: get("UnitFileState"),
        };
        Ok((!props.load.is_empty()).then_some(props))
    }

    /// Writes a unit through a temp file and a rename, so it is whole or absent.
    fn write_unit(&self, unit: &str, text: &str) -> Result<()> {
        let temp = self.temp_path(unit);
        let target = self.units_dir.join(unit);
        let written = fs::write(&temp, text).and_then(|()| fs::rename(&temp, &target));
        if let Err(e) = written {
            let _ = fs::remove_file(&temp);
            return Err(Error::io(format!("cannot write {}", target.display()), e));
        }
        Ok(())
    }

    /// Removes both units (and stale temp files) but nothing else.
    fn remove_units(&self) -> Result<()> {
        for path in [
            self.timer_path(),
            self.service_path(),
            self.temp_path(SERVICE),
            self.temp_path(TIMER),
        ] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(format!("cannot remove {}", path.display()), e)),
            }
        }
        Ok(())
    }

    fn install_units(&self, exe: &str) -> Result<()> {
        fs::create_dir_all(&self.units_dir)
            .map_err(|e| Error::io(format!("cannot create {}", self.units_dir.display()), e))?;
        self.write_unit(SERVICE, &service_unit(exe))?;
        self.write_unit(TIMER, &timer_unit())?;
        self.systemctl(&["daemon-reload"])?;
        self.systemctl(&["enable", "--now", TIMER])?;
        Ok(())
    }
}

fn no_manager_in(stderr: &str) -> bool {
    stderr.contains("Failed to connect to bus")
        || stderr.contains("System has not been booted with systemd")
        || stderr.contains("Host is down")
}

fn no_user_manager(cause: &str) -> Error {
    Error::Other(format!(
        "no systemd user instance is available ({cause}); tamias service needs systemd, which \
         containers and WSL without systemd do not run"
    ))
}

impl Scheduler for SystemdTimer {
    fn install(&self, exe: &Path) -> Result<()> {
        let exe = exe
            .to_str()
            .ok_or_else(|| Error::Other(format!("{} is not valid UTF-8", exe.display())))?;
        self.systemctl(&["show-environment"])?;
        let result = self.install_units(exe);
        if result.is_err() {
            let _ = self.remove_units();
            let _ = self.systemctl(&["daemon-reload"]);
        }
        result
    }

    fn uninstall(&self) -> Result<()> {
        self.systemctl(&["show-environment"])?;
        let known = self
            .show()?
            .is_some_and(|s| !s.file.is_empty() || s.active == "active");
        if known {
            self.systemctl(&["disable", "--now", TIMER])?;
        }
        self.remove_units()?;
        self.systemctl(&["daemon-reload"])?;
        Ok(())
    }

    fn status(&self) -> Result<SchedulerState> {
        self.systemctl(&["show-environment"])?;
        let shown = self.show()?;
        let service_path = self.service_path();
        let service_text = match fs::read_to_string(&service_path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => {
                return Err(Error::io(
                    format!("cannot read {}", service_path.display()),
                    e,
                ));
            }
        };
        let registered = shown
            .as_ref()
            .is_some_and(|s| s.file == "enabled" && s.active == "active");
        let loaded = shown.as_ref().is_some_and(|s| s.load == "loaded");
        Ok(SchedulerState {
            installed: service_text.is_some() || loaded,
            registered,
            exe: service_text
                .as_deref()
                .and_then(exe_of_service)
                .map(PathBuf::from),
            definition: service_text.is_some().then_some(service_path),
        })
    }
}

/// The service unit running `exe collect`.
#[must_use]
pub fn service_unit(exe: &str) -> String {
    format!(
        "[Unit]\n\
         Description=tamias collector (polls Claude usage once)\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={} collect\n\
         SuccessExitStatus=3\n",
        quote_exec(exe)
    )
}

/// The timer unit: 1 minute after boot, then 5 minutes after the last run.
#[must_use]
pub fn timer_unit() -> String {
    "[Unit]\n\
     Description=Run the tamias collector every 5 minutes\n\
     \n\
     [Timer]\n\
     OnBootSec=1min\n\
     OnUnitActiveSec=5min\n\
     Persistent=true\n\
     \n\
     [Install]\n\
     WantedBy=timers.target\n"
        .to_owned()
}

/// `ExecStart` word: quoted when it holds characters systemd would split or expand.
fn quote_exec(path: &str) -> String {
    let plain = path
        .chars()
        .all(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '\\' | '%' | '$'));
    if plain {
        return path.to_owned();
    }
    let mut out = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Executable of the `ExecStart=` line of a unit written by [`service_unit`].
fn exe_of_service(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.strip_prefix("ExecStart="))?;
    let line = line.strip_suffix(" collect")?;
    let Some(inner) = line.strip_prefix('"').and_then(|l| l.strip_suffix('"')) else {
        return Some(line.to_owned());
    };
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\\', Some(&n)) => {
                out.push(n);
                chars.next();
            }
            ('%', Some('%')) | ('$', Some('$')) => {
                out.push(c);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
