//! The short form of a [`Report`]: one compact line for status lines, plugins and the tray, the
//! format of `tamias today --short`.
//!
//! One segment per set-up provider, binding window only: `C 62%` (Claude seven-day used %),
//! `CP 41%` (Copilot monthly used %), joined by [`SEPARATOR`]. A stale provider (or one without
//! data of its binding window yet) is `C ?`; an `unavailable` provider is left out.

use crate::budget::Colour;
use crate::snapshot::{self, Unit};

use super::{Report, Status, Window, windows};

/// Separator between segments.
pub const SEPARATOR: &str = " · ";

/// One provider of the short line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Which provider.
    pub provider: snapshot::Provider,
    /// `C 62%`, `CP 41%`, `C ?`.
    pub text: String,
    /// Colour of the segment: the colour of the binding window, [`Colour::Grey`] when stale,
    /// `None` when the binding window has no data yet.
    pub colour: Option<Colour>,
}

/// Label of `provider` in the short line.
#[must_use]
pub fn label(provider: snapshot::Provider) -> &'static str {
    match provider {
        snapshot::Provider::Claude => "C",
        snapshot::Provider::Copilot => "CP",
    }
}

/// The segments of `report`, in the order of its providers; empty when no provider is set up.
#[must_use]
pub fn segments(report: &Report) -> Vec<Segment> {
    report
        .providers
        .iter()
        .filter_map(|provider| {
            let label = label(provider.provider);
            let (text, colour) = match provider.status {
                Status::Unavailable => return None,
                Status::Stale => (format!("{label} ?"), Some(Colour::Grey)),
                Status::Ok => match binding(provider) {
                    None => (format!("{label} ?"), None),
                    Some(window) => match used_percent(window) {
                        Some(percent) => (format!("{label} {percent:.0}%"), Some(window.colour)),
                        None => (format!("{label} ?"), Some(window.colour)),
                    },
                },
            };
            Some(Segment {
                provider: provider.provider,
                text,
                colour,
            })
        })
        .collect()
}

