//! `tamias collect` and `tamias today` against a temp data dir, a credentials file and a fake
//! usage endpoint on 127.0.0.1. No network, no Keychain.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use assert_cmd::Command;
use jiff::{SignedDuration, Timestamp};
use predicates::prelude::*;
use tamias_core::snapshot::{Provider, Window};
use tamias_core::store::Store;
use tempfile::TempDir;

const GH_TOKEN: &str = "gho_FAKE-GH-TEST-TOKEN-must-never-leak";
const TOKEN: &str = "sk-ant-oat01-CLI-TEST-TOKEN-must-never-leak";

struct Env {
    dir: TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        env.write_config("");
        env
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn gh_path(&self) -> PathBuf {
        self.path(if cfg!(windows) { "gh.cmd" } else { "gh" })
    }

    /// A fake `gh` that prints a token for `gh auth token`.
    fn fake_gh(&self) {
        let path = self.gh_path();
        if cfg!(windows) {
            std::fs::write(&path, format!("@echo off\r\necho {GH_TOKEN}\r\n")).unwrap();
        } else {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&path, format!("#!/bin/sh\necho {GH_TOKEN}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// The collect log, one entry per line.
    fn log_lines(&self) -> Vec<String> {
        let text = std::fs::read_to_string(self.path("data").join("logs").join("collect.log"))
            .unwrap_or_default();
        text.lines().map(str::to_owned).collect()
    }

    fn store(&self) -> Store {
        Store::open_read_only(self.path("data").join("tamias.db")).unwrap()
    }

    fn write_config(&self, text: &str) {
        std::fs::write(self.path("config.toml"), text).unwrap();
    }

    fn write_credentials(&self) {
        let expires = Timestamp::now()
            .checked_add(SignedDuration::from_hours(2))
            .unwrap()
            .as_millisecond();
        let json = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","refreshToken":"r","expiresAt":{expires}}}}}"#
        );
        std::fs::write(self.path("credentials.json"), json).unwrap();
    }

    /// `tamias <args>` with the temp data dir and config, the credentials file and `url`.
    fn tamias(&self, url: &str) -> Command {
        let mut cmd = Command::cargo_bin("tamias").expect("binary tamias is built");
        cmd.env("TAMIAS_DATA_DIR", self.path("data"))
            .env(
                "TAMIAS_CLAUDE_CREDENTIALS_FILE",
                self.path("credentials.json"),
            )
            .env("TAMIAS_CLAUDE_USAGE_URL", url)
            // Hermetic Copilot: no `gh` (unless a test writes `fake_gh`) and nothing to call.
            .env("TAMIAS_GH", self.gh_path())
            .env("TAMIAS_COPILOT_USER_URL", closed_url())
            .env("TZ", "UTC")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("NO_COLOR")
            .arg("--config")
            .arg(self.path("config.toml"));
        cmd
    }
}

/// Serves `responses` one connection each on 127.0.0.1; the request heads go to the receiver.
fn serve(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let _ = tx.send(String::from_utf8_lossy(&head).into_owned());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, rx)
}

/// URL of a port nobody listens on (any request fails fast).
fn closed_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn in_hours(hours: i64) -> String {
    Timestamp::now()
        .checked_add(SignedDuration::from_hours(hours))
        .unwrap()
        .to_string()
}

fn usage_ok() -> String {
    let body = format!(
        r#"{{"five_hour":{{"utilization":12.0,"resets_at":"{}"}},"seven_day":{{"utilization":34.0,"resets_at":"{}"}},"extra_usage":null}}"#,
        in_hours(2),
        in_hours(72)
    );
    response("200 OK", &body)
}

fn stdout_of(cmd: &mut Command) -> String {
    stdout_with_code(cmd, 0)
}

/// Stdout of `cmd`, asserting its exit code.
fn stdout_with_code(cmd: &mut Command, code: i32) -> String {
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn no_token() -> impl Predicate<str> {
    predicate::str::contains("TEST-TOKEN").not()
}

/// Loopback server answering one Copilot user request with `body`; returns the URL.
fn copilot_server(body: &str) -> String {
    let (url, _requests) = serve(vec![response("200 OK", body)]);
    url
}

fn copilot_paid() -> String {
    format!(
        r#"{{"quota_snapshots":{{"premium_interactions":{{"entitlement":300,"remaining":250,"unlimited":false,"has_quota":true}}}},"quota_reset_date_utc":"{}"}}"#,
        in_hours(240)
    )
}

fn copilot_free() -> String {
    r#"{"quota_snapshots":{"premium_interactions":{"entitlement":0,"remaining":0,"unlimited":false,"has_quota":false}},"quota_reset_date_utc":null}"#.to_owned()
}

fn copilot_snapshots(env: &Env) -> usize {
    let now = Timestamp::now();
    let from = now.checked_sub(SignedDuration::from_hours(1)).unwrap();
    let to = now.checked_add(SignedDuration::from_hours(1)).unwrap();
    env.store()
        .snapshots(Provider::Copilot, Window::Monthly, from, to)
        .unwrap()
        .len()
}

#[test]
fn collect_then_today_shows_both_claude_windows() {
    let env = Env::new();
    env.write_credentials();
    let (url, requests) = serve(vec![usage_ok()]);

    env.tamias(&url)
        .arg("collect")
        .assert()
        .success()
        .stdout("")
        .stderr(no_token());
    let head = requests.recv().unwrap().to_lowercase();
    assert!(head.starts_with("get /api/oauth/usage "), "{head}");
    assert!(head.contains(&format!("authorization: bearer {}", TOKEN.to_lowercase())));

    let table = stdout_of(env.tamias(&closed_url()).arg("today"));
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines.len(), 3, "{table}");
    assert!(lines[0].starts_with("PROVIDER  WINDOW"), "{table}");
    assert!(lines[1].starts_with("claude    seven_day  34%"), "{table}");
    assert!(lines[1].ends_with("green"), "{table}");
    assert!(
        lines[2].starts_with("claude    five_hour  12%   -      -"),
        "{table}"
    );
    assert!(lines[2].ends_with("green"), "{table}");
    assert!(
        !table.contains('\x1b'),
        "no colour when stdout is not a TTY"
    );
    assert!(
        std::fs::metadata(env.path("data").join("tamias.db")).is_ok(),
        "database in TAMIAS_DATA_DIR"
    );
}

#[test]
fn failing_endpoint_exits_3_and_today_shows_stale_with_the_error_class() {
    let env = Env::new();
    env.write_credentials();
    let (url, _requests) = serve(vec![response(
        "500 Internal Server Error",
        r#"{"error":"secret-looking body"}"#,
    )]);

    env.tamias(&url)
        .arg("collect")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("500").and(no_token()));

