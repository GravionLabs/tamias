use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{Provider, Report, SCHEMA_VERSION, Stale, Status, provider, read, stale, unavailable};
use crate::budget::{Colour, Thresholds};
use crate::snapshot::{self, Snapshot, Source, Unit, Window};
use crate::store::{SourceStatus, Store};

const NOW: &str = "2026-11-03T10:05:00Z";

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn snap(
    provider: snapshot::Provider,
    window: Window,
    used: f64,
    resets_at: &str,
    observed_at: &str,
) -> Snapshot {
    let (limit, unit) = match window {
        Window::Monthly => (300.0, Unit::Requests),
        _ => (100.0, Unit::Percent),
    };
    Snapshot {
        provider,
        window,
        used,
        limit,
        unit,
        resets_at: ts(resets_at),
        observed_at: ts(observed_at),
        source: Source::Poll,
    }
}

/// Seven-day: 50 % at midnight, 54 % now, reset in 5 local days: budget 10, usage 4, ratio 0.4.
fn claude() -> Vec<Snapshot> {
    let c = snapshot::Provider::Claude;
    vec![
        snap(
            c,
            Window::SevenDay,
            50.0,
            "2026-11-07T12:00:00Z",
            "2026-11-02T23:55:00Z",
        ),
        snap(
            c,
            Window::SevenDay,
            54.0,
            "2026-11-07T12:00:00Z",
            "2026-11-03T10:00:00Z",
        ),
        snap(
            c,
            Window::FiveHour,
            12.0,
            "2026-11-03T13:30:00Z",
            "2026-11-03T10:00:00Z",
        ),
    ]
}

/// Monthly: 20 of 300 at midnight, 26 now, reset Dec 1 (28 days incl. today): budget 10, usage 6.
fn copilot() -> Vec<Snapshot> {
    let c = snapshot::Provider::Copilot;
    vec![
        snap(
            c,
            Window::Monthly,
            20.0,
            "2026-12-01T00:00:00Z",
            "2026-11-02T23:55:00Z",
        ),
        snap(
            c,
            Window::Monthly,
            26.0,
            "2026-12-01T00:00:00Z",
            "2026-11-03T10:00:00Z",
        ),
    ]
}

fn ok(provider: snapshot::Provider, at: &str) -> SourceStatus {
    SourceStatus {
        provider,
        last_ok_at: Some(ts(at)),
        last_error_at: None,
        last_error: None,
    }
}

fn build(p: snapshot::Provider, snaps: &[Snapshot], status: Option<&SourceStatus>) -> Provider {
    provider(
        p,
        snaps,
        status,
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    )
}

/// The pinned document; also the example the CLI tests validate against the schema.
const DOCUMENT: &str = include_str!("../../../../docs/schema/today-v1.example.json");

fn document() -> Report {
    let claude_status = ok(snapshot::Provider::Claude, "2026-11-03T10:00:00Z");
    let copilot_status = ok(snapshot::Provider::Copilot, "2026-11-03T09:00:00Z");
    Report::new(
        ts(NOW),
        vec![
            build(snapshot::Provider::Claude, &claude(), Some(&claude_status)),
            build(
                snapshot::Provider::Copilot,
                &copilot(),
                Some(&copilot_status),
            ),
        ],
    )
}

#[test]
fn complete_document_is_pinned() {
    assert_eq!(
        serde_json::to_string_pretty(&document()).unwrap(),
        DOCUMENT.trim_end()
    );
}

#[test]
fn document_round_trips() {
    let parsed: Report = serde_json::from_str(DOCUMENT).unwrap();
    assert_eq!(parsed, document());
    assert_eq!(parsed.schema_version, SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 1);
}

#[test]
fn unknown_status_is_rejected() {
    let text = DOCUMENT.replacen("\"ok\"", "\"fine\"", 1);
    assert!(serde_json::from_str::<Report>(&text).is_err());
}

#[test]
fn informational_window_has_no_budget_fields() {
    let status = ok(snapshot::Provider::Claude, "2026-11-03T10:00:00Z");
    let p = build(snapshot::Provider::Claude, &claude(), Some(&status));
    let five = &p.windows[1];
    assert_eq!(five.window, Window::FiveHour);
    assert!(!five.binding);
    assert_eq!(
        (five.today_usage, five.budget, five.ratio),
        (None, None, None)
    );
}

#[test]
fn unavailable_provider_has_no_windows() {
    let p = unavailable(snapshot::Provider::Claude);
    assert_eq!(p.status, Status::Unavailable);
    assert!(p.windows.is_empty());
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"provider":"claude","status":"unavailable","windows":[]}"#
    );
}

