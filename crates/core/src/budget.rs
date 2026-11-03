//! Daily usage and dynamic daily budget, derived from a series of snapshots.
//!
//! Pure functions: the snapshots, the time zone, the current instant and the provider status are
//! all arguments. Call `TimeZone::system()` at the call site, never here.
//!
//! - A window is identified by `(provider, window, resets_at)` with `resets_at` rounded to the
//!   nearest minute, because the source jitters by about a second.
//! - Within a window only the running maximum of `used` counts (high-water mark), whatever the
//!   source; a lower value after a higher one is never usage.
//! - The daily budget is `(limit - used) / remaining_days`, taken at the start of the local day.
//!   On a reset day the new window's daily budget is added, prorated by the share of the local day
//!   after the reset.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, ToSpan};

use serde::{Deserialize, Serialize};

use crate::snapshot::{Provider, Snapshot, Window};

/// Traffic-light colour of a day or a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
    Green,
    Yellow,
    Red,
    /// The provider status is stale; the numbers may be outdated.
    Grey,
}

/// Colour thresholds and staleness limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Ratios from this value on are yellow (default 0.8).
    pub yellow_at: f64,
    /// Ratios above this value are red (default 1.0).
    pub red_at: f64,
    /// A provider whose last successful read is older than this is stale (default 15 min).
    pub stale_after: SignedDuration,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            yellow_at: 0.8,
            red_at: 1.0,
            stale_after: SignedDuration::from_mins(15),
        }
    }
}

impl Thresholds {
    /// Colour of a ratio: green below `yellow_at`, yellow up to and including `red_at`, red above.
    #[must_use]
    pub fn colour(&self, ratio: f64) -> Colour {
        if ratio < self.yellow_at {
            Colour::Green
        } else if ratio <= self.red_at {
            Colour::Yellow
        } else {
            Colour::Red
        }
    }

    fn is_stale(&self, status: Status) -> bool {
        status
            .last_ok_at
            .is_none_or(|ok| status.now.duration_since(ok) > self.stale_after)
    }

    /// Colour of `amount` against `allowance`. Without an allowance (0) any amount is red and
    /// nothing at all is green.
    fn judge(&self, amount: f64, ratio: Option<f64>, status: Status) -> Colour {
        if self.is_stale(status) {
            return Colour::Grey;
        }
        match ratio {
            Some(r) => self.colour(r),
            None if amount > 0.0 => Colour::Red,
            None => Colour::Green,
        }
    }
}

/// Current instant and provider status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub now: Timestamp,
    /// Last successful read of the provider, `None` if it never succeeded.
    pub last_ok_at: Option<Timestamp>,
}

/// Usage and budget of one local calendar day for a binding window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    /// Rise of the high-water marks within the day, in the window's unit.
    pub usage: f64,
    /// Budget of the day, in the window's unit.
    pub budget: f64,
    /// `usage / budget`; `None` when the budget is 0.
    pub ratio: Option<f64>,
    pub colour: Colour,
}

/// Current fill of an informational window (Claude five-hour), without a budget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    /// High-water mark of the current window; 0 once its `resets_at` has passed.
    pub used: f64,
    pub limit: f64,
    /// `used / limit`; `None` when the limit is 0.
    pub ratio: Option<f64>,
    pub colour: Colour,
}

/// [`Day`] of the local day of `status.now` in `tz`.
#[must_use]
pub fn today(
    snapshots: &[Snapshot],
    provider: Provider,
    window: Window,
    tz: &TimeZone,
    status: Status,
    thresholds: &Thresholds,
) -> Option<Day> {
    let date = status.now.to_zoned(tz.clone()).date();
    day(snapshots, provider, window, date, tz, status, thresholds)
}

/// Usage, budget and colour of the local calendar day `date` in `tz` for a binding window
/// (Claude `seven_day`, Copilot `monthly`).
///
/// Returns `None` when no window of the series is open at the start of the day and observed
/// before its end, or when the dates are out of range.
#[must_use]
pub fn day(
    snapshots: &[Snapshot],
    provider: Provider,
    window: Window,
    date: Date,
    tz: &TimeZone,
    status: Status,
    thresholds: &Thresholds,
) -> Option<Day> {
    let start = date.to_zoned(tz.clone()).ok()?.timestamp();
    let end = date.tomorrow().ok()?.to_zoned(tz.clone()).ok()?.timestamp();
    let wins = series(snapshots, provider, window);

    let usage = wins
        .iter()
        .filter(|w| w.touched(start, end))
        .map(|w| (w.max_before(end).unwrap_or(0.0) - baseline(&wins, w, start)).max(0.0))
        .sum();

    let old = wins
        .iter()
        .find(|w| w.key > start && w.first().observed_at < end)?;
    let used = baseline(&wins, old, start);
    let mut budget = (old.limit() - used).max(0.0) / remaining_days(date, old.key, tz)?;
    if old.key < end {
        let (next_key, next_limit) = match wins.iter().find(|w| w.key > old.key) {
            Some(next) => (next.key, next.limit()),
            None => (next_reset(window, old.key)?, old.limit()),
        };
        let share =
            end.duration_since(old.key).as_secs_f64() / end.duration_since(start).as_secs_f64();
        budget += next_limit / remaining_days(date, next_key, tz)? * share;
    }

    let ratio = (budget > 0.0).then(|| usage / budget);
    Some(Day {
        usage,
        budget,
        ratio,
        colour: thresholds.judge(usage, ratio, status),
    })
}