    env.tamias(&closed_url())
        .arg("today")
        .assert()
        .code(3)
        .stdout(
            predicate::str::contains("stale, never collected (http 500)")
                .and(predicate::str::contains("secret-looking").not()),
        );
}

#[test]
fn failure_after_a_success_is_stale_since_the_success() {
    let env = Env::new();
    env.write_credentials();
    let (url, _requests) = serve(vec![usage_ok()]);
    env.tamias(&url).arg("collect").assert().success();
    env.tamias(&closed_url())
        .arg("collect")
        .assert()
        .code(3)
        .stderr(no_token());

    let table = stdout_with_code(env.tamias(&closed_url()).arg("today"), 3);
    let seven_day = table.lines().nth(1).unwrap();
    assert!(seven_day.starts_with("claude    seven_day  34%"), "{table}");
    assert!(seven_day.contains("stale since "), "{table}");
    assert!(seven_day.ends_with("(network)"), "{table}");
}

#[test]
fn missing_credentials_with_auto_are_unavailable_and_exit_0() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = \"auto\"\n");
    // No credentials file written; the endpoint must not be called.
    let url = closed_url();

    env.tamias(&url).arg("collect").assert().success();
    env.tamias(&url)
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "claude    -       -     -      -       -       unavailable (not set up)",
        ));
}

#[test]
fn missing_credentials_with_true_are_an_error_and_stale() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = true\n");
    let url = closed_url();

    env.tamias(&url)
        .arg("collect")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no Claude credentials"));
    env.tamias(&url)
        .arg("today")
        .assert()
        .code(3)
        .stdout(predicate::str::contains(
            "stale, never collected (no credentials)",
        ));
}

#[test]
fn disabled_provider_is_skipped_and_shown_unavailable() {
    let env = Env::new();
    env.write_credentials();
    env.write_config("[providers]\nclaude = false\n");
    let url = closed_url();

    env.tamias(&url)
        .arg("collect")
        .assert()
        .success()
        .stderr("");
    env.tamias(&url)
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("unavailable (disabled)"));
}

