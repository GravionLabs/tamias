//! The start argument `--hidden` decides whether the main window is shown at the start.

use tamias_app_lib::launch::initial_visibility;

#[test]
fn no_arguments_shows_the_window() {
    assert!(initial_visibility(Vec::<String>::new()));
}

#[test]
fn hidden_keeps_the_window_hidden() {
    assert!(!initial_visibility(["--hidden"]));
}

#[test]
fn hidden_among_other_arguments_keeps_the_window_hidden() {
    assert!(!initial_visibility(["--foo", "--hidden"]));
}

#[test]
fn similar_arguments_show_the_window() {
    assert!(initial_visibility(["--hidden=1", "hidden", "--Hidden", ""]));
}
