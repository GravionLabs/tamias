//! State of all providers as one versioned document: the model behind `tamias today` and
//! `tamias today --json`.
//!
//! The JSON form is described by `docs/schema/today-v1.schema.json`. Within schema version 1,
//! fields are only ever added as optional; a rename or removal bumps [`SCHEMA_VERSION`].
//!
//! - A window without any snapshot in the store is left out.
//! - `today_usage`, `budget`, `ratio` and `month_usage` exist only for binding windows (Claude `seven_day`,
//!   Copilot `monthly`) once a day can be derived ([`budget::today`]).
//! - A stale provider shows every window `grey`.
//!
//! [`short`] renders a report as the compact line of `tamias today --short` and the tray.

pub mod short;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::budget::{self, Colour, Thresholds};
use crate::snapshot::{self, Snapshot, Unit};
use crate::store::{self, SourceStatus, Store};

/// Version of the document format.
pub const SCHEMA_VERSION: u32 = 1;

/// State of all providers at `generated_at`.
///
/// Read the output of `tamias today --json` with [`Report::parse`], which rejects any
/// `schema_version` other than [`SCHEMA_VERSION`]; plain `serde_json::from_str` does not check it.
///
/// The struct is `#[non_exhaustive]`: fields added within schema version 1 do not break
/// consumers. Build one with [`Report::new`].
///
/// `Provider` and `Window` of this module share their names with [`snapshot::Provider`] and
/// [`snapshot::Window`] (which the fields `provider` and `window` use); import one of the pairs
/// with an alias, for example `use tamias_core::report::Provider as ProviderReport;`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Report {
    /// Always [`SCHEMA_VERSION`] when written by this build.
    pub schema_version: u32,
    /// When the report was generated.
    pub generated_at: Timestamp,
    /// Every provider, the set-up ones and the `unavailable` ones.
    pub providers: Vec<Provider>,
}

impl Report {
    /// Report of [`SCHEMA_VERSION`].
    #[must_use]
    pub fn new(generated_at: Timestamp, providers: Vec<Provider>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generated_at,
            providers,
        }
    }

    /// Reads the JSON of `tamias today --json`, checking `schema_version` first.
    ///
    /// # Errors
    ///
    /// [`ParseError::Json`] when `json` is not a report document, [`ParseError::UnsupportedVersion`]
    /// when its `schema_version` is not [`SCHEMA_VERSION`] (the rest of such a document is not read).
    pub fn parse(json: &str) -> Result<Self, ParseError> {
        #[derive(Deserialize)]
        struct Version {
            schema_version: u32,
        }
        let Version { schema_version } = serde_json::from_str(json)?;
        if schema_version != SCHEMA_VERSION {
            return Err(ParseError::UnsupportedVersion {
                found: schema_version,
            });
        }
        Ok(serde_json::from_str(json)?)
    }
}

/// Why [`Report::parse`] failed.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Not valid JSON, or not a report document.
    #[error("not a tamias report: {0}")]
    Json(#[from] serde_json::Error),
    /// A `schema_version` this build does not read.
    #[error("unsupported schema_version {found}, expected {SCHEMA_VERSION}")]
    UnsupportedVersion {
        /// The `schema_version` of the document.
        found: u32,
    },
}

/// Health of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Collected recently and the last run succeeded.
    Ok,
    /// The last run failed, or the last success is older than `stale_after` (or there was none).
    Stale,
    /// Disabled or not set up on this machine.
    Unavailable,
}

/// State of one provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Provider {
    /// Which provider.
    pub provider: snapshot::Provider,
    /// Health of the provider.
    pub status: Status,
    /// Last successful collect of a `stale` provider; absent when it never succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<Timestamp>,
    /// Windows with data, the binding one first.
    pub windows: Vec<Window>,
}

/// State of one usage window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Window {
    /// Which window.
    pub window: snapshot::Window,
    /// Whether the window has a daily budget.
    pub binding: bool,
    /// Unit of `used`, `limit`, `today_usage` and `budget`.
    pub unit: Unit,
    /// High-water mark of the current window; 0 once its reset has passed.
    pub used: f64,
    /// Limit of the window, in its unit.
    pub limit: f64,
    /// End of the most recently observed window (may be past when no newer one was seen).
    pub resets_at: Timestamp,
    /// Colour of the window: relative to the daily budget for binding windows, `grey` when stale.
    pub colour: Colour,
    /// Usage of the local day (binding windows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub today_usage: Option<f64>,
    /// Budget of the local day (binding windows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    /// `today_usage / budget` (binding windows, absent when the budget is 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    /// Usage of the local month up to and including today (binding windows): the sum of the
    /// daily usage of [`budget::day`], days without snapshots counting 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month_usage: Option<f64>,
}

