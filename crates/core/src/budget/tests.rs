//! Fixture tests of the daily usage and budget. Expected values are hand-computed.

use jiff::civil::{Date, date};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use super::{Colour, Point, Status, Thresholds, day, intraday, level, today};
use crate::snapshot::{Provider, Snapshot, Source, Unit, Window};

fn ts(s: &str) -> Timestamp {
    s.parse().expect("valid timestamp")
}

fn berlin() -> TimeZone {
    TimeZone::get("Europe/Berlin").expect("Europe/Berlin in the tzdb")
}

fn snap(window: Window, used: f64, resets_at: &str, observed_at: &str, source: Source) -> Snapshot {
    Snapshot {
        provider: Provider::Claude,
        window,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at: ts(resets_at),
        observed_at: ts(observed_at),
        source,
    }
}

fn week(used: f64, resets_at: &str, observed_at: &str) -> Snapshot {
    snap(Window::SevenDay, used, resets_at, observed_at, Source::Poll)
}

fn copilot(used: f64, observed_at: &str) -> Snapshot {
    Snapshot {
        provider: Provider::Copilot,
        window: Window::Monthly,
        used,
        limit: 300.0,
        unit: Unit::Requests,
        resets_at: ts("2026-12-01T00:00:00Z"),
        observed_at: ts(observed_at),
        source: Source::Poll,
    }
}

/// Provider read successfully right now.
fn fresh(now: &str) -> Status {
    Status {
        now: ts(now),
        last_ok_at: Some(ts(now)),
    }
}

fn claude_day(snaps: &[Snapshot], d: Date, now: &str) -> super::Day {
    day(
        snaps,
        Provider::Claude,
        Window::SevenDay,
        d,
        &berlin(),
        fresh(now),
        &Thresholds::default(),
    )
    .expect("a day")
}

fn copilot_day(snaps: &[Snapshot], d: Date, now: &str) -> super::Day {
    day(
        snaps,
        Provider::Copilot,
        Window::Monthly,
        d,
        &TimeZone::UTC,
        fresh(now),
        &Thresholds::default(),
    )
    .expect("a day")
}

#[track_caller]
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

const WEEK: &str = "2026-11-09T21:00:00Z";

#[test]
fn steady_usage_is_the_rise_of_the_day_against_a_budget_spread_over_the_remaining_days() {
    let snaps = [
        week(25.0, WEEK, "2026-11-03T10:00:00Z"),
        week(30.0, WEEK, "2026-11-03T22:00:00Z"), // 23:00 local, Nov 3
        week(33.0, WEEK, "2026-11-04T07:00:00Z"),
        week(36.0, WEEK, "2026-11-04T11:00:00Z"),
        week(40.0, WEEK, "2026-11-04T19:00:00Z"),
    ];
    let d = claude_day(&snaps, date(2026, 11, 4), "2026-11-04T19:00:00Z");
    close(d.usage, 10.0);
    // (100 - 30) / 6 days (Nov 4..=9)
    close(d.budget, 11.666_666_666_666_666);
    close(d.ratio.unwrap(), 0.857_142_857_142_857);
    assert_eq!(d.colour, Colour::Yellow);
}

#[test]
fn interleaved_poll_and_statusline_values_with_jitter_count_no_extra_usage() {
    let snaps = [
        week(40.0, WEEK, "2026-11-03T22:00:00Z"),
        snap(
            Window::SevenDay,
            42.0,
            "2026-11-09T20:59:59.524353Z",
            "2026-11-04T07:00:00Z",
            Source::Poll,
        ),
        snap(
            Window::SevenDay,
            41.6,
            "2026-11-09T21:00:00Z",
            "2026-11-04T07:01:00Z",
            Source::Statusline,
        ),
        snap(
            Window::SevenDay,
            42.0,
            "2026-11-09T21:00:00.48Z",
            "2026-11-04T07:05:00Z",
            Source::Poll,
        ),
    ];
    let d = claude_day(&snaps, date(2026, 11, 4), "2026-11-04T07:05:00Z");
    close(d.usage, 2.0);
    close(d.budget, 10.0); // (100 - 40) / 6
    close(d.ratio.unwrap(), 0.2);
    assert_eq!(d.colour, Colour::Green);
}

