//! `tamias today`: today's usage against today's budget, one line per provider window.
//!
//! The binding window (Claude `seven_day`) shows used %, today's usage, today's budget and the
//! colour; the informational window (Claude `five_hour`) shows used % and the reset time only.

use std::fmt::Write as _;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use tamias_core::budget::{self, Colour, Status, Thresholds};
use tamias_core::snapshot::{Provider, Snapshot, Unit, Window};
use tamias_core::store::{self, SourceStatus, Store};

use crate::claude::{self, Unavailable};
use crate::config::Config;

/// Snapshots older than this before the start of today are not needed: the binding window is
/// at most seven days long, the margin covers a late reset.
const LOOKBACK: SignedDuration = SignedDuration::from_hours(8 * 24);

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

/// Reads the store and derives the lines of every provider at `now`.
///
/// # Errors
///
/// Store errors.
pub fn lines(
    config: &Config,
    store: &Store,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<Vec<Line>, store::Error> {
    let provider = Provider::Claude;
    if let Some(why) = claude::availability(config.claude) {
        return Ok(vec![unavailable(provider, why)]);
    }
    let start = now
        .to_zoned(tz.clone())
        .start_of_day()
        .map_or(now, |z| z.timestamp());
    let from = start.checked_sub(LOOKBACK).unwrap_or(start);
    let to = now
        .checked_add(SignedDuration::from_hours(48))
        .unwrap_or(now);
    let mut snapshots = Vec::new();
    for window in [Window::SevenDay, Window::FiveHour] {
        snapshots.extend(store.snapshots(provider, window, from, to)?);
    }
    let status = store.status(provider)?;
    Ok(claude_lines(
        &snapshots,
        status.as_ref(),
        now,
        tz,
        &config.thresholds,
    ))
}

/// Whether at least one set-up provider is stale (exit code 3 of `today`).
#[must_use]
pub fn any_stale(lines: &[Line]) -> bool {
    lines
        .iter()
        .any(|line| matches!(line.state, State::Stale { .. }))
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
#[must_use]
pub fn claude_lines(
    snapshots: &[Snapshot],
    status: Option<&SourceStatus>,
    now: Timestamp,
    tz: &TimeZone,
    thresholds: &Thresholds,
) -> Vec<Line> {
    let provider = Provider::Claude;
    let last_ok_at = status.and_then(|s| s.last_ok_at);
    let budget_status = Status { now, last_ok_at };
    let stale_line = stale_state(status, now, thresholds);

    let mut lines = Vec::new();
    for window in [Window::SevenDay, Window::FiveHour] {
        let level = budget::level(snapshots, provider, window, budget_status, thresholds);
        let day = (window == Window::SevenDay)
            .then(|| budget::today(snapshots, provider, window, tz, budget_status, thresholds))
            .flatten();
        let colour = day.map(|d| d.colour).or(level.map(|l| l.colour));
        let state = match (&stale_line, colour) {
            (Some(stale), _) => stale.clone(),
            (None, Some(colour)) => State::Colour(colour),
            (None, None) => State::NoData,
        };
        lines.push(Line {
            provider,
            window: Some(window),
            unit: Unit::Percent,
            used: level.map(|l| l.used),
            limit: level.map(|l| l.limit),
            usage: day.map(|d| d.usage),
            budget: day.map(|d| d.budget),
            resets_at: next_reset(snapshots, provider, window, now),
            state,
        });
    }
    lines
}

/// `Some` when the provider is stale: its last run failed, or its last success is older than
/// `stale_after` (or there was none).
fn stale_state(
    status: Option<&SourceStatus>,
    now: Timestamp,
    thresholds: &Thresholds,
) -> Option<State> {
    let since = status.and_then(|s| s.last_ok_at);
    let failed = status.and_then(|s| {
        let at = s.last_error_at?;
        since
            .is_none_or(|ok| at > ok)
            .then(|| s.last_error.clone().unwrap_or_default())
    });
    let old = since.is_none_or(|ok| now.duration_since(ok) > thresholds.stale_after);
    (failed.is_some() || old).then_some(State::Stale {
        since,
        error: failed.filter(|e| !e.is_empty()),
    })
}

/// `resets_at` of the most recently observed snapshot of `window`, if still ahead.
fn next_reset(
    snapshots: &[Snapshot],
    provider: Provider,
    window: Window,
    now: Timestamp,
) -> Option<Timestamp> {
    snapshots
        .iter()
        .filter(|s| s.provider == provider && s.window == window)
        .max_by_key(|s| s.observed_at)
        .map(|s| s.resets_at)
        .filter(|&at| at > now)
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
