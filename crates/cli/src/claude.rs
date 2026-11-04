//! Wiring of the Claude provider: credential source, usage client and availability.
//!
//! Two undocumented overrides let tests run the binary without the macOS Keychain or the
//! network:
//!
//! - `TAMIAS_CLAUDE_CREDENTIALS_FILE`: read the Claude Code credential JSON from this file on
//!   every OS instead of the OS default source.
//! - `TAMIAS_CLAUDE_USAGE_URL`: usage endpoint URL; honoured only for `http://` loopback hosts
//!   (`127.0.0.1`, `localhost`, `[::1]`) so the token can never be sent elsewhere.

use tamias_core::credentials::{self, CredentialSource, FileSource};
use tamias_core::provider::claude::{self, ClaudeProvider, Error, TIMEOUT, UsageClient};

use tamias_core::config::Enabled;

use crate::unavailable::Unavailable;

/// Test-only override of the credential source.
pub const CREDENTIALS_FILE_ENV: &str = "TAMIAS_CLAUDE_CREDENTIALS_FILE";
/// Test-only override of the usage endpoint (loopback only).
pub const USAGE_URL_ENV: &str = "TAMIAS_CLAUDE_USAGE_URL";

/// Whether `error` means "no credentials on this machine" for the setting `enabled`.
#[must_use]
pub fn unavailable_for(enabled: Enabled, error: &Error) -> Option<Unavailable> {
    match (enabled, error) {
        (Enabled::Off, _) => Some(Unavailable::Disabled),
        (Enabled::Auto, Error::NoCredentials(_)) => Some(Unavailable::NotSetUp),
        _ => None,
    }
}

/// The Claude provider of this machine.
///
/// # Errors
///
/// [`Error::NoCredentials`] when the OS default source cannot be located.
pub fn provider() -> claude::Result<ClaudeProvider<Box<dyn CredentialSource>>> {
    Ok(ClaudeProvider::new(source()?, client()))
}

/// `Some` when the config alone makes Claude `unavailable` (disabled).
#[must_use]
pub fn availability_from_config(enabled: Enabled) -> Option<Unavailable> {
    (enabled == Enabled::Off).then_some(Unavailable::Disabled)
}

/// Availability of Claude without fetching usage: `Some` when it is `unavailable`.
///
/// For `auto` this reads the credential (and drops it at once) to see whether one exists.
#[must_use]
pub fn availability(enabled: Enabled) -> Option<Unavailable> {
    match enabled {
        Enabled::Off => Some(Unavailable::Disabled),
        Enabled::On => None,
        Enabled::Auto => {
            let probe = source().and_then(|s| s.claude_token());
            tracing::debug!(
                "claude auto: credential probe {}",
                if probe.is_ok() {
                    "found a credential"
                } else {
                    "found no usable credential"
                }
            );
            match probe {
                Ok(_) => None,
                Err(e) => unavailable_for(enabled, &e),
            }
        }
    }
}

fn source() -> claude::Result<Box<dyn CredentialSource>> {
    match std::env::var_os(CREDENTIALS_FILE_ENV).filter(|v| !v.is_empty()) {
        Some(path) => {
            tracing::debug!("{CREDENTIALS_FILE_ENV} set; reading the credentials file");
            Ok(Box::new(FileSource::new(path)))
        }
        None => credentials::os_default(),
    }
}

fn client() -> UsageClient {
    match std::env::var(USAGE_URL_ENV) {
        Ok(url) if is_loopback(&url) => {
            tracing::debug!("{USAGE_URL_ENV} set; using {url}");
            UsageClient::with_url(url, TIMEOUT)
        }
        Ok(url) if !url.is_empty() => {
            tracing::warn!("ignoring {USAGE_URL_ENV}: only http loopback URLs are allowed");
            UsageClient::new()
        }
        _ => UsageClient::new(),
    }
}

/// Whether `url` is `http://` to exactly `127.0.0.1`, `localhost` or `[::1]` (any port, no
/// userinfo).
pub(crate) fn is_loopback(url: &str) -> bool {
    let Ok(uri) = url.parse::<http::Uri>() else {
        return false;
    };
    let Some(authority) = uri.authority() else {
        return false;
    };
    uri.scheme() == Some(&http::uri::Scheme::HTTP)
        && !authority.as_str().contains('@')
        && matches!(authority.host(), "127.0.0.1" | "localhost" | "[::1]")
}

#[cfg(test)]
mod tests {
    use tamias_core::provider::claude::Error;

    use super::{is_loopback, unavailable_for};
    use crate::unavailable::Unavailable;
    use tamias_core::config::Enabled;

    #[test]
    fn loopback_urls_are_accepted() {
        assert!(is_loopback("http://127.0.0.1:4711/api/oauth/usage"));
        assert!(is_loopback("http://localhost:80"));
        assert!(is_loopback("http://[::1]:8080/x"));
    }

    #[test]
    fn other_urls_are_rejected() {
        assert!(!is_loopback("https://127.0.0.1/"));
        assert!(!is_loopback("http://example.com/"));
        assert!(!is_loopback("http://127.0.0.1.example.com/"));
        assert!(!is_loopback("http://localhost@example.com/"));
        assert!(!is_loopback("http://user@127.0.0.1/"));
        assert!(!is_loopback("http://127.0.0.1:80@example.com/"));
        assert!(!is_loopback("http://localhost:x@evil.host/"));
        assert!(!is_loopback("http://[::1]@evil.host/"));
        assert!(!is_loopback("http://127.0.0.1.evil.host/"));
        assert!(!is_loopback("http://localhost.evil.host/"));
        assert!(!is_loopback("http://127.0.0.1%40evil.host/"));
        assert!(!is_loopback("http://evil.host%40127.0.0.1/"));
        assert!(!is_loopback("http://localhost%40evil.host:80/"));
        assert!(!is_loopback(""));
    }

    #[test]
    fn missing_credentials_are_unavailable_only_with_auto() {
        let missing = Error::NoCredentials("gone".to_owned());
        assert_eq!(
            unavailable_for(Enabled::Auto, &missing),
            Some(Unavailable::NotSetUp)
        );
        assert_eq!(unavailable_for(Enabled::On, &missing), None);
        assert_eq!(
            unavailable_for(Enabled::Off, &missing),
            Some(Unavailable::Disabled)
        );
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