/// Reset of the seven-day window at 22:00 local on Nov 9 (as observed in `docs/sources.md`).
fn reset_day_snapshots() -> Vec<Snapshot> {
    vec![
        snap(
            Window::SevenDay,
            93.0,
            "2026-11-09T20:59:59.524353Z",
            "2026-11-08T20:00:00Z",
            Source::Poll,
        ),
        snap(
            Window::SevenDay,
            95.0,
            "2026-11-09T21:00:00.48Z",
            "2026-11-09T12:00:00Z",
            Source::Statusline,
        ),
        snap(
            Window::SevenDay,
            95.0,
            "2026-11-09T21:00:00Z",
            "2026-11-09T20:55:00Z",
            Source::Poll,
        ),
        snap(
            Window::SevenDay,
            0.0,
            "2026-11-16T21:00:00Z",
            "2026-11-09T21:05:00Z",
            Source::Poll,
        ),
        snap(
            Window::SevenDay,
            1.5,
            "2026-11-16T21:00:00.3Z",
            "2026-11-09T22:30:00Z",
            Source::Statusline,
        ),
    ]
}

#[test]
fn jittered_resets_at_is_one_window_and_next_weeks_reset_is_a_new_one() {
    let d = claude_day(
        &reset_day_snapshots(),
        date(2026, 11, 9),
        "2026-11-09T22:30:00Z",
    );
    // old window 95 - 93, new window 1.5 - 0
    close(d.usage, 3.5);
    assert_eq!(d.colour, Colour::Green);
}

#[test]
fn reset_at_22_local_adds_the_prorated_budget_of_the_new_window() {
    let d = claude_day(
        &reset_day_snapshots(),
        date(2026, 11, 9),
        "2026-11-09T22:30:00Z",
    );
    // old: (100 - 93) / 1 = 7; new: 100 / 8 days (Nov 9..=16) * 2 h / 24 h = 1.041666…
    close(d.budget, 8.041_666_666_666_666);
    close(d.ratio.unwrap(), 0.435_233_160_621_761_7);
}

#[test]
fn day_after_the_reset_budgets_the_new_window_from_its_high_water_mark() {
    let mut snaps = reset_day_snapshots();
    snaps.push(snap(
        Window::SevenDay,
        4.5,
        "2026-11-16T21:00:00Z",
        "2026-11-10T09:00:00Z",
        Source::Poll,
    ));
    let d = claude_day(&snaps, date(2026, 11, 10), "2026-11-10T09:00:00Z");
    close(d.usage, 3.0);
    close(d.budget, 14.071_428_571_428_571); // (100 - 1.5) / 7 (Nov 10..=16)
}

#[test]
fn reset_inside_the_day_not_yet_observed_uses_the_expected_next_window() {
    let resets = "2026-11-09T05:00:00Z"; // 06:00 local
    let snaps = [
        week(60.0, resets, "2026-11-08T20:00:00Z"),
        week(65.0, resets, "2026-11-09T03:30:00Z"),
    ];
    let d = claude_day(&snaps, date(2026, 11, 9), "2026-11-09T04:00:00Z");
    close(d.usage, 5.0);
    // old: 40 / 1; new (resets Nov 16): 100 / 8 * 18 h / 24 h = 9.375
    close(d.budget, 49.375);
    assert_eq!(d.colour, Colour::Green);
}

#[test]
fn gap_over_midnight_attributes_the_usage_to_the_day_of_the_next_snapshot() {
    let snaps = [
        week(25.0, WEEK, "2026-11-03T06:00:00Z"),
        week(30.0, WEEK, "2026-11-03T20:00:00Z"), // 21:00 local, Nov 3
        week(35.0, WEEK, "2026-11-04T07:00:00Z"), // 08:00 local, Nov 4
    ];
    let nov3 = claude_day(&snaps, date(2026, 11, 3), "2026-11-04T07:00:00Z");
    close(nov3.usage, 5.0); // start of the data: from the first value 25
    close(nov3.budget, 10.714_285_714_285_714); // 75 / 7 (Nov 3..=9)
    let nov4 = claude_day(&snaps, date(2026, 11, 4), "2026-11-04T07:00:00Z");
    close(nov4.usage, 5.0);
    close(nov4.budget, 11.666_666_666_666_666); // 70 / 6
}

