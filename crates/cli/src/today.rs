//! `tamias today`: today's usage against today's budget, one line per provider window.
//!
//! The binding window (Claude `seven_day`) shows used %, today's usage, today's budget and the
//! colour; the informational window (Claude `five_hour`) shows used % and the reset time only.
//! [`read`] gathers the store once into a [`Today`]: `--json` prints its [`Report`], `--short`
//! renders the same report, the table renders its [`Line`]s, which are derived from that report,
//! so all three show the same numbers.

use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_core::budget::{Colour, Thresholds};
use tamias_core::report::{self, Report};
use tamias_core::snapshot::{Provider, Unit, Window};
use tamias_core::store::{self, SourceStatus, Store};

use crate::claude::{self, Unavailable};
use tamias_core::config::Config;

/// State shown in the last column.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Colour(Colour),
    /// The last collect failed or the last success is older than `stale_after`.
    Stale {
        since: Option<Timestamp>,
        error: Option<String>,
    },
    Unavailable(Unavailable),
    /// Collected, but no snapshot of this window yet.
    NoData,
}

/// One line of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub provider: Provider,
    pub window: Option<Window>,
    pub unit: Unit,
    pub used: Option<f64>,
    /// Limit of the window (100 for percent windows, the quota for request windows).
    pub limit: Option<f64>,
    pub usage: Option<f64>,
    pub budget: Option<f64>,
    pub resets_at: Option<Timestamp>,
    pub state: State,
}

/// Everything `today` shows, gathered once at one `now`.
#[derive(Debug, Clone, PartialEq)]
pub struct Today {
    /// The document of `--json` and the source of `--short`.
    pub report: Report,
    /// The rows of the table, derived from `report`.
    pub lines: Vec<Line>,
}

impl Today {
    /// Whether at least one set-up provider is stale (exit code 3 of `today`, every mode).
    #[must_use]
    pub fn any_stale(&self) -> bool {
        any_stale(&self.report)
    }
}

