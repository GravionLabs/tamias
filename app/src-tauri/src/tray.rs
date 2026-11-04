//! The tray icon: the traffic light of today's [`Report`] and its short summary; the app lives
//! in the tray.
//!
//! - The icon colour is the worst colour over the binding windows of the set-up providers
//!   ([`tray_state`]); the summary is the line of `tamias today --short` (`C 62% · CP 41%`).
//! - The summary is the tooltip (Windows, macOS) and the first, disabled menu item, because
//!   Linux (libappindicator) shows no tooltips. The menu: summary, "Open tamias", "Quit".
//! - A left click (Windows, macOS) or "Open tamias" shows and focuses the window; closing the
//!   window hides it ([`hide_window`]); "Quit" exits the app. On macOS the dock icon is hidden
//!   with the window (`ActivationPolicy::Accessory`).
//! - [`refresh`] reads the report on start and on every `store-changed`. Only a complete report
//!   changes the state; a failing read keeps the last summary, turns the icon grey and appends
//!   the error ([`TrayState::next`]).

use std::sync::{Mutex, PoisonError};

use tamias_core::budget::Colour;
use tamias_core::report::{Report, Status, short};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::{CommandError, today};

/// Label of the main window (`tauri.conf.json`).
pub const MAIN_WINDOW: &str = "main";

/// Summary when no provider is set up.
pub const NOTHING_SET_UP: &str = "No provider set up";

/// Summary until the first report is read.
pub const STARTING: &str = "Reading usage…";

/// Id of the "Open tamias" menu item.
const OPEN: &str = "open";
/// Id of the "Quit" menu item.
const QUIT: &str = "quit";

/// Colour of the tray icon, one icon each in `icons/tray/` (made by `icons/tray/generate.mjs`).
///
/// | State                                     | Colour     | Icon          |
/// |-------------------------------------------|------------|---------------|
/// | no provider set up                        | `Neutral`  | hollow ring   |
/// | all binding windows green                 | `Green`    | green disc    |
/// | worst binding window yellow               | `Yellow`   | yellow disc   |
/// | worst binding window red                  | `Red`      | red disc      |
/// | a provider stale, without data, or failed | `Grey`     | grey disc     |
///
/// Grey wins over red: a stale provider's numbers may be outdated, so the light is not trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrayColour {
    /// No provider is set up.
    Neutral,
    /// Every binding window is green.
    Green,
    /// The worst binding window is yellow.
    Yellow,
    /// The worst binding window is red.
    Red,
    /// A set-up provider is stale or has no data of its binding window, or the read failed.
    Grey,
}

impl TrayColour {
    /// Every colour, for the icon checks.
    pub const ALL: [Self; 5] = [
        Self::Neutral,
        Self::Green,
        Self::Yellow,
        Self::Red,
        Self::Grey,
    ];

    /// The PNG of the icon (64x64 RGBA, not a macOS template image so the colour shows).
    #[must_use]
    pub fn png(self) -> &'static [u8] {
        match self {
            Self::Neutral => include_bytes!("../icons/tray/neutral.png"),
            Self::Green => include_bytes!("../icons/tray/green.png"),
            Self::Yellow => include_bytes!("../icons/tray/yellow.png"),
            Self::Red => include_bytes!("../icons/tray/red.png"),
            Self::Grey => include_bytes!("../icons/tray/grey.png"),
        }
    }

    /// Colour of a segment of the short line: no data of the binding window is grey (`C ?`).
    fn of_segment(colour: Option<Colour>) -> Self {
        match colour {
            Some(Colour::Green) => Self::Green,
            Some(Colour::Yellow) => Self::Yellow,
            Some(Colour::Red) => Self::Red,
            Some(Colour::Grey) | None => Self::Grey,
        }
    }
}

/// Icon colour and summary of `report`: the worst colour over the binding windows of the set-up
/// providers ([`TrayColour`]) and the line of `tamias today --short`; [`NOTHING_SET_UP`] and
/// [`TrayColour::Neutral`] when no provider is set up.
#[must_use]
pub fn tray_state(report: &Report) -> (TrayColour, String) {
    if report
        .providers
        .iter()
        .all(|p| p.status == Status::Unavailable)
    {
        return (TrayColour::Neutral, NOTHING_SET_UP.to_owned());
    }
    let colour = short::segments(report)
        .into_iter()
        .map(|segment| TrayColour::of_segment(segment.colour))
        .max()
        .unwrap_or(TrayColour::Neutral);
    (colour, short::line(report))
}

