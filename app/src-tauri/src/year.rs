//! The `year` command: the twelve months of one year of every provider with their usage against
//! the budget, plus each provider's `status` / `stale_since` as in [`today`].
//!
//! - A provider that is `unavailable` has no months.
//! - A month without data (also a future month) is marked `has_data: false`.

use std::path::Path;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;
use tamias_core::config::{self, Config};
use tamias_core::paths;
use tamias_core::report::{self, MonthEntry, Status};
use tamias_core::snapshot::{Provider, Unit};
use tamias_core::store::{self, Store};

use crate::today::{self, Error};

/// One provider's year.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderYear {
    /// Which provider.
    pub provider: Provider,
    /// Health of the provider.
    pub status: Status,
    /// Last successful collect of a stale provider; absent when it never succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<Timestamp>,
    /// Unit of the months' `usage` and `budget`.
    pub unit: Unit,
    /// The months 1-12 in order; empty for an unavailable provider.
    pub months: Vec<MonthEntry>,
}

/// The year of every provider, in [`today::PROVIDERS`] order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct YearReport {
    /// Year as requested.
    pub year: i16,
    /// When the report was generated.
    pub generated_at: Timestamp,
    /// Every provider.
    pub providers: Vec<ProviderYear>,
}

/// The year now, in the system time zone, from the default database and `config.toml`.
///
/// # Errors
///
/// [`Error`].
pub fn current(year: i16) -> Result<YearReport, Error> {
    let config = config::load(None)?;
    let database = paths::database_path()?;
    Ok(read(
        &database,
        &config,
        year,
        Timestamp::now(),
        &TimeZone::system(),
    )?)
}

/// The year of every provider at `now`, the days taken in `tz`, from the database file
/// `database`, opened read-only. A missing database leaves every provider `unavailable`.
///
/// # Errors
///
/// Store errors of an existing database.
pub fn read(
    database: &Path,
    config: &Config,
    year: i16,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<YearReport, store::Error> {
    let overview = today::read(database, config, now, tz)?;
    let store = if database.is_file() {
        Some(Store::open_read_only(database)?)
    } else {
        None
    };
    let mut providers = Vec::new();
    for state in &overview.providers {
        let months = match (&store, state.status) {
            (Some(store), Status::Ok | Status::Stale) => {
                report::year(store, state.provider, year, tz, &config.thresholds, now)?
            }
            _ => Vec::new(),
        };
        providers.push(ProviderYear {
            provider: state.provider,
            status: state.status,
            stale_since: state.stale_since,
            unit: report::unit(report::windows(state.provider)[0]),
            months,
        });
    }
    Ok(YearReport {
        year,
        generated_at: now,
        providers,
    })
}
