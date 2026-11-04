//! How the app was started: the autostart entry passes [`HIDDEN`], which keeps the window
//! hidden at the start.
//!
//! The tray icon ([`crate::tray`]) is the way to a hidden window: a left click (Windows, macOS)
//! or "Open tamias" in its menu. When the tray cannot be created, the window is shown anyway.

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
