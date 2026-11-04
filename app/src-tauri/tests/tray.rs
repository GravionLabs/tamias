//! The tray state of a report: icon colour and summary, and how a failed read changes it.

use serde_json::{Value, json};
use tamias_app_lib::CommandError;
use tamias_app_lib::tray::{NOTHING_SET_UP, STARTING, TrayColour, TrayState, tray_state};
use tamias_core::report::Report;
use tauri::image::Image;

/// A report document of `providers`, as `tamias today --json` writes it.
fn report(providers: &Value) -> Report {
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

fn window(window: &str, unit: &str, used: f64, limit: f64, colour: &str) -> Value {
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

/// Claude: seven-day 62 % in `seven_day`, five-hour 90 % red (informational, never counted).
fn claude(seven_day: &str) -> Value {
    json!({
        "provider": "claude",
        "status": "ok",
        "windows": [
            window("seven_day", "percent", 62.0, 100.0, seven_day),
            window("five_hour", "percent", 90.0, 100.0, "red"),
        ],
    })
}

fn stale_claude() -> Value {
    json!({
        "provider": "claude",
        "status": "stale",
        "stale_since": "2026-11-03T08:00:00Z",
        "windows": [
            window("seven_day", "percent", 62.0, 100.0, "grey"),
            window("five_hour", "percent", 12.0, 100.0, "grey"),
        ],
    })
}

/// Copilot: 123 of 300 requests (41 %).
fn copilot(colour: &str) -> Value {
    json!({
        "provider": "copilot",
        "status": "ok",
        "windows": [window("monthly", "requests", 123.0, 300.0, colour)],
    })
}

fn unavailable(provider: &str) -> Value {
    json!({ "provider": provider, "status": "unavailable", "windows": [] })
}

fn state(providers: &Value) -> (TrayColour, String) {
    tray_state(&report(providers))
}

#[test]
fn all_green_is_green() {
    assert_eq!(
        state(&json!([claude("green"), copilot("green")])),
        (TrayColour::Green, "C 62% · CP 41%".to_owned())
    );
}

#[test]
fn one_yellow_is_yellow() {
    assert_eq!(
        state(&json!([claude("green"), copilot("yellow")])),
        (TrayColour::Yellow, "C 62% · CP 41%".to_owned())
    );
}

#[test]
fn one_red_is_red() {
    assert_eq!(
        state(&json!([claude("red"), copilot("yellow")])),
        (TrayColour::Red, "C 62% · CP 41%".to_owned())
    );
}

#[test]
fn the_informational_window_does_not_colour_the_icon() {
    // Claude's five-hour window is red, its binding seven-day window green.
    assert_eq!(state(&json!([claude("green")])).0, TrayColour::Green);
}

#[test]
fn stale_claude_is_grey_with_a_question_mark() {
    assert_eq!(
        state(&json!([stale_claude(), copilot("red")])),
        (TrayColour::Grey, "C ? · CP 41%".to_owned())
    );
}

#[test]
fn a_provider_without_data_of_its_binding_window_is_grey() {
    let no_data = json!({ "provider": "claude", "status": "ok", "windows": [] });
    assert_eq!(
        state(&json!([no_data, copilot("green")])),
        (TrayColour::Grey, "C ? · CP 41%".to_owned())
    );
}

#[test]
fn unavailable_copilot_is_left_out_of_the_summary_and_the_colour() {
    assert_eq!(
        state(&json!([claude("yellow"), unavailable("copilot")])),
        (TrayColour::Yellow, "C 62%".to_owned())
    );
}

#[test]
fn nothing_set_up_is_neutral() {
    let expected = (TrayColour::Neutral, NOTHING_SET_UP.to_owned());
    assert_eq!(
        state(&json!([unavailable("claude"), unavailable("copilot")])),
        expected
    );
    assert_eq!(state(&json!([])), expected);
}

fn error(message: &str, busy: bool) -> CommandError {
    CommandError {
        message: message.to_owned(),
        busy,
    }
}

fn green_state() -> TrayState {
    TrayState::of_report(&report(&json!([claude("green"), copilot("green")])))
}

#[test]
fn starting_is_grey_and_says_so() {
    let state = TrayState::starting();
    assert_eq!(state.colour, TrayColour::Grey);
    assert_eq!(state.text(), STARTING);
}

#[test]
fn a_complete_report_replaces_the_state() {
    let next = TrayState::starting().next(Ok(&report(&json!([claude("red")]))));
    assert_eq!(
        next,
        TrayState {
            colour: TrayColour::Red,
            summary: "C 62%".to_owned(),
            error: None,
        }
    );
    assert_eq!(next.text(), "C 62%");
}

#[test]
fn a_failed_read_keeps_the_last_summary_grey_with_the_error() {
    let next = green_state().next(Err(&error("database disk image is malformed", false)));
    assert_eq!(next.colour, TrayColour::Grey);
    assert_eq!(next.summary, "C 62% · CP 41%");
    assert_eq!(
        next.text(),
        "C 62% · CP 41% (error: database disk image is malformed)"
    );
}

#[test]
fn a_failed_read_before_the_first_report_is_grey_with_the_error() {
    let next = TrayState::starting().next(Err(&error("no home directory", false)));
    assert_eq!(next.colour, TrayColour::Grey);
    assert_eq!(
        next.text(),
        format!("{STARTING} (error: no home directory)")
    );
}

#[test]
fn errors_do_not_pile_up() {
    let once = green_state().next(Err(&error("first", false)));
    let twice = once.next(Err(&error("second", false)));
    assert_eq!(twice.text(), "C 62% · CP 41% (error: second)");
}

#[test]
fn a_report_after_a_failed_read_clears_the_error() {
    let failed = green_state().next(Err(&error("locked", false)));
    let next = failed.next(Ok(&report(&json!([claude("yellow")]))));
    assert_eq!(next.colour, TrayColour::Yellow);
    assert_eq!(next.text(), "C 62%");
}

#[test]
fn a_busy_store_keeps_the_state() {
    let state = green_state();
    assert_eq!(state.next(Err(&error("database is locked", true))), state);
}

#[test]
fn every_icon_is_a_visible_64px_image() {
    for colour in TrayColour::ALL {
        let image = Image::from_bytes(colour.png()).unwrap();
        assert_eq!((image.width(), image.height()), (64, 64), "{colour:?}");
        let opaque = image.rgba().chunks(4).filter(|px| px[3] == 255).count();
        assert!(opaque > 300, "{colour:?} has {opaque} opaque pixels");
        // The corners stay transparent: a disc, not a square.
        assert_eq!(image.rgba()[3], 0, "{colour:?}");
    }
}

#[test]
fn every_colour_has_its_own_icon() {
    for (i, a) in TrayColour::ALL.iter().enumerate() {
        for b in &TrayColour::ALL[i + 1..] {
            assert_ne!(a.png(), b.png(), "{a:?} and {b:?}");
        }
    }
}