#[test]
fn never_collected_is_stale_without_since_and_windows() {
    let status = SourceStatus {
        provider: snapshot::Provider::Claude,
        last_ok_at: None,
        last_error_at: Some(ts("2026-11-03T10:04:00Z")),
        last_error: Some("http 500".to_owned()),
    };
    let p = build(snapshot::Provider::Claude, &[], Some(&status));
    assert_eq!(p.status, Status::Stale);
    assert_eq!(p.stale_since, None);
    assert!(p.windows.is_empty());
    let none = build(snapshot::Provider::Claude, &[], None);
    assert_eq!(none.status, Status::Stale);
}

#[test]
fn failure_after_success_is_stale_with_grey_windows_but_numbers_kept() {
    let status = SourceStatus {
        provider: snapshot::Provider::Claude,
        last_ok_at: Some(ts("2026-11-03T10:00:00Z")),
        last_error_at: Some(ts("2026-11-03T10:04:00Z")),
        last_error: Some("network".to_owned()),
    };
    let p = build(snapshot::Provider::Claude, &claude(), Some(&status));
    assert_eq!(p.status, Status::Stale);
    assert_eq!(p.stale_since, Some(ts("2026-11-03T10:00:00Z")));
    assert!(p.windows.iter().all(|w| w.colour == Colour::Grey));
    assert!((p.windows[0].used - 54.0).abs() < f64::EPSILON);
    assert_eq!(p.windows[0].budget, Some(10.0));
}

#[test]
fn stale_reports_the_error_only_when_after_the_success() {
    let thresholds = Thresholds::default();
    let mut status = SourceStatus {
        provider: snapshot::Provider::Claude,
        last_ok_at: Some(ts("2026-11-03T10:00:00Z")),
        last_error_at: Some(ts("2026-11-03T09:55:00Z")),
        last_error: Some("timeout".to_owned()),
    };
    assert_eq!(stale(Some(&status), ts(NOW), &thresholds), None);
    status.last_error_at = Some(ts("2026-11-03T10:01:00Z"));
    assert_eq!(
        stale(Some(&status), ts(NOW), &thresholds),
        Some(Stale {
            since: Some(ts("2026-11-03T10:00:00Z")),
            error: Some("timeout".to_owned()),
        })
    );
    status.last_error = Some(String::new());
    assert_eq!(
        stale(Some(&status), ts(NOW), &thresholds).unwrap().error,
        None
    );
}

#[test]
fn success_exactly_stale_after_ago_is_ok_and_one_minute_more_is_stale() {
    let thresholds = Thresholds::default();
    let fresh = ok(snapshot::Provider::Claude, "2026-11-03T09:50:00Z");
    assert_eq!(stale(Some(&fresh), ts(NOW), &thresholds), None);
    let old = ok(snapshot::Provider::Claude, "2026-11-03T09:49:00Z");
    assert!(stale(Some(&old), ts(NOW), &thresholds).is_some());
}

#[test]
fn read_takes_the_numbers_from_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("tamias.db")).unwrap();
    store
        .record_run(
            snapshot::Provider::Copilot,
            &copilot(),
            ts("2026-11-03T10:00:00Z"),
        )
        .unwrap();
    let p = read(
        &store,
        snapshot::Provider::Copilot,
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    )
    .unwrap();
    assert_eq!(p.status, Status::Ok);
    assert_eq!(p.windows.len(), 1);
    let monthly = &p.windows[0];
    assert_eq!(monthly.window, Window::Monthly);
    assert_eq!(monthly.unit, Unit::Requests);
    assert!((monthly.used - 26.0).abs() < f64::EPSILON);
    assert!((monthly.limit - 300.0).abs() < f64::EPSILON);
    assert_eq!(monthly.today_usage, Some(6.0));
    assert_eq!(monthly.budget, Some(10.0));
    assert_eq!(monthly.ratio, Some(0.6));
    assert_eq!(monthly.colour, Colour::Green);
}

#[test]
fn read_of_an_empty_store_is_stale_without_windows() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("tamias.db")).unwrap();
    let p = read(
        &store,
        snapshot::Provider::Claude,
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    )
    .unwrap();
    assert_eq!(p.status, Status::Stale);
    assert!(p.windows.is_empty());
}

#[test]
fn parse_reads_the_schema_example() {
    let report = Report::parse(DOCUMENT).unwrap();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.providers.len(), 2);
}

#[test]
fn parse_ignores_added_fields_within_version_1() {
    let text = DOCUMENT.replacen("\"binding\"", "\"future\": 1, \"binding\"", 1);
    assert!(Report::parse(&text).is_ok());
}

#[test]
fn parse_rejects_another_schema_version_even_with_another_shape() {
    let err = Report::parse(r#"{"schema_version": 2, "rows": []}"#).unwrap_err();
    assert!(matches!(
        err,
        super::ParseError::UnsupportedVersion { found: 2 }
    ));
}

#[test]
fn parse_rejects_garbage_and_a_missing_version() {
    assert!(matches!(
        Report::parse("nope").unwrap_err(),
        super::ParseError::Json(_)
    ));
    assert!(matches!(
        Report::parse("{}").unwrap_err(),
        super::ParseError::Json(_)
    ));
}
