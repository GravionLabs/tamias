//! The `month` command against a temp data dir: both providers, Copilot `unavailable`.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_app_lib::month;
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
fn month_returns_both_providers_with_copilot_unavailable() {
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
    let report = month::read(&database, &no_gh(), 2026, 11, ts(NOW), &TimeZone::UTC).unwrap();

    assert_eq!((report.year, report.month), (2026, 11));
    let [claude, copilot] = report.providers.as_slice() else {
        panic!("two providers expected");
    };
    assert_eq!(claude.provider, Provider::Claude);
    assert_eq!(claude.status, Status::Ok);
    assert_eq!(claude.unit, Unit::Percent);
    let today = claude.days.last().unwrap();
    assert_eq!(today.date.to_string(), "2026-11-03");
    assert!((today.usage - 4.0).abs() < 1e-9);
    assert_eq!(copilot.provider, Provider::Copilot);
    assert_eq!(copilot.status, Status::Unavailable);
    assert_eq!(copilot.days, []);
}

#[test]
fn month_without_a_database_is_unavailable_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join(DATABASE_FILE);
    let report = month::read(
        &database,
        &Config::default(),
        2026,
        11,
        ts(NOW),
        &TimeZone::UTC,
    )
    .unwrap();
    assert!(
        report
            .providers
            .iter()
            .all(|p| p.status == Status::Unavailable && p.days.is_empty())
    );
}

#[test]
fn month_serialises_for_the_frontend() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join(DATABASE_FILE);
    let report = month::read(
        &database,
        &Config::default(),
        2026,
        11,
        ts(NOW),
        &TimeZone::UTC,
    )
    .unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["providers"][1]["provider"], "copilot");
    assert_eq!(json["providers"][1]["status"], "unavailable");
    assert_eq!(json["providers"][1]["unit"], "requests");
}