#[test]
fn invalid_config_is_a_clear_error() {
    let env = Env::new();
    env.write_config("yelow_at = 0.5\n");
    let config = env.path("config.toml");
    for command in ["collect", "today"] {
        env.tamias(&closed_url())
            .arg(command)
            .assert()
            .code(1)
            .stdout("")
            .stderr(
                predicate::str::starts_with(format!("error: invalid config {}", config.display()))
                    .and(predicate::str::contains("yelow_at")),
            );
    }
}

#[test]
fn invalid_value_names_the_key() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = \"maybe\"\n");
    env.tamias(&closed_url())
        .arg("today")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "providers.claude = \"maybe\" must be \"auto\", true or false",
        ));
}

#[test]
fn missing_explicit_config_is_an_error() {
    let env = Env::new();
    std::fs::remove_file(env.path("config.toml")).unwrap();
    env.tamias(&closed_url())
        .arg("today")
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("config.toml").and(predicate::str::contains("cannot read")),
        );
}

#[test]
fn verbose_logs_debug_to_stderr_without_the_token() {
    let env = Env::new();
    env.write_credentials();
    let (url, _requests) = serve(vec![usage_ok()]);
    env.tamias(&url)
        .args(["-v", "collect"])
        .assert()
        .success()
        .stdout("")
        .stderr(
            predicate::str::contains("DEBUG")
                .and(predicate::str::contains("stored 2 snapshots"))
                .and(no_token()),
        );
}

#[test]
fn non_loopback_usage_url_is_ignored() {
    let env = Env::new();
    // No credentials file, so no request is made; the override is checked when building the client.
    env.write_config("[providers]\nclaude = true\n");
    env.tamias("http://example.com/api/oauth/usage")
        .arg("collect")
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "ignoring TAMIAS_CLAUDE_USAGE_URL: only http loopback URLs are allowed",
        ));
}

/// Collects one successful poll (seven-day 34 %, five-hour 12 %) into `env`.
fn seed_ok(env: &Env) {
    env.write_credentials();
    let (url, _requests) = serve(vec![usage_ok()]);
    env.tamias(&url).arg("collect").assert().success();
}

#[test]
fn short_shows_the_binding_window_and_exits_0() {
    let env = Env::new();
    seed_ok(&env);
    env.tamias(&closed_url())
        .args(["today", "--short"])
        .assert()
        .code(0)
        .stdout("C 34%\n");
}

#[test]
fn short_colour_always_colours_the_segment() {
    let env = Env::new();
    seed_ok(&env);
    env.tamias(&closed_url())
        .args(["today", "--short", "--color", "always"])
        .assert()
        .code(0)
        .stdout("\x1b[32mC 34%\x1b[0m\n");
}

#[test]
fn short_with_no_color_has_no_escape_codes() {
    let env = Env::new();
    seed_ok(&env);
    env.tamias(&closed_url())
        .env("NO_COLOR", "1")
        .args(["today", "--short", "--color", "auto"])
        .assert()
        .code(0)
        .stdout("C 34%\n");
    env.tamias(&closed_url())
        .env("NO_COLOR", "1")
        .args(["today", "--short", "--color", "never"])
        .assert()
        .code(0)
        .stdout("C 34%\n");
}

#[test]
fn short_stale_provider_is_a_question_mark_and_exits_3() {
    let env = Env::new();
    seed_ok(&env);
    env.tamias(&closed_url()).arg("collect").assert().code(3);
    env.tamias(&closed_url())
        .args(["today", "--short"])
        .assert()
        .code(3)
        .stdout("C ?\n");
    env.tamias(&closed_url())
        .args(["today", "--short", "--color", "always"])
        .assert()
        .code(3)
        .stdout("\x1b[90mC ?\x1b[0m\n");
}

#[test]
fn short_with_nothing_set_up_is_an_empty_line_and_exits_0() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = \"auto\"\n");
    env.tamias(&closed_url())
        .args(["today", "--short"])
        .assert()
        .code(0)
        .stdout("\n");
    env.write_config("[providers]\nclaude = false\n");
    env.tamias(&closed_url())
        .args(["today", "--short", "--color", "always"])
        .assert()
        .code(0)
        .stdout("\n");
}

#[test]
fn unreadable_store_exits_1_in_every_mode() {
    let env = Env::new();
    std::fs::create_dir_all(env.path("data")).unwrap();
    std::fs::write(
        env.path("data").join("tamias.db"),
        "this is not a sqlite database, padded to more than one header length",
    )
    .unwrap();
    for args in [&["today"][..], &["today", "--short"][..], &["collect"][..]] {
        env.tamias(&closed_url())
            .args(args)
            .assert()
            .code(1)
            .stdout("")
            .stderr(predicate::str::starts_with("error: "));
    }
}

