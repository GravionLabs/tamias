//! `tamias collect`: poll every available provider once and store the result.

use std::path::Path;

use jiff::Timestamp;
use tamias_core::paths;
use tamias_core::snapshot::Provider;
use tamias_core::store::Store;

use crate::claude;
use crate::collect_log::{CollectLog, RunResult};
use crate::config::{self, Config};

/// Longest message of a fatal-error line in the log.
const FATAL_MAX_CHARS: usize = 300;

/// Outcome of one collect run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every enabled provider was read (or none is available).
    Ok,
    /// At least one provider failed; its error is recorded.
    ProviderFailed,
}

/// Polls Claude, records the run in `store` and appends one line per provider to `log`.
///
/// # Errors
///
/// Store errors; a provider failure is recorded and reported as [`Outcome::ProviderFailed`].
pub fn run(
    config: &Config,
    store: &mut Store,
    log: &CollectLog,
) -> Result<Outcome, tamias_core::store::Error> {
    let provider = Provider::Claude;
    if let Some(why) = claude::availability_from_config(config.claude) {
        tracing::info!("{provider}: unavailable ({}), skipped", why.reason());
        log.record(Timestamp::now(), provider, RunResult::Ok, &skipped(why));
        return Ok(Outcome::Ok);
    }
    let now = Timestamp::now();
    let result = claude::provider().and_then(|p| p.poll(now));
    match result {
        Ok(snapshots) => {
            let inserted = store.record_run(provider, &snapshots, now)?;
            tracing::info!("{provider}: stored {inserted} snapshots");
            log.record(
                now,
                provider,
                RunResult::Ok,
                &format!("stored {inserted} snapshots"),
            );
            Ok(Outcome::Ok)
        }
        Err(e) => {
            if let Some(why) = claude::unavailable_for(config.claude, &e) {
                tracing::info!("{provider}: unavailable ({}), skipped: {e}", why.reason());
                log.record(now, provider, RunResult::Ok, &skipped(why));
                return Ok(Outcome::Ok);
            }
            tracing::error!("{provider}: {e}");
            log.record(now, provider, RunResult::Error, &e.class());
            store.record_error(provider, now, &e.class())?;
            Ok(Outcome::ProviderFailed)
        }
    }
}

/// A whole collect run as the binaries start it: loads the config, opens the store and runs.
///
/// A fatal error (invalid config, data directory, store) is appended to the collect log and,
/// when the store can be opened, recorded there as the last error of the provider, so that
/// `tamias service status` shows it. Neither step fails the run further.
///
/// # Errors
///
/// The fatal error, after it was logged.
pub fn run_logged(config_path: Option<&Path>) -> Result<Outcome, Box<dyn std::error::Error>> {
    let data_dir = paths::data_dir();
    let result = run_in(config_path, data_dir.as_deref().ok());
    if let (Err(e), Ok(dir)) = (&result, &data_dir) {
        let now = Timestamp::now();
        let message = fatal_message(&e.to_string());
        CollectLog::in_data_dir(dir).record(now, Provider::Claude, RunResult::Error, &message);
        if let Ok(mut store) = Store::open(dir.join(paths::DATABASE_FILE)) {
            let _ = store.record_error(Provider::Claude, now, &message);
        }
    }
    result
}

fn run_in(
    config_path: Option<&Path>,
    data_dir: Option<&Path>,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    let config = config::load(config_path)?;
    let data_dir = match data_dir {
        Some(dir) => dir.to_path_buf(),
        None => paths::data_dir()?,
    };
    let log = CollectLog::in_data_dir(&data_dir);
    let mut store = Store::open(data_dir.join(paths::DATABASE_FILE))?;
    Ok(run(&config, &mut store, &log)?)
}

fn fatal_message(error: &str) -> String {
    let flat: String = error.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "fatal: {}",
        flat.chars().take(FATAL_MAX_CHARS).collect::<String>()
    )
}

fn skipped(why: claude::Unavailable) -> String {
    format!("skipped ({})", why.reason())
}