/// The short line of `report` without colours, without a trailing newline; empty when no
/// provider is set up.
#[must_use]
pub fn line(report: &Report) -> String {
    segments(report)
        .into_iter()
        .map(|segment| segment.text)
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

/// The binding window of `provider` when it has data.
fn binding(provider: &super::Provider) -> Option<&Window> {
    let window = windows(provider.provider)[0];
    provider.windows.iter().find(|w| w.window == window)
}

/// Used share of the window in percent; `None` for a request window without a limit.
fn used_percent(window: &Window) -> Option<f64> {
    match window.unit {
        Unit::Percent => Some(window.used),
        Unit::Requests => (window.limit > 0.0).then(|| window.used / window.limit * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::{Segment, line, segments};
    use crate::budget::Colour;
    use crate::report::{Provider, Report, Status, Window, unavailable};
    use crate::snapshot::{self, Unit};

    fn now() -> Timestamp {
        "2026-11-03T10:05:00Z".parse().unwrap()
    }

    fn window(window: snapshot::Window, used: f64, limit: f64, colour: Colour) -> Window {
        Window {
            window,
            binding: window != snapshot::Window::FiveHour,
            unit: if window == snapshot::Window::Monthly {
                Unit::Requests
            } else {
                Unit::Percent
            },
            used,
            limit,
            resets_at: now(),
            colour,
            today_usage: None,
            budget: None,
            ratio: None,
            month_usage: None,
        }
    }

    fn provider(provider: snapshot::Provider, status: Status, windows: Vec<Window>) -> Provider {
        Provider {
            provider,
            status,
            stale_since: None,
            windows,
        }
    }

    /// Claude seven-day 62 % yellow, five-hour 12 % green.
    fn claude(status: Status) -> Provider {
        let colour = |c| {
            if status == Status::Stale {
                Colour::Grey
            } else {
                c
            }
        };
        provider(
            snapshot::Provider::Claude,
            status,
            vec![
                window(
                    snapshot::Window::SevenDay,
                    62.0,
                    100.0,
                    colour(Colour::Yellow),
                ),
                window(
                    snapshot::Window::FiveHour,
                    12.0,
                    100.0,
                    colour(Colour::Green),
                ),
            ],
        )
    }

    /// Copilot 123 of 300 requests.
    fn copilot(colour: Colour) -> Provider {
        provider(
            snapshot::Provider::Copilot,
            Status::Ok,
            vec![window(snapshot::Window::Monthly, 123.0, 300.0, colour)],
        )
    }

    fn report(providers: Vec<Provider>) -> Report {
        Report::new(now(), providers)
    }

    #[test]
    fn both_providers_ok_show_the_binding_windows() {
        let report = report(vec![claude(Status::Ok), copilot(Colour::Green)]);
        assert_eq!(line(&report), "C 62% · CP 41%");
        assert_eq!(
            segments(&report),
            vec![
                Segment {
                    provider: snapshot::Provider::Claude,
                    text: "C 62%".to_owned(),
                    colour: Some(Colour::Yellow),
                },
                Segment {
                    provider: snapshot::Provider::Copilot,
                    text: "CP 41%".to_owned(),
                    colour: Some(Colour::Green),
                },
            ]
        );
    }

    #[test]
    fn stale_provider_is_a_grey_question_mark() {
        let report = report(vec![claude(Status::Stale), copilot(Colour::Red)]);
        assert_eq!(line(&report), "C ? · CP 41%");
        assert_eq!(segments(&report)[0].colour, Some(Colour::Grey));
        assert_eq!(segments(&report)[1].colour, Some(Colour::Red));
    }

    #[test]
    fn stale_provider_without_any_window_is_a_question_mark() {
        let report = report(vec![provider(
            snapshot::Provider::Claude,
            Status::Stale,
            Vec::new(),
        )]);
        assert_eq!(line(&report), "C ?");
    }

    #[test]
    fn unavailable_provider_is_left_out() {
        let report = report(vec![
            unavailable(snapshot::Provider::Claude),
            copilot(Colour::Green),
        ]);
        assert_eq!(line(&report), "CP 41%");
    }

    #[test]
    fn nothing_set_up_is_an_empty_line() {
        let report = report(vec![
            unavailable(snapshot::Provider::Claude),
            unavailable(snapshot::Provider::Copilot),
        ]);
        assert_eq!(line(&report), "");
        assert!(segments(&report).is_empty());
        assert_eq!(line(&Report::new(now(), Vec::new())), "");
    }

    #[test]
    fn no_data_of_the_binding_window_is_an_uncoloured_question_mark() {
        // Only the informational five-hour window has data.
        let report = report(vec![provider(
            snapshot::Provider::Claude,
            Status::Ok,
            vec![window(
                snapshot::Window::FiveHour,
                12.0,
                100.0,
                Colour::Green,
            )],
        )]);
        assert_eq!(
            segments(&report),
            vec![Segment {
                provider: snapshot::Provider::Claude,
                text: "C ?".to_owned(),
                colour: None,
            }]
        );
    }

    #[test]
    fn copilot_with_zero_limit_has_no_percent() {
        let report = report(vec![provider(
            snapshot::Provider::Copilot,
            Status::Ok,
            vec![window(snapshot::Window::Monthly, 5.0, 0.0, Colour::Green)],
        )]);
        assert_eq!(line(&report), "CP ?");
        assert_eq!(segments(&report)[0].colour, Some(Colour::Green));
    }

    #[test]
    fn percent_is_rounded_to_whole_numbers() {
        let report = report(vec![provider(
            snapshot::Provider::Claude,
            Status::Ok,
            vec![window(
                snapshot::Window::SevenDay,
                61.6,
                100.0,
                Colour::Green,
            )],
        )]);
        assert_eq!(line(&report), "C 62%");
    }
}
