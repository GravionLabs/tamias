//! macOS: a per-user launchd agent (no admin rights).
//!
//! `~/Library/LaunchAgents/com.gravionlabs.tamias.collect.plist` runs `<exe> collect` at load
//! and every 300 seconds; launchd runs a missed interval once on wake. It is loaded with
//! `launchctl bootstrap gui/<uid>` and removed with `launchctl bootout`. stdout and stderr go
//! to `/dev/null`: the collector writes its own log.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::{Error, Result, Runner, Scheduler, SchedulerState, run_checked};

/// launchd label of the collector.
pub const LABEL: &str = "com.gravionlabs.tamias.collect";
/// Seconds between two runs.
pub const INTERVAL_SECONDS: u32 = 300;

const LAUNCHCTL: &str = "launchctl";
/// Waits for launchd to finish a `bootout` before the next `bootstrap`.
const BOOTOUT_POLLS: u32 = 20;
const BOOTOUT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// `~/Library/LaunchAgents` of `home`.
#[must_use]
pub fn agents_dir(home: &Path) -> PathBuf {
    home.join("Library").join("LaunchAgents")
}

/// The launchd agent of the collector in one `LaunchAgents` directory.
pub struct LaunchAgent {
    agents_dir: PathBuf,
    runner: Box<dyn Runner>,
}

impl std::fmt::Debug for LaunchAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchAgent")
            .field("agents_dir", &self.agents_dir)
            .finish_non_exhaustive()
    }
}

impl LaunchAgent {
    /// Agent with its plist in `agents_dir`, running commands with `runner`.
    #[must_use]
    pub fn new(agents_dir: PathBuf, runner: Box<dyn Runner>) -> Self {
        Self { agents_dir, runner }
    }

    /// Path of the plist.
    #[must_use]
    pub fn plist_path(&self) -> PathBuf {
        self.agents_dir.join(format!("{LABEL}.plist"))
    }

    fn temp_path(&self) -> PathBuf {
        self.agents_dir.join(format!(".{LABEL}.plist.tmp"))
    }

    fn domain(&self) -> Result<String> {
        let output = run_checked(self.runner.as_ref(), "id", &["-u"])?;
        let uid = output.stdout.trim();
        if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Other(format!(
                "unexpected output of `id -u`: {uid:?}"
            )));
        }
        Ok(format!("gui/{uid}"))
    }

    /// `launchctl print` output when the agent is registered, else `None`.
    fn registration(&self, domain: &str) -> Result<Option<String>> {
        let target = format!("{domain}/{LABEL}");
        let output = self
            .runner
            .run(LAUNCHCTL, &["print", &target])
            .map_err(|e| Error::io(format!("cannot run `{LAUNCHCTL} print {target}`"), e))?;
        Ok(output.success.then_some(output.stdout))
    }

    fn bootout(&self, domain: &str) -> Result<()> {
        let target = format!("{domain}/{LABEL}");
        run_checked(self.runner.as_ref(), LAUNCHCTL, &["bootout", &target])?;
        for _ in 0..BOOTOUT_POLLS {
            if self.registration(domain)?.is_none() {
                return Ok(());
            }
            thread::sleep(BOOTOUT_POLL_INTERVAL);
        }
        Err(Error::Other(format!(
            "launchd still has {target} registered after bootout"
        )))
    }

    /// Steps of an install after the temp file is written; the caller cleans up on error.
    fn replace_and_load(&self, domain: &str, temp: &Path) -> Result<()> {
        if self.registration(domain)?.is_some() {
            self.bootout(domain)?;
        }
        let plist = self.plist_path();
        fs::rename(temp, &plist)
            .map_err(|e| Error::io(format!("cannot write {}", plist.display()), e))?;
        let plist_arg = plist
            .to_str()
            .ok_or_else(|| Error::Other(format!("{} is not valid UTF-8", plist.display())))?;
        if let Err(e) = run_checked(
            self.runner.as_ref(),
            LAUNCHCTL,
            &["bootstrap", domain, plist_arg],
        ) {
            let _ = fs::remove_file(&plist);
            return Err(e);
        }
        Ok(())
    }
}

impl Scheduler for LaunchAgent {
    fn install(&self, exe: &Path) -> Result<()> {
        let exe = exe
            .to_str()
            .ok_or_else(|| Error::Other(format!("{} is not valid UTF-8", exe.display())))?;
        let domain = self.domain()?;
        fs::create_dir_all(&self.agents_dir)
            .map_err(|e| Error::io(format!("cannot create {}", self.agents_dir.display()), e))?;
        let temp = self.temp_path();
        if let Err(e) = fs::write(&temp, plist(exe)) {
            let _ = fs::remove_file(&temp);
            return Err(Error::io(format!("cannot write {}", temp.display()), e));
        }
        let result = self.replace_and_load(&domain, &temp);
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn uninstall(&self) -> Result<()> {
        let domain = self.domain()?;
        if self.registration(&domain)?.is_some() {
            self.bootout(&domain)?;
        }
        for path in [self.plist_path(), self.temp_path()] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(format!("cannot remove {}", path.display()), e)),
            }
        }
        Ok(())
    }

    fn status(&self) -> Result<SchedulerState> {
        let domain = self.domain()?;
        let registration = self.registration(&domain)?;
        let plist_path = self.plist_path();
        let plist_text = match fs::read_to_string(&plist_path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(Error::io(
                    format!("cannot read {}", plist_path.display()),
                    e,
                ));
            }
        };
        let exe = plist_text
            .as_deref()
            .and_then(program_of_plist)
            .or_else(|| registration.as_deref().and_then(program_of_print))
            .map(PathBuf::from);
        Ok(SchedulerState {
            installed: plist_text.is_some() || registration.is_some(),
            registered: registration.is_some(),
            exe,
            definition: plist_text.is_some().then_some(plist_path),
        })
    }
}

/// The plist of the agent running `exe collect`.
#[must_use]
pub fn plist(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>collect</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>StartInterval</key>
	<integer>{INTERVAL_SECONDS}</integer>
	<key>StandardOutPath</key>
	<string>/dev/null</string>
	<key>StandardErrorPath</key>
	<string>/dev/null</string>
</dict>
</plist>
"#,
        escape(exe)
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// First `ProgramArguments` entry of a plist written by [`plist`].
fn program_of_plist(text: &str) -> Option<String> {
    let rest = &text[text.find("<key>ProgramArguments</key>")?..];
    let start = rest.find("<string>")? + "<string>".len();
    let len = rest[start..].find("</string>")?;
    Some(unescape(&rest[start..start + len]))
}

/// `program = <path>` of `launchctl print`.
fn program_of_print(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("program = "))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
