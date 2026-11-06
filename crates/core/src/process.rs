//! The one place tamias starts child processes.
//!
//! `tamiasw.exe` and the app are Windows GUI programs without a console. A console program such
//! as `gh.exe` or `schtasks.exe` started from them opens a visible console window unless the
//! process is created with `CREATE_NO_WINDOW`. [`command`] sets that flag on Windows and does
//! nothing elsewhere; `clippy.toml` forbids `std::process::Command::new` everywhere else.

use std::ffi::OsStr;
use std::process::Command;

/// Creates a [`Command`] for `program` that never opens a console window on Windows.
#[must_use]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    // The single allowed call: every other site goes through this helper.
    #[allow(clippy::disallowed_methods)]
    let mut command = Command::new(program);
    hide_console(&mut command);
    command
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn keeps_program_and_args() {
        let mut c = command("gh");
        c.args(["auth", "token"]);
        assert_eq!(c.get_program(), "gh");
        assert_eq!(c.get_args().collect::<Vec<_>>(), ["auth", "token"]);
    }

    #[cfg(windows)]
    #[test]
    fn runs_a_console_program_with_the_flag_set() {
        // `cmd.exe` is a console program; with CREATE_NO_WINDOW it still runs and reports its exit.
        let status = command("cmd.exe").args(["/C", "exit 3"]).status().unwrap();
        assert_eq!(status.code(), Some(3));
    }
}
