//! `tamias today --short`: one compact line for status lines and plugins.
//!
//! The segments come from [`tamias_core::report::short`] (the tray shows the same line): one
//! per set-up provider, binding window only, `C 62% · CP 41%`; a stale provider (or one without
//! data yet) is `C ?`; an unavailable provider is left out. This module adds the ANSI colours.

use tamias_core::report::Report;
use tamias_core::report::short::{self, SEPARATOR};

use crate::today;

/// Renders the short line of `report`, without a trailing newline; `colour` adds ANSI colours
/// to each segment. Empty when no provider is set up.
#[must_use]
pub fn render(report: &Report, colour: bool) -> String {
    short::segments(report)
        .into_iter()
        .map(|segment| match segment.colour.filter(|_| colour) {
            Some(c) => format!("\x1b[{}m{}\x1b[0m", today::ansi(c), segment.text),
            None => segment.text,
        })
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tamias_core::report::Report;

    use super::render;

    /// A report document of `providers` (the JSON of `tamias today --json`).
    fn report(providers: &serde_json::Value) -> Report {
        Report::parse(
            &json!({
                "schema_version": 1,
                "generated_at": "2026-11-03T10:05:00Z",
                "providers": providers,
            })
            .to_string(),
        )
        .unwrap()
    }

    fn window(window: &str, unit: &str, used: f64, limit: f64, colour: &str) -> serde_json::Value {
        json!({
            "window": window,
            "binding": window != "five_hour",
            "unit": unit,
            "used": used,
            "limit": limit,
            "resets_at": "2026-11-07T12:00:00Z",
            "colour": colour,
        })
    }

    fn claude(status: &str, colour: &str) -> serde_json::Value {
        json!({
            "provider": "claude",
            "status": status,
            "windows": [
                window("seven_day", "percent", 62.0, 100.0, colour),
                window("five_hour", "percent", 12.0, 100.0, colour),
            ],
        })
    }

    fn copilot(colour: &str) -> serde_json::Value {
        json!({
            "provider": "copilot",
            "status": "ok",
            "windows": [window("monthly", "requests", 123.0, 300.0, colour)],
        })
    }

    #[test]
    fn both_providers_ok_without_colour() {
        let report = report(&json!([claude("ok", "yellow"), copilot("green")]));
        assert_eq!(render(&report, false), "C 62% · CP 41%");
    }

    #[test]
    fn both_providers_ok_with_colour() {
        let report = report(&json!([claude("ok", "yellow"), copilot("green")]));
        assert_eq!(
            render(&report, true),
            "\x1b[33mC 62%\x1b[0m · \x1b[32mCP 41%\x1b[0m"
        );
    }

    #[test]
    fn stale_provider_is_a_grey_question_mark() {
        let report = report(&json!([claude("stale", "grey"), copilot("red")]));
        assert_eq!(render(&report, false), "C ? · CP 41%");
        assert_eq!(
            render(&report, true),
            "\x1b[90mC ?\x1b[0m · \x1b[31mCP 41%\x1b[0m"
        );
    }

    #[test]
    fn unavailable_provider_is_left_out() {
        let report = report(&json!([
            { "provider": "claude", "status": "unavailable", "windows": [] },
            copilot("green"),
        ]));
        assert_eq!(render(&report, true), "\x1b[32mCP 41%\x1b[0m");
    }

    #[test]
    fn nothing_set_up_is_an_empty_line() {
        let report = report(&json!([
            { "provider": "claude", "status": "unavailable", "windows": [] },
        ]));
        assert_eq!(render(&report, true), "");
    }

    #[test]
    fn no_data_yet_is_an_uncoloured_question_mark() {
        let report = report(&json!([{ "provider": "claude", "status": "ok", "windows": [] }]));
        assert_eq!(render(&report, true), "C ?");
    }
}
