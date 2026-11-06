//! The `day` command: the intraday curve of one local day of every provider against the day's
//! budget, plus each provider's `status` / `stale_since` as in [`today`].
//!
//! - A provider that is `unavailable` has no points and no budget.
//! - Snapshots after `now` do not count, as in [`crate::month`]; a day without data (or in the
//!   future) has no points, no usage and no budget.
//! - Past days are judged as if fresh; a stale provider greys only today.
//! - `five_hour_points` is the fill of the five-hour window over the day (Claude only).
//! - `five_hour` is the current level of Claude's five-hour window at `now`, extra information
//!   whatever day is asked for.

use std::path::Path;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::Serialize;
use tamias_core::budget::{self, Colour, FillPoint, Point};
use tamias_core::config::{self, Config};
use tamias_core::paths;
use tamias_core::report::{self, Status};
use tamias_core::snapshot::{Provider, Snapshot, Unit, Window};
use tamias_core::store::{self, Store};

use crate::today;

/// Why the day could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The requested date is not a `YYYY-MM-DD` day.
    #[error("not a date (YYYY-MM-DD): {0}")]
    InvalidDate(String),
    /// Everything [`today`] can fail with.
    #[error(transparent)]
    Read(#[from] today::Error),
}

impl Error {
    /// Whether the store was locked longer than its busy timeout; the next refresh retries.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Read(error) if error.is_busy())
    }
}

impl From<store::Error> for Error {
    fn from(error: store::Error) -> Self {
        Self::Read(error.into())
    }
}

/// Current level of the five-hour window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FiveHour {
    /// High-water mark of the current window; 0 once its reset has passed.
    pub used: f64,
    /// Limit of the window, in percent.
    pub limit: f64,
    /// Colour of the level.
    pub colour: Colour,
}

/// One provider's day.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderDay {
    /// Which provider.
    pub provider: Provider,
    /// Health of the provider.
    pub status: Status,
    /// Last successful collect of a stale provider; absent when it never succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<Timestamp>,
    /// Unit of `points`, `usage` and `budget`.
    pub unit: Unit,
    /// The running usage of the day, from 0 at local midnight; empty without data.
    pub points: Vec<Point>,
    /// Usage of the whole day; absent without data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<f64>,
    /// Budget of the day; absent without data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    /// Colour of the day; absent without data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<Colour>,
    /// Current level of Claude's five-hour window; absent for Copilot and without data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<FiveHour>,
    /// Fill of Claude's five-hour window over the day, one point per snapshot, oldest first;
    /// absent for Copilot and without data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_hour_points: Option<Vec<FillPoint>>,
}

/// The day of every provider, in [`today::PROVIDERS`] order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayReport {
    /// The local day as requested.
    pub date: Date,
    /// When the report was generated.
    pub generated_at: Timestamp,
    /// Every provider.
    pub providers: Vec<ProviderDay>,
}

/// The day `date` now, in the system time zone, from the default database and `config.toml`.
///
/// # Errors
///
/// [`Error`].
pub fn current(date: &str) -> Result<DayReport, Error> {
    let config = config::load(None).map_err(today::Error::from)?;
    let database = paths::database_path().map_err(today::Error::from)?;
    read(
        &database,
        &config,
        date,
        Timestamp::now(),
        &TimeZone::system(),
    )
}

/// The local day `date` (`YYYY-MM-DD`, in `tz`) of every provider at `now`, from the database
/// file `database`, opened read-only. A missing database leaves every provider `unavailable`.
///
/// # Errors
///
/// [`Error::InvalidDate`], or store errors of an existing database.
pub fn read(
    database: &Path,
    config: &Config,
    date: &str,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<DayReport, Error> {
    let date: Date = Some(date)
        .filter(|d| d.len() == 10)
        .and_then(|d| d.parse().ok())
        .ok_or_else(|| Error::InvalidDate(date.to_owned()))?;
    let overview = today::read(database, config, now, tz)?;
    let store = if database.is_file() {
        Some(Store::open_read_only(database)?)
    } else {
        None
    };
    let is_today = now.to_zoned(tz.clone()).date() == date;
    let mut providers = Vec::new();
    for state in &overview.providers {
        let window = report::windows(state.provider)[0];
        let mut entry = ProviderDay {
            provider: state.provider,
            status: state.status,
            stale_since: state.stale_since,
            unit: report::unit(window),
            points: Vec::new(),
            usage: None,
            budget: None,
            colour: None,
            five_hour_points: None,
            five_hour: state
                .windows
                .iter()
                .find(|w| w.window == Window::FiveHour)
                .map(|w| FiveHour {
                    used: w.used,
                    limit: w.limit,
                    colour: w.colour,
                }),
        };
        if let (Some(store), Status::Ok | Status::Stale) = (&store, state.status) {
            let snapshots = snapshots(store, state.provider, window, date, now, tz)?;
            entry.points = budget::intraday(&snapshots, state.provider, window, date, tz);
            if report::windows(state.provider).contains(&Window::FiveHour) {
                let five = snapshots(store, state.provider, Window::FiveHour, date, now, tz)?;
                entry.five_hour_points = Some(budget::window_curve(
                    &five,
                    state.provider,
                    Window::FiveHour,
                    date,
                    tz,
                    now,
                ));
            }
            // Only today can be stale; a past day is not a property of the provider's health.
            let status = budget::Status {
                now,
                last_ok_at: (!(is_today && state.status == Status::Stale)).then_some(now),
            };
            if let Some(day) = budget::day(
                &snapshots,
                state.provider,
                window,
                date,
                tz,
                status,
                &config.thresholds,
            ) {
                entry.usage = Some(day.usage);
                entry.budget = Some(day.budget);
                entry.colour = Some(day.colour);
            }
        }
        providers.push(entry);
    }
    Ok(DayReport {
        date,
        generated_at: now,
        providers,
    })
}

/// Snapshots needed for `date`: from the lookback before its start to its end or `now`,
/// whichever is first, so nothing observed after `now` takes part.
fn snapshots(
    store: &Store,
    provider: Provider,
    window: Window,
    date: Date,
    now: Timestamp,
    tz: &TimeZone,
) -> store::Result<Vec<Snapshot>> {
    let (Ok(start), Ok(end)) = (
        date.to_zoned(tz.clone()),
        date.tomorrow().and_then(|d| d.to_zoned(tz.clone())),
    ) else {
        return Ok(Vec::new());
    };
    let start = start.timestamp();
    let from = start.checked_sub(report::lookback(window)).unwrap_or(start);
    let to = end.timestamp().min(
        now.checked_add(SignedDuration::from_nanos(1))
            .unwrap_or(now),
    );
    store.snapshots(provider, window, from, to)
}
