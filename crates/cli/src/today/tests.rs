use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_core::budget::{Colour, Thresholds};
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};
use tamias_core::store::SourceStatus;

use super::{Line, State, claude_lines, render, unavailable};
use crate::claude::Unavailable;

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn snap(window: Window, used: f64, resets_at: &str, observed_at: &str) -> Snapshot {
    Snapshot {
        provider: Provider::Claude,
        window,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at: ts(resets_at),
        observed_at: ts(observed_at),
        source: Source::Poll,
    }
}

fn ok_status(at: &str) -> SourceStatus {
    SourceStatus {
        provider: Provider::Claude,
        last_ok_at: Some(ts(at)),
        last_error_at: None,
        last_error: None,
        unavailable: None,
    }
}

/// Seven-day window resetting in 5 local days (Nov 7, 12:00 UTC), 50 % used at midnight and
/// 54 % now: budget (100 - 50) / 5 = 10 %, usage 4 %, ratio 0.4, green.
fn series() -> Vec<Snapshot> {
    vec![
        snap(
            Window::SevenDay,
            50.0,
            "2026-11-07T12:00:00Z",
            "2026-11-02T23:55:00Z",
        ),
        snap(
            Window::SevenDay,
            54.0,
            "2026-11-07T12:00:00Z",
            "2026-11-03T10:00:00Z",
        ),
        snap(
            Window::FiveHour,
            12.0,
            "2026-11-03T13:30:00Z",
            "2026-11-03T10:00:00Z",
        ),
    ]
}

const NOW: &str = "2026-11-03T10:05:00Z";

#[test]
fn seven_day_has_budget_and_five_hour_only_used_and_reset() {
    let status = ok_status("2026-11-03T10:00:00Z");
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(
        lines,
        vec![
            Line {
                provider: Provider::Claude,
                window: Some(Window::SevenDay),
                unit: Unit::Percent,
                used: Some(54.0),
                limit: Some(100.0),
                usage: Some(4.0),
                budget: Some(10.0),
                resets_at: Some(ts("2026-11-07T12:00:00Z")),
                state: State::Colour(Colour::Green),
            },
            Line {
                provider: Provider::Claude,
                window: Some(Window::FiveHour),
                unit: Unit::Percent,
                used: Some(12.0),
                limit: Some(100.0),
                usage: None,
                budget: None,
                resets_at: Some(ts("2026-11-03T13:30:00Z")),
                state: State::Colour(Colour::Green),
            },
        ]
    );
}

#[test]
fn table_without_colour_has_no_escape_codes() {
    let status = ok_status("2026-11-03T10:00:00Z");
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    let table = render(&lines, ts(NOW), &TimeZone::UTC, false);
    assert_eq!(
        table,
        "PROVIDER  WINDOW     USED  TODAY  BUDGET  RESETS            STATUS\n\
         claude    seven_day  54%   4.0%   10.0%   2026-11-07 12:00  green\n\
         claude    five_hour  12%   -      -       13:30             green\n"
    );
}

#[test]
fn table_with_colour_colours_the_status() {
    let status = ok_status("2026-11-03T10:00:00Z");
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    let table = render(&lines, ts(NOW), &TimeZone::UTC, true);
    assert!(table.contains("\x1b[32mgreen\x1b[0m\n"), "{table:?}");
    assert!(!table.lines().next().unwrap().contains('\x1b'));
}