#[test]
fn dst_start_in_berlin_is_a_23_hour_day() {
    let old = "2027-03-28T20:00:00Z"; // 22:00 CEST
    let new = "2027-04-04T20:00:00Z";
    let snaps = [
        week(90.0, old, "2027-03-27T22:30:00Z"), // 23:30 CET, Mar 27
        week(91.0, old, "2027-03-27T23:30:00Z"), // 00:30 CET, Mar 28
        week(92.0, old, "2027-03-28T19:00:00Z"),
        week(2.0, new, "2027-03-28T21:00:00Z"),
    ];
    let d = claude_day(&snaps, date(2027, 3, 28), "2027-03-28T21:00:00Z");
    close(d.usage, 4.0);
    // old: 10 / 1; new: 100 / 8 (Mar 28..=Apr 4) * 2 h / 23 h = 25 / 23
    close(d.budget, 11.086_956_521_739_13);
}

#[test]
fn dst_end_in_berlin_is_a_25_hour_day() {
    let old = "2026-10-25T21:00:00Z"; // 22:00 CET
    let new = "2026-11-01T21:00:00Z";
    let snaps = [
        week(80.0, old, "2026-10-24T21:30:00Z"), // 23:30 CEST, Oct 24
        week(84.0, old, "2026-10-25T10:00:00Z"),
        week(3.0, new, "2026-10-25T22:30:00Z"), // 23:30 CET, Oct 25
    ];
    let d = claude_day(&snaps, date(2026, 10, 25), "2026-10-25T22:30:00Z");
    close(d.usage, 7.0);
    // old: 20 / 1; new: 100 / 8 (Oct 25..=Nov 1) * 2 h / 25 h = 1
    close(d.budget, 21.0);
}

#[test]
fn budget_with_one_remaining_day_is_all_that_is_left() {
    // Copilot resets at 00:00 UTC on Dec 1, so Nov 30 is its last day.
    let snaps = [
        copilot(240.0, "2026-11-29T12:00:00Z"),
        copilot(255.0, "2026-11-30T12:00:00Z"),
    ];
    let d = copilot_day(&snaps, date(2026, 11, 30), "2026-11-30T12:00:00Z");
    close(d.usage, 15.0);
    close(d.budget, 60.0);
    close(d.ratio.unwrap(), 0.25);
    assert_eq!(d.colour, Colour::Green);
}

#[test]
fn exhausted_limit_gives_budget_zero_and_red_on_any_usage() {
    let snaps = [
        copilot(300.0, "2026-11-20T12:00:00Z"),
        copilot(302.0, "2026-11-21T12:00:00Z"),
    ];
    let d = copilot_day(&snaps, date(2026, 11, 21), "2026-11-21T12:00:00Z");
    close(d.usage, 2.0);
    close(d.budget, 0.0);
    assert_eq!(d.ratio, None);
    assert_eq!(d.colour, Colour::Red);
}

#[test]
fn exhausted_limit_without_usage_is_green() {
    let snaps = [
        copilot(300.0, "2026-11-20T12:00:00Z"),
        copilot(300.0, "2026-11-21T12:00:00Z"),
    ];
    let d = copilot_day(&snaps, date(2026, 11, 21), "2026-11-21T12:00:00Z");
    close(d.usage, 0.0);
    close(d.budget, 0.0);
    assert_eq!(d.ratio, None);
    assert_eq!(d.colour, Colour::Green);
}

