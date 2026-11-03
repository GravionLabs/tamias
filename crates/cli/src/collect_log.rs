//! The collect log: `<data dir>/logs/collect.log`, one line per provider and run.
//!
//! A line is `<UTC time> <provider> ok|error <short message>`; the message is a fixed text or
//! an error class, never a token. Above [`MAX_BYTES`] the file is rotated to `collect.log.1`
//! (one old generation is kept).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use tamias_core::snapshot::Provider;

/// Directory of the log inside the data directory.
pub const DIR: &str = "logs";
/// File name of the log.
pub const FILE: &str = "collect.log";
/// File name of the rotated log.
pub const ROTATED_FILE: &str = "collect.log.1";
/// Size above which the log is rotated (1 MiB).
pub const MAX_BYTES: u64 = 1024 * 1024;

/// Result of one provider in one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunResult {
    Ok,
    Error,
}

impl RunResult {
    fn as_str(self) -> &'static str {
        match self {
            RunResult::Ok => "ok",
            RunResult::Error => "error",
        }
    }
}

/// Size-capped collect log in one directory.
#[derive(Debug, Clone)]
pub struct CollectLog {
    dir: PathBuf,
    max_bytes: u64,
}

impl CollectLog {
    /// Log in `<data_dir>/logs` with the default cap [`MAX_BYTES`].
    #[must_use]
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(DIR), MAX_BYTES)
    }

    /// Log in `dir`, rotated when the file is larger than `max_bytes`.
    #[must_use]
    pub fn new(dir: PathBuf, max_bytes: u64) -> Self {
        Self { dir, max_bytes }
    }

    /// Path of the current log file.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    /// Appends one line; rotates first when the file is above the cap.
    ///
    /// # Errors
    ///
    /// I/O errors creating the directory, rotating or writing.
    pub fn append(
        &self,
        at: Timestamp,
        provider: Provider,
        result: RunResult,
        message: &str,
    ) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path();
        if fs::metadata(&path).is_ok_and(|m| m.len() > self.max_bytes) {
            fs::rename(&path, self.dir.join(ROTATED_FILE))?;
        }
        let line = format_line(at, provider, result, message);
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(line.as_bytes())
    }

    /// As [`CollectLog::append`], but a failure only logs a warning: the log never fails a run.
    pub fn record(&self, at: Timestamp, provider: Provider, result: RunResult, message: &str) {
        if let Err(e) = self.append(at, provider, result, message) {
            tracing::warn!("cannot write {}: {e}", self.path().display());
        }
    }
}

fn format_line(at: Timestamp, provider: Provider, result: RunResult, message: &str) -> String {
    let message: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!(
        "{} {provider} {} {message}\n",
        at.strftime("%Y-%m-%dT%H:%M:%SZ"),
        result.as_str()
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use jiff::Timestamp;
    use tamias_core::snapshot::Provider;

    use super::{CollectLog, RunResult};

    fn at() -> Timestamp {
        "2026-11-03T10:15:42.987Z".parse().unwrap()
    }

    #[test]
    fn append_creates_the_directory_and_writes_one_line_per_call() {
        let tmp = tempfile::tempdir().unwrap();
        let log = CollectLog::in_data_dir(tmp.path());
        log.append(at(), Provider::Claude, RunResult::Ok, "stored 2 snapshots")
            .unwrap();
        log.append(at(), Provider::Claude, RunResult::Error, "unauthorized")
            .unwrap();
        assert_eq!(log.path(), tmp.path().join("logs").join("collect.log"));
        assert_eq!(
            fs::read_to_string(log.path()).unwrap(),
            "2026-11-03T10:15:42Z claude ok stored 2 snapshots\n\
             2026-11-03T10:15:42Z claude error unauthorized\n"
        );
    }

    #[test]
    fn control_characters_in_the_message_cannot_split_a_line() {
        let tmp = tempfile::tempdir().unwrap();
        let log = CollectLog::new(tmp.path().to_path_buf(), 1024);
        log.append(at(), Provider::Copilot, RunResult::Error, "a\nb\r")
            .unwrap();
        assert_eq!(
            fs::read_to_string(log.path()).unwrap(),
            "2026-11-03T10:15:42Z copilot error a b \n"
        );
    }

    #[test]
    fn a_file_at_the_cap_is_not_rotated() {
        let tmp = tempfile::tempdir().unwrap();
        let log = CollectLog::new(tmp.path().to_path_buf(), 10);
        fs::write(log.path(), "0123456789").unwrap();
        log.append(at(), Provider::Claude, RunResult::Ok, "x")
            .unwrap();
        assert!(!tmp.path().join("collect.log.1").exists());
        assert!(
            fs::read_to_string(log.path())
                .unwrap()
                .starts_with("0123456789")
        );
    }

    #[test]
    fn a_file_above_the_cap_is_rotated_before_the_line_is_written() {
        let tmp = tempfile::tempdir().unwrap();
        let log = CollectLog::new(tmp.path().to_path_buf(), 10);
        fs::write(log.path(), "0123456789A").unwrap();
        fs::write(tmp.path().join("collect.log.1"), "older").unwrap();
        log.append(at(), Provider::Claude, RunResult::Ok, "x")
            .unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("collect.log.1")).unwrap(),
            "0123456789A"
        );
        assert_eq!(
            fs::read_to_string(log.path()).unwrap(),
            "2026-11-03T10:15:42Z claude ok x\n"
        );
    }

    #[test]
    fn record_swallows_write_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let blocker = tmp.path().join("file");
        fs::write(&blocker, "").unwrap();
        let log = CollectLog::new(blocker.join("logs"), 10);
        assert!(
            log.append(at(), Provider::Claude, RunResult::Ok, "x")
                .is_err()
        );
        log.record(at(), Provider::Claude, RunResult::Ok, "x");
    }
}
