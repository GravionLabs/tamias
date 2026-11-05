//! The `today` command: today's [`Report`] of every provider, the same document as
//! `tamias today --json`, read from the snapshot store without writing to it.
//!
//! - A missing database file is not an error: the collector has not run yet, so every provider
//!   is `unavailable`.
//! - Claude switched off in `config.toml` (`claude = false`) is `unavailable`.
//! - Copilot is `unavailable` as in the CLI ([`tamias_core::unavailable::copilot`]): switched off
//!   (`copilot = false`), `auto` without a `gh` executable, or the seat has no countable quota
//!   (stored reason, unless an error was recorded after it).
//! - Claude that never ran and has no snapshot is `unavailable`, except switched on explicitly
//!   (`claude = true`), which is `stale` then, as in the CLI. Copilot `auto` with `gh` but never
//!   collected is `stale`, as in the CLI.

use std::path::Path;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_core::config::{self, Config, Enabled};
use tamias_core::paths;
use tamias_core::report::{self, Report};
use tamias_core::snapshot::Provider;
use tamias_core::store::{self, Store};
use tamias_core::unavailable;

/// Providers of the report, in this order.
pub const PROVIDERS: [Provider; 2] = [Provider::Claude, Provider::Copilot];

/// Why the report could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No data directory (no home directory and no `TAMIAS_DATA_DIR`).
    #[error(transparent)]
    NoHomeDir(#[from] paths::NoHomeDir),
    /// `config.toml` is unreadable or invalid.
    #[error(transparent)]
    Config(#[from] config::Error),
    /// The store could not be read.
    #[error(transparent)]
    Store(#[from] store::Error),
}

impl Error {
    /// Whether the store was locked longer than its busy timeout; the next refresh retries.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Store(error) if error.is_busy())
    }
}

/// The report now, in the system time zone, from the database at [`paths::database_path`]
/// (`TAMIAS_DATA_DIR` respected) with the default `config.toml` (`TAMIAS_CONFIG_DIR` respected).
///
/// # Errors
///
/// [`Error`].
pub fn current() -> Result<Report, Error> {
    let config = config::load(None)?;
    let database = paths::database_path()?;
    Ok(read(
        &database,
        &config,
        Timestamp::now(),
        &TimeZone::system(),
    )?)
}

/// The report of every provider at `now`, the day taken in `tz`, from the database file
/// `database`, opened read-only.
///
/// # Errors
///
/// Store errors of an existing database (unreadable, written by a newer tamias).
pub fn read(
    database: &Path,
    config: &Config,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<Report, store::Error> {
    if !database.is_file() {
        return Ok(Report::new(
            now,
            PROVIDERS.into_iter().map(report::unavailable).collect(),
        ));
    }
    let store = Store::open_read_only(database)?;
    let providers = PROVIDERS
        .into_iter()
        .map(|provider| read_provider(&store, provider, config, now, tz))
        .collect::<Result<_, _>>()?;
    Ok(Report::new(now, providers))
}

fn read_provider(
    store: &Store,
    provider: Provider,
    config: &Config,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<report::Provider, store::Error> {
    if provider == Provider::Copilot {
        return if unavailable::copilot(config, store)?.is_some() {
            Ok(report::unavailable(provider))
        } else {
            report::read(store, provider, now, tz, &config.thresholds)
        };
    }
    if config.claude == Enabled::Off {
        return Ok(report::unavailable(provider));
    }
    let read = report::read(store, provider, now, tz, &config.thresholds)?;
    let never_ran = store.status(provider)?.is_none() && read.windows.is_empty();
    if never_ran && config.claude == Enabled::Auto {
        return Ok(report::unavailable(provider));
    }
    Ok(read)
}
