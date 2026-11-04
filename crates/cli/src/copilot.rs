//! Wiring of the Copilot provider: `gh` lookup, user client and availability.
//!
//! Two undocumented overrides let tests run the binary without a real `gh` or the network:
//!
//! - `TAMIAS_GH`: path of the `gh` executable (a fake one); wins over `gh_path` of the config.
//! - `TAMIAS_COPILOT_USER_URL`: user endpoint URL; honoured only for `http://` loopback hosts so
//!   the token can never be sent elsewhere.

use std::path::PathBuf;

use tamias_core::config::{Config, Enabled};
use tamias_core::provider::copilot::{CopilotProvider, CopilotUser, Error, GhTokenSource, TIMEOUT};

use crate::claude::is_loopback;

/// Test-only override of the `gh` executable.
pub const GH_ENV: &str = "TAMIAS_GH";
/// Test-only override of the user endpoint (loopback only).
pub const USER_URL_ENV: &str = "TAMIAS_COPILOT_USER_URL";

/// Why Copilot is `unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// Disabled in the config.
    Disabled,
    /// `auto` and `gh` is missing or not logged in.
    NotSetUp,
}

impl Unavailable {
    /// Short reason for the log.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Unavailable::Disabled => "disabled",
            Unavailable::NotSetUp => "not set up",
        }
    }
}

/// `Some` when the config alone makes Copilot `unavailable` (disabled).
#[must_use]
pub fn availability_from_config(enabled: Enabled) -> Option<Unavailable> {
    (enabled == Enabled::Off).then_some(Unavailable::Disabled)
}

/// Whether `error` means "`gh` is missing or logged out" for the setting `enabled`.
#[must_use]
pub fn unavailable_for(enabled: Enabled, error: &Error) -> Option<Unavailable> {
    match (enabled, error) {
        (Enabled::Off, _) => Some(Unavailable::Disabled),
        (Enabled::Auto, Error::NoCredentials(_)) => Some(Unavailable::NotSetUp),
        _ => None,
    }
}

/// The Copilot provider of this machine: `gh` from `TAMIAS_GH`, else `gh_path`, else `PATH` and
/// the known install locations.
#[must_use]
pub fn provider(config: &Config) -> CopilotProvider<GhTokenSource> {
    let explicit = std::env::var_os(GH_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| config.gh_path.clone());
    CopilotProvider::new(GhTokenSource::new(explicit), client())
}

fn client() -> CopilotUser {
    match std::env::var(USER_URL_ENV) {
        Ok(url) if is_loopback(&url) => {
            tracing::debug!("{USER_URL_ENV} set; using {url}");
            CopilotUser::with_url(url, TIMEOUT)
        }
        Ok(url) if !url.is_empty() => {
            tracing::warn!("ignoring {USER_URL_ENV}: only http loopback URLs are allowed");
            CopilotUser::new()
        }
        _ => CopilotUser::new(),
    }
}

#[cfg(test)]
mod tests {
    use tamias_core::config::Enabled;
    use tamias_core::provider::copilot::Error;

    use super::{Unavailable, availability_from_config, unavailable_for};

    #[test]
    fn only_off_is_unavailable_from_the_config() {
        assert_eq!(
            availability_from_config(Enabled::Off),
            Some(Unavailable::Disabled)
        );
        assert_eq!(availability_from_config(Enabled::Auto), None);
        assert_eq!(availability_from_config(Enabled::On), None);
    }

    #[test]
    fn missing_gh_is_unavailable_only_with_auto() {
        let missing = Error::NoCredentials("gh was not found".to_owned());
        assert_eq!(
            unavailable_for(Enabled::Auto, &missing),
            Some(Unavailable::NotSetUp)
        );
        assert_eq!(unavailable_for(Enabled::On, &missing), None);
    }

    #[test]
    fn other_errors_are_never_unavailable_when_enabled() {
        for enabled in [Enabled::Auto, Enabled::On] {
            assert_eq!(unavailable_for(enabled, &Error::Unauthorized), None);
            assert_eq!(
                unavailable_for(enabled, &Error::Timeout("t".to_owned())),
                None
            );
        }
    }
}
