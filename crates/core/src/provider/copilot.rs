//! Copilot provider: the premium-request quota of a GitHub Copilot seat.
//!
//! `GET https://api.github.com/copilot_internal/user` with the token of `gh auth token`; only
//! `quota_snapshots.premium_interactions.{entitlement, remaining, unlimited, has_quota}` and
//! `quota_reset_date_utc` are read, every other key is ignored (see `docs/sources.md`).
//! [`parse_user`] maps a response body without network, [`CopilotUser::fetch`] does the request,
//! [`GhTokenSource`] runs `gh`, [`CopilotProvider::poll`] combines both.
//!
//! A failed poll returns an error and no snapshot; the caller records the error.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde::Deserialize;

use crate::config::Config;
use crate::snapshot::{Provider, Snapshot, Source, Unit, Window};

/// User endpoint of GitHub Copilot.
pub const USER_URL: &str = "https://api.github.com/copilot_internal/user";
/// Limit for the whole user request.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// Limit for `gh auth token`.
pub const GH_TIMEOUT: Duration = Duration::from_secs(5);

/// Error of the Copilot provider. Messages never contain the token, a body or a path.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `gh` is not installed or not logged in.
    #[error("no GitHub credentials: {0}")]
    NoCredentials(String),
    /// The endpoint rejected the token (HTTP 401).
    #[error("GitHub rejected the token (HTTP 401); run `gh auth login`")]
    Unauthorized,
    /// The request or the `gh` subprocess did not finish in time.
    #[error("timeout: {0}")]
    Timeout(String),
    /// Transport failure or an unexpected HTTP status.
    #[error("Copilot user request failed: {0}")]
    Http(String),
    /// The response does not have the expected shape.
    #[error("unexpected Copilot data: {0}")]
    Schema(String),
}

/// Start of the [`Error::Http`] message for an unexpected HTTP status.
const STATUS_PREFIX: &str = "unexpected HTTP status ";

impl Error {
    /// Short, stable error class for the source status: `no credentials`, `unauthorized`,
    /// `timeout`, `http <status>`, `network` or `schema`.
    ///
    /// Never contains response bodies, paths or credential data.
    #[must_use]
    pub fn class(&self) -> String {
        match self {
            Error::NoCredentials(_) => "no credentials".to_owned(),
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

/// Result of the Copilot provider.
pub type Result<T> = std::result::Result<T, Error>;

/// Why a seat has no countable premium quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoQuota {
    /// The plan has no premium quota (`has_quota` false or entitlement 0).
    NoPremiumQuota,
    /// The quota is unlimited.
    Unlimited,
}

impl NoQuota {
    /// Reason shown with `unavailable`: `no premium quota` or `unlimited`.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            NoQuota::NoPremiumQuota => "no premium quota",
            NoQuota::Unlimited => "unlimited",
        }
    }

    /// The variant whose [`reason`](Self::reason) is `reason`; `None` for a text this version
    /// does not know.
    #[must_use]
    pub fn from_reason(reason: &str) -> Option<Self> {
        [NoQuota::NoPremiumQuota, NoQuota::Unlimited]
            .into_iter()
            .find(|no_quota| no_quota.reason() == reason)
    }
}

/// Outcome of a successful poll.
#[derive(Debug, Clone, PartialEq)]
pub enum Poll {
    /// A countable monthly quota.
    Quota(Snapshot),
    /// A seat without a countable quota.
    NoQuota(NoQuota),
}

#[derive(Deserialize)]
struct UserJson {
    quota_snapshots: Option<QuotasJson>,
    quota_reset_date_utc: Option<String>,
}

#[derive(Deserialize)]
struct QuotasJson {
    premium_interactions: Option<PremiumJson>,
}

#[derive(Deserialize)]
struct PremiumJson {
    entitlement: Option<f64>,
    remaining: Option<f64>,
    unlimited: Option<bool>,
    has_quota: Option<bool>,
}

