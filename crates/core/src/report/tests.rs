use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{
    DayEntry, Provider, Report, SCHEMA_VERSION, Stale, Status, month, provider, read, stale,
    unavailable, year,
};
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
        unavailable: None,
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
        unavailable: None,
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
        unavailable: None,
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
        unavailable: None,
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
    assert_eq!(monthly.month_usage, Some(6.0));
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

fn month_store(snaps: &[Snapshot]) -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("tamias.db")).unwrap();
    store
        .record_run(
            snapshot::Provider::Claude,
            snaps,
            ts("2026-11-30T12:00:00Z"),
        )
        .unwrap();
    (dir, store)
}

fn claude_month(now: &str) -> Vec<DayEntry> {
    // Seven-day windows resetting on Nov 12 and Nov 19 (UTC noon): a reset in the middle.
    let c = snapshot::Provider::Claude;
    let s = |used, resets: &str, observed: &str| snap(c, Window::SevenDay, used, resets, observed);
    let (_dir, store) = month_store(&[
        s(10.0, "2026-11-12T12:00:00Z", "2026-11-09T01:00:00Z"),
        s(30.0, "2026-11-12T12:00:00Z", "2026-11-10T20:00:00Z"),
        s(50.0, "2026-11-12T12:00:00Z", "2026-11-11T20:00:00Z"),
        s(5.0, "2026-11-19T12:00:00Z", "2026-11-12T20:00:00Z"),
        s(25.0, "2026-11-19T12:00:00Z", "2026-11-13T20:00:00Z"),
    ]);
    month(
        &store,
        c,
        2026,
        11,
        &TimeZone::UTC,
        &Thresholds::default(),
        ts(now),
    )
    .unwrap()
}

#[test]
fn month_matches_budget_day_across_a_reset() {
    let c = snapshot::Provider::Claude;
    let days = claude_month("2026-11-15T10:00:00Z");
    let snaps = [
        snap(
            c,
            Window::SevenDay,
            10.0,
            "2026-11-12T12:00:00Z",
            "2026-11-09T01:00:00Z",
        ),
        snap(
            c,
            Window::SevenDay,
            30.0,
            "2026-11-12T12:00:00Z",
            "2026-11-10T20:00:00Z",
        ),
        snap(
            c,
            Window::SevenDay,
            50.0,
            "2026-11-12T12:00:00Z",
            "2026-11-11T20:00:00Z",
        ),
        snap(
            c,
            Window::SevenDay,
            5.0,
            "2026-11-19T12:00:00Z",
            "2026-11-12T20:00:00Z",
        ),
        snap(
            c,
            Window::SevenDay,
            25.0,
            "2026-11-19T12:00:00Z",
            "2026-11-13T20:00:00Z",
        ),
    ];
    let status = crate::budget::Status {
        now: ts("2026-11-15T10:00:00Z"),
        last_ok_at: Some(ts("2026-11-15T10:00:00Z")),
    };
    let entry = |d: &str| days.iter().find(|e| e.date.to_string() == d).copied();
    for d in ["2026-11-10", "2026-11-11", "2026-11-12", "2026-11-13"] {
        let alone = crate::budget::day(
            &snaps,
            c,
            Window::SevenDay,
            d.parse().unwrap(),
            &TimeZone::UTC,
            status,
            &Thresholds::default(),
        )
        .unwrap();
        let got = entry(d).unwrap();
        assert_eq!(
            (got.usage, got.budget, got.ratio, got.colour),
            (alone.usage, alone.budget, alone.ratio, alone.colour),
            "{d}"
        );
    }
    // Literal anchor: Nov 11 rose from 30 to 50.
    assert!((entry("2026-11-11").unwrap().usage - 20.0).abs() < 1e-9);
}