/// Why a provider is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    /// Last successful collect, `None` if there was none.
    pub since: Option<Timestamp>,
    /// Error of the last run when it failed after the last success and recorded a message.
    pub error: Option<String>,
}

/// Windows of `provider`, the binding one first.
#[must_use]
pub fn windows(provider: snapshot::Provider) -> &'static [snapshot::Window] {
    match provider {
        snapshot::Provider::Claude => &[snapshot::Window::SevenDay, snapshot::Window::FiveHour],
        snapshot::Provider::Copilot => &[snapshot::Window::Monthly],
    }
}

/// Whether `window` has a daily budget.
#[must_use]
pub fn is_binding(window: snapshot::Window) -> bool {
    match window {
        snapshot::Window::SevenDay | snapshot::Window::Monthly => true,
        snapshot::Window::FiveHour => false,
    }
}

/// Unit of `window`.
#[must_use]
pub fn unit(window: snapshot::Window) -> Unit {
    match window {
        snapshot::Window::FiveHour | snapshot::Window::SevenDay => Unit::Percent,
        snapshot::Window::Monthly => Unit::Requests,
    }
}

/// How far before the start of today snapshots of `window` are needed: the window length plus
/// a margin for a late reset.
#[must_use]
pub fn lookback(window: snapshot::Window) -> SignedDuration {
    match window {
        snapshot::Window::FiveHour | snapshot::Window::SevenDay => {
            SignedDuration::from_hours(8 * 24)
        }
        snapshot::Window::Monthly => SignedDuration::from_hours(32 * 24),
    }
}

/// `Some` when the provider is stale: its last run failed after the last success, or its last
/// success is older than `stale_after` (or there was none).
#[must_use]
pub fn stale(
    status: Option<&SourceStatus>,
    now: Timestamp,
    thresholds: &Thresholds,
) -> Option<Stale> {
    let since = status.and_then(|s| s.last_ok_at);
    let failed = status.and_then(|s| {
        let at = s.last_error_at?;
        since
            .is_none_or(|ok| at > ok)
            .then(|| s.last_error.clone().unwrap_or_default())
    });
    let old = since.is_none_or(|ok| now.duration_since(ok) > thresholds.stale_after);
    (failed.is_some() || old).then_some(Stale {
        since,
        error: failed.filter(|e| !e.is_empty()),
    })
}

/// An `unavailable` provider.
#[must_use]
pub fn unavailable(provider: snapshot::Provider) -> Provider {
    Provider {
        provider,
        status: Status::Unavailable,
        stale_since: None,
        windows: Vec::new(),
    }
}

/// State of `provider` from its `snapshots` and `status` at `now`, the day taken in `tz`.
#[must_use]
pub fn provider(
    provider: snapshot::Provider,
    snapshots: &[Snapshot],
    status: Option<&SourceStatus>,
    now: Timestamp,
    tz: &TimeZone,
    thresholds: &Thresholds,
) -> Provider {
    let budget_status = budget::Status {
        now,
        last_ok_at: status.and_then(|s| s.last_ok_at),
    };
    let stale = stale(status, now, thresholds);
    let windows = windows(provider)
        .iter()
        .filter_map(|&window| {
            let level = budget::level(snapshots, provider, window, budget_status, thresholds)?;
            let latest = snapshots
                .iter()
                .filter(|s| s.provider == provider && s.window == window)
                .max_by_key(|s| s.observed_at)?;
            let day = is_binding(window)
                .then(|| budget::today(snapshots, provider, window, tz, budget_status, thresholds))
                .flatten();
            let colour = if stale.is_some() {
                Colour::Grey
            } else {
                day.map_or(level.colour, |d| d.colour)
            };
            Some(Window {
                window,
                binding: is_binding(window),
                unit: latest.unit,
                used: level.used,
                limit: level.limit,
                resets_at: latest.resets_at,
                colour,
                today_usage: day.map(|d| d.usage),
                budget: day.map(|d| d.budget),
                ratio: day.and_then(|d| d.ratio),
                month_usage: day.map(|_| {
                    month_usage(snapshots, provider, window, tz, budget_status, thresholds)
                }),
            })
        })
        .collect();
    Provider {
        provider,
        status: if stale.is_some() {
            Status::Stale
        } else {
            Status::Ok
        },
        stale_since: stale.and_then(|s| s.since),
        windows,
    }
}