/// Maps a user response body to the monthly snapshot (or the reason there is none), observed
/// at `now`.
///
/// # Errors
///
/// [`Error::Schema`] when the body is not JSON, `premium_interactions` or one of the fields
/// needed for the outcome is missing or has the wrong type, a number is not finite and at
/// least 0, or `quota_reset_date_utc` is not an RFC 3339 timestamp. Nothing is returned
/// partially.
pub fn parse_user(body: &[u8], now: Timestamp) -> Result<Poll> {
    let user: UserJson =
        serde_json::from_slice(body).map_err(|e| Error::Schema(format!("user response: {e}")))?;
    let premium = user
        .quota_snapshots
        .and_then(|q| q.premium_interactions)
        .ok_or_else(|| Error::Schema("premium_interactions is missing or null".to_owned()))?;
    let unlimited = premium
        .unlimited
        .ok_or_else(|| Error::Schema("premium_interactions.unlimited is missing".to_owned()))?;
    let has_quota = premium
        .has_quota
        .ok_or_else(|| Error::Schema("premium_interactions.has_quota is missing".to_owned()))?;
    if unlimited {
        return Ok(Poll::NoQuota(NoQuota::Unlimited));
    }
    if !has_quota {
        return Ok(Poll::NoQuota(NoQuota::NoPremiumQuota));
    }
    let entitlement = count("entitlement", premium.entitlement)?;
    if entitlement == 0.0 {
        return Ok(Poll::NoQuota(NoQuota::NoPremiumQuota));
    }
    let remaining = count("remaining", premium.remaining)?;
    let raw = user
        .quota_reset_date_utc
        .ok_or_else(|| Error::Schema("quota_reset_date_utc is missing or null".to_owned()))?;
    let resets_at: Timestamp = raw
        .parse()
        .map_err(|_| Error::Schema("quota_reset_date_utc is not a timestamp".to_owned()))?;
    Ok(Poll::Quota(Snapshot {
        provider: Provider::Copilot,
        window: Window::Monthly,
        used: (entitlement - remaining).max(0.0),
        limit: entitlement,
        unit: Unit::Requests,
        resets_at,
        observed_at: now,
        source: Source::Poll,
    }))
}

fn count(name: &str, value: Option<f64>) -> Result<f64> {
    let value =
        value.ok_or_else(|| Error::Schema(format!("premium_interactions.{name} is missing")))?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(Error::Schema(format!(
            "premium_interactions.{name} is not a count"
        )))
    }
}

/// GitHub token from `gh auth token`. The value is never shown by `Debug`; there is no `Display`.
pub struct GhToken {
    value: String,
}

impl GhToken {
    /// Wraps a token.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self { value }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for GhToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GhToken")
            .field("value", &"<redacted>")
            .finish()
    }
}

/// A source of the GitHub token.
pub trait TokenSource: fmt::Debug {
    /// Reads the current GitHub token.
    ///
    /// # Errors
    ///
    /// [`Error::NoCredentials`] when there is no token, [`Error::Timeout`] when reading it took
    /// too long.
    fn gh_token(&self) -> Result<GhToken>;
}

/// Test-only override of the `gh` executable; wins over `gh_path` of the config.
pub const GH_ENV: &str = "TAMIAS_GH";

/// The `gh` token source of this machine: `TAMIAS_GH`, else `gh_path` of the config, else `PATH`
/// and the known install locations.
#[must_use]
pub fn gh_source(config: &Config) -> GhTokenSource {
    let explicit = std::env::var_os(GH_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| config.gh_path.clone());
    GhTokenSource::new(explicit)
}

/// Install locations of `gh` tried after `PATH`.
#[must_use]
pub fn known_gh_locations() -> Vec<PathBuf> {
    let mut known: Vec<PathBuf> = [
        "/opt/homebrew/bin/gh",
        "/usr/local/bin/gh",
        "/usr/bin/gh",
        "/snap/bin/gh",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    for (var, rest) in [
        ("ProgramFiles", r"GitHub CLI\gh.exe"),
        ("LOCALAPPDATA", r"Programs\GitHub CLI\gh.exe"),
    ] {
        if let Some(base) = std::env::var_os(var) {
            known.push(PathBuf::from(base).join(rest));
        }
    }
    known
}

/// Token source running `gh auth token`.
///
/// `gh` is resolved from the explicit path, else `PATH`, else the known install locations.
#[derive(Debug, Clone)]
pub struct GhTokenSource {
    explicit: Option<PathBuf>,
    path_var: Option<OsString>,
    known: Vec<PathBuf>,
    timeout: Duration,
}

impl GhTokenSource {
    /// Source with the process `PATH`, the known locations and [`GH_TIMEOUT`]; `explicit` is the
    /// configured `gh_path`.
    #[must_use]
    pub fn new(explicit: Option<PathBuf>) -> Self {
        Self::with_search(
            explicit,
            std::env::var_os("PATH"),
            known_gh_locations(),
            GH_TIMEOUT,
        )
    }

    /// Source with another `PATH` value, known locations and `timeout` (tests).
    #[must_use]
    pub fn with_search(
        explicit: Option<PathBuf>,
        path_var: Option<OsString>,
        known: Vec<PathBuf>,
        timeout: Duration,
    ) -> Self {
        Self {
            explicit,
            path_var,
            known,
            timeout,
        }
    }

    /// The `gh` executable to run: an explicit path is used as is when it is a file (it does not
    /// fall back), else the first `gh` on `PATH`, else the first known location that exists.
    #[must_use]
    pub fn resolve(&self) -> Option<PathBuf> {
        if let Some(explicit) = &self.explicit {
            return explicit.is_file().then(|| explicit.clone());
        }
        let name = if cfg!(windows) { "gh.exe" } else { "gh" };
        if let Some(path_var) = &self.path_var {
            let found = std::env::split_paths(path_var)
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file());
            if found.is_some() {
                return found;
            }
        }
        self.known.iter().find(|p| p.is_file()).cloned()
    }
}