#[test]
fn stale_provider_is_grey() {
    let snaps = [
        week(30.0, WEEK, "2026-11-03T22:00:00Z"),
        week(40.0, WEEK, "2026-11-04T07:00:00Z"),
    ];
    let at = |last_ok: Option<&str>| {
        day(
            &snaps,
            Provider::Claude,
            Window::SevenDay,
            date(2026, 11, 4),
            &berlin(),
            Status {
                now: ts("2026-11-04T08:00:00Z"),
                last_ok_at: last_ok.map(ts),
            },
            &Thresholds::default(),
        )
        .unwrap()
        .colour
    };
    assert_eq!(at(Some("2026-11-04T07:45:00Z")), Colour::Yellow); // exactly 15 min
    assert_eq!(at(Some("2026-11-04T07:44:59Z")), Colour::Grey);
    assert_eq!(at(None), Colour::Grey);
}

#[test]
fn today_is_the_local_day_of_now() {
    let snaps = [
        week(30.0, WEEK, "2026-11-03T22:00:00Z"),
        week(31.0, WEEK, "2026-11-03T23:30:00Z"), // 00:30 local, Nov 4
    ];
    let t = today(
        &snaps,
        Provider::Claude,
        Window::SevenDay,
        &berlin(),
        fresh("2026-11-03T23:30:00Z"),
        &Thresholds::default(),
    )
    .unwrap();
    close(t.usage, 1.0);
    close(t.budget, 11.666_666_666_666_666); // 70 / 6 (Nov 4..=9)
}

#[test]
fn no_snapshots_of_the_series_gives_no_day() {
    let snaps = [copilot(10.0, "2026-11-04T07:00:00Z")];
    let d = day(
        &snaps,
        Provider::Claude,
        Window::SevenDay,
        date(2026, 11, 4),
        &berlin(),
        fresh("2026-11-04T07:00:00Z"),
        &Thresholds::default(),
    );
    assert_eq!(d, None);
}

#[test]
fn snapshots_of_other_series_do_not_count() {
    let mut snaps = vec![
        week(30.0, WEEK, "2026-11-03T22:00:00Z"),
        week(33.0, WEEK, "2026-11-04T07:00:00Z"),
    ];
    snaps.push(snap(
        Window::FiveHour,
        90.0,
        "2026-11-04T10:00:00Z",
        "2026-11-04T07:00:00Z",
        Source::Poll,
    ));
    snaps.push(copilot(200.0, "2026-11-04T07:00:00Z"));
    let d = claude_day(&snaps, date(2026, 11, 4), "2026-11-04T07:00:00Z");
    close(d.usage, 3.0);
}

const FIVE: &str = "2026-11-02T22:39:59.524335Z";

fn five_hour(used: f64, now: &str, last_ok: &str) -> super::Level {
    let snaps = [snap(
        Window::FiveHour,
        used,
        FIVE,
        "2026-11-02T20:55:00Z",
        Source::Poll,
    )];
    level(
        &snaps,
        Provider::Claude,
        Window::FiveHour,
        Status {
            now: ts(now),
            last_ok_at: Some(ts(last_ok)),
        },
        &Thresholds::default(),
    )
    .unwrap()
}

#[test]
fn five_hour_window_is_coloured_by_used_over_limit_without_budget() {
    let now = "2026-11-02T21:05:00Z";
    let l = five_hour(23.0, now, now);
    close(l.used, 23.0);
    close(l.limit, 100.0);
    close(l.ratio.unwrap(), 0.23);
    assert_eq!(l.colour, Colour::Green);
    assert_eq!(five_hour(79.0, now, now).colour, Colour::Green);
    assert_eq!(five_hour(80.0, now, now).colour, Colour::Yellow);
    assert_eq!(five_hour(100.0, now, now).colour, Colour::Yellow);
    assert_eq!(
        five_hour(23.0, now, "2026-11-02T20:49:00Z").colour,
        Colour::Grey
    );
}

#[test]
fn five_hour_window_past_its_reset_is_empty() {
    let now = "2026-11-02T22:41:00Z";
    let l = five_hour(95.0, now, now);
    close(l.used, 0.0);
    assert_eq!(l.colour, Colour::Green);
}

