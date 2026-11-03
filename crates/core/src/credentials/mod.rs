//! Credential sources for the Claude OAuth token.
//!
//! Claude Code stores its OAuth credential as JSON `claudeAiOauth{accessToken, expiresAt, …}`:
//! on macOS in the login Keychain (read through `/usr/bin/security`, see [`KeychainSource`]),
//! on Linux and Windows in `.credentials.json` below the Claude config directory
//! (see [`FileSource`]). [`os_default`] picks the source of the running OS.
//!
//! tamias never refreshes the token: a refresh rotates the refresh token and Claude Code would
//! then hold a dead one.

mod file;
mod keychain;

use std::fmt;

use jiff::Timestamp;
use serde::Deserialize;

use crate::provider::claude::{Error, Result};

pub use file::FileSource;
pub use keychain::KeychainSource;

/// A source of the Claude OAuth access token.
pub trait CredentialSource: fmt::Debug {
    /// Reads the current Claude OAuth access token.
    ///
    /// # Errors
    ///
    /// [`Error::NoCredentials`] when the credential is missing or unreadable, or holds no
    /// `claudeAiOauth` entry (API key login); [`Error::Timeout`] when reading it took too long;
    /// [`Error::Schema`] when the credential is not the expected JSON.
    fn claude_token(&self) -> Result<Secret>;
}

impl<S: CredentialSource + ?Sized> CredentialSource for Box<S> {
    fn claude_token(&self) -> Result<Secret> {
        (**self).claude_token()
    }
}

/// OAuth access token with its expiry.
///
/// The value is never shown by `Debug` and there is no `Display`; it leaves the crate only as
/// the `Authorization` header of the usage request. Pass it by value to the request so it is
/// dropped right after.
pub struct Secret {
    value: String,
    expires_at: Timestamp,
}

impl Secret {
    /// Wraps a token that expires at `expires_at`.
    #[must_use]
    pub fn new(value: String, expires_at: Timestamp) -> Self {
        Self { value, expires_at }
    }

    /// When the token expires.
    #[must_use]
    pub fn expires_at(&self) -> Timestamp {
        self.expires_at
    }

    /// Whether the token is expired at `now`.
    #[must_use]
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expires_at
    }

    pub(crate) fn expose(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret")
            .field("value", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Deserialize)]
struct CredentialsJson {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<OAuthJson>,
}

#[derive(Deserialize)]
struct OAuthJson {
    #[serde(rename = "accessToken")]
    access_token: String,
    /// Epoch milliseconds.
    #[serde(rename = "expiresAt")]
    expires_at: i64,
}

/// Parses the Claude Code credential JSON (Keychain item or `.credentials.json`).
///
/// # Errors
///
/// [`Error::NoCredentials`] when there is no `claudeAiOauth` entry or its token is empty;
/// [`Error::Schema`] when the input is not the expected JSON. Error messages never contain
/// parts of the input.
pub fn parse_credentials(json: &[u8]) -> Result<Secret> {
    let parsed: CredentialsJson = serde_json::from_slice(json).map_err(|e| {
        // The serde message can quote input values, so only the position is reported.
        Error::Schema(format!(
            "Claude credentials are not the expected JSON (line {}, column {})",
            e.line(),
            e.column()
        ))
    })?;
    let Some(oauth) = parsed.oauth else {
        return Err(Error::NoCredentials(
            "no claudeAiOauth entry in the Claude credentials (API key login?)".to_owned(),
        ));
    };
    if oauth.access_token.is_empty() {
        return Err(Error::NoCredentials(
            "empty accessToken in the Claude credentials".to_owned(),
        ));
    }
    let expires_at = Timestamp::from_millisecond(oauth.expires_at)
        .map_err(|_| Error::Schema("claudeAiOauth.expiresAt is out of range".to_owned()))?;
    Ok(Secret::new(oauth.access_token, expires_at))
}

/// The credential source of the running OS: the Keychain on macOS, the credentials file
/// elsewhere.
///
/// # Errors
///
/// [`Error::NoCredentials`] when the credentials file cannot be located (no home directory).
pub fn os_default() -> Result<Box<dyn CredentialSource>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(KeychainSource::new()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(Box::new(FileSource::from_env()?))
    }
}