#[test]
fn fatal_collect_error_does_not_make_today_stale() {
    let env = Env::new();
    env.write_credentials();
    let (url, _requests) = serve(vec![usage_ok()]);
    env.tamias(&url).arg("collect").assert().code(0);
    env.write_config("bogus_key = 1\n");
    env.tamias(&closed_url()).arg("collect").assert().code(1);
    env.write_config("");
    let json = stdout_of(env.tamias(&closed_url()).args(["today", "--json"]));
    assert!(
        json.contains(r#""status": "ok""#) || json.contains(r#""status":"ok""#),
        "{json}"
    );
    assert!(!json.contains("stale"), "{json}");
    assert!(!json.contains("fatal"), "{json}");
}

#[test]
fn provider_failure_with_a_failing_record_error_writes_one_error_line() {
    let env = Env::new();
    env.write_credentials();
    let (ok_url, _requests) = serve(vec![usage_ok()]);
    env.tamias(&ok_url).arg("collect").assert().code(0);
    // From now on every write of an error into the store fails.
    let db = rusqlite::Connection::open(env.path("data").join("tamias.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_error_write BEFORE UPDATE ON source_status
         BEGIN SELECT RAISE(ABORT, 'store is read-only'); END;",
    )
    .unwrap();
    drop(db);
    let (url, _requests) = serve(vec![response("500 Internal Server Error", "{}")]);

    env.tamias(&url).arg("collect").assert().code(1);

    let log = std::fs::read_to_string(env.path("data").join("logs").join("collect.log")).unwrap();
    let errors: Vec<&str> = log
        .lines()
        .filter(|l| l.contains(" claude error "))
        .collect();
    assert_eq!(errors.len(), 1, "{log}");
    assert!(errors[0].contains("fatal: "), "{errors:?}");
}

#[test]
fn invalid_color_value_is_a_usage_error() {
    let env = Env::new();
    env.tamias(&closed_url())
        .args(["today", "--color", "sometimes"])
        .assert()
        .code(2);
}

#[test]
fn help_documents_the_exit_codes() {
    let env = Env::new();
    for command in ["today", "collect"] {
        env.tamias(&closed_url())
            .args([command, "--help"])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("Exit codes:")
                    .and(predicate::str::contains("  1  error"))
                    .and(predicate::str::contains(
                        "  3  at least one set-up provider",
                    )),
            );
    }
}

#[test]
fn paid_copilot_seat_stores_one_monthly_snapshot_next_to_claude() {
    let env = Env::new();
    env.write_credentials();
    env.fake_gh();
    let (claude_url, _r) = serve(vec![usage_ok()]);
    let copilot_url = copilot_server(&copilot_paid());

    env.tamias(&claude_url)
        .env("TAMIAS_COPILOT_USER_URL", copilot_url)
        .arg("collect")
        .assert()
        .success()
        .stderr(predicate::str::contains("GH-TEST-TOKEN").not());

    assert_eq!(copilot_snapshots(&env), 1);
    let store = env.store();
    let status = store.status(Provider::Copilot).unwrap().unwrap();
    assert!(status.last_ok_at.is_some());
    assert_eq!(status.unavailable, None);
    assert!(
        store
            .status(Provider::Claude)
            .unwrap()
            .unwrap()
            .last_ok_at
            .is_some()
    );
    let log = env.log_lines();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].ends_with("claude ok stored 2 snapshots"), "{log:?}");
    assert!(log[1].ends_with("copilot ok stored 1 snapshots"), "{log:?}");
}

#[test]
fn free_copilot_seat_is_recorded_unavailable_without_a_snapshot() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = false\n");
    env.fake_gh();
    let copilot_url = copilot_server(&copilot_free());

    env.tamias(&closed_url())
        .env("TAMIAS_COPILOT_USER_URL", copilot_url)
        .arg("collect")
        .assert()
        .success();

    assert_eq!(copilot_snapshots(&env), 0);
    let status = env.store().status(Provider::Copilot).unwrap().unwrap();
    assert_eq!(status.unavailable.as_deref(), Some("no premium quota"));
    assert!(status.last_ok_at.is_some());
    let log = env.log_lines();
    assert!(
        log[1].ends_with("copilot ok skipped (no premium quota)"),
        "{log:?}"
    );
}

