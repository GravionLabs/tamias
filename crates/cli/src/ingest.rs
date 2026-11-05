//! `tamias ingest-statusline`: the `statusLine` command of Claude Code.
//!
//! Reads the status line JSON from stdin, stores its `rate_limits` as Claude snapshots (the
//! fallback source when the OAuth poll fails or did not run) and prints the line of
//! `tamias today --short`. It never breaks the status line: every error is logged at `debug`,
//! the line is printed from what can be read (or an empty line) and the exit code is 0.
//! No network access: nothing here builds an HTTP client.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use tamias_core::config::{self, Config, Enabled};
use tamias_core::paths;
use tamias_core::provider::claude::parse_statusline;
use tamias_core::snapshot::{Provider, Snapshot, Source};
use tamias_core::store::{self, Store};

use crate::{short, today};

/// A window is stored again when its latest `statusline` snapshot is older than this.
const REFRESH_AFTER: SignedDuration = SignedDuration::from_mins(5);
/// How long a write waits for a busy store; a status line must stay responsive.
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);
/// Most bytes of stdin that are read.
const MAX_INPUT: u64 = 1 << 20;

/// Handles one status line call: reads stdin, stores, and returns the line to print (ending with
/// a newline). Never fails.
#[must_use]
pub fn run(config_path: Option<&Path>, colour: bool) -> String {
    let mut body = Vec::new();
    if let Err(e) = std::io::stdin().take(MAX_INPUT).read_to_end(&mut body) {
        tracing::debug!("ingest-statusline: cannot read stdin: {e}");
    }
    let config = config::load(config_path).unwrap_or_else(|e| {
        tracing::debug!("ingest-statusline: config: {}", e.reason());
        Config::default()
    });
    let Ok(data_dir) = paths::data_dir() else {
        tracing::debug!("ingest-statusline: no data directory");
        return "\n".to_owned();
    };
    let database = data_dir.join(paths::DATABASE_FILE);
    // Whole seconds, like `today`.
    let now =
        Timestamp::from_second(Timestamp::now().as_second()).unwrap_or_else(|_| Timestamp::now());
    let mut store = match Store::open_with_busy_timeout(&database, BUSY_TIMEOUT) {
        Ok(store) => store,
        Err(e) => {
            tracing::debug!("ingest-statusline: store: {e}");
            return "\n".to_owned();
        }
    };
    if let Err(e) = ingest(&config, &mut store, &body, now) {
        tracing::debug!("ingest-statusline: {e}");
    }
    match today::read(&config, &store, now, &TimeZone::system()) {
        Ok(today) => short::render(&today.report, colour) + "\n",
        Err(e) => {
            tracing::debug!("ingest-statusline: read: {e}");
            "\n".to_owned()
        }
    }
}

/// Parses `body` and stores the windows that changed, with `last_ok_at = now`, in one
/// transaction. Returns the number of stored snapshots; nothing is written when none changed,
/// the body has no limits or Claude is switched off.
///
/// # Errors
///
/// A body that is not a valid status line document, or store errors; nothing is stored then.
pub fn ingest(
    config: &Config,
    store: &mut Store,
    body: &[u8],
    now: Timestamp,
) -> Result<usize, Box<dyn std::error::Error>> {
    if config.claude == Enabled::Off {
        return Ok(0);
    }
    let mut changed = Vec::new();
    for snapshot in parse_statusline(body, now)? {
        if is_new(store, &snapshot, now)? {
            changed.push(snapshot);
        }
    }
    if changed.is_empty() {
        return Ok(0);
    }
    Ok(store.record_run(Provider::Claude, &changed, now)?)
}

/// Whether `snapshot` differs from the latest `statusline` snapshot of its window in `used` or
/// in `resets_at` rounded to the minute, or that one is older than [`REFRESH_AFTER`].
fn is_new(store: &Store, snapshot: &Snapshot, now: Timestamp) -> Result<bool, store::Error> {
    let from = now.checked_sub(REFRESH_AFTER).unwrap_or(now);
    let to = now.checked_add(SignedDuration::from_secs(1)).unwrap_or(now);
    let recent = store.snapshots(snapshot.provider, snapshot.window, from, to)?;
    let latest = recent.iter().rev().find(|s| s.source == Source::Statusline);
    Ok(latest.is_none_or(|last| {
        last.used.to_bits() != snapshot.used.to_bits()
            || minute(last.resets_at) != minute(snapshot.resets_at)
    }))
}

fn minute(at: Timestamp) -> i64 {
    (at.as_second() + 30).div_euclid(60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tamias_core::snapshot::Window;

    fn body(five: f64, resets: i64) -> Vec<u8> {
        format!(
            r#"{{"rate_limits":{{"five_hour":{{"used_percentage":{five},"resets_at":{resets}}}}}}}"#
        )
        .into_bytes()
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn count(store: &Store, now: Timestamp) -> usize {
        let from = now.checked_sub(SignedDuration::from_hours(24)).unwrap();
        let to = now.checked_add(SignedDuration::from_hours(24)).unwrap();
        store
            .snapshots(Provider::Claude, Window::FiveHour, from, to)
            .unwrap()
            .len()
    }

    fn fresh_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("t.db")).unwrap();
        (dir, store)
    }

    #[test]
    fn throttle_stores_first_then_skips_identical_then_stores_changes_and_old() {
        let (_dir, mut store) = fresh_store();
        let config = Config::default();
        let t0 = at("2026-11-05T10:00:00Z");
        let reset = t0.as_second() + 3600;
        assert_eq!(
            ingest(&config, &mut store, &body(10.0, reset), t0).unwrap(),
            1
        );
        let t1 = t0.checked_add(SignedDuration::from_secs(60)).unwrap();
        assert_eq!(
            ingest(&config, &mut store, &body(10.0, reset), t1).unwrap(),
            0
        );
        // a reset jitter within the same rounded minute is no change
        assert_eq!(
            ingest(&config, &mut store, &body(10.0, reset + 1), t1).unwrap(),
            0
        );
        assert_eq!(
            ingest(&config, &mut store, &body(11.0, reset), t1).unwrap(),
            1
        );
        let t2 = t1.checked_add(SignedDuration::from_secs(61)).unwrap();
        assert_eq!(
            ingest(&config, &mut store, &body(11.0, reset + 120), t2).unwrap(),
            1
        );
        let t3 = t2.checked_add(SignedDuration::from_mins(6)).unwrap();
        assert_eq!(
            ingest(&config, &mut store, &body(11.0, reset + 120), t3).unwrap(),
            1
        );
        assert_eq!(count(&store, t3), 4);
    }

    #[test]
    fn disabled_claude_stores_nothing() {
        let (_dir, mut store) = fresh_store();
        let config = config::parse("[providers]\nclaude = false\n").unwrap();
        let now = at("2026-11-05T10:00:00Z");
        assert_eq!(ingest(&config, &mut store, &body(10.0, 1), now).unwrap(), 0);
        assert!(store.status(Provider::Claude).unwrap().is_none());
    }

    #[test]
    fn invalid_body_stores_nothing_and_errors() {
        let (_dir, mut store) = fresh_store();
        let now = at("2026-11-05T10:00:00Z");
        assert!(ingest(&Config::default(), &mut store, b"garbage", now).is_err());
        assert!(store.status(Provider::Claude).unwrap().is_none());
    }

    #[test]
    fn minute_rounds_to_nearest() {
        assert_eq!(minute(Timestamp::from_second(89).unwrap()), 1);
        assert_eq!(minute(Timestamp::from_second(90).unwrap()), 2);
    }
}
