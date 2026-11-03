//! State of all providers as one versioned document: the model behind `tamias today` and
//! `tamias today --json`.
//!
//! The JSON form is described by `docs/schema/today-v1.schema.json`. Within schema version 1,
//! fields are only ever added as optional; a rename or removal bumps [`SCHEMA_VERSION`].
//!
//! - A window without any snapshot in the store is left out.
//! - `today_usage`, `budget` and `ratio` exist only for binding windows (Claude `seven_day`,
//!   Copilot `monthly`) once a day can be derived ([`budget::today`]).
//! - A stale provider shows every window `grey`.

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::budget::{self, Colour, Thresholds};
use crate::snapshot::{self, Snapshot, Unit};
use crate::store::{self, SourceStatus, Store};

/// Version of the document format.
pub const SCHEMA_VERSION: u32 = 1;

/// State of all providers at `generated_at`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Always [`SCHEMA_VERSION`] when written by this build.
    pub schema_version: u32,
    pub generated_at: Timestamp,
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
pub struct Provider {
    pub provider: snapshot::Provider,
    pub status: Status,
    /// Last successful collect of a `stale` provider; absent when it never succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<Timestamp>,
    pub windows: Vec<Window>,
}

/// State of one usage window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub window: snapshot::Window,
    /// Whether the window has a daily budget.
    pub binding: bool,
    pub unit: Unit,
    /// High-water mark of the current window; 0 once its reset has passed.
    pub used: f64,
    pub limit: f64,
    /// End of the most recently observed window (may be past when no newer one was seen).
    pub resets_at: Timestamp,
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

/// Snapshots of every window of `provider` needed at `now` (from [`lookback`] before the start
/// of the local day in `tz`), oldest first per window.
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
        .start_of_day()
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

#[cfg(test)]
mod tests;
