//! Why a provider is `unavailable`, shared by the collector, `tamias today` and the app.

use crate::config::{Config, Enabled};
use crate::provider::copilot::{self, NoQuota};
use crate::snapshot::Provider;
use crate::store::{self, Store};

/// Why a provider is `unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// Disabled in the config.
    Disabled,
    /// `auto` and no credentials (Claude) or no `gh` (Copilot) on this machine.
    NotSetUp,
    /// Copilot: the plan has no premium quota.
    NoPremiumQuota,
    /// Copilot: the premium quota is unlimited.
    Unlimited,
}

impl Unavailable {
    /// Short reason shown by `today` and written to the log.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Unavailable::Disabled => "disabled",
            Unavailable::NotSetUp => "not set up",
            Unavailable::NoPremiumQuota => NoQuota::NoPremiumQuota.reason(),
            Unavailable::Unlimited => NoQuota::Unlimited.reason(),
        }
    }

    /// The reason stored by a poll without a countable quota (`SourceStatus.unavailable`);
    /// `None` for a text this version does not know.
    #[must_use]
    pub fn from_stored(reason: &str) -> Option<Self> {
        NoQuota::from_reason(reason).map(|no_quota| match no_quota {
            NoQuota::NoPremiumQuota => Self::NoPremiumQuota,
            NoQuota::Unlimited => Self::Unlimited,
        })
    }
}

/// Availability of Copilot without polling: `Some` when it is disabled, or `auto` and no `gh`
/// executable can be found (the login is not probed).
#[must_use]
pub fn copilot_from_config(config: &Config) -> Option<Unavailable> {
    match config.copilot {
        Enabled::Off => Some(Unavailable::Disabled),
        Enabled::On => None,
        Enabled::Auto => copilot::gh_source(config)
            .resolve()
            .is_none()
            .then_some(Unavailable::NotSetUp),
    }
}

/// Why Copilot is `unavailable`: the config, a missing `gh`, then the reason stored by the last
/// poll without a countable quota. The stored reason takes precedence over older snapshots of
/// the store, but not over a poll error recorded after it: then the state is unknown and
/// Copilot is read (and shown stale).
///
/// # Errors
///
/// Store errors.
pub fn copilot(config: &Config, store: &Store) -> Result<Option<Unavailable>, store::Error> {
    if let Some(why) = copilot_from_config(config) {
        return Ok(Some(why));
    }
    let Some(status) = store.status(Provider::Copilot)? else {
        return Ok(None);
    };
    let failed_since = status.last_error_at > status.last_ok_at;
    Ok(status
        .unavailable
        .as_deref()
        .and_then(Unavailable::from_stored)
        .filter(|_| !failed_since))
}

#[cfg(test)]
mod tests {
    use super::Unavailable;

    #[test]
    fn reasons_are_the_documented_texts() {
        assert_eq!(Unavailable::Disabled.reason(), "disabled");
        assert_eq!(Unavailable::NotSetUp.reason(), "not set up");
        assert_eq!(Unavailable::NoPremiumQuota.reason(), "no premium quota");
        assert_eq!(Unavailable::Unlimited.reason(), "unlimited");
    }

    #[test]
    fn stored_reasons_are_parsed() {
        assert_eq!(
            Unavailable::from_stored("no premium quota"),
            Some(Unavailable::NoPremiumQuota)
        );
        assert_eq!(
            Unavailable::from_stored("unlimited"),
            Some(Unavailable::Unlimited)
        );
        assert_eq!(Unavailable::from_stored("disabled"), None);
        assert_eq!(Unavailable::from_stored(""), None);
    }
}