#[test]
fn level_above_the_limit_is_red() {
    let snaps = [copilot(330.0, "2026-11-21T12:00:00Z")];
    let l = level(
        &snaps,
        Provider::Copilot,
        Window::Monthly,
        fresh("2026-11-21T12:00:00Z"),
        &Thresholds::default(),
    )
    .unwrap();
    close(l.ratio.unwrap(), 1.1);
    assert_eq!(l.colour, Colour::Red);
}

#[test]
fn thresholds_split_green_yellow_red() {
    let t = Thresholds::default();
    assert_eq!(t.colour(0.79), Colour::Green);
    assert_eq!(t.colour(0.8), Colour::Yellow);
    assert_eq!(t.colour(1.0), Colour::Yellow);
    assert_eq!(t.colour(1.01), Colour::Red);
    let custom = Thresholds {
        yellow_at: 0.5,
        red_at: 0.9,
        stale_after: SignedDuration::from_mins(5),
    };
    assert_eq!(custom.colour(0.49), Colour::Green);
    assert_eq!(custom.colour(0.5), Colour::Yellow);
    assert_eq!(custom.colour(0.91), Colour::Red);
}

fn curve(snaps: &[Snapshot], d: Date) -> Vec<Point> {
    intraday(snaps, Provider::Claude, Window::SevenDay, d, &berlin())
}

fn usages(points: &[Point]) -> Vec<f64> {
    points.iter().map(|p| p.usage).collect()
}

/// The invariants of every curve: starts at 0, never decreases, ends at the usage of `day`.
fn assert_curve_of_day(snaps: &[Snapshot], d: Date, now: &str) {
    let points = curve(snaps, d);
    close(points[0].usage, 0.0);
    assert!(
        points
            .windows(2)
            .all(|w| w[0].at < w[1].at && w[0].usage <= w[1].usage)
    );
    close(
        points.last().unwrap().usage,
        claude_day(snaps, d, now).usage,
    );
}

#[test]
fn intraday_starts_at_zero_at_local_midnight_and_rises_with_the_snapshots() {
    let snaps = [
        week(25.0, WEEK, "2026-11-03T06:00:00Z"),
        week(27.0, WEEK, "2026-11-03T20:00:00Z"),
        week(30.0, WEEK, "2026-11-04T07:00:00Z"),
        week(33.0, WEEK, "2026-11-04T09:00:00Z"),
    ];
    let points = curve(&snaps, date(2026, 11, 4));
    let at: Vec<Timestamp> = points.iter().map(|p| p.at).collect();
    assert_eq!(
        at,
        [
            ts("2026-11-03T23:00:00Z"),
            ts("2026-11-04T07:00:00Z"),
            ts("2026-11-04T09:00:00Z")
        ]
    );
    assert_eq!(usages(&points), [0.0, 3.0, 6.0]);
    assert_curve_of_day(&snaps, date(2026, 11, 4), "2026-11-04T09:00:00Z");
}

#[test]
fn intraday_lower_value_after_a_higher_one_adds_nothing() {
    let snaps = [
        week(10.0, WEEK, "2026-11-03T20:00:00Z"),
        week(14.0, WEEK, "2026-11-04T07:00:00Z"),
        week(12.0, WEEK, "2026-11-04T08:00:00Z"),
        week(15.0, WEEK, "2026-11-04T09:00:00Z"),
    ];
    assert_eq!(
        usages(&curve(&snaps, date(2026, 11, 4))),
        [0.0, 4.0, 4.0, 5.0]
    );
    assert_curve_of_day(&snaps, date(2026, 11, 4), "2026-11-04T09:00:00Z");
}

#[test]
fn intraday_reset_during_the_day_continues_the_sum() {
    let snaps = reset_day_snapshots();
    // 93 (baseline) -> 95 (+2) -> 95 -> new window 0 (+0) -> 1.5 (+1.5)
    assert_eq!(
        usages(&curve(&snaps, date(2026, 11, 9))),
        [0.0, 2.0, 2.0, 2.0, 3.5]
    );
    assert_curve_of_day(&snaps, date(2026, 11, 9), "2026-11-09T22:30:00Z");
}