/// Reads the store once and derives the report and the lines of every provider at `now`.
///
/// # Errors
///
/// Store errors.
pub fn read(
    config: &Config,
    store: &Store,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<Today, store::Error> {
    let mut providers = Vec::new();
    let mut lines = Vec::new();
    for entry in gather(config, store, now, tz)? {
        match entry {
            Entry::Unavailable(provider, why) => {
                providers.push(report::unavailable(provider));
                lines.push(unavailable(provider, why));
            }
            Entry::Read(report, status) => {
                lines.extend(provider_lines(
                    &report,
                    status.as_ref(),
                    now,
                    &config.thresholds,
                ));
                providers.push(report);
            }
        }
    }
    Ok(Today {
        report: Report::new(now, providers),
        lines,
    })
}

/// Whether at least one provider of `report` is stale, as decided by [`report::stale`].
#[must_use]
pub fn any_stale(report: &Report) -> bool {
    report
        .providers
        .iter()
        .any(|p| p.status == report::Status::Stale)
}

/// `report` as pretty JSON (schema `docs/schema/today-v1.schema.json`), ending with a newline.
/// Never coloured.
///
/// # Errors
///
/// Serialization errors.
pub fn json(report: &Report) -> serde_json::Result<String> {
    let mut out = serde_json::to_string_pretty(report)?;
    out.push('\n');
    Ok(out)
}

/// One provider as read for `today`: the table and the JSON are both built from it.
enum Entry {
    Unavailable(Provider, Unavailable),
    Read(report::Provider, Option<SourceStatus>),
}

/// Claude (unless unavailable), then Copilot when the store has anything of it.
fn gather(
    config: &Config,
    store: &Store,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<Vec<Entry>, store::Error> {
    let mut entries = Vec::new();
    if let Some(why) = claude::availability(config.claude) {
        entries.push(Entry::Unavailable(Provider::Claude, why));
    } else {
        entries.push(read_provider(
            store,
            Provider::Claude,
            now,
            tz,
            &config.thresholds,
        )?);
    }
    // No Copilot provider yet (#5): shown only when seeded into the store.
    let copilot = read_provider(store, Provider::Copilot, now, tz, &config.thresholds)?;
    if let Entry::Read(report, status) = &copilot
        && (status.is_some() || !report.windows.is_empty())
    {
        entries.push(copilot);
    }
    Ok(entries)
}

fn read_provider(
    store: &Store,
    provider: Provider,
    now: Timestamp,
    tz: &TimeZone,
    thresholds: &Thresholds,
) -> Result<Entry, store::Error> {
    let report = report::read(store, provider, now, tz, thresholds)?;
    Ok(Entry::Read(report, store.status(provider)?))
}

/// Line of an `unavailable` provider.
#[must_use]
pub fn unavailable(provider: Provider, why: Unavailable) -> Line {
    Line {
        provider,
        window: None,
        unit: Unit::Percent,
        used: None,
        limit: None,
        usage: None,
        budget: None,
        resets_at: None,
        state: State::Unavailable(why),
    }
}

/// Lines of Claude: `seven_day` with today's budget, `five_hour` with used % only.
#[cfg(test)]
#[must_use]
pub fn claude_lines(
    snapshots: &[tamias_core::snapshot::Snapshot],
    status: Option<&SourceStatus>,
    now: Timestamp,
    tz: &TimeZone,
    thresholds: &Thresholds,
) -> Vec<Line> {
    let report = report::provider(Provider::Claude, snapshots, status, now, tz, thresholds);
    provider_lines(&report, status, now, thresholds)
}

/// One line per window of the provider of `report`, from the same numbers as the JSON.
fn provider_lines(
    report: &report::Provider,
    status: Option<&SourceStatus>,
    now: Timestamp,
    thresholds: &Thresholds,
) -> Vec<Line> {
    // The same check that made `report.status` stale, for the error text of the table.
    let stale_state = report::stale(status, now, thresholds).map(|s| State::Stale {
        since: s.since,
        error: s.error,
    });
    report::windows(report.provider)
        .iter()
        .map(|&window| {
            let data = report.windows.iter().find(|w| w.window == window);
            let state = match (&stale_state, data) {
                (Some(stale), _) => stale.clone(),
                (None, Some(w)) => State::Colour(w.colour),
                (None, None) => State::NoData,
            };
            Line {
                provider: report.provider,
                window: Some(window),
                unit: data.map_or_else(|| report::unit(window), |w| w.unit),
                used: data.map(|w| w.used),
                limit: data.map(|w| w.limit),
                usage: data.and_then(|w| w.today_usage),
                budget: data.and_then(|w| w.budget),
                resets_at: data.map(|w| w.resets_at).filter(|&at| at > now),
                state,
            }
        })
        .collect()
}

const HEADER: [&str; 7] = [
    "PROVIDER", "WINDOW", "USED", "TODAY", "BUDGET", "RESETS", "STATUS",
];

/// Renders `lines` as a table; `colour` adds ANSI colours to the status column.
#[must_use]
pub fn render(lines: &[Line], now: Timestamp, tz: &TimeZone, colour: bool) -> String {
    let rows: Vec<([String; 7], Option<&str>)> = lines
        .iter()
        .map(|line| {
            let (status, ansi) = status_cell(&line.state, now, tz);
            let cells = [
                line.provider.to_string(),
                line.window.map_or_else(dash, |w| w.to_string()),
                line.used.map_or_else(dash, |v| amount(v, line.unit, 0)),
                line.usage.map_or_else(dash, |v| amount(v, line.unit, 1)),
                line.budget.map_or_else(dash, |v| amount(v, line.unit, 1)),
                line.resets_at.map_or_else(dash, |t| local_time(t, now, tz)),
                status,
            ];
            (cells, ansi)
        })
        .collect();
    let mut widths = HEADER.map(str::len);
    for (cells, _) in &rows {
        for (width, cell) in widths.iter_mut().zip(cells) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    push_row(&mut out, &HEADER.map(str::to_owned), &widths, None);
    for (cells, ansi) in &rows {
        push_row(&mut out, cells, &widths, ansi.filter(|_| colour));
    }
    out
}

fn push_row(out: &mut String, cells: &[String; 7], widths: &[usize; 7], ansi: Option<&str>) {
    let last = cells.len() - 1;
    for (i, cell) in cells.iter().enumerate() {
        if i == last {
            match ansi {
                Some(code) => {
                    let _ = write!(out, "\x1b[{code}m{cell}\x1b[0m");
                }
                None => out.push_str(cell),
            }
        } else {
            let _ = write!(out, "{cell:<width$}  ", width = widths[i]);
        }
    }
    out.push('\n');
}

fn dash() -> String {
    "-".to_owned()
}

fn amount(value: f64, unit: Unit, decimals: usize) -> String {
    match unit {
        Unit::Percent => format!("{value:.decimals$}%"),
        Unit::Requests => format!("{value:.decimals$}"),
    }
}

/// Status text and its ANSI colour code.
fn status_cell(state: &State, now: Timestamp, tz: &TimeZone) -> (String, Option<&'static str>) {
    match state {
        State::Colour(colour) => (colour_name(*colour).to_owned(), Some(ansi(*colour))),
        State::Stale { since, error } => {
            let mut text = match since {
                Some(at) => format!("stale since {}", local_time(*at, now, tz)),
                None => "stale, never collected".to_owned(),
            };
            if let Some(error) = error {
                let _ = write!(text, " ({error})");
            }
            (text, Some(ansi(Colour::Grey)))
        }
        State::Unavailable(why) => (
            format!("unavailable ({})", why.reason()),
            Some(ansi(Colour::Grey)),
        ),
        State::NoData => ("no data".to_owned(), None),
    }
}

/// ANSI SGR code of `colour`.
#[must_use]
pub fn ansi(colour: Colour) -> &'static str {
    match colour {
        Colour::Green => "32",
        Colour::Yellow => "33",
        Colour::Red => "31",
        Colour::Grey => "90",
    }
}

fn colour_name(colour: Colour) -> &'static str {
    match colour {
        Colour::Green => "green",
        Colour::Yellow => "yellow",
        Colour::Red => "red",
        Colour::Grey => "grey",
    }
}

/// `HH:MM` on the local day of `now`, else `YYYY-MM-DD HH:MM`.
fn local_time(at: Timestamp, now: Timestamp, tz: &TimeZone) -> String {
    let at = at.to_zoned(tz.clone());
    if at.date() == now.to_zoned(tz.clone()).date() {
        at.strftime("%H:%M").to_string()
    } else {
        at.strftime("%Y-%m-%d %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests;
