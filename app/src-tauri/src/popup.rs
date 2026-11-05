//! The usage card of the tray: a small borderless window (label [`POPUP_WINDOW`], route `/tray`,
//! `tauri.conf.json`) that hovering the tray icon shows next to the icon.
//!
//! - The window exists hidden from the start, so it already holds the report when it is shown and
//!   updates on `store-changed` like the main view. It has no taskbar/dock entry (`skipTaskbar`).
//! - [`pointer_over_icon`] shows it (without taking focus) and keeps it; when the pointer leaves
//!   the icon it hides after [`HIDE_DELAY`], unless the pointer went into the card
//!   ([`tray_popup_hover`], reported by the frontend), which keeps it open until the pointer
//!   leaves the card.
//! - It also hides on losing focus ([`on_focus_lost`]), on Escape and on "Open tamias" (the
//!   frontend calls [`tray_popup_hide`] and [`tray_popup_open_main`]); closing it hides it too.
//!   After Escape or a focus loss it stays hidden until the pointer has left the icon.
//! - The frontend reports the height of the card ([`tray_popup_fit`]); [`position`] places the
//!   window beside the tray icon, inside the monitor, at any DPI scale.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Label of the popup window (`tauri.conf.json`).
pub const POPUP_WINDOW: &str = "tray";

/// Width of the popup in logical pixels (`tauri.conf.json`).
pub const WIDTH: f64 = 320.0;

/// Height of the popup in logical pixels until the frontend reports the card.
const INITIAL_HEIGHT: f64 = 200.0;

/// Gap between the tray icon and the popup in physical pixels.
const GAP: f64 = 8.0;

/// Margin kept to the edge of the monitor in physical pixels.
const MARGIN: f64 = 8.0;

/// How long the popup stays after the pointer left the icon or the card, to move from one to
/// the other.
pub const HIDE_DELAY: Duration = Duration::from_millis(300);

/// A rectangle in physical pixels of the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    fn centre_x(self) -> f64 {
        self.x + self.width / 2.0
    }

    fn centre_y(self) -> f64 {
        self.y + self.height / 2.0
    }
}

/// Top-left corner (physical pixels) of a popup of `size` (width, height) beside the tray icon
/// `anchor` on `monitor`: centred on the icon, below it when the icon is in the upper half of
/// the monitor (menu bar), above it otherwise (taskbar); clamped into the monitor with a margin,
/// so nothing is clipped at the edge. A popup bigger than the monitor sticks to its top-left.
#[must_use]
pub fn position(anchor: Rect, monitor: Rect, size: (f64, f64)) -> (f64, f64) {
    let (width, height) = size;
    let x = anchor.centre_x() - width / 2.0;
    let y = if anchor.centre_y() < monitor.centre_y() {
        anchor.y + anchor.height + GAP
    } else {
        anchor.y - height - GAP
    };
    (
        clamp(
            x,
            monitor.x + MARGIN,
            monitor.x + monitor.width - width - MARGIN,
        ),
        clamp(
            y,
            monitor.y + MARGIN,
            monitor.y + monitor.height - height - MARGIN,
        ),
    )
}

/// `value` within `low..=high`; `low` wins when the range is empty.
fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.min(high).max(low)
}

/// Whether the hide scheduled as number `scheduled` is still due: nothing was scheduled or
/// cancelled since (`current`), and the pointer is neither over the icon nor over the card.
#[must_use]
pub fn hide_due(scheduled: u64, current: u64, over_icon: bool, over_card: bool) -> bool {
    scheduled == current && !over_icon && !over_card
}

/// Where the popup is and what it was last told, managed state.
struct Popup {
    state: Mutex<PopupState>,
}

struct PopupState {
    /// The tray icon (or the pointer) the popup is placed at; kept to place it again on a resize.
    anchor: Option<Rect>,
    /// Height of the card in logical pixels.
    height: f64,
    /// The pointer is over the tray icon.
    over_icon: bool,
    /// The pointer is over the card.
    over_card: bool,
    /// Hidden by Escape or a focus loss while the pointer is still on the icon.
    dismissed: bool,
    /// Number of the last scheduled or cancelled hide ([`hide_due`]).
    generation: u64,
}

