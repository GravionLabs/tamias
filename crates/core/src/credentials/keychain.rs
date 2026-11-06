//! macOS Keychain source through `/usr/bin/security`.
//!
//! Claude Code writes its Keychain item with `/usr/bin/security`, so that binary is on the item's
//! access list and reading through it raises no Keychain dialog, also under launchd.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{CredentialSource, Secret, parse_credentials};
use crate::provider::claude::{Error, Result};

/// Reads the generic password `Claude Code-credentials` with
/// `security find-generic-password -s <service> -w`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeychainSource {
    program: PathBuf,
    service: String,
    timeout: Duration,
}

impl KeychainSource {
    /// The `security` binary Claude Code uses.
    pub const PROGRAM: &'static str = "/usr/bin/security";
    /// Service name of Claude Code's Keychain item.
    pub const SERVICE: &'static str = "Claude Code-credentials";
    /// Default limit for the `security` subprocess.
    pub const TIMEOUT: Duration = Duration::from_secs(10);

    /// Source with the default program, service and timeout.
    #[must_use]
    pub fn new() -> Self {
        Self::with_program(Self::PROGRAM, Self::SERVICE, Self::TIMEOUT)
    }

    /// Source running `program` (a stand-in for `security`) for `service`, killed after
    /// `timeout`.
    #[must_use]
    pub fn with_program(
        program: impl Into<PathBuf>,
        service: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            program: program.into(),
            service: service.into(),
            timeout,
        }
    }
}

impl Default for KeychainSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialSource for KeychainSource {
    fn claude_token(&self) -> Result<Secret> {
        let args = ["find-generic-password", "-s", self.service.as_str(), "-w"];
        let (status, stdout) = run_with_timeout(&self.program, &args, self.timeout)?;
        if !status.success() {
            let code = status
                .code()
                .map_or_else(|| "a signal".to_owned(), |c| c.to_string());
            return Err(Error::NoCredentials(format!(
                "Keychain item '{}' not readable (security exit {code})",
                self.service
            )));
        }
        parse_credentials(stdout.trim_ascii())
    }
}

/// Runs `program` with `args`, returns its exit status and stdout; kills it after `timeout`.
fn run_with_timeout(
    program: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>)> {
    let timed_out = || {
        Error::Timeout(format!(
            "{} did not finish within {timeout:?}",
            program.display()
        ))
    };
    let deadline = Instant::now() + timeout;
    let mut child = crate::process::command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::NoCredentials(format!("cannot run {}: {e}", program.display())))?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::NoCredentials(format!(
            "no stdout from {}",
            program.display()
        )));
    };
    // Read on a thread so a full pipe cannot block the child while we poll for its exit.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        let read = stdout.read_to_end(&mut buf).map(|_| buf);
        let _ = tx.send(read);
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(timed_out());
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                let _ = child.kill();
                return Err(Error::NoCredentials(format!(
                    "waiting for {} failed: {e}",
                    program.display()
                )));
            }
        }
    };
    // A grandchild may still hold the pipe open; never wait past the deadline (plus a grace).
    let grace = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(100));
    match rx.recv_timeout(grace) {
        Ok(Ok(out)) => Ok((status, out)),
        Ok(Err(e)) => Err(Error::NoCredentials(format!(
            "reading the output of {} failed: {e}",
            program.display()
        ))),
        Err(_) => Err(timed_out()),
    }
}