#[test]
fn unlimited_copilot_seat_is_recorded_unavailable_with_its_reason() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = false\n");
    env.fake_gh();
    let copilot_url = copilot_server(
        r#"{"quota_snapshots":{"premium_interactions":{"entitlement":0,"remaining":0,"unlimited":true,"has_quota":true}}}"#,
    );
    env.tamias(&closed_url())
        .env("TAMIAS_COPILOT_USER_URL", copilot_url)
        .arg("collect")
        .assert()
        .success();
    let status = env.store().status(Provider::Copilot).unwrap().unwrap();
    assert_eq!(status.unavailable.as_deref(), Some("unlimited"));
}

#[test]
fn a_later_paid_poll_clears_the_unavailable_reason() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = false\n");
    env.fake_gh();
    let free = copilot_server(&copilot_free());
    env.tamias(&closed_url())
        .env("TAMIAS_COPILOT_USER_URL", free)
        .arg("collect")
        .assert()
        .success();
    let paid = copilot_server(&copilot_paid());
    env.tamias(&closed_url())
        .env("TAMIAS_COPILOT_USER_URL", paid)
        .arg("collect")
        .assert()
        .success();
    assert_eq!(
        env.store()
            .status(Provider::Copilot)
            .unwrap()
            .unwrap()
            .unavailable,
        None
    );
    assert_eq!(copilot_snapshots(&env), 1);
}

#[test]
fn missing_gh_with_auto_is_skipped_not_set_up() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = false\n");
    // No fake gh written.
    env.tamias(&closed_url()).arg("collect").assert().success();
    assert_eq!(env.store().status(Provider::Copilot).unwrap(), None);
    let log = env.log_lines();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(
        log[1].ends_with("copilot ok skipped (not set up)"),
        "{log:?}"
    );
}

#[test]
fn missing_gh_with_true_is_an_error_and_recorded() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = false\ncopilot = true\n");
    env.tamias(&closed_url())
        .arg("collect")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no GitHub credentials"));
    let status = env.store().status(Provider::Copilot).unwrap().unwrap();
    assert_eq!(status.last_error.as_deref(), Some("no credentials"));
    assert!(env.log_lines()[1].ends_with("copilot error no credentials"));
}

#[test]
fn disabled_copilot_is_skipped() {
    let env = Env::new();
    env.write_config("[providers]\nclaude = false\ncopilot = false\n");
    env.fake_gh();
    env.tamias(&closed_url()).arg("collect").assert().success();
    assert_eq!(env.store().status(Provider::Copilot).unwrap(), None);
    assert!(env.log_lines()[1].ends_with("copilot ok skipped (disabled)"));
}

#[test]
fn gh_path_of_the_config_is_used() {
    let env = Env::new();
    env.fake_gh();
    let gh = env.gh_path();
    env.write_config(&format!(
        "gh_path = {:?}\n[providers]\nclaude = false\n",
        gh.display().to_string()
    ));
    let copilot_url = copilot_server(&copilot_paid());
    env.tamias(&closed_url())
        .env_remove("TAMIAS_GH")
        .env("TAMIAS_COPILOT_USER_URL", copilot_url)
        .arg("collect")
        .assert()
        .success();
    assert_eq!(copilot_snapshots(&env), 1);
}

#[test]
fn copilot_failure_never_skips_claude_and_exits_3() {
    let env = Env::new();
    env.write_credentials();
    env.fake_gh();
    let (claude_url, _r) = serve(vec![usage_ok()]);
    let (copilot_url, _r2) = serve(vec![response("500 Internal Server Error", "{}")]);

    env.tamias(&claude_url)
        .env("TAMIAS_COPILOT_USER_URL", copilot_url)
        .arg("collect")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("GH-TEST-TOKEN").not());

    let store = env.store();
    let claude = store.status(Provider::Claude).unwrap().unwrap();
    assert!(claude.last_ok_at.is_some());
    assert_eq!(claude.last_error, None);
    let copilot = store.status(Provider::Copilot).unwrap().unwrap();
    assert_eq!(copilot.last_error.as_deref(), Some("http 500"));
    let log = env.log_lines();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].ends_with("claude ok stored 2 snapshots"), "{log:?}");
    assert!(log[1].ends_with("copilot error http 500"), "{log:?}");
}

#[test]
fn claude_failure_does_not_skip_copilot() {
    let env = Env::new();
    env.write_credentials();
    env.fake_gh();
    let copilot_url = copilot_server(&copilot_paid());
    env.tamias(&closed_url())
        .env("TAMIAS_COPILOT_USER_URL", copilot_url)
        .arg("collect")
        .assert()
        .code(3);
    assert_eq!(copilot_snapshots(&env), 1);
    assert_eq!(env.log_lines().len(), 2);
}
