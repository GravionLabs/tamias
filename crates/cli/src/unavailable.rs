//! Why a provider is `unavailable`, shared by the collector and `today`.

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
            Unavailable::NoPremiumQuota => "no premium quota",
            Unavailable::Unlimited => "unlimited",
        }
    }

    /// The reason stored by a poll without a countable quota (`SourceStatus.unavailable`);
    /// `None` for a text this version does not know.
    #[must_use]
    pub fn from_stored(reason: &str) -> Option<Self> {
        [Self::NoPremiumQuota, Self::Unlimited]
            .into_iter()
            .find(|why| why.reason() == reason)
    }
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