impl PopupState {
    fn new() -> Self {
        Self {
            anchor: None,
            height: INITIAL_HEIGHT,
            over_icon: false,
            over_card: false,
            dismissed: false,
            generation: 0,
        }
    }

    /// An enter or move event of the icon: whether the popup is to be shown now (it is hidden
    /// and not dismissed). Cancels a pending hide.
    fn pointer_over(&mut self, visible: bool) -> bool {
        self.over_icon = true;
        self.generation += 1;
        !self.dismissed && !visible
    }

    /// The pointer left the icon: a dismissed popup may show again on the next enter.
    fn pointer_left(&mut self) {
        self.over_icon = false;
        self.dismissed = false;
    }

    /// Whether the card is wanted after the hide scheduled as number `scheduled` ran: something
    /// happened since, and the pointer is on the icon or the card without a dismissal.
    fn wants_card(&self, scheduled: u64) -> bool {
        self.generation != scheduled && !self.dismissed && (self.over_icon || self.over_card)
    }

    /// The popup is hidden on purpose (Escape, focus loss, click on the icon, menu): it stays
    /// hidden until the pointer has left the icon.
    fn dismiss(&mut self) {
        self.dismissed = true;
        self.over_card = false;
        self.generation += 1;
    }
}

/// Registers the popup state; the window itself is created by `tauri.conf.json`.
pub fn init(app: &AppHandle) {
    app.manage(Popup {
        state: Mutex::new(PopupState::new()),
    });
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(POPUP_WINDOW)
}

fn lock(popup: &Popup) -> std::sync::MutexGuard<'_, PopupState> {
    popup.state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Shows the popup at `anchor` (the tray icon); without one (Linux menu entry) at the pointer.
/// It takes focus only when `focus` (opened from the menu); a hover must not steal it.
pub fn show(app: &AppHandle, anchor: Option<Rect>, focus: bool) {
    let (Some(window), Some(popup)) = (window(app), app.try_state::<Popup>()) else {
        return;
    };
    let anchor = anchor.or_else(|| pointer(app));
    lock(&popup).anchor = anchor;
    place(app, &window, &popup);
    let mut shown = window.show();
    if focus {
        shown = shown.and_then(|()| window.set_focus());
    }
    if let Err(error) = shown {
        eprintln!("tamias-app: cannot show the usage card: {error}");
    }
}

/// The pointer is over the tray icon at `anchor` (an enter or move event): shows the popup
/// unless Escape or a focus loss dismissed it, and cancels a pending hide.
pub fn pointer_over_icon(app: &AppHandle, anchor: Rect) {
    let Some(popup) = app.try_state::<Popup>() else {
        return;
    };
    let visible = window(app).is_some_and(|w| w.is_visible().unwrap_or(false));
    let show_now = lock(&popup).pointer_over(visible);
    if show_now {
        show(app, Some(anchor), false);
    }
}

/// The pointer left the tray icon: hides the popup after [`HIDE_DELAY`] unless the pointer is
/// in the card by then.
pub fn pointer_left_icon(app: &AppHandle) {
    let Some(popup) = app.try_state::<Popup>() else {
        return;
    };
    lock(&popup).pointer_left();
    schedule_hide(app);
}

fn schedule_hide(app: &AppHandle) {
    let Some(popup) = app.try_state::<Popup>() else {
        return;
    };
    let scheduled = {
        let mut state = lock(&popup);
        state.generation += 1;
        state.generation
    };
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(HIDE_DELAY);
        let Some(popup) = handle.try_state::<Popup>() else {
            return;
        };
        let due = {
            let state = lock(&popup);
            hide_due(
                scheduled,
                state.generation,
                state.over_icon,
                state.over_card,
            )
        };
        if !due {
            return;
        }
        // The lock must not be held over `hide`: the window's focus-loss handler takes it on the
        // main thread. So an enter or move in between is caught afterwards: the card is shown
        // again when the pointer came back.
        hide(&handle);
        let back = {
            let state = lock(&popup);
            state.wants_card(scheduled).then_some(state.anchor)
        };
        if let Some(anchor) = back {
            show(&handle, anchor, false);
        }
    });
}

