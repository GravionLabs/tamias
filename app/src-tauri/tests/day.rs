//! The `day` command against a temp data dir: the curve against the budget, nothing after `now`.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_app_lib::day::{self, Error};
use tamias_core::budget::Colour;
use tamias_core::paths::DATABASE_FILE;
use tamias_core::report::Status;
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};
use tamias_core::store::Store;

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn snap(window: Window, used: f64, observed_at: &str) -> Snapshot {
    Snapshot {
        provider: Provider::Claude,
        window,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at: ts("2026-11-07T12:00:00Z"),
        observed_at: ts(observed_at),
        source: Source::Poll,
    }
}

const NOW: &str = "2026-11-03T10:05:00Z";

fn database(dir: &tempfile::TempDir, snapshots: &[Snapshot]) -> PathBuf {
    let path = dir.path().join(DATABASE_FILE);
    Store::open(&path)
        .unwrap()
        .record_run(Provider::Claude, snapshots, ts("2026-11-03T10:00:00Z"))
        .unwrap();
    path
}

fn read(path: &Path, date: &str) -> Result<day::DayReport, Error> {
    // No `gh` whatever this machine has, so Copilot `auto` is unavailable (not set up).
    let config = tamias_core::config::parse("gh_path = \"/nonexistent/tamias-test/gh\"\n").unwrap();
    day::read(path, &config, date, ts(NOW), &TimeZone::UTC)
}

#[test]
fn day_has_the_curve_the_budget_and_the_five_hour_level() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(
        &dir,
        &[
            snap(Window::SevenDay, 50.0, "2026-11-02T23:55:00Z"),
            snap(Window::SevenDay, 52.0, "2026-11-03T06:00:00Z"),
            snap(Window::SevenDay, 54.0, "2026-11-03T10:00:00Z"),
            snap(Window::FiveHour, 30.0, "2026-11-03T10:00:00Z"),
            // After `now`: must not count.
            snap(Window::SevenDay, 90.0, "2026-11-03T12:00:00Z"),
        ],
    );
    let report = read(&path, "2026-11-03").unwrap();
    assert_eq!(report.date.to_string(), "2026-11-03");
    let claude = &report.providers[0];
    assert_eq!(claude.status, Status::Ok);
    let usage: Vec<f64> = claude.points.iter().map(|p| p.usage).collect();
    assert_eq!(usage, [0.0, 2.0, 4.0]);
    assert!((claude.usage.unwrap() - 4.0).abs() < 1e-9);
    assert!(claude.budget.unwrap() > 0.0);
    assert_eq!(claude.colour, Some(Colour::Green));
    assert!((claude.five_hour.unwrap().used - 30.0).abs() < 1e-9);

    let copilot = &report.providers[1];
    assert_eq!(copilot.status, Status::Unavailable);
    assert!(copilot.points.is_empty() && copilot.budget.is_none());
}

#[test]
fn a_day_without_data_has_no_points() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(
        &dir,
        &[snap(Window::SevenDay, 50.0, "2026-11-03T06:00:00Z")],
    );
    let report = read(&path, "2026-11-20").unwrap();
    let claude = &report.providers[0];
    assert_eq!(claude.points, []);
    assert_eq!(
        (claude.usage, claude.budget, claude.colour),
        (None, None, None)
    );
}

#[test]
fn a_missing_database_is_unavailable_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let report = read(&dir.path().join(DATABASE_FILE), "2026-11-03").unwrap();
    assert!(
        report
            .providers
            .iter()
            .all(|p| p.status == Status::Unavailable && p.points.is_empty())
    );
}

#[test]
fn an_invalid_date_is_an_error_and_not_busy() {
    let dir = tempfile::tempdir().unwrap();
    let error = read(&dir.path().join(DATABASE_FILE), "yesterday").unwrap_err();
    assert!(matches!(error, Error::InvalidDate(_)));
    assert!(!error.is_busy());
}

#[test]
fn day_serialises_for_the_frontend() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(
        &dir,
        &[snap(Window::SevenDay, 50.0, "2026-11-03T06:00:00Z")],
    );
    let json = serde_json::to_value(read(&path, "2026-11-03").unwrap()).unwrap();
    assert_eq!(json["date"], "2026-11-03");
    assert_eq!(json["providers"][0]["unit"], "percent");
    assert_eq!(
        json["providers"][0]["points"][0]["at"],
        "2026-11-03T00:00:00Z"
    );
    assert_eq!(json["providers"][1]["status"], "unavailable");
}

#[test]
fn a_datetime_is_not_a_date() {
    let dir = tempfile::tempdir().unwrap();
    let error = read(&dir.path().join(DATABASE_FILE), "2026-11-04T10:00").unwrap_err();
    assert!(matches!(error, Error::InvalidDate(_)));
}

#[test]
fn a_future_day_has_no_points_and_no_error() {
    let dir = tempfile::tempdir().unwrap();
    let report = read(&dir.path().join(DATABASE_FILE), "2099-01-01").unwrap();
    assert!(report.providers.iter().all(|p| p.points.is_empty()));
}

#[test]
fn day_has_the_five_hour_curve_for_claude_only_and_nothing_after_now() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(
        &dir,
        &[
            snap(Window::SevenDay, 50.0, "2026-11-03T06:00:00Z"),
            snap(Window::FiveHour, 80.0, "2026-11-03T04:00:00Z"),
            snap(Window::FiveHour, 10.0, "2026-11-03T09:00:00Z"),
            snap(Window::FiveHour, 99.0, "2026-11-03T12:00:00Z"), // after now
        ],
    );
    let report = read(&path, "2026-11-03").unwrap();
    let points = report.providers[0].five_hour_points.as_ref().unwrap();
    let used: Vec<f64> = points.iter().map(|p| p.used).collect();
    assert_eq!(used, [80.0, 10.0]);
    assert!(report.providers[1].five_hour_points.is_none());

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["providers"][0]["five_hour_points"][1]["limit"], 100.0);
    assert!(json["providers"][1].get("five_hour_points").is_none());
}

#[test]
fn day_report_is_schema_v1_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(
        &dir,
        &[
            snap(Window::SevenDay, 50.0, "2026-11-03T06:00:00Z"),
            snap(Window::SevenDay, 52.0, "2026-11-03T09:00:00Z"),
            snap(Window::FiveHour, 30.0, "2026-11-03T09:00:00Z"),
        ],
    );
    let json = serde_json::to_value(read(&path, "2026-11-03").unwrap()).unwrap();
    let schema_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema/day-v1.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&json)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}\n{json:#}");
    assert_eq!(json["providers"][0]["five_hour_points"][0]["used"], 30.0);
}