/// Sum of [`budget::day`] usage over the local days of the month of `status.now`, up to and
/// including that day.
fn month_usage(
    snapshots: &[Snapshot],
    provider: snapshot::Provider,
    window: snapshot::Window,
    tz: &TimeZone,
    status: budget::Status,
    thresholds: &Thresholds,
) -> f64 {
    let today = status.now.to_zoned(tz.clone()).date();
    let mut total = 0.0;
    let mut date = today.first_of_month();
    while date <= today {
        if let Some(d) = budget::day(snapshots, provider, window, date, tz, status, thresholds) {
            total += d.usage;
        }
        let Ok(next) = date.tomorrow() else { break };
        date = next;
    }
    total
}

/// Snapshots of every window of `provider` needed at `now` (from [`lookback`] before the start
/// of the local month in `tz`), oldest first per window.
///
/// # Errors
///
/// Store errors.
pub fn snapshots(
    store: &Store,
    provider: snapshot::Provider,
    now: Timestamp,
    tz: &TimeZone,
) -> store::Result<Vec<Snapshot>> {
    let start = now
        .to_zoned(tz.clone())
        .date()
        .first_of_month()
        .to_zoned(tz.clone())
        .map_or(now, |z| z.timestamp());
    let to = now
        .checked_add(SignedDuration::from_hours(48))
        .unwrap_or(now);
    let mut snapshots = Vec::new();
    for &window in windows(provider) {
        let from = start.checked_sub(lookback(window)).unwrap_or(start);
        snapshots.extend(store.snapshots(provider, window, from, to)?);
    }
    Ok(snapshots)
}

/// State of `provider` read from `store`.
///
/// # Errors
///
/// Store errors.
pub fn read(
    store: &Store,
    provider: snapshot::Provider,
    now: Timestamp,
    tz: &TimeZone,
    thresholds: &Thresholds,
) -> store::Result<Provider> {
    let snaps = snapshots(store, provider, now, tz)?;
    let status = store.status(provider)?;
    Ok(self::provider(
        provider,
        &snaps,
        status.as_ref(),
        now,
        tz,
        thresholds,
    ))
}

/// Usage against the budget of one local day, one entry of [`month`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DayEntry {
    /// The local calendar day.
    pub date: Date,
    /// Usage of the day, in the unit of the binding window.
    pub usage: f64,
    /// Budget of the day, in the same unit.
    pub budget: f64,
    /// `usage / budget`; absent when the budget is 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    /// Colour of the day against `thresholds`. Never `grey`: the staleness of the provider is
    /// not a property of a past day (see [`stale`]).
    pub colour: Colour,
}

/// The days of `month` of `year` (local, in `tz`) up to the local day of `now`, each with the
/// usage and budget of the binding window of `provider` (`seven_day`, `monthly`).
///
/// Reads the snapshots from [`lookback`] before the first day to the end of the month or `now`, whichever is first, once and
/// derives every day with [`budget::day`], so a day has the same values as that call alone.
/// Days without a result (no window observed) and days after `now` are absent; an invalid
/// `year`/`month` gives no days.
///
/// # Errors
///
/// Store errors.
pub fn month(
    store: &Store,
    provider: snapshot::Provider,
    year: i16,
    month: i8,
    tz: &TimeZone,
    thresholds: &Thresholds,
    now: Timestamp,
) -> store::Result<Vec<DayEntry>> {
    let window = windows(provider)[0];
    let Ok(first) = Date::new(year, month, 1) else {
        return Ok(Vec::new());
    };
    let today = now.to_zoned(tz.clone()).date();
    let last = first.last_of_month().min(today);
    let (Ok(from), Ok(to)) = (
        first.to_zoned(tz.clone()),
        first
            .last_of_month()
            .tomorrow()
            .and_then(|d| d.to_zoned(tz.clone())),
    ) else {
        return Ok(Vec::new());
    };
    let from = from
        .timestamp()
        .checked_sub(lookback(window))
        .unwrap_or_else(|_| from.timestamp());
    // Nothing observed after `now` takes part, so today is judged as `budget::day` alone would.
    let to = to.timestamp().min(
        now.checked_add(SignedDuration::from_nanos(1))
            .unwrap_or(now),
    );
    let snapshots = store.snapshots(provider, window, from, to)?;
    // Past days are judged as if fresh: a stale provider greys only today, in the caller.
    let status = budget::Status {
        now,
        last_ok_at: Some(now),
    };
    let mut days = Vec::new();
    let mut date = first;
    while date <= last {
        if let Some(d) = budget::day(&snapshots, provider, window, date, tz, status, thresholds) {
            days.push(DayEntry {
                date,
                usage: d.usage,
                budget: d.budget,
                ratio: d.ratio,
                colour: d.colour,
            });
        }
        let Ok(next) = date.tomorrow() else { break };
        date = next;
    }
    Ok(days)
}

#[cfg(test)]
mod tests;
