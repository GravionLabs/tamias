//! The `month` command: the days of one month of every provider with their usage against the
//! budget, plus each provider's `status` / `stale_since` as in [`today`].
//!
//! - A provider that is `unavailable` (switched off, not set up, Copilot until it has a
//!   collector) has no days.
//! - Days are the local days of the system time zone up to today; a day without data is absent.

use std::path::Path;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;
use tamias_core::config::{self, Config};
use tamias_core::paths;
use tamias_core::report::{self, DayEntry, Status};
use tamias_core::snapshot::{Provider, Unit};
use tamias_core::store::{self, Store};

use crate::today::{self, Error};

/// One provider's month.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderMonth {
    /// Which provider.
    pub provider: Provider,
    /// Health of the provider.
    pub status: Status,
    /// Last successful collect of a stale provider; absent when it never succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<Timestamp>,
    /// Unit of the days' `usage` and `budget`.
    pub unit: Unit,
    /// Days with data, oldest first.
    pub days: Vec<DayEntry>,
}

/// The month of every provider, in [`today::PROVIDERS`] order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthReport {
    /// Year as requested.
    pub year: i16,
    /// Month 1-12 as requested.
    pub month: i8,
    /// When the report was generated.
    pub generated_at: Timestamp,
    /// Every provider.
    pub providers: Vec<ProviderMonth>,
}

/// The month now, in the system time zone, from the default database and `config.toml`.
///
/// # Errors
///
/// [`Error`].
pub fn current(year: i16, month: i8) -> Result<MonthReport, Error> {
    let config = config::load(None)?;
    let database = paths::database_path()?;
    Ok(read(
        &database,
        &config,
        year,
        month,
        Timestamp::now(),
        &TimeZone::system(),
    )?)
}

/// The month of every provider at `now`, the days taken in `tz`, from the database file
/// `database`, opened read-only. A missing database leaves every provider `unavailable`.
///
/// # Errors
///
/// Store errors of an existing database.
pub fn read(
    database: &Path,
    config: &Config,
    year: i16,
    month: i8,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<MonthReport, store::Error> {
    let overview = today::read(database, config, now, tz)?;
    let store = if database.is_file() {
        Some(Store::open_read_only(database)?)
    } else {
        None
    };
    let mut providers = Vec::new();
    for state in &overview.providers {
        let days = match (&store, state.status) {
            (Some(store), Status::Ok | Status::Stale) => report::month(
                store,
                state.provider,
                year,
                month,
                tz,
                &config.thresholds,
                now,
            )?,
            _ => Vec::new(),
        };
        providers.push(ProviderMonth {
            provider: state.provider,
            status: state.status,
            stale_since: state.stale_since,
            unit: report::unit(report::windows(state.provider)[0]),
            days,
        });
    }
    Ok(MonthReport {
        year,
        month,
        generated_at: now,
        providers,
    })
}
