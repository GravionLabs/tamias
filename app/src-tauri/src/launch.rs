//! How the app was started: the autostart entry passes [`HIDDEN`], which keeps the window
//! hidden at the start.
//!
//! NOTE: the tray (#86) is the way back to a hidden window. As long as it is not merged, a
//! `--hidden` start leaves no way to open the window; it only happens when the user enabled
//! autostart (off by default) or passes the argument by hand. Until #86 lands, do not make
//! autostart the default.

/// The argument the autostart entry passes.
pub const HIDDEN: &str = "--hidden";

/// Whether the main window is shown at the start: unless `args` (without the program name)
/// contain [`HIDDEN`].
pub fn initial_visibility<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    !args.into_iter().any(|arg| arg.as_ref() == HIDDEN)
}
