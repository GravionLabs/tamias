//! `tamias collect` writes the collect log; `tamias service` on every OS. Nothing is installed:
//! only `service status` (read-only) runs against the real scheduler, with a temp HOME.

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;

use assert_cmd::Command;
use jiff::{SignedDuration, Timestamp};
#[cfg(not(target_os = "linux"))]
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

    fn tamias(&self) -> Command {
        let mut cmd = Command::cargo_bin("tamias").expect("binary tamias is built");
        cmd.env("TAMIAS_DATA_DIR", self.path("data"))
            .env(
                "TAMIAS_CLAUDE_CREDENTIALS_FILE",
                self.path("credentials.json"),
            )
            .env("TAMIAS_CLAUDE_USAGE_URL", closed_url())
            .env("HOME", self.path("home"))
            .env("TZ", "UTC")
            .env_remove("CLAUDE_CONFIG_DIR")
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
    assert_eq!(lines.len(), 2, "{log}");
    for line in lines {
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
    assert!(log.ends_with(" claude ok skipped (disabled)\n"), "{log}");
}

#[test]
fn tamiasw_runs_collect_with_the_same_exit_code_and_log() {
    let env = Env::new();
    env.write_credentials();
    let mut cmd = Command::cargo_bin("tamiasw").expect("binary tamiasw is built");
    cmd.env("TAMIAS_DATA_DIR", env.path("data"))
        .env(
            "TAMIAS_CLAUDE_CREDENTIALS_FILE",
            env.path("credentials.json"),
        )
        .env("TAMIAS_CLAUDE_USAGE_URL", closed_url())
        .env("HOME", env.path("home"))
        .env_remove("CLAUDE_CONFIG_DIR")
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
        .stdout(predicate::str::contains("copilot: no run yet"))
        .stdout(predicate::str::contains(format!(
            "log: {}",
            env.log_path().display()
        )))
        .stdout(predicate::str::contains("TEST-TOKEN").not());
}
