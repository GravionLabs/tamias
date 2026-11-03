//! `tamias collect`: poll every available provider once and store the result.

use jiff::Timestamp;
use tamias_core::snapshot::Provider;
use tamias_core::store::Store;

use crate::claude;
use crate::config::Config;

/// Outcome of one collect run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every enabled provider was read (or none is available).
    Ok,
    /// At least one provider failed; its error is recorded.
    ProviderFailed,
}

/// Polls Claude and records the run in `store`.
///
/// # Errors
///
/// Store errors; a provider failure is recorded and reported as [`Outcome::ProviderFailed`].
pub fn run(config: &Config, store: &mut Store) -> Result<Outcome, tamias_core::store::Error> {
    let provider = Provider::Claude;
    if let Some(why) = claude::availability_from_config(config.claude) {
        tracing::info!("{provider}: unavailable ({}), skipped", why.reason());
        return Ok(Outcome::Ok);
    }
    let now = Timestamp::now();
    let result = claude::provider().and_then(|p| p.poll(now));
    match result {
        Ok(snapshots) => {
            let inserted = store.record_run(provider, &snapshots, now)?;
            tracing::info!("{provider}: stored {inserted} snapshots");
            Ok(Outcome::Ok)
        }
        Err(e) => {
            if let Some(why) = claude::unavailable_for(config.claude, &e) {
                tracing::info!("{provider}: unavailable ({}), skipped: {e}", why.reason());
                return Ok(Outcome::Ok);
            }
            tracing::error!("{provider}: {e}");
            store.record_error(provider, now, &e.class())?;
            Ok(Outcome::ProviderFailed)
        }
    }
}
