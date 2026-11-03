//! Platform-independent parts of `tamias service`.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_core::snapshot::Provider;
use tamias_core::store::{SourceStatus, Store};

use super::{
    Action, Error, Output, Result, Runner, Scheduler, SchedulerState, execute, render_status,
    run_checked,
};

fn ts(text: &str) -> Timestamp {
    text.parse().unwrap()
}

#[allow(clippy::unnecessary_wraps)]
fn status(ok: Option<&str>, err: Option<&str>, message: Option<&str>) -> Option<SourceStatus> {
    Some(SourceStatus {
        provider: Provider::Claude,
        last_ok_at: ok.map(ts),
        last_error_at: err.map(ts),
        last_error: message.map(str::to_owned),
    })
}

fn not_installed() -> SchedulerState {
    SchedulerState {
        installed: false,
        registered: false,
        exe: None,
        definition: None,
    }
}

#[test]
fn status_of_an_installed_agent_with_runs() {
    let exe = std::env::current_exe().unwrap();
    let state = SchedulerState {
        installed: true,
        registered: true,
        exe: Some(exe.clone()),
        definition: Some(PathBuf::from("/h/agent.plist")),
    };
    let text = render_status(
        &state,
        &[
            (
                Provider::Claude,
                status(Some("2026-11-03T10:05:00Z"), None, None),
            ),
            (Provider::Copilot, None),
        ],
        Path::new("/d/logs/collect.log"),
        &TimeZone::UTC,
    );
    assert_eq!(
        text,
        format!(
            "service: installed\n\
             definition: /h/agent.plist\n\
             exe: {}\n\
             claude: last run 2026-11-03 10:05 ok\n\
             copilot: no run yet\n\
             log: /d/logs/collect.log\n",
            exe.display()
        )
    );
}

#[test]
fn status_warns_when_the_registered_exe_is_gone() {
    let state = SchedulerState {
        installed: true,
        registered: true,
        exe: Some(PathBuf::from("/definitely/not/here/tamias")),
        definition: None,
    };
    let text = render_status(&state, &[], Path::new("l"), &TimeZone::UTC);
    assert!(
        text.contains("warning: the registered exe no longer exists"),
        "{text}"
    );
}

#[test]
fn status_of_a_definition_that_is_not_loaded() {
    let state = SchedulerState {
        installed: true,
        registered: false,
        exe: None,
        definition: None,
    };
    let text = render_status(&state, &[], Path::new("l"), &TimeZone::UTC);
    assert!(
        text.starts_with("service: installed but not loaded"),
        "{text}"
    );
}

#[test]
fn status_when_not_installed_shows_the_log_path() {
    let text = render_status(
        &not_installed(),
        &[],
        Path::new("/d/collect.log"),
        &TimeZone::UTC,
    );
    assert_eq!(
        text,
        "service: not installed (run `tamias service install`)\nlog: /d/collect.log\n"
    );
}

#[test]
fn last_run_is_the_newer_of_ok_and_error() {
    let cases = [
        (
            status(
                Some("2026-11-03T10:00:00Z"),
                Some("2026-11-03T10:05:00Z"),
                Some("unauthorized"),
            ),
            "claude: last run 2026-11-03 10:05 error (unauthorized)\n",
        ),
        (
            status(
                Some("2026-11-03T10:10:00Z"),
                Some("2026-11-03T10:05:00Z"),
                Some("unauthorized"),
            ),
            "claude: last run 2026-11-03 10:10 ok\n",
        ),
        (
            status(None, Some("2026-11-03T10:05:00Z"), None),
            "claude: last run 2026-11-03 10:05 error (unknown)\n",
        ),
        (status(None, None, None), "claude: no run yet\n"),
    ];
    for (status, expected) in cases {
        let text = render_status(
            &not_installed(),
            &[(Provider::Claude, status)],
            Path::new("l"),
            &TimeZone::UTC,
        );
        assert!(text.contains(expected), "{text} lacks {expected}");
    }
}

struct FakeRunner(Output);

impl Runner for FakeRunner {
    fn run(&self, _program: &str, _args: &[&str]) -> io::Result<Output> {
        Ok(self.0.clone())
    }
}

#[test]
fn run_checked_reports_command_status_and_stderr() {
    let runner = FakeRunner(Output {
        success: false,
        status: "exit status: 5".to_owned(),
        stdout: String::new(),
        stderr: "Bootstrap failed: 5\n".to_owned(),
    });
    let err = run_checked(&runner, "launchctl", &["bootstrap", "gui/1", "x"]).unwrap_err();
    assert_eq!(
        err.to_string(),
        "`launchctl bootstrap gui/1 x` failed (exit status: 5): Bootstrap failed: 5"
    );
}

