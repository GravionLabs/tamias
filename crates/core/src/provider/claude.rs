//! Claude provider: the OAuth usage endpoint of the Claude subscription.
//!
//! `GET https://api.anthropic.com/api/oauth/usage` with the Claude Code OAuth token; only
//! `five_hour` and `seven_day` `{utilization, resets_at}` are read, every other key is ignored
//! (see `docs/sources.md`). [`parse_usage`] maps a response body without network,
//! [`UsageClient::fetch`] does the request, [`ClaudeProvider::poll`] combines it with a
//! [`CredentialSource`].
//!
//! A failed poll returns an error and no snapshot; the caller records the error.

use std::io;
use std::time::Duration;

use jiff::Timestamp;
use serde::Deserialize;

use crate::credentials::{CredentialSource, Secret};
use crate::snapshot::{Provider, Snapshot, Source, Unit, Window};

/// Usage endpoint of the Claude subscription.
pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Value of the `anthropic-beta` header Claude Code sends with the usage request.
pub const BETA_HEADER: &str = "oauth-2025-04-20";
/// Limit for the whole usage request.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Error of the Claude provider. Messages never contain the token.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No usable credential: missing Keychain item or file, or an API key login.
    #[error("no Claude credentials: {0}")]
    NoCredentials(String),
    /// The OAuth token expired; no request was made.
    #[error(
        "Claude OAuth token expired at {expires_at}; run Claude Code once to refresh it (tamias does not refresh tokens)"
    )]
    Expired {
        /// When the token expired.
        expires_at: Timestamp,
    },
    /// The endpoint rejected the token (HTTP 401).
    #[error(
        "Claude rejected the OAuth token (HTTP 401); run Claude Code once to refresh it (tamias does not refresh tokens)"
    )]
    Unauthorized,
    /// The request or the `security` subprocess did not finish in time.
    #[error("timeout: {0}")]
    Timeout(String),
    /// Transport failure or an unexpected HTTP status.
    #[error("Claude usage request failed: {0}")]
    Http(String),
    /// The credential or the response does not have the expected shape.
    #[error("unexpected Claude data: {0}")]
    Schema(String),
}

/// Start of the [`Error::Http`] message for an unexpected HTTP status.
const STATUS_PREFIX: &str = "unexpected HTTP status ";

impl Error {
    /// Short, stable error class for the source status: `no credentials`, `expired`,
    /// `unauthorized`, `timeout`, `http <status>`, `network` or `schema`.
    ///
    /// Never contains response bodies, paths or credential data.
    #[must_use]
    pub fn class(&self) -> String {
        match self {
            Error::NoCredentials(_) => "no credentials".to_owned(),
            Error::Expired { .. } => "expired".to_owned(),
            Error::Unauthorized => "unauthorized".to_owned(),
            Error::Timeout(_) => "timeout".to_owned(),
            Error::Http(message) => message
                .strip_prefix(STATUS_PREFIX)
                .and_then(|code| code.parse::<u16>().ok())
                .map_or_else(|| "network".to_owned(), |code| format!("http {code}")),
            Error::Schema(_) => "schema".to_owned(),
        }
    }
}

/// Result of the Claude provider.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Deserialize)]
struct UsageJson {
    five_hour: Option<WindowJson>,
    seven_day: Option<WindowJson>,
}