#[test]
fn over_budget_is_red() {
    let mut snapshots = series();
    snapshots.push(snap(
        Window::SevenDay,
        65.0,
        "2026-11-07T12:00:00Z",
        "2026-11-03T10:01:00Z",
    ));
    let status = ok_status("2026-11-03T10:01:00Z");
    let lines = claude_lines(
        &snapshots,
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(lines[0].state, State::Colour(Colour::Red));
    let table = render(&lines, ts(NOW), &TimeZone::UTC, true);
    assert!(table.contains("\x1b[31mred\x1b[0m"), "{table:?}");
}

#[test]
fn failed_last_run_is_stale_since_the_last_success_with_the_error_class() {
    let status = SourceStatus {
        provider: Provider::Claude,
        last_ok_at: Some(ts("2026-11-03T10:00:00Z")),
        last_error_at: Some(ts("2026-11-03T10:04:00Z")),
        last_error: Some("http 500".to_owned()),
        unavailable: None,
    };
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    let stale = State::Stale {
        since: Some(ts("2026-11-03T10:00:00Z")),
        error: Some("http 500".to_owned()),
    };
    assert_eq!(lines[0].state, stale);
    assert_eq!(lines[1].state, stale);
    let table = render(&lines, ts(NOW), &TimeZone::UTC, false);
    assert!(table.contains("stale since 10:00 (http 500)\n"), "{table}");
}

#[test]
fn error_before_the_last_success_is_not_stale() {
    let status = SourceStatus {
        provider: Provider::Claude,
        last_ok_at: Some(ts("2026-11-03T10:00:00Z")),
        last_error_at: Some(ts("2026-11-03T09:55:00Z")),
        last_error: Some("timeout".to_owned()),
        unavailable: None,
    };
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(lines[0].state, State::Colour(Colour::Green));
}

#[test]
fn old_success_is_stale_after_stale_after() {
    let status = ok_status("2026-11-03T09:49:00Z");
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(
        lines[0].state,
        State::Stale {
            since: Some(ts("2026-11-03T09:49:00Z")),
            error: None
        }
    );
}

#[test]
fn success_exactly_stale_after_ago_is_fresh() {
    let status = ok_status("2026-11-03T09:50:00Z");
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(lines[0].state, State::Colour(Colour::Green));
}

#[test]
fn never_collected_is_stale_without_numbers() {
    let status = SourceStatus {
        provider: Provider::Claude,
        last_ok_at: None,
        last_error_at: Some(ts("2026-11-03T10:04:00Z")),
        last_error: Some("unauthorized".to_owned()),
        unavailable: None,
    };
    let lines = claude_lines(
        &[],
        Some(&status),
        ts(NOW),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].used, None);
    assert_eq!(lines[0].budget, None);
    let table = render(&lines, ts(NOW), &TimeZone::UTC, false);
    assert!(
        table.contains("claude    seven_day  -     -      -       -       stale, never collected (unauthorized)\n"),
        "{table}"
    );
}

#[test]
fn passed_reset_shows_no_reset_time_and_zero_five_hour_usage() {
    let status = ok_status("2026-11-03T14:00:00Z");
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts("2026-11-03T14:01:00Z"),
        &TimeZone::UTC,
        &Thresholds::default(),
    );
    assert_eq!(lines[1].resets_at, None);
    assert_eq!(lines[1].used, Some(0.0));
}

#[test]
fn unavailable_provider_is_one_line() {
    let lines = [unavailable(Provider::Claude, Unavailable::NotSetUp)];
    let table = render(&lines, ts(NOW), &TimeZone::UTC, false);
    assert_eq!(
        table.lines().nth(1).unwrap(),
        "claude    -       -     -      -       -       unavailable (not set up)"
    );
    let disabled = [unavailable(Provider::Claude, Unavailable::Disabled)];
    let table = render(&disabled, ts(NOW), &TimeZone::UTC, false);
    assert!(table.contains("unavailable (disabled)"), "{table}");
}

#[test]
fn times_are_shown_in_the_local_time_zone() {
    let status = SourceStatus {
        provider: Provider::Claude,
        last_ok_at: Some(ts("2026-11-03T08:00:00Z")),
        last_error_at: Some(ts("2026-11-03T10:00:00Z")),
        last_error: Some("timeout".to_owned()),
        unavailable: None,
    };
    let tz = TimeZone::get("Europe/Zurich").unwrap();
    let lines = claude_lines(
        &series(),
        Some(&status),
        ts(NOW),
        &tz,
        &Thresholds::default(),
    );
    let table = render(&lines, ts(NOW), &tz, false);
    assert!(table.contains("stale since 09:00 (timeout)"), "{table}");
    assert!(table.contains("14:30"), "{table}");
}