/// What the tray shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    /// Colour of the icon.
    pub colour: TrayColour,
    /// Summary of the last complete report (or [`STARTING`]).
    pub summary: String,
    /// Error of the last read, when it failed.
    pub error: Option<String>,
}

impl TrayState {
    /// The state before the first read: grey, [`STARTING`].
    #[must_use]
    pub fn starting() -> Self {
        Self {
            colour: TrayColour::Grey,
            summary: STARTING.to_owned(),
            error: None,
        }
    }

    /// The state of a complete report ([`tray_state`]).
    #[must_use]
    pub fn of_report(report: &Report) -> Self {
        let (colour, summary) = tray_state(report);
        Self {
            colour,
            summary,
            error: None,
        }
    }

    /// The state after a read of the report: a complete report replaces the state; a failed read
    /// keeps the last summary, turns grey and records the error; a busy store (the collector
    /// holds the lock, the next `store-changed` retries) keeps the state as it is.
    #[must_use]
    pub fn next(&self, read: Result<&Report, &CommandError>) -> Self {
        match read {
            Ok(report) => Self::of_report(report),
            Err(error) if error.busy => self.clone(),
            Err(error) => Self {
                colour: TrayColour::Grey,
                summary: self.summary.clone(),
                error: Some(error.message.clone()),
            },
        }
    }

    /// The text of the tooltip and of the summary menu item.
    #[must_use]
    pub fn text(&self) -> String {
        match &self.error {
            Some(error) => format!("{} (error: {error})", self.summary),
            None => self.summary.clone(),
        }
    }
}

/// The tray of the running app, managed state.
struct Tray {
    icon: TrayIcon,
    summary: MenuItem<tauri::Wry>,
    state: Mutex<TrayState>,
}

/// Creates the tray icon in [`TrayState::starting`]; [`refresh`] reads the first report.
///
/// # Errors
///
/// When the menu or the tray icon cannot be created (on Linux without an app indicator library).
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let state = TrayState::starting();
    let summary = MenuItem::with_id(app, "summary", state.text(), false, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "Open tamias", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&summary, &separator, &open, &quit])?;
    let icon = TrayIconBuilder::with_id("tamias")
        .icon(Image::from_bytes(state.colour.png())?)
        .icon_as_template(false)
        .tooltip(state.text())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN => show_window(app),
            QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(Tray {
        icon,
        summary,
        state: Mutex::new(state),
    });
    Ok(())
}

/// Whether the tray icon exists: without it, the window must not hide, there would be no way
/// back.
#[must_use]
pub fn exists(app: &AppHandle) -> bool {
    app.try_state::<Tray>().is_some()
}

/// Reads today's report and updates the icon, tooltip and summary. Blocks on the store and on
/// the main thread: call it off the main thread and off the watcher thread.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.try_state::<Tray>() else {
        return;
    };
    // Held over the read, so two refreshes apply their reports in order.
    let mut state = tray.state.lock().unwrap_or_else(PoisonError::into_inner);
    let read = today::current().map_err(CommandError::from);
    let next = state.next(read.as_ref());
    if next == *state {
        return;
    }
    if let Err(error) = apply(&tray, &state, &next) {
        eprintln!("tamias-app: tray: cannot update: {error}");
        return;
    }
    *state = next;
}

fn apply(tray: &Tray, current: &TrayState, next: &TrayState) -> tauri::Result<()> {
    if next.colour != current.colour {
        tray.icon
            .set_icon(Some(Image::from_bytes(next.colour.png())?))?;
    }
    let text = next.text();
    tray.icon.set_tooltip(Some(&text))?;
    tray.summary.set_text(&text)
}

/// Shows and focuses the main window; on macOS the dock icon comes back with it.
pub fn show_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
        eprintln!("tamias-app: cannot show the dock icon: {error}");
    }
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let shown = window
        .show()
        .and_then(|()| window.unminimize())
        .and_then(|()| window.set_focus());
    if let Err(error) = shown {
        eprintln!("tamias-app: cannot show the window: {error}");
    }
}

/// Hides the main window to the tray; on macOS the dock icon goes with it.
pub fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW)
        && let Err(error) = window.hide()
    {
        eprintln!("tamias-app: cannot hide the window: {error}");
    }
    hide_dock_icon(app);
}

/// On macOS, hides the dock icon (`ActivationPolicy::Accessory`) while the window is hidden.
pub fn hide_dock_icon(
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))] app: &AppHandle,
) {
    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Accessory) {
        eprintln!("tamias-app: cannot hide the dock icon: {error}");
    }
}