/// Current fill and colour of an informational window (Claude `five_hour`): `used / limit`
/// against the thresholds, no budget. `None` without snapshots.
#[must_use]
pub fn level(
    snapshots: &[Snapshot],
    provider: Provider,
    window: Window,
    status: Status,
    thresholds: &Thresholds,
) -> Option<Level> {
    let wins = series(snapshots, provider, window);
    let current = wins.last()?;
    let used = if current.key <= status.now {
        0.0
    } else {
        current.high()
    };
    let limit = current.limit();
    let ratio = (limit > 0.0).then(|| used / limit);
    Some(Level {
        used,
        limit,
        ratio,
        colour: thresholds.judge(used, ratio, status),
    })
}

/// Snapshots of one window, sorted by `observed_at` (never empty).
struct Win<'a> {
    key: Timestamp,
    snaps: Vec<&'a Snapshot>,
}

impl Win<'_> {
    fn first(&self) -> &Snapshot {
        self.snaps[0]
    }

    fn limit(&self) -> f64 {
        self.snaps[self.snaps.len() - 1].limit
    }

    fn high(&self) -> f64 {
        self.snaps.iter().map(|s| s.used).fold(0.0, f64::max)
    }

    fn max_before(&self, t: Timestamp) -> Option<f64> {
        self.snaps
            .iter()
            .filter(|s| s.observed_at < t)
            .map(|s| s.used)
            .reduce(f64::max)
    }

    fn touched(&self, start: Timestamp, end: Timestamp) -> bool {
        self.snaps
            .iter()
            .any(|s| start <= s.observed_at && s.observed_at < end)
    }
}

/// Windows of one provider and window kind, sorted by rounded `resets_at`.
fn series(snapshots: &[Snapshot], provider: Provider, window: Window) -> Vec<Win<'_>> {
    let mut snaps: Vec<&Snapshot> = snapshots
        .iter()
        .filter(|s| s.provider == provider && s.window == window)
        .collect();
    snaps.sort_by_key(|s| s.observed_at);
    let mut wins: Vec<Win<'_>> = Vec::new();
    for s in snaps {
        let key = window_key(s.resets_at);
        match wins.iter_mut().find(|w| w.key == key) {
            Some(w) => w.snaps.push(s),
            None => wins.push(Win {
                key,
                snaps: vec![s],
            }),
        }
    }
    wins.sort_by_key(|w| w.key);
    wins
}

/// `resets_at` rounded to the nearest minute (half away from zero).
fn window_key(resets_at: Timestamp) -> Timestamp {
    resets_at.round(jiff::Unit::Minute).unwrap_or(resets_at)
}

/// Level of `win` at `start`: its high-water mark before `start`; else 0 when an earlier window
/// was observed before it (it started while we were watching); else its first value (start of
/// the data, the level before is unknown).
fn baseline(wins: &[Win<'_>], win: &Win<'_>, start: Timestamp) -> f64 {
    if let Some(high) = win.max_before(start) {
        return high;
    }
    let first = win.first();
    let watched = wins
        .iter()
        .any(|w| w.key < win.key && w.first().observed_at < first.observed_at);
    if watched { 0.0 } else { first.used }
}

/// Local calendar days from `from` up to and including the last day of the window ending at
/// `resets_at`, at least 1. A window ending exactly at local midnight does not count that day.
fn remaining_days(from: Date, resets_at: Timestamp, tz: &TimeZone) -> Option<f64> {
    let last = resets_at
        .checked_sub(SignedDuration::from_nanos(1))
        .ok()?
        .to_zoned(tz.clone())
        .date();
    let days = from.until(last).ok()?.get_days() + 1;
    Some(f64::from(days.max(1)))
}

/// Expected `resets_at` of the window after the one ending at `key`.
fn next_reset(window: Window, key: Timestamp) -> Option<Timestamp> {
    match window {
        Window::FiveHour => key.checked_add(SignedDuration::from_hours(5)).ok(),
        Window::SevenDay => key.checked_add(SignedDuration::from_hours(7 * 24)).ok(),
        Window::Monthly => Some(
            key.to_zoned(TimeZone::UTC)
                .checked_add(1.month())
                .ok()?
                .timestamp(),
        ),
    }
}

#[cfg(test)]
mod tests;
