//! Collection setup through the CLI sidecar, against fake sidecar scripts: `/bin/sh` scripts on
//! Unix, `.cmd` batch files on Windows, each printing what `tamias service ...` prints.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tamias_app_lib::service::{Error, ServiceLock, ServiceStatus, Sidecar, State};
use tempfile::TempDir;

/// Generous for a fake that answers at once; the fakes that hang use [`SHORT`].
const LONG: Duration = Duration::from_secs(30);
/// The injected timeout of the timeout tests (instead of the real 10 s).
const SHORT: Duration = Duration::from_millis(300);

/// What a fake sidecar does: the lines it prints, and its exit code.
struct Fake<'a> {
    stdout: &'a [&'a str],
    stderr: &'a [&'a str],
    code: i32,
    hang: bool,
}

const OK: Fake<'static> = Fake {
    stdout: &[],
    stderr: &[],
    code: 0,
    hang: false,
};

/// Writes the fake sidecar `name` into `dir`.
#[cfg(unix)]
fn write_fake(dir: &Path, name: &str, fake: &Fake<'_>) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let mut script = vec!["#!/bin/sh".to_owned()];
    script.extend(fake.stdout.iter().map(|line| format!("echo '{line}'")));
    script.extend(fake.stderr.iter().map(|line| format!("echo '{line}' >&2")));
    if fake.hang {
        script.push("exec sleep 30".to_owned());
    }
    script.push(format!("exit {}\n", fake.code));
    let script = script.join("\n");
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Writes the fake sidecar `name` into `dir`.
#[cfg(windows)]
fn write_fake(dir: &Path, name: &str, fake: &Fake<'_>) -> PathBuf {
    let mut script = vec!["@echo off".to_owned()];
    script.extend(fake.stdout.iter().map(|line| format!("echo {line}")));
    script.extend(fake.stderr.iter().map(|line| format!("echo {line} 1>&2")));
    if fake.hang {
        script.push("ping -n 30 127.0.0.1 >nul".to_owned());
    }
    script.push(format!("exit /b {}\r\n", fake.code));
    let script = script.join("\r\n");
    let path = dir.join(format!("{name}.cmd"));
    std::fs::write(&path, script).unwrap();
    path
}

fn sidecar(fake: &Fake<'_>, timeout: Duration) -> (TempDir, Sidecar) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_fake(dir.path(), "tamias", fake);
    (dir, Sidecar::new(path, timeout))
}

fn status_of(fake: &Fake<'_>) -> ServiceStatus {
    let (_dir, sidecar) = sidecar(fake, LONG);
    sidecar.status()
}

#[test]
fn installed_is_installed_with_its_exe() {
    let status = status_of(&Fake {
        stdout: &[
            "service: installed",
            "definition: /home/u/.config/systemd/user/tamias-collect.timer",
            "exe: /home/u/.local/bin/tamias",
            "claude: last run 2026-11-04 10:05 ok",
            "copilot: not polled yet (coming with #5)",
            "log: /home/u/.local/share/tamias/logs/collect.log",
        ],
        ..OK
    });
    assert_eq!(
        status,
        ServiceStatus {
            state: State::Installed,
            message: None,
            exe: Some("/home/u/.local/bin/tamias".to_owned()),
        }
    );
}

#[test]
fn not_installed_is_not_installed_without_a_message() {
    let status = status_of(&Fake {
        stdout: &[
            "service: not installed (run `tamias service install`)",
            "claude: no run yet",
            "copilot: not polled yet (coming with #5)",
            "log: /home/u/.local/share/tamias/logs/collect.log",
        ],
        ..OK
    });
    assert_eq!(
        status,
        ServiceStatus {
            state: State::NotInstalled,
            message: None,
            exe: None,
        }
    );
}

#[test]
fn installed_but_not_loaded_is_not_installed_with_the_note() {
    let status = status_of(&Fake {
        stdout: &[
            "service: installed but not loaded; run `tamias service install` again",
            "exe: /opt/tamias/tamias",
        ],
        ..OK
    });
    assert_eq!(status.state, State::NotInstalled);
    assert_eq!(
        status.message.as_deref(),
        Some("installed but not loaded; run `tamias service install` again")
    );
    assert_eq!(status.exe.as_deref(), Some("/opt/tamias/tamias"));
}

#[test]
fn installed_with_a_missing_exe_is_not_installed_with_the_warning() {
    let status = status_of(&Fake {
        stdout: &[
            "service: installed",
            "exe: /tmp/.mount_tamias/usr/bin/tamias",
            "warning: the registered exe no longer exists; run `tamias service install` again",
        ],
        ..OK
    });
    assert_eq!(status.state, State::NotInstalled);
    assert_eq!(
        status.message.as_deref(),
        Some("the registered exe no longer exists; run `tamias service install` again")
    );
}

#[test]
fn a_non_zero_exit_is_an_error_with_the_message_of_stderr() {
    let status = status_of(&Fake {
        stderr: &["error: invalid config.toml: unknown field `claud`"],
        code: 1,
        ..OK
    });
    assert_eq!(status.state, State::Error);
    let message = status.message.unwrap();
    assert!(
        message.contains("invalid config.toml: unknown field `claud`"),
        "{message}"
    );
    assert!(message.contains("service status"), "{message}");
    assert!(!message.contains("error: invalid"), "{message}");
}

#[test]
fn a_non_zero_exit_without_stderr_shows_stdout() {
    let status = status_of(&Fake {
        stdout: &["something went wrong"],
        code: 2,
        ..OK
    });
    assert_eq!(status.state, State::Error);
    assert!(
        status
            .message
            .as_deref()
            .unwrap()
            .contains("something went wrong")
    );
}

#[test]
fn unexpected_output_is_an_error() {
    let status = status_of(&Fake {
        stdout: &["Usage: tamias [COMMAND]"],
        ..OK
    });
    assert_eq!(status.state, State::Error);
    assert!(
        status
            .message
            .as_deref()
            .unwrap()
            .contains("unexpected output")
    );
}

#[test]
fn empty_output_is_an_error() {
    assert_eq!(status_of(&OK).state, State::Error);
}

#[test]
fn a_sidecar_that_does_not_answer_is_killed_and_an_error_after_the_timeout() {
    let (_dir, sidecar) = sidecar(
        &Fake {
            stdout: &["service: installed"],
            hang: true,
            ..OK
        },
        SHORT,
    );
    let started = Instant::now();
    let status = sidecar.status();
    let took = started.elapsed();
    assert_eq!(status.state, State::Error);
    let message = status.message.unwrap();
    assert!(message.contains("did not answer within 0.3 s"), "{message}");
    // Well below the fake's 30 s: the call returned at the timeout.
    assert!(took < Duration::from_secs(10), "took {took:?}");
}

#[test]
fn the_timeout_applies_to_uninstall_too() {
    let (_dir, sidecar) = sidecar(&Fake { hang: true, ..OK }, SHORT);
    let started = Instant::now();
    let result = sidecar.uninstall();
    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_missing_sidecar_is_an_error_naming_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no-such-tamias");
    let status = Sidecar::new(&path, LONG).status();
    assert_eq!(status.state, State::Error);
    assert!(
        status
            .message
            .as_deref()
            .unwrap()
            .contains("no-such-tamias")
    );
}

#[test]
fn uninstall_returns_the_output_of_the_cli() {
    let (_dir, sidecar) = sidecar(
        &Fake {
            stdout: &["uninstalled; the data and the log are kept"],
            ..OK
        },
        LONG,
    );
    assert_eq!(
        sidecar.uninstall().unwrap().trim(),
        "uninstalled; the data and the log are kept"
    );
}

// On Linux `install` copies to the real `~/.local/bin`; `linux_copy` covers it with a temp home.
#[cfg(not(target_os = "linux"))]
#[test]
fn a_failed_install_is_an_error_with_the_message() {
    let (_dir, sidecar) = sidecar(
        &Fake {
            stderr: &["error: no systemd user manager"],
            code: 1,
            ..OK
        },
        LONG,
    );
    let error = sidecar.install().unwrap_err();
    assert!(matches!(error, Error::Failed { .. }), "{error:?}");
    assert!(error.to_string().contains("no systemd user manager"));
}

#[test]
fn the_lock_refuses_a_second_concurrent_setup_until_the_first_ends() {
    let lock = ServiceLock::default();
    let first = lock.try_lock().unwrap();
    assert!(matches!(lock.try_lock(), Err(Error::Busy)));
    // A clone is the same lock (the app manages one, each command gets a reference).
    assert!(matches!(lock.clone().try_lock(), Err(Error::Busy)));
    drop(first);
    assert!(lock.try_lock().is_ok());
}

#[cfg(unix)]
mod linux_copy {
    //! `service_install` on Linux: the sidecar is copied to `~/.local/bin/tamias` first and the
    //! install runs that copy. Pure file logic, so it runs on every Unix.

    use std::os::unix::fs::PermissionsExt;

    use tamias_app_lib::service::copy_to_local_bin;

    use super::*;

    /// A fake `tamias` that prints the path it runs from, as `tamias service install` does.
    fn install_fake(dir: &Path) -> PathBuf {
        let path = dir.join("tamias");
        std::fs::write(&path, "#!/bin/sh\necho \"installed: $0 $*\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn install_copies_the_sidecar_to_local_bin_executable_and_runs_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = Sidecar::new(install_fake(dir.path()), LONG);
        let home = tempfile::tempdir().unwrap();

        let out = sidecar.install_copied(home.path()).unwrap();

        let copy = home.path().join(".local/bin/tamias");
        assert_eq!(
            out.trim(),
            format!("installed: {} service install", copy.display())
        );
        let mode = std::fs::metadata(&copy).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        let bin = home.path().join(".local/bin");
        let names: Vec<_> = std::fs::read_dir(&bin)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["tamias"], "no partial file is left behind");
    }

    #[test]
    fn install_replaces_an_older_copy() {
        let dir = tempfile::tempdir().unwrap();
        let fake = install_fake(dir.path());
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tamias"), "#!/bin/sh\necho old version\n").unwrap();
        std::fs::set_permissions(bin.join("tamias"), std::fs::Permissions::from_mode(0o700))
            .unwrap();

        let out = Sidecar::new(&fake, LONG)
            .install_copied(home.path())
            .unwrap();

        assert!(!out.contains("old version"), "{out}");
        assert_eq!(
            std::fs::read(bin.join("tamias")).unwrap(),
            std::fs::read(&fake).unwrap()
        );
        let mode = std::fs::metadata(bin.join("tamias"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn copy_returns_the_path_in_local_bin() {
        let dir = tempfile::tempdir().unwrap();
        let fake = write_fake(dir.path(), "tamias", &OK);
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            copy_to_local_bin(&fake, home.path()).unwrap(),
            home.path().join(".local/bin/tamias")
        );
    }

    #[test]
    fn a_missing_sidecar_fails_the_copy_without_touching_an_older_copy() {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tamias"), "old").unwrap();

        let result = copy_to_local_bin(&home.path().join("missing"), home.path());

        assert!(matches!(result, Err(Error::Copy { .. })), "{result:?}");
        assert_eq!(std::fs::read_to_string(bin.join("tamias")).unwrap(), "old");
        assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 1);
    }

    #[test]
    fn a_failing_install_of_the_copy_is_an_error() {
        let (_dir, sidecar) = sidecar(
            &Fake {
                stderr: &["error: systemctl --user failed"],
                code: 1,
                ..OK
            },
            LONG,
        );
        let home = tempfile::tempdir().unwrap();
        let error = sidecar.install_copied(home.path()).unwrap_err();
        assert!(error.to_string().contains("systemctl --user failed"));
    }
}
