//! `config.toml`: colour thresholds, staleness limit and the enabled providers.
//!
//! ```toml
//! yellow_at = 0.8       # ratio of today's usage to today's budget from which a day is yellow
//! red_at = 1.0          # ratio above which a day is red
//! stale_after = "15m"   # age of the last successful collect after which data is stale
//!
//! [providers]
//! claude = "auto"       # "auto" (on when credentials exist), true or false
//! ```
//!
//! Every key is optional; a missing default file means all defaults.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use jiff::SignedDuration;
use serde::Deserialize;
use tamias_core::budget::Thresholds;
use tamias_core::paths;

/// File name of the config inside the config directory.
pub const FILE_NAME: &str = "config.toml";

/// Whether a provider is collected and shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Enabled {
    /// On when its credentials exist on this machine, else `unavailable`.
    #[default]
    Auto,
    /// Always on; missing credentials are an error (`stale`).
    On,
    /// Off: `unavailable`.
    Off,
}

/// Effective configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    pub thresholds: Thresholds,
    pub claude: Enabled,
}

/// The config could not be read or is invalid.
#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid config {}: {}",
            self.path.display(),
            self.message
        )
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    yellow_at: Option<f64>,
    red_at: Option<f64>,
    stale_after: Option<String>,
    #[serde(default)]
    providers: RawProviders,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProviders {
    claude: Option<RawEnabled>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawEnabled {
    Bool(bool),
    Text(String),
}

/// Loads `path`, or the default file in the config directory when `path` is `None`.
///
/// A missing default file (or no home directory) yields the defaults; a missing explicit
/// `path` is an error.
///
/// # Errors
///
/// [`Error`] when the file cannot be read, is not TOML, has unknown keys or invalid values.
pub fn load(path: Option<&Path>) -> Result<Config, Error> {
    let (path, explicit) = match path {
        Some(path) => (path.to_path_buf(), true),
        None => match paths::config_dir() {
            Ok(dir) => (dir.join(FILE_NAME), false),
            Err(e) => {
                tracing::debug!("no config directory ({e}); using the defaults");
                return Ok(Config::default());
            }
        },
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            tracing::debug!("reading config {}", path.display());
            parse(&text).map_err(|message| Error { path, message })
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound && !explicit => {
            tracing::debug!("no config at {}; using the defaults", path.display());
            Ok(Config::default())
        }
        Err(e) => Err(Error {
            path,
            message: format!("cannot read it: {e}"),
        }),
    }
}

/// Parses the text of a config file.
///
/// # Errors
///
/// A message naming the offending key.
pub fn parse(text: &str) -> Result<Config, String> {
    let raw: RawConfig = toml::from_str(text).map_err(|e| e.to_string().trim_end().to_owned())?;
    let defaults = Thresholds::default();
    let yellow_at = raw.yellow_at.unwrap_or(defaults.yellow_at);
    let red_at = raw.red_at.unwrap_or(defaults.red_at);
    if !yellow_at.is_finite() || yellow_at <= 0.0 {
        return Err(format!("yellow_at = {yellow_at} must be a number above 0"));
    }
    if !red_at.is_finite() || red_at < yellow_at {
        return Err(format!(
            "red_at = {red_at} must be a number not below yellow_at = {yellow_at}"
        ));
    }
    let stale_after = match raw.stale_after {
        None => defaults.stale_after,
        Some(text) => parse_duration(&text)?,
    };
    let claude = match raw.providers.claude {
        None => Enabled::Auto,
        Some(RawEnabled::Bool(true)) => Enabled::On,
        Some(RawEnabled::Bool(false)) => Enabled::Off,
        Some(RawEnabled::Text(text)) if text == "auto" => Enabled::Auto,
        Some(RawEnabled::Text(text)) => {
            return Err(format!(
                "providers.claude = {text:?} must be \"auto\", true or false"
            ));
        }
    };
    Ok(Config {
        thresholds: Thresholds {
            yellow_at,
            red_at,
            stale_after,
        },
        claude,
    })
}

fn parse_duration(text: &str) -> Result<SignedDuration, String> {
    let duration: SignedDuration = text.parse().map_err(|_| {
        format!("stale_after = {text:?} is not a duration (examples: \"15m\", \"1h 30m\")")
    })?;
    if duration.is_zero() || duration.is_negative() {
        return Err(format!("stale_after = {text:?} must be positive"));
    }
    Ok(duration)
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;

    use super::{Config, Enabled, parse};

    #[test]
    fn empty_file_is_the_defaults() {
        let config = parse("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.claude, Enabled::Auto);
        assert!((config.thresholds.yellow_at - 0.8).abs() < f64::EPSILON);
        assert!((config.thresholds.red_at - 1.0).abs() < f64::EPSILON);
        assert_eq!(config.thresholds.stale_after, SignedDuration::from_mins(15));
    }

    #[test]
    fn all_keys_are_read() {
        let config = parse(
            "yellow_at = 0.5\nred_at = 1.2\nstale_after = \"1h 30m\"\n[providers]\nclaude = false\n",
        )
        .unwrap();
        assert!((config.thresholds.yellow_at - 0.5).abs() < f64::EPSILON);
        assert!((config.thresholds.red_at - 1.2).abs() < f64::EPSILON);
        assert_eq!(config.thresholds.stale_after, SignedDuration::from_mins(90));
        assert_eq!(config.claude, Enabled::Off);
    }

    #[test]
    fn claude_accepts_auto_true_and_false() {
        for (value, expected) in [
            ("\"auto\"", Enabled::Auto),
            ("true", Enabled::On),
            ("false", Enabled::Off),
        ] {
            let config = parse(&format!("[providers]\nclaude = {value}\n")).unwrap();
            assert_eq!(config.claude, expected, "{value}");
        }
    }

    #[test]
    fn other_claude_value_is_rejected() {
        let err = parse("[providers]\nclaude = \"yes\"\n").unwrap_err();
        assert_eq!(
            err,
            "providers.claude = \"yes\" must be \"auto\", true or false"
        );
        assert!(parse("[providers]\nclaude = 1\n").is_err());
    }

    #[test]
    fn unknown_key_is_rejected_with_its_name() {
        let err = parse("yelow_at = 0.5\n").unwrap_err();
        assert!(err.contains("yelow_at"), "{err}");
        let err = parse("[providers]\ncopilot = true\n").unwrap_err();
        assert!(err.contains("copilot"), "{err}");
    }

    #[test]
    fn invalid_thresholds_are_rejected() {
        assert!(parse("yellow_at = 0\n").unwrap_err().contains("yellow_at"));
        assert!(parse("yellow_at = -1.0\n").is_err());
        assert!(parse("yellow_at = nan\n").is_err());
        assert!(parse("red_at = 0.5\n").unwrap_err().contains("red_at"));
        assert!(parse("yellow_at = \"high\"\n").is_err());
    }

    #[test]
    fn equal_thresholds_are_allowed() {
        assert!(parse("yellow_at = 1.0\nred_at = 1.0\n").is_ok());
    }

    #[test]
    fn invalid_stale_after_is_rejected() {
        assert!(
            parse("stale_after = \"soon\"\n")
                .unwrap_err()
                .contains("stale_after")
        );
        assert!(
            parse("stale_after = \"0s\"\n")
                .unwrap_err()
                .contains("positive")
        );
        assert!(parse("stale_after = \"-5m\"\n").is_err());
        assert!(parse("stale_after = 15\n").is_err());
    }

    #[test]
    fn broken_toml_is_rejected() {
        assert!(parse("yellow_at = \n").is_err());
    }
}
