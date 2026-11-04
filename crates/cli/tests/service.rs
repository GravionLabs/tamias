//! `tamias collect` writes the collect log; `tamias service` on every OS. Nothing is installed:
//! only `service status` (read-only) runs against the real scheduler, with a temp HOME.

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;

use assert_cmd::Command;
use jiff::{SignedDuration, Timestamp};
#[cfg(any(
    target_os = "macos",
    not(any(target_os = "linux", target_os = "windows"))
))]
use predicates::prelude::*;
use tempfile::TempDir;

const TOKEN: &str = "sk-ant-oat01-SERVICE-TEST-TOKEN-must-never-leak";

struct Env {
    dir: TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(env.path("config.toml"), "").unwrap();
        env
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn write_credentials(&self) {
        let expires = Timestamp::now()
            .checked_add(SignedDuration::from_hours(2))
            .unwrap()
            .as_millisecond();
        let json = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","refreshToken":"r","expiresAt":{expires}}}}}"#
        );
        fs::write(self.path("credentials.json"), json).unwrap();
    }

    fn log_path(&self) -> PathBuf {
        self.path("data").join("logs").join("collect.log")
    }

    /// Points the config dir, data dir and home of every OS at the temp dir, so a command without
    /// `--config` never reads the developer's real config. `TAMIAS_CONFIG_DIR` is what covers
    /// Windows, where the OS config dir comes from a known-folder API that ignores env vars.
    fn isolate<'a>(&self, cmd: &'a mut Command) -> &'a mut Command {
        let home = self.path("home");
        cmd.env("TAMIAS_DATA_DIR", self.path("data"))
            .env("TAMIAS_CONFIG_DIR", self.dir.path())
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", self.path("xdg-config"))
            .env("XDG_DATA_HOME", self.path("xdg-data"))
            .env("APPDATA", self.path("appdata"))
            .env("LOCALAPPDATA", self.path("localappdata"))
            .env_remove("CLAUDE_CONFIG_DIR")
            // No `gh`: Copilot is skipped as "not set up" on every machine.
            .env("TAMIAS_GH", self.path("no-such-gh"))
    }

    fn tamias(&self) -> Command {
        let mut cmd = Command::cargo_bin("tamias").expect("binary tamias is built");
        self.isolate(&mut cmd)
            .env(
                "TAMIAS_CLAUDE_CREDENTIALS_FILE",
                self.path("credentials.json"),
            )
            .env("TAMIAS_CLAUDE_USAGE_URL", closed_url())
            .env("TZ", "UTC")
            .arg("--config")
            .arg(self.path("config.toml"));
        cmd
    }
}

/// URL of a port nobody listens on (any request fails fast).
fn closed_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    drop(listener);
    url
}

#[test]
fn a_failed_poll_exits_3_and_appends_an_error_line_without_the_token() {
    let env = Env::new();
    env.write_credentials();
    env.tamias().arg("collect").assert().code(3);
    env.tamias().arg("collect").assert().code(3);

    let log = fs::read_to_string(env.log_path()).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    // Two runs, each with one Claude and one Copilot line.
    assert_eq!(lines.len(), 4, "{log}");
    for line in lines.iter().step_by(2) {
        let fields: Vec<&str> = line.splitn(4, ' ').collect();
        assert!(fields[0].ends_with('Z'), "{line}");
        assert_eq!(fields[1..3], ["claude", "error"], "{line}");
        assert!(!line.contains("TEST-TOKEN"), "{line}");
    }
}

#[test]
fn a_skipped_provider_is_logged_as_ok() {
    let env = Env::new();
    fs::write(env.path("config.toml"), "[providers]\nclaude = false\n").unwrap();
    env.tamias().arg("collect").assert().success();
    let log = fs::read_to_string(env.log_path()).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(lines[0].ends_with(" claude ok skipped (disabled)"), "{log}");
    assert!(
        lines[1].ends_with(" copilot ok skipped (not set up)"),
        "{log}"
    );
}

#[test]
fn tamiasw_runs_collect_with_the_same_exit_code_and_log() {
    let env = Env::new();
    env.write_credentials();
    let mut cmd = Command::cargo_bin("tamiasw").expect("binary tamiasw is built");
    env.isolate(&mut cmd)
        .env(
            "TAMIAS_CLAUDE_CREDENTIALS_FILE",
            env.path("credentials.json"),
        )
        .env("TAMIAS_CLAUDE_USAGE_URL", closed_url())
        .assert()
        .code(3);
    let log = fs::read_to_string(env.log_path()).unwrap();
    assert!(log.contains(" claude error "), "{log}");
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
#[test]
fn service_on_unsupported_platforms_cannot_run_and_exits_1() {
    let env = Env::new();
    for action in ["install", "uninstall", "status"] {
        env.tamias()
            .args(["service", action])
            .assert()
            .code(1)
            .stderr(predicate::str::contains(
                "not supported yet on this platform",
            ));
    }
}

#[cfg(target_os = "macos")]
#[test]
fn service_status_shows_the_last_run_and_the_log_path() {
    let env = Env::new();
    env.write_credentials();
    env.tamias().arg("collect").assert().code(3);
    env.tamias()
        .args(["service", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude: last run "))
        .stdout(predicate::str::contains(
            "copilot: not polled yet (coming with #5)",
        ))
        .stdout(predicate::str::contains(format!(
            "log: {}",
            env.log_path().display()
        )))
        .stdout(predicate::str::contains("TEST-TOKEN").not());
}

#[test]
fn a_fatal_error_of_collect_is_logged_without_the_config_path() {
    let env = Env::new();
    fs::write(env.path("config.toml"), "bogus_key = 1\n").unwrap();
    env.tamias().arg("collect").assert().code(1);
    let log = fs::read_to_string(env.log_path()).unwrap();
    assert!(log.contains(" claude error fatal: "), "{log}");
    assert!(log.contains("bogus_key"), "{log}");
    assert!(
        !log.contains(env.path("config.toml").to_str().unwrap()),
        "{log}"
    );
    assert_eq!(log.lines().count(), 1, "{log}");
    // The config fails to load before the store is created, so no database exists to show an
    // error to `service status`.
    assert!(!env.path("data").join("tamias.db").exists());
}

#[test]
fn a_fatal_error_of_tamiasw_is_logged() {
    let env = Env::new();
    // A directory where the database file belongs: the store cannot be opened.
    fs::create_dir_all(env.path("data").join("tamias.db")).unwrap();
    let mut cmd = Command::cargo_bin("tamiasw").expect("binary tamiasw is built");
    env.isolate(&mut cmd).assert().code(1);
    let log = fs::read_to_string(env.log_path()).unwrap();
    assert!(log.contains(" claude error fatal: "), "{log}");
}