impl TokenSource for GhTokenSource {
    fn gh_token(&self) -> Result<GhToken> {
        let gh = self
            .resolve()
            .ok_or_else(|| Error::NoCredentials("gh was not found".to_owned()))?;
        run_gh(&gh, self.timeout)
    }
}

fn run_gh(gh: &Path, timeout: Duration) -> Result<GhToken> {
    let mut child = Command::new(gh)
        .args(["auth", "token"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::NoCredentials("gh could not be started".to_owned()))?;
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_end(&mut out);
        }
        out
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Timeout("gh auth token".to_owned()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::NoCredentials("gh could not be run".to_owned()));
            }
        }
    };
    if !status.success() {
        return Err(Error::NoCredentials(
            "gh is not logged in (run `gh auth login`)".to_owned(),
        ));
    }
    let out = reader.join().unwrap_or_default();
    let token = String::from_utf8_lossy(&out).trim().to_owned();
    if token.is_empty() {
        return Err(Error::NoCredentials("gh printed no token".to_owned()));
    }
    Ok(GhToken::new(token))
}

/// HTTP client of the user endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotUser {
    url: String,
    timeout: Duration,
}

impl CopilotUser {
    /// Client of [`USER_URL`] with [`TIMEOUT`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_url(USER_URL, TIMEOUT)
    }

    /// Client of another `url` (a test server) with another `timeout`.
    #[must_use]
    pub fn with_url(url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            url: url.into(),
            timeout,
        }
    }

    /// Fetches the user with `token` and maps it to a [`Poll`] observed at `now`.
    ///
    /// The token is consumed and dropped after the request.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] on HTTP 401, [`Error::Timeout`] when the request exceeds the
    /// timeout, [`Error::Http`] on other transport failures or statuses, [`Error::Schema`] from
    /// [`parse_user`].
    pub fn fetch(&self, token: GhToken, now: Timestamp) -> Result<Poll> {
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
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", concat!("tamias/", env!("CARGO_PKG_VERSION")))
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
        parse_user(&body, now)
    }
}

impl Default for CopilotUser {
    fn default() -> Self {
        Self::new()
    }
}

fn map_ureq_error(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Timeout(t) => Error::Timeout(format!("Copilot user request ({t})")),
        ureq::Error::Io(io) if io.kind() == io::ErrorKind::TimedOut => {
            Error::Timeout(format!("Copilot user request ({io})"))
        }
        other => Error::Http(other.to_string()),
    }
}

/// Copilot provider: a token source plus the user client.
#[derive(Debug)]
pub struct CopilotProvider<S> {
    source: S,
    client: CopilotUser,
}

impl<S: TokenSource> CopilotProvider<S> {
    /// Provider reading the token from `source` and the quota through `client`.
    #[must_use]
    pub fn new(source: S, client: CopilotUser) -> Self {
        Self { source, client }
    }

    /// Reads the token and fetches the monthly quota, observed at `now`.
    ///
    /// # Errors
    ///
    /// Any [`Error`] of the token source or of [`CopilotUser::fetch`]; no snapshot then.
    pub fn poll(&self, now: Timestamp) -> Result<Poll> {
        let token = self.source.gh_token()?;
        self.client.fetch(token, now)
    }
}