#[test]
fn intraday_equal_instants_make_one_point_and_later_days_do_not_count() {
    let snaps = [
        week(10.0, WEEK, "2026-11-03T20:00:00Z"),
        week(12.0, WEEK, "2026-11-04T07:00:00Z"),
        week(13.0, WEEK, "2026-11-04T07:00:00Z"),
        week(40.0, WEEK, "2026-11-04T23:30:00Z"), // Nov 5, 00:30 local
    ];
    assert_eq!(usages(&curve(&snaps, date(2026, 11, 4))), [0.0, 3.0]);
}

#[test]
fn intraday_without_a_snapshot_of_the_day_is_empty() {
    assert!(curve(&[], date(2026, 11, 4)).is_empty());
    let snaps = [week(10.0, WEEK, "2026-11-03T20:00:00Z")];
    assert!(curve(&snaps, date(2026, 11, 4)).is_empty());
}

#[test]
fn intraday_ignores_other_series() {
    let snaps = [
        week(10.0, WEEK, "2026-11-03T20:00:00Z"),
        week(12.0, WEEK, "2026-11-04T07:00:00Z"),
        copilot(250.0, "2026-11-04T08:00:00Z"),
        snap(
            Window::FiveHour,
            90.0,
            WEEK,
            "2026-11-04T09:00:00Z",
            Source::Poll,
        ),
    ];
    assert_eq!(usages(&curve(&snaps, date(2026, 11, 4))), [0.0, 2.0]);
}

#[test]
fn intraday_on_the_23_hour_dst_start_day_runs_from_local_midnight() {
    let old = "2027-03-28T20:00:00Z";
    let new = "2027-04-04T20:00:00Z";
    let snaps = [
        week(90.0, old, "2027-03-27T22:30:00Z"),
        week(91.0, old, "2027-03-27T23:30:00Z"), // 00:30 CET, Mar 28
        week(92.0, old, "2027-03-28T19:00:00Z"),
        week(2.0, new, "2027-03-28T21:00:00Z"), // 23:00 CEST
    ];
    let points = curve(&snaps, date(2027, 3, 28));
    assert_eq!(points[0].at, ts("2027-03-27T23:00:00Z")); // 00:00 CET
    assert_eq!(points.last().unwrap().at, ts("2027-03-28T21:00:00Z"));
    assert_curve_of_day(&snaps, date(2027, 3, 28), "2027-03-28T21:00:00Z");
}

#[test]
fn intraday_on_the_25_hour_dst_end_day_starts_at_local_midnight() {
    let old = "2026-10-25T21:00:00Z";
    let new = "2026-11-01T21:00:00Z";
    let snaps = [
        week(80.0, old, "2026-10-24T21:30:00Z"),
        week(84.0, old, "2026-10-25T10:00:00Z"),
        week(3.0, new, "2026-10-25T22:30:00Z"),
    ];
    let points = curve(&snaps, date(2026, 10, 25));
    assert_eq!(points[0].at, ts("2026-10-24T22:00:00Z")); // 00:00 CEST
    assert_eq!(usages(&points), [0.0, 4.0, 7.0]);
    assert_curve_of_day(&snaps, date(2026, 10, 25), "2026-10-25T22:30:00Z");
}

#[test]
fn intraday_across_the_year_boundary_belongs_to_the_local_day() {
    let resets = "2027-01-05T21:00:00Z";
    let snaps = [
        week(50.0, resets, "2026-12-31T22:30:00Z"), // 23:30 CET, Dec 31
        week(52.0, resets, "2026-12-31T23:30:00Z"), // 00:30 CET, Jan 1
        week(55.0, resets, "2027-01-01T10:00:00Z"),
    ];
    let points = curve(&snaps, date(2027, 1, 1));
    assert_eq!(points[0].at, ts("2026-12-31T23:00:00Z"));
    assert_eq!(usages(&points), [0.0, 2.0, 5.0]);
    assert_curve_of_day(&snaps, date(2027, 1, 1), "2027-01-01T10:00:00Z");
}
