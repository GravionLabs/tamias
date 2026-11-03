//! Snapshot of one usage window as observed by one source at one instant.
//!
//! Shared by the store, the budget derivation and the providers.

use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// Usage provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Anthropic Claude (subscription limits).
    Claude,
    /// GitHub Copilot (premium requests).
    Copilot,
}

/// Usage window of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    /// Claude five-hour window (informational).
    FiveHour,
    /// Claude seven-day window (binding).
    SevenDay,
    /// Copilot monthly premium-request quota (binding).
    Monthly,
}

/// Unit of `used` and `limit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Percent of the limit (0 to 100).
    Percent,
    /// Number of requests.
    Requests,
}

/// Where a snapshot came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// The collector polled the provider.
    Poll,
    /// The Claude Code status line handed it over.
    Statusline,
}

/// One observation of one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Provider the snapshot is of.
    pub provider: Provider,
    /// Window the snapshot is of.
    pub window: Window,
    /// Used amount in `unit` (0..=limit for percent; may exceed `limit` for requests).
    pub used: f64,
    /// Limit in `unit` (100 for percent).
    pub limit: f64,
    /// Unit of the usage.
    pub unit: Unit,
    /// End of the window as reported by the source (not rounded).
    pub resets_at: Timestamp,
    /// When the source was read.
    pub observed_at: Timestamp,
    /// Where the snapshot came from.
    pub source: Source,
}

/// Error of parsing one of the enums from its stable string form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: &'static str,
    value: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {}: {:?}", self.kind, self.value)
    }
}

impl std::error::Error for ParseError {}

macro_rules! stable_str {
    ($ty:ident, $kind:literal, { $($variant:ident => $s:literal),+ $(,)? }) => {
        impl $ty {
            /// Stable string form, used in the store and in JSON output.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $($ty::$variant => $s),+ }
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $ty {
            type Err = ParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok($ty::$variant),)+
                    other => Err(ParseError { kind: $kind, value: other.to_owned() }),
                }
            }
        }
    };
}

stable_str!(Provider, "provider", { Claude => "claude", Copilot => "copilot" });
stable_str!(Window, "window", { FiveHour => "five_hour", SevenDay => "seven_day", Monthly => "monthly" });
stable_str!(Unit, "unit", { Percent => "percent", Requests => "requests" });
stable_str!(Source, "source", { Poll => "poll", Statusline => "statusline" });

#[cfg(test)]
mod tests {
    use super::{Provider, Source, Unit, Window};

    #[test]
    fn stable_strings_round_trip() {
        for (p, s) in [(Provider::Claude, "claude"), (Provider::Copilot, "copilot")] {
            assert_eq!(p.as_str(), s);
            assert_eq!(s.parse::<Provider>(), Ok(p));
        }
        for (w, s) in [
            (Window::FiveHour, "five_hour"),
            (Window::SevenDay, "seven_day"),
            (Window::Monthly, "monthly"),
        ] {
            assert_eq!(w.as_str(), s);
            assert_eq!(s.parse::<Window>(), Ok(w));
        }
        for (u, s) in [(Unit::Percent, "percent"), (Unit::Requests, "requests")] {
            assert_eq!(u.as_str(), s);
            assert_eq!(s.parse::<Unit>(), Ok(u));
        }
        for (src, s) in [(Source::Poll, "poll"), (Source::Statusline, "statusline")] {
            assert_eq!(src.as_str(), s);
            assert_eq!(s.parse::<Source>(), Ok(src));
        }
    }

    #[test]
    fn unknown_string_is_rejected_with_its_kind() {
        let err = "gemini".parse::<Provider>().unwrap_err();
        assert_eq!(err.to_string(), "unknown provider: \"gemini\"");
        assert!("".parse::<Window>().is_err());
        assert!("Percent".parse::<Unit>().is_err());
    }
}
