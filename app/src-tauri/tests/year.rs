//! The `year` command against a temp data dir: both providers, Copilot `unavailable`.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_app_lib::year;
use tamias_core::config::{self, Config};
use tamias_core::paths::DATABASE_FILE;
use tamias_core::report::Status;
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};
use tamias_core::store::Store;

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn snap(used: f64, observed_at: &str) -> Snapshot {
    Snapshot {
        provider: Provider::Claude,
        window: Window::SevenDay,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at: ts("2026-11-07T12:00:00Z"),
        observed_at: ts(observed_at),
        source: Source::Poll,
    }
}

const NOW: &str = "2026-11-03T10:05:00Z";

/// No `gh` whatever this machine has, so Copilot `auto` is unavailable (not set up).
fn no_gh() -> Config {
    config::parse("gh_path = \"/nonexistent/tamias-test/gh\"\n").unwrap()
}

#[test]
fn year_returns_twelve_months_for_claude_and_none_for_unavailable_copilot() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join(DATABASE_FILE);
    Store::open(&database)
        .unwrap()
        .record_run(
            Provider::Claude,
            &[
                snap(50.0, "2026-11-02T23:55:00Z"),
                snap(54.0, "2026-11-03T10:00:00Z"),
            ],
            ts("2026-11-03T10:00:00Z"),
        )
        .unwrap();
    let report = year::read(&database, &no_gh(), 2026, ts(NOW), &TimeZone::UTC).unwrap();
    assert_eq!(report.year, 2026);
    let claude = &report.providers[0];
    assert_eq!(claude.provider, Provider::Claude);
    assert_eq!(claude.months.len(), 12);
    assert!(claude.months[10].has_data);
    assert!(!claude.months[0].has_data);
    let copilot = &report.providers[1];
    assert_eq!(copilot.status, Status::Unavailable);
    assert!(copilot.months.is_empty());
}

#[test]
fn year_serialises_for_the_frontend() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join(DATABASE_FILE);
    let report = year::read(&database, &Config::default(), 2026, ts(NOW), &TimeZone::UTC).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["year"], 2026);
    assert_eq!(json["providers"][1]["provider"], "copilot");
    assert_eq!(json["providers"][1]["unit"], "requests");
    assert_eq!(json["providers"][1]["months"], serde_json::json!([]));
}