#[test]
fn run_checked_returns_the_output_on_success() {
    let runner = FakeRunner(Output {
        success: true,
        status: "exit status: 0".to_owned(),
        stdout: "501\n".to_owned(),
        stderr: String::new(),
    });
    assert_eq!(run_checked(&runner, "id", &["-u"]).unwrap().stdout, "501\n");
}

#[test]
fn a_program_that_cannot_start_is_an_io_error() {
    struct Missing;
    impl Runner for Missing {
        fn run(&self, _: &str, _: &[&str]) -> io::Result<Output> {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }
    let err = run_checked(&Missing, "launchctl", &["print"]).unwrap_err();
    assert!(matches!(err, Error::Io { .. }));
    assert!(err.to_string().starts_with("cannot run `launchctl print`"));
}

#[test]
fn unsupported_names_the_platform() {
    assert_eq!(
        Error::Unsupported("linux").to_string(),
        "tamias service is not supported yet on this platform (linux)"
    );
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn platform_is_not_supported_yet_on_windows() {
    let Err(err) = super::platform() else {
        panic!("platform() must fail on this OS");
    };
    assert!(err.to_string().contains("not supported yet"), "{err}");
}

/// Scheduler that records calls and holds one installed exe.
#[derive(Default)]
struct FakeScheduler {
    exe: RefCell<Option<PathBuf>>,
    uninstalls: RefCell<u32>,
}

impl Scheduler for FakeScheduler {
    fn install(&self, exe: &Path) -> Result<()> {
        *self.exe.borrow_mut() = Some(exe.to_path_buf());
        Ok(())
    }
    fn uninstall(&self) -> Result<()> {
        *self.uninstalls.borrow_mut() += 1;
        *self.exe.borrow_mut() = None;
        Ok(())
    }
    fn status(&self) -> Result<SchedulerState> {
        let exe = self.exe.borrow().clone();
        Ok(SchedulerState {
            installed: exe.is_some(),
            registered: exe.is_some(),
            exe,
            definition: None,
        })
    }
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("tamias.db")).unwrap();
    (dir, store)
}

/// Store opener that fails like a database that cannot be opened.
fn broken_store() -> std::result::Result<Store, Box<dyn std::error::Error>> {
    Err("cannot open the database".into())
}

#[test]
fn install_registers_the_canonical_current_exe() {
    let scheduler = FakeScheduler::default();
    let text = execute(
        Action::Install,
        &scheduler,
        broken_store,
        Path::new("/d/collect.log"),
    )
    .unwrap();
    let exe = std::env::current_exe().unwrap().canonicalize().unwrap();
    assert_eq!(scheduler.exe.borrow().as_deref(), Some(exe.as_path()));
    assert!(text.contains("every 5 minutes"), "{text}");
    assert!(text.contains("log: /d/collect.log"), "{text}");
}

#[test]
fn uninstall_when_not_installed_succeeds_with_a_note() {
    let scheduler = FakeScheduler::default();
    let text = execute(Action::Uninstall, &scheduler, broken_store, Path::new("l")).unwrap();
    assert_eq!(text, "not installed, nothing to remove\n");
}

#[test]
fn uninstall_after_install_says_the_data_is_kept() {
    let scheduler = FakeScheduler::default();
    execute(Action::Install, &scheduler, broken_store, Path::new("l")).unwrap();
    let text = execute(Action::Uninstall, &scheduler, broken_store, Path::new("l")).unwrap();
    assert_eq!(text, "uninstalled; the data and the log are kept\n");
    assert_eq!(*scheduler.uninstalls.borrow(), 1);
}

#[test]
fn status_reads_the_last_run_from_the_store() {
    let (_dir, mut store) = store();
    store
        .record_error(Provider::Claude, ts("2026-11-03T10:05:00Z"), "rate limited")
        .unwrap();
    let text = execute(
        Action::Status,
        &FakeScheduler::default(),
        || Ok(store),
        Path::new("l"),
    )
    .unwrap();
    assert!(text.contains("claude: last run 2026-11-03"), "{text}");
    assert!(text.contains("error (rate limited)"), "{text}");
    assert!(text.contains("copilot: no run yet"), "{text}");
}

#[test]
fn status_reports_a_store_that_cannot_be_opened() {
    let err = execute(
        Action::Status,
        &FakeScheduler::default(),
        broken_store,
        Path::new("l"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "cannot open the database");
}