/// The pointer as an empty rectangle, the anchor of a popup opened from a menu.
fn pointer(app: &AppHandle) -> Option<Rect> {
    let at = app.cursor_position().ok()?;
    Some(Rect {
        x: at.x,
        y: at.y,
        width: 0.0,
        height: 0.0,
    })
}

/// Moves and sizes the window for the anchor and the height of the card.
fn place(app: &AppHandle, window: &WebviewWindow, popup: &Popup) {
    let (anchor, height) = {
        let state = lock(popup);
        (state.anchor, state.height)
    };
    let monitor = anchor
        .and_then(|a| {
            app.monitor_from_point(a.centre_x(), a.centre_y())
                .ok()
                .flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let size = (WIDTH * scale, height * scale);
    let area = Rect {
        x: f64::from(monitor.position().x),
        y: f64::from(monitor.position().y),
        width: f64::from(monitor.size().width),
        height: f64::from(monitor.size().height),
    };
    // Without an anchor the popup goes to the top-right corner of the monitor.
    let anchor = anchor.unwrap_or(Rect {
        x: area.x + area.width,
        y: area.y,
        width: 0.0,
        height: 0.0,
    });
    let (x, y) = position(anchor, area, size);
    // Rounded: the window takes whole physical pixels.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let moved = window
        .set_size(PhysicalSize::new(
            size.0.round() as u32,
            size.1.round() as u32,
        ))
        .and_then(|()| {
            window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
        });
    if let Err(error) = moved {
        eprintln!("tamias-app: cannot place the usage card: {error}");
    }
}

/// Hides the popup.
pub fn hide(app: &AppHandle) {
    if let Some(window) = window(app)
        && let Err(error) = window.hide()
    {
        eprintln!("tamias-app: cannot hide the usage card: {error}");
    }
}

/// The popup lost focus: hides it, and keeps it hidden until the pointer left the icon.
pub fn on_focus_lost(app: &AppHandle) {
    dismiss(app);
}

/// Hides the popup on purpose and keeps it hidden until the pointer left the icon: the pointer
/// is still on the icon after a click, and the move events that follow must not show it again.
pub fn dismiss(app: &AppHandle) {
    if let Some(popup) = app.try_state::<Popup>() {
        lock(&popup).dismiss();
    }
    hide(app);
}

/// Hides the popup (Escape).
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "tauri injects `AppHandle` by value"
)]
pub fn tray_popup_hide(app: AppHandle) {
    dismiss(&app);
}

/// The pointer entered (`true`) or left (`false`) the card; leaving hides after [`HIDE_DELAY`]
/// unless the pointer is back on the icon or the card.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "tauri injects `AppHandle` by value"
)]
pub fn tray_popup_hover(app: AppHandle, over: bool) {
    let Some(popup) = app.try_state::<Popup>() else {
        return;
    };
    {
        let mut state = lock(&popup);
        state.over_card = over;
        state.generation += 1;
    }
    if !over {
        schedule_hide(&app);
    }
}

/// Shows the main window and hides the popup ("Open tamias").
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "tauri injects `AppHandle` by value"
)]
pub fn tray_popup_open_main(app: AppHandle) {
    dismiss(&app);
    crate::tray::show_window(&app);
}

