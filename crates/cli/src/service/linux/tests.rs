//! The systemd timer against a fake `systemctl` and a temp unit directory.

use std::cell::RefCell;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tempfile::TempDir;

use super::{SERVICE, SystemdTimer, TIMER, service_unit, timer_unit, units_dir};
use crate::service::{Output, Runner, Scheduler};

const EXPECTED_SERVICE: &str = "[Unit]
Description=tamias collector (polls Claude usage once)

[Service]
Type=oneshot
ExecStart=/opt/tamias/bin/tamias collect
SuccessExitStatus=3
";

const EXPECTED_TIMER: &str = "[Unit]
Description=Run the tamias collector every 5 minutes

[Timer]
OnBootSec=1min
OnUnitActiveSec=5min
Persistent=true

[Install]
WantedBy=timers.target
";

fn failed(stderr: &str) -> Output {
    Output {
        success: false,
        status: "exit status: 1".to_owned(),
        stdout: String::new(),
        stderr: stderr.to_owned(),
    }
}

fn ok(stdout: &str) -> Output {
    Output {
        success: true,
        status: "exit status: 0".to_owned(),
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

/// Fake systemd user manager: `enable --now` marks the timer enabled and active, `disable
/// --now` clears it.
#[derive(Debug, Default)]
struct FakeSystemd {
    enabled: RefCell<bool>,
    /// No user manager: every command fails like in a container.
    no_bus: RefCell<bool>,
    /// `systemctl --user` subcommand that fails.
    fail: RefCell<Option<&'static str>>,
    calls: RefCell<Vec<String>>,
}

impl Runner for Rc<FakeSystemd> {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        self.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        assert_eq!(program, "systemctl");
        let ["--user", sub, rest @ ..] = args else {
            panic!("systemctl without --user: {args:?}");
        };
        if *self.no_bus.borrow() {
            return Ok(failed("Failed to connect to bus: No medium found"));
        }
        if *self.fail.borrow() == Some(*sub) {
            return Ok(failed("Failed to enable unit: Access denied"));
        }
        match (*sub, rest) {
            ("show-environment" | "daemon-reload", []) => Ok(ok("")),
            ("enable", ["--now", unit]) => {
                assert_eq!(*unit, TIMER);
                *self.enabled.borrow_mut() = true;
                Ok(ok(""))
            }
            ("disable", ["--now", unit]) => {
                assert_eq!(*unit, TIMER);
                *self.enabled.borrow_mut() = false;
                Ok(ok(""))
            }
            ("show", [unit, "--property=LoadState,ActiveState,UnitFileState"]) => {
                assert_eq!(*unit, TIMER);
                Ok(ok(if *self.enabled.borrow() {
                    "LoadState=loaded\nActiveState=active\nUnitFileState=enabled\n"
                } else {
                    "LoadState=not-found\nActiveState=inactive\nUnitFileState=\n"
                }))
            }
            _ => Err(io::Error::other(format!(
                "unexpected command {program} {args:?}"
            ))),
        }
    }
}

struct Fixture {
    home: TempDir,
    systemd: Rc<FakeSystemd>,
    timer: SystemdTimer,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let systemd = Rc::new(FakeSystemd::default());
        let dir = units_dir(None, Some(home.path())).unwrap();
        let timer = SystemdTimer::new(dir, Box::new(Rc::clone(&systemd)));
        Self {
            home,
            systemd,
            timer,
        }
    }

    fn dir(&self) -> PathBuf {
        units_dir(None, Some(self.home.path())).unwrap()
    }

    /// Names of all files in the unit directory.
    fn files(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.dir()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn calls(&self) -> Vec<String> {
        self.systemd.calls.borrow().clone()
    }

    fn enabled(&self) -> bool {
        *self.systemd.enabled.borrow()
    }
}

fn unit_names() -> Vec<String> {
    vec![SERVICE.to_owned(), TIMER.to_owned()]
}

#[test]
fn service_unit_is_a_oneshot_that_accepts_exit_3() {
    assert_eq!(service_unit("/opt/tamias/bin/tamias"), EXPECTED_SERVICE);
}

#[test]
fn timer_unit_runs_one_minute_after_boot_then_every_five_minutes() {
    assert_eq!(timer_unit(), EXPECTED_TIMER);
}

#[test]
fn service_unit_quotes_an_exe_path_with_spaces_and_specifiers() {
    let text = service_unit("/home/a b/100%/tamias");
    assert!(
        text.contains("ExecStart=\"/home/a b/100%%/tamias\" collect\n"),
        "{text}"
    );
    assert_eq!(
        super::exe_of_service(&text).as_deref(),
        Some("/home/a b/100%/tamias")
    );
}

#[test]
fn units_dir_prefers_an_absolute_xdg_config_home() {
    #[cfg(unix)]
    let (xdg, expected) = ("/xdg/conf", PathBuf::from("/xdg/conf"));
    #[cfg(windows)]
    let (xdg, expected) = ("C:\\xdg\\conf", PathBuf::from("C:\\xdg\\conf"));
    assert_eq!(
        units_dir(Some(OsString::from(xdg)), Some(Path::new("home"))),
        Some(expected.join("systemd").join("user"))
    );
}

#[test]
fn units_dir_falls_back_to_dot_config_when_xdg_is_unset_empty_or_relative() {
    let home = Path::new("home");
    let expected = Some(home.join(".config").join("systemd").join("user"));
    for xdg in [
        None,
        Some(OsString::new()),
        Some(OsString::from("rel/conf")),
    ] {
        assert_eq!(units_dir(xdg.clone(), Some(home)), expected, "{xdg:?}");
    }
    assert_eq!(units_dir(None, None), None);
}

#[test]
fn install_writes_both_units_then_reloads_and_enables_the_timer() {
    let f = Fixture::new();
    f.timer
        .install(Path::new("/opt/tamias/bin/tamias"))
        .unwrap();

    assert_eq!(f.files(), unit_names());
    assert_eq!(
        fs::read_to_string(f.timer.service_path()).unwrap(),
        EXPECTED_SERVICE
    );
    assert_eq!(
        fs::read_to_string(f.timer.timer_path()).unwrap(),
        EXPECTED_TIMER
    );
    assert_eq!(
        f.calls(),
        vec![
            "systemctl --user show-environment",
            "systemctl --user daemon-reload",
            "systemctl --user enable --now tamias-collect.timer",
        ]
    );
    let state = f.timer.status().unwrap();
    assert!(state.installed && state.registered);
    assert_eq!(state.exe, Some(PathBuf::from("/opt/tamias/bin/tamias")));
    assert_eq!(state.definition, Some(f.timer.service_path()));
}

#[test]
fn install_is_idempotent_and_the_last_exe_wins() {
    let f = Fixture::new();
    for exe in ["/old/tamias", "/new/tamias", "/new/tamias"] {
        f.timer.install(Path::new(exe)).unwrap();
        assert_eq!(f.files(), unit_names(), "after installing {exe}");
        assert!(f.enabled());
        assert_eq!(
            fs::read_to_string(f.timer.service_path()).unwrap(),
            service_unit(exe)
        );
    }
    assert_eq!(
        f.timer.status().unwrap().exe,
        Some(PathBuf::from("/new/tamias"))
    );
}

#[test]
fn failing_enable_leaves_no_unit_and_no_temp_file_behind() {
    let f = Fixture::new();
    *f.systemd.fail.borrow_mut() = Some("enable");

    let err = f.timer.install(Path::new("/x/tamias")).unwrap_err();
    assert!(err.to_string().contains("Access denied"), "{err}");
    assert!(f.files().is_empty(), "{:?}", f.files());
    assert!(!f.timer.status().unwrap().installed);
}

#[test]
fn failing_daemon_reload_leaves_no_unit_behind() {
    let f = Fixture::new();
    *f.systemd.fail.borrow_mut() = Some("daemon-reload");

    assert!(f.timer.install(Path::new("/x/tamias")).is_err());
    assert!(f.files().is_empty(), "{:?}", f.files());
}

#[test]
fn a_stale_temp_file_does_not_survive_an_install() {
    let f = Fixture::new();
    fs::create_dir_all(f.dir()).unwrap();
    fs::write(f.dir().join(format!(".{SERVICE}.tmp")), "stale").unwrap();

    f.timer.install(Path::new("/x/tamias")).unwrap();
    assert_eq!(f.files(), unit_names());
}

#[test]
fn uninstall_disables_removes_both_units_but_keeps_the_data() {
    let f = Fixture::new();
    let data = f.home.path().join("data").join("tamias");
    fs::create_dir_all(data.join("logs")).unwrap();
    fs::write(data.join("logs").join("collect.log"), "line\n").unwrap();
    fs::write(data.join("tamias.db"), "db").unwrap();

    f.timer.install(Path::new("/x/tamias")).unwrap();
    f.timer.uninstall().unwrap();

    assert!(f.files().is_empty());
    assert!(!f.enabled());
    assert_eq!(
        fs::read_to_string(data.join("logs").join("collect.log")).unwrap(),
        "line\n"
    );
    assert_eq!(fs::read_to_string(data.join("tamias.db")).unwrap(), "db");
    let calls = f.calls();
    assert!(calls.contains(&"systemctl --user disable --now tamias-collect.timer".to_owned()));
    assert_eq!(
        calls.last().map(String::as_str),
        Some("systemctl --user daemon-reload")
    );
}

#[test]
fn uninstall_when_not_installed_is_ok_and_does_not_disable() {
    let f = Fixture::new();
    f.timer.uninstall().unwrap();
    assert!(!f.calls().iter().any(|c| c.contains("disable")));
}

#[test]
fn failing_disable_keeps_the_units() {
    let f = Fixture::new();
    f.timer.install(Path::new("/x/tamias")).unwrap();
    *f.systemd.fail.borrow_mut() = Some("disable");

    assert!(f.timer.uninstall().is_err());
    assert_eq!(f.files(), unit_names());
}

#[test]
fn status_when_not_installed() {
    let f = Fixture::new();
    let state = f.timer.status().unwrap();
    assert!(!state.installed && !state.registered);
    assert_eq!(state.exe, None);
    assert_eq!(state.definition, None);
}

#[test]
fn status_of_units_that_are_not_enabled() {
    let f = Fixture::new();
    f.timer.install(Path::new("/x/tamias")).unwrap();
    *f.systemd.enabled.borrow_mut() = false;

    let state = f.timer.status().unwrap();
    assert!(state.installed);
    assert!(!state.registered);
    assert_eq!(state.exe, Some(PathBuf::from("/x/tamias")));
}

#[test]
fn without_a_systemd_user_instance_every_action_names_the_cause() {
    let f = Fixture::new();
    *f.systemd.no_bus.borrow_mut() = true;

    let errors = [
        f.timer.install(Path::new("/x/tamias")).unwrap_err(),
        f.timer.uninstall().unwrap_err(),
        f.timer.status().unwrap_err(),
    ];
    for err in errors {
        let text = err.to_string();
        assert!(text.contains("no systemd user instance"), "{text}");
        assert!(text.contains("Failed to connect to bus"), "{text}");
    }
    assert!(f.files().is_empty());
}

#[test]
fn a_missing_systemctl_is_reported_as_no_systemd() {
    struct Missing;
    impl Runner for Missing {
        fn run(&self, _: &str, _: &[&str]) -> io::Result<Output> {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let timer = SystemdTimer::new(dir.path().to_path_buf(), Box::new(Missing));
    let err = timer.status().unwrap_err();
    assert!(err.to_string().contains("systemctl was not found"), "{err}");
}