#[derive(Deserialize)]
struct WindowJson {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

/// Maps a usage response body to the `five_hour` and `seven_day` snapshots, observed at `now`.
///
/// # Errors
///
/// [`Error::Schema`] when the body is not JSON or a window, its `utilization` (finite, at least
/// 0) or its `resets_at` (RFC 3339) is missing or invalid. Nothing is returned partially.
pub fn parse_usage(body: &[u8], now: Timestamp) -> Result<Vec<Snapshot>> {
    let usage: UsageJson =
        serde_json::from_slice(body).map_err(|e| Error::Schema(format!("usage response: {e}")))?;
    Ok(vec![
        snapshot(Window::FiveHour, usage.five_hour, now)?,
        snapshot(Window::SevenDay, usage.seven_day, now)?,
    ])
}

fn snapshot(window: Window, json: Option<WindowJson>, now: Timestamp) -> Result<Snapshot> {
    let name = window.as_str();
    let json = json.ok_or_else(|| Error::Schema(format!("{name} is missing or null")))?;
    let used = json
        .utilization
        .ok_or_else(|| Error::Schema(format!("{name}.utilization is missing or null")))?;
    if !used.is_finite() || used < 0.0 {
        return Err(Error::Schema(format!(
            "{name}.utilization {used} is not a percentage"
        )));
    }
    let raw = json
        .resets_at
        .ok_or_else(|| Error::Schema(format!("{name}.resets_at is missing or null")))?;
    let resets_at: Timestamp = raw
        .parse()
        .map_err(|e| Error::Schema(format!("{name}.resets_at {raw:?} is not a timestamp: {e}")))?;
    Ok(Snapshot {
        provider: Provider::Claude,
        window,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at,
        observed_at: now,
        source: Source::Poll,
    })
}

/// HTTP client of the usage endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageClient {
    url: String,
    timeout: Duration,
}

impl UsageClient {
    /// Client of [`USAGE_URL`] with [`TIMEOUT`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_url(USAGE_URL, TIMEOUT)
    }

    /// Client of another `url` (a test server) with another `timeout`.
    #[must_use]
    pub fn with_url(url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            url: url.into(),
            timeout,
        }
    }

    /// Fetches the usage with `token` and maps it to snapshots observed at `now`.
    ///
    /// The token is consumed and dropped after the request. An expired token is rejected before
    /// any network access.
    ///
    /// # Errors
    ///
    /// [`Error::Expired`] (no request made), [`Error::Unauthorized`] on HTTP 401,
    /// [`Error::Timeout`] when the request exceeds the timeout, [`Error::Http`] on other
    /// transport failures or statuses, [`Error::Schema`] from [`parse_usage`].
    pub fn fetch(&self, token: Secret, now: Timestamp) -> Result<Vec<Snapshot>> {
        if token.is_expired(now) {
            return Err(Error::Expired {
                expires_at: token.expires_at(),
            });
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let response = {
            let authorization = format!("Bearer {}", token.expose());
            let response = agent
                .get(&self.url)
                .header("Authorization", &authorization)
                .header("anthropic-beta", BETA_HEADER)
                .call();
            drop(authorization);
            drop(token);
            response
        };
        let mut response = response.map_err(map_ureq_error)?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(Error::Unauthorized);
        }
        if status != 200 {
            return Err(Error::Http(format!("{STATUS_PREFIX}{status}")));
        }
        let body = response.body_mut().read_to_vec().map_err(map_ureq_error)?;
        parse_usage(&body, now)
    }
}

impl Default for UsageClient {
    fn default() -> Self {
        Self::new()
    }
}

fn map_ureq_error(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Timeout(t) => Error::Timeout(format!("Claude usage request ({t})")),
        ureq::Error::Io(io) if io.kind() == io::ErrorKind::TimedOut => {
            Error::Timeout(format!("Claude usage request ({io})"))
        }
        other => Error::Http(other.to_string()),
    }
}

/// Claude provider: a credential source plus the usage client.
#[derive(Debug)]
pub struct ClaudeProvider<S> {
    source: S,
    client: UsageClient,
}

impl<S: CredentialSource> ClaudeProvider<S> {
    /// Provider reading the token from `source` and the usage through `client`.
    #[must_use]
    pub fn new(source: S, client: UsageClient) -> Self {
        Self { source, client }
    }

    /// Reads the token and fetches the `five_hour` and `seven_day` snapshots, observed at `now`.
    ///
    /// # Errors
    ///
    /// Any [`Error`] of the credential source or of [`UsageClient::fetch`]; no snapshot then.
    pub fn poll(&self, now: Timestamp) -> Result<Vec<Snapshot>> {
        let token = self.source.claude_token()?;
        self.client.fetch(token, now)
    }
}