#[test]
fn month_leaves_out_days_without_snapshots() {
    let days = claude_month("2026-11-15T10:00:00Z");
    assert!(
        days.iter()
            .all(|e| e.date.to_string().as_str() >= "2026-11-09")
    );
    assert!(
        days.iter()
            .all(|e| e.date.to_string().as_str() != "2026-11-01")
    );
    assert!(!days.is_empty());
}

#[test]
fn month_leaves_out_days_after_now() {
    let days = claude_month("2026-11-12T10:00:00Z");
    assert_eq!(days.last().unwrap().date.to_string(), "2026-11-12");
}

#[test]
fn month_of_an_empty_store_or_invalid_month_is_empty() {
    let (_dir, store) = month_store(&[]);
    let c = snapshot::Provider::Claude;
    let t = Thresholds::default();
    assert!(
        month(&store, c, 2026, 11, &TimeZone::UTC, &t, ts(NOW))
            .unwrap()
            .is_empty()
    );
    assert!(
        month(&store, c, 2026, 13, &TimeZone::UTC, &t, ts(NOW))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn month_in_the_future_is_empty() {
    let (_dir, store) = month_store(&claude());
    let c = snapshot::Provider::Claude;
    let t = Thresholds::default();
    assert!(
        month(&store, c, 2026, 12, &TimeZone::UTC, &t, ts(NOW))
            .unwrap()
            .is_empty()
    );
}

fn month_of(snaps: &[Snapshot], year: i16, m: i8, now: &str) -> Vec<String> {
    let (_dir, store) = month_store(snaps);
    month(
        &store,
        snapshot::Provider::Claude,
        year,
        m,
        &TimeZone::UTC,
        &Thresholds::default(),
        ts(now),
    )
    .unwrap()
    .iter()
    .map(|e| e.date.to_string())
    .collect()
}

#[test]
fn month_leaves_out_a_day_in_the_middle_without_a_result() {
    let c = snapshot::Provider::Claude;
    let s = |used, resets: &str, observed: &str| snap(c, Window::SevenDay, used, resets, observed);
    // Window A is observed Nov 2-5; window B is first observed Nov 15: Nov 6-14 have no result.
    let days = month_of(
        &[
            s(10.0, "2026-11-05T12:00:00Z", "2026-11-02T10:00:00Z"),
            s(20.0, "2026-11-05T12:00:00Z", "2026-11-04T10:00:00Z"),
            s(5.0, "2026-11-20T12:00:00Z", "2026-11-15T10:00:00Z"),
            s(9.0, "2026-11-20T12:00:00Z", "2026-11-16T10:00:00Z"),
        ],
        2026,
        11,
        "2026-11-18T10:00:00Z",
    );
    let expect: Vec<String> = [2, 3, 4, 5, 15, 16, 17, 18]
        .iter()
        .map(|d| format!("2026-11-{d:02}"))
        .collect();
    assert_eq!(days, expect);
}

#[test]
fn month_in_january_looks_back_into_the_previous_year() {
    let c = snapshot::Provider::Claude;
    let s = |used, resets: &str, observed: &str| snap(c, Window::SevenDay, used, resets, observed);
    let days = month_of(
        &[
            s(10.0, "2026-01-03T12:00:00Z", "2025-12-28T10:00:00Z"),
            s(40.0, "2026-01-03T12:00:00Z", "2026-01-02T10:00:00Z"),
        ],
        2026,
        1,
        "2026-01-03T10:00:00Z",
    );
    assert_eq!(days, ["2026-01-01", "2026-01-02", "2026-01-03"]);
}

#[test]
fn month_of_a_leap_february_has_29_days_and_none_after_now() {
    let c = snapshot::Provider::Claude;
    let snaps = [snap(
        c,
        Window::SevenDay,
        10.0,
        "2028-03-05T12:00:00Z",
        "2028-02-01T01:00:00Z",
    )];
    let all = month_of(&snaps, 2028, 2, "2028-03-02T10:00:00Z");
    assert_eq!(all.len(), 29);
    assert_eq!(all.last().unwrap(), "2028-02-29");
    let part = month_of(&snaps, 2028, 2, "2028-02-27T10:00:00Z");
    assert_eq!(part.len(), 27);
    assert_eq!(part.last().unwrap(), "2028-02-27");
}

#[test]
fn month_ignores_snapshots_observed_after_now() {
    let c = snapshot::Provider::Claude;
    let s = |used, resets: &str, observed: &str| snap(c, Window::SevenDay, used, resets, observed);
    let (_dir, store) = month_store(&[
        s(10.0, "2026-11-19T12:00:00Z", "2026-11-09T01:00:00Z"),
        s(30.0, "2026-11-19T12:00:00Z", "2026-11-12T08:00:00Z"),
        s(90.0, "2026-11-19T12:00:00Z", "2026-11-12T20:00:00Z"),
    ]);
    let days = month(
        &store,
        c,
        2026,
        11,
        &TimeZone::UTC,
        &Thresholds::default(),
        ts("2026-11-12T10:00:00Z"),
    )
    .unwrap();
    let today = days.last().unwrap();
    assert_eq!(today.date.to_string(), "2026-11-12");
    // 10 at the start of the day (carried), 30 by `now`; the 90 observed later is not seen.
    assert!((today.usage - 20.0).abs() < 1e-9);
}

fn month_usage_of(snaps: &[Snapshot], now: &str) -> Option<f64> {
    let p = provider(
        snapshot::Provider::Claude,
        snaps,
        None,
        ts(now),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    p.windows
        .iter()
        .find(|w| w.window == Window::SevenDay)
        .and_then(|w| w.month_usage)
}

#[test]
fn month_usage_sums_the_days_of_a_month_with_gaps() {
    let c = snapshot::Provider::Claude;
    let s =
        |used, observed: &str| snap(c, Window::SevenDay, used, "2026-11-12T12:00:00Z", observed);
    // Activity on Nov 2 and Nov 5 only; Nov 1, 3, 4 and 6 to 8 have no snapshots.
    let snaps = [
        s(10.0, "2026-11-02T01:00:00Z"),
        s(30.0, "2026-11-02T20:00:00Z"),
        s(30.0, "2026-11-04T20:00:00Z"),
        s(50.0, "2026-11-05T20:00:00Z"),
    ];
    assert_eq!(month_usage_of(&snaps, "2026-11-08T10:00:00Z"), Some(40.0));
}

#[test]
fn month_usage_does_not_count_negative_or_double_across_a_reset() {
    let c = snapshot::Provider::Claude;
    let s = |used, resets: &str, observed: &str| snap(c, Window::SevenDay, used, resets, observed);
    let snaps = [
        s(10.0, "2026-11-12T12:00:00Z", "2026-11-09T01:00:00Z"),
        s(50.0, "2026-11-12T12:00:00Z", "2026-11-11T20:00:00Z"),
        s(5.0, "2026-11-19T12:00:00Z", "2026-11-12T20:00:00Z"),
        s(25.0, "2026-11-19T12:00:00Z", "2026-11-13T20:00:00Z"),
    ];
    // 10 -> 50 before the reset (the first observation is the baseline), then 5 -> 25 plus the
    // 5 already used in the new window: 40 + 25, not negative and not counted twice.
    assert_eq!(month_usage_of(&snaps, "2026-11-15T10:00:00Z"), Some(65.0));
}

#[test]
fn month_usage_is_absent_for_informational_windows() {
    let p = provider(
        snapshot::Provider::Claude,
        &claude(),
        None,
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    let five = p.windows.iter().find(|w| w.window == Window::FiveHour);
    assert_eq!(five.unwrap().month_usage, None);
}

fn year_of(snaps: &[Snapshot], y: i16, now: &str) -> Vec<super::MonthEntry> {
    let (_dir, store) = month_store(snaps);
    year(
        &store,
        snapshot::Provider::Claude,
        y,
        &TimeZone::UTC,
        &Thresholds::default(),
        ts(now),
    )
    .unwrap()
}

fn seven_day(used: f64, observed: &str) -> Snapshot {
    snap(
        snapshot::Provider::Claude,
        Window::SevenDay,
        used,
        "2026-12-30T12:00:00Z",
        observed,
    )
}

#[test]
fn year_in_progress_sums_elapsed_days_and_marks_other_months_without_data() {
    let snaps = [
        seven_day(10.0, "2026-10-05T10:00:00Z"),
        seven_day(20.0, "2026-10-20T10:00:00Z"),
        seven_day(30.0, "2026-11-02T10:00:00Z"),
    ];
    let months = year_of(&snaps, 2026, "2026-11-03T10:00:00Z");
    assert_eq!(months.len(), 12);
    assert_eq!(
        months.iter().map(|m| m.month).collect::<Vec<_>>(),
        (1..=12).collect::<Vec<_>>()
    );
    let nov = months[10];
    assert!(nov.has_data);
    // Only Nov 1-3 count, not the 30 days of the month.
    let days = month_of(&snaps, 2026, 11, "2026-11-03T10:00:00Z");
    assert_eq!(days, ["2026-11-02", "2026-11-03"]);
    assert!(nov.budget > 0.0);
    assert!(nov.ratio.is_some());
    for m in &months[11..] {
        assert!(!m.has_data);
        assert_eq!(
            (m.usage, m.budget, m.ratio, m.colour),
            (0.0, 0.0, None, Colour::Grey)
        );
    }
}

#[test]
fn year_month_without_data_is_marked_and_grey() {
    let months = year_of(
        &[seven_day(10.0, "2026-10-05T10:00:00Z")],
        2026,
        "2026-11-03T10:00:00Z",
    );
    assert!(months[9].has_data);
    assert!(!months[0].has_data);
    assert_eq!(months[0].colour, Colour::Grey);
    assert_eq!(months[0].ratio, None);
}

#[test]
fn year_of_an_empty_store_has_twelve_months_without_data() {
    let months = year_of(&[], 2026, "2026-11-03T10:00:00Z");
    assert_eq!(months.len(), 12);
    assert!(months.iter().all(|m| !m.has_data));
}

#[test]
fn year_of_a_leap_year_budgets_29_days_of_february() {
    let c = snapshot::Provider::Claude;
    let snaps = [snap(
        c,
        Window::SevenDay,
        10.0,
        "2028-03-05T12:00:00Z",
        "2028-02-01T01:00:00Z",
    )];
    let now = "2028-03-02T10:00:00Z";
    let months = year_of(&snaps, 2028, now);
    let days = month_of(&snaps, 2028, 2, now);
    assert_eq!(days.len(), 29);
    let feb = months[1];
    assert!(feb.has_data);
    let (_dir, store) = month_store(&snaps);
    let entries = month(
        &store,
        c,
        2028,
        2,
        &TimeZone::UTC,
        &Thresholds::default(),
        ts(now),
    )
    .unwrap();
    assert_eq!(entries.len(), 29);
    let expected: f64 = entries.iter().map(|d| d.budget).sum();
    assert!((feb.budget - expected).abs() < 1e-9);
}

#[test]
fn year_colours_follow_the_day_rule() {
    // 10 % used in a 7-day window with a ~14 %/day budget: green; then far above: red.
    let low = year_of(
        &[seven_day(1.0, "2026-10-05T10:00:00Z")],
        2026,
        "2026-11-03T10:00:00Z",
    );
    assert_eq!(low[9].colour, Colour::Green);
    let high = year_of(
        &[
            seven_day(0.0, "2026-10-05T00:30:00Z"),
            seven_day(100.0, "2026-10-05T10:00:00Z"),
        ],
        2026,
        "2026-11-03T10:00:00Z",
    );
    assert_eq!(high[9].colour, Colour::Red);
}
