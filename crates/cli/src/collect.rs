//! `tamias collect`: poll every available provider once and store the result.

use jiff::Timestamp;
use tamias_core::snapshot::Provider;
use tamias_core::store::Store;

use crate::claude;
use crate::collect_log::{CollectLog, RunResult};
use crate::config::Config;

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

fn skipped(why: claude::Unavailable) -> String {
    format!("skipped ({})", why.reason())
}