/// Sizes the popup to the card: `height` in CSS pixels, placed again when it is shown.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "tauri injects `AppHandle` by value"
)]
pub fn tray_popup_fit(app: AppHandle, height: f64) {
    let (Some(window), Some(popup)) = (window(&app), app.try_state::<Popup>()) else {
        return;
    };
    if !height.is_finite() || height <= 0.0 {
        return;
    }
    lock(&popup).height = height;
    place(&app, &window, &popup);
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the positions are sums of small whole numbers, exact in f64"
)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };
    const SIZE: (f64, f64) = (320.0, 400.0);

    fn icon(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 32.0,
            height: 32.0,
        }
    }

    #[test]
    fn menu_bar_icon_gets_the_popup_below_it_centred() {
        // Icon centre at x = 1000, y = 12 (top of the screen).
        let (x, y) = position(icon(984.0, 0.0), MONITOR, SIZE);
        assert_eq!((x, y), (840.0, 40.0));
    }

    #[test]
    fn taskbar_icon_gets_the_popup_above_it() {
        // Icon in the bottom taskbar, y = 1040.
        let (x, y) = position(icon(984.0, 1040.0), MONITOR, SIZE);
        assert_eq!((x, y), (840.0, 632.0));
    }

    #[test]
    fn icon_at_the_right_edge_keeps_the_popup_inside_the_monitor() {
        let (x, _) = position(icon(1888.0, 1040.0), MONITOR, SIZE);
        assert_eq!(x, 1920.0 - 320.0 - 8.0);
    }

    #[test]
    fn icon_at_the_left_edge_keeps_the_popup_inside_the_monitor() {
        let (x, _) = position(icon(0.0, 0.0), MONITOR, SIZE);
        assert_eq!(x, 8.0);
    }

    #[test]
    fn popup_is_placed_on_a_monitor_left_of_the_origin() {
        let left = Rect {
            x: -1920.0,
            ..MONITOR
        };
        let (x, y) = position(icon(-40.0, 1040.0), left, SIZE);
        assert_eq!(x, -1920.0 + 1920.0 - 320.0 - 8.0);
        assert_eq!(y, 632.0);
    }

    #[test]
    fn tall_popup_sticks_to_the_top_of_the_monitor() {
        let (_, y) = position(icon(984.0, 1040.0), MONITOR, (320.0, 1200.0));
        assert_eq!(y, 8.0);
    }

    #[test]
    fn pointer_anchor_without_size_works_like_an_icon() {
        let pointer = Rect {
            x: 1920.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        let (x, y) = position(pointer, MONITOR, SIZE);
        assert_eq!((x, y), (1920.0 - 320.0 - 8.0, 8.0));
    }

    #[test]
    fn scaled_sizes_still_fit() {
        // 200 % scale: the popup is 640 x 800 physical pixels.
        let monitor = Rect {
            width: 3840.0,
            height: 2160.0,
            ..MONITOR
        };
        let (x, y) = position(icon(3800.0, 2100.0), monitor, (640.0, 800.0));
        assert_eq!((x, y), (3840.0 - 640.0 - 8.0, 2100.0 - 800.0 - 8.0));
    }

    #[test]
    fn a_click_dismisses_the_card_until_the_pointer_leaves_the_icon() {
        let mut state = PopupState::new();
        assert!(state.pointer_over(false), "hover shows the card");
        state.dismiss();
        assert!(!state.pointer_over(false), "move events after the click");
        assert!(!state.pointer_over(false));
        state.pointer_left();
        assert!(
            state.pointer_over(false),
            "enter after leave shows it again"
        );
    }

    #[test]
    fn a_visible_card_is_not_shown_again() {
        let mut state = PopupState::new();
        assert!(!state.pointer_over(true));
        assert!(state.over_icon);
    }

    #[test]
    fn dismiss_cancels_a_pending_hide_and_clears_the_card_hover() {
        let mut state = PopupState::new();
        state.over_card = true;
        let scheduled = state.generation;
        state.dismiss();
        assert!(!state.over_card);
        assert!(!hide_due(scheduled, state.generation, false, false));
    }

    #[test]
    fn the_card_is_wanted_again_when_the_pointer_came_back_during_the_hide() {
        let mut state = PopupState::new();
        let scheduled = state.generation;
        assert!(!state.wants_card(scheduled), "nothing happened");
        state.pointer_over(false);
        assert!(state.wants_card(scheduled));
        state.dismiss();
        assert!(!state.wants_card(scheduled), "dismissed meanwhile");
    }

    #[test]
    fn hide_is_due_only_when_the_pointer_is_gone_and_nothing_newer_was_scheduled() {
        assert!(hide_due(3, 3, false, false));
        assert!(!hide_due(3, 4, false, false), "cancelled by a newer event");
        assert!(!hide_due(3, 3, true, false), "pointer back on the icon");
        assert!(!hide_due(3, 3, false, true), "pointer in the card");
    }
}
