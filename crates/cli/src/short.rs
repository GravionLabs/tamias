//! `tamias today --short`: one compact line for status lines and plugins.
//!
//! One segment per set-up provider, binding window only: `C 62%` (Claude seven-day used %),
//! `CP 41%` (Copilot monthly used %), joined by ` · `. A stale provider (or one without data
//! yet) is `C ?`; an unavailable provider is left out.

use tamias_core::budget::Colour;
use tamias_core::snapshot::{Provider, Unit, Window};

use crate::today::{self, Line, State};

/// Separator between segments.
const SEPARATOR: &str = " · ";

/// Renders the short line of `lines`, without a trailing newline; `colour` adds ANSI colours
/// to each segment. Empty when no provider is set up.
#[must_use]
pub fn render(lines: &[Line], colour: bool) -> String {
    lines
        .iter()
        .filter(|line| line.window == Some(binding_window(line.provider)))
        .filter_map(|line| segment(line, colour))
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

/// The window whose budget is binding for `provider`.
fn binding_window(provider: Provider) -> Window {
    match provider {
        Provider::Claude => Window::SevenDay,
        Provider::Copilot => Window::Monthly,
    }
}

fn label(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "C",
        Provider::Copilot => "CP",
    }
}

fn segment(line: &Line, colour: bool) -> Option<String> {
    let label = label(line.provider);
    let (text, ansi) = match &line.state {
        State::Unavailable(_) => return None,
        State::Stale { .. } => (format!("{label} ?"), Some(Colour::Grey)),
        State::NoData => (format!("{label} ?"), None),
        State::Colour(c) => match used_percent(line) {
            Some(percent) => (format!("{label} {percent:.0}%"), Some(*c)),
            None => (format!("{label} ?"), Some(*c)),
        },
    };
    Some(match ansi.filter(|_| colour) {
        Some(c) => format!("\x1b[{}m{text}\x1b[0m", today::ansi(c)),
        None => text,
    })
}

/// Used share of the window in percent.
fn used_percent(line: &Line) -> Option<f64> {
    let used = line.used?;
    match line.unit {
        Unit::Percent => Some(used),
        Unit::Requests => line
            .limit
            .filter(|&limit| limit > 0.0)
            .map(|limit| used / limit * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use tamias_core::budget::Colour;
    use tamias_core::snapshot::{Provider, Unit, Window};

    use super::render;
    use crate::claude::Unavailable;
    use crate::today::{Line, State, unavailable};

    fn claude(window: Window, used: f64, state: State) -> Line {
        Line {
            provider: Provider::Claude,
            window: Some(window),
            unit: Unit::Percent,
            used: Some(used),
            limit: Some(100.0),
            usage: None,
            budget: None,
            resets_at: None,
            state,
        }
    }

    fn copilot(used: f64, limit: f64, state: State) -> Line {
        Line {
            provider: Provider::Copilot,
            window: Some(Window::Monthly),
            unit: Unit::Requests,
            used: Some(used),
            limit: Some(limit),
            usage: None,
            budget: None,
            resets_at: None,
            state,
        }
    }

    fn stale() -> State {
        State::Stale {
            since: None,
            error: Some("network".to_owned()),
        }
    }

    fn both_ok() -> Vec<Line> {
        vec![
            claude(Window::SevenDay, 62.0, State::Colour(Colour::Yellow)),
            claude(Window::FiveHour, 12.0, State::Colour(Colour::Green)),
            copilot(123.0, 300.0, State::Colour(Colour::Green)),
        ]
    }

    #[test]
    fn both_providers_ok_show_the_binding_windows() {
        assert_eq!(render(&both_ok(), false), "C 62% · CP 41%");
    }

    #[test]
    fn both_providers_ok_with_colour() {
        assert_eq!(
            render(&both_ok(), true),
            "\x1b[33mC 62%\x1b[0m · \x1b[32mCP 41%\x1b[0m"
        );
    }

    #[test]
    fn stale_provider_is_a_grey_question_mark() {
        let lines = vec![
            claude(Window::SevenDay, 62.0, stale()),
            claude(Window::FiveHour, 12.0, stale()),
            copilot(123.0, 300.0, State::Colour(Colour::Red)),
        ];
        assert_eq!(render(&lines, false), "C ? · CP 41%");
        assert_eq!(
            render(&lines, true),
            "\x1b[90mC ?\x1b[0m · \x1b[31mCP 41%\x1b[0m"
        );
    }

    #[test]
    fn unavailable_provider_is_left_out() {
        let lines = vec![
            unavailable(Provider::Claude, Unavailable::NotSetUp),
            copilot(123.0, 300.0, State::Colour(Colour::Green)),
        ];
        assert_eq!(render(&lines, true), "\x1b[32mCP 41%\x1b[0m");
    }

    #[test]
    fn nothing_set_up_is_an_empty_line() {
        let lines = vec![unavailable(Provider::Claude, Unavailable::Disabled)];
        assert_eq!(render(&lines, true), "");
        assert_eq!(render(&[], false), "");
    }

    #[test]
    fn without_colour_there_are_no_escape_codes() {
        let line = render(&both_ok(), false);
        assert!(!line.contains('\x1b'), "{line:?}");
    }

    #[test]
    fn no_data_yet_is_an_uncoloured_question_mark() {
        let mut line = claude(Window::SevenDay, 0.0, State::NoData);
        line.used = None;
        assert_eq!(render(&[line], true), "C ?");
    }

    #[test]
    fn copilot_with_zero_limit_has_no_percent() {
        let lines = vec![copilot(5.0, 0.0, State::Colour(Colour::Green))];
        assert_eq!(render(&lines, false), "CP ?");
    }
}
