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
use tempfile::TempDir;

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
    let output = cmd.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn no_token() -> impl Predicate<str> {
    predicate::str::contains("TEST-TOKEN").not()
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
fn failing_endpoint_exits_1_and_today_shows_stale_with_the_error_class() {
    let env = Env::new();
    env.write_credentials();
    let (url, _requests) = serve(vec![response(
        "500 Internal Server Error",
        r#"{"error":"secret-looking body"}"#,
    )]);

    env.tamias(&url)
        .arg("collect")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("500").and(no_token()));

    env.tamias(&closed_url())
        .arg("today")
        .assert()
        .success()
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
        .code(1)
        .stderr(no_token());

    let table = stdout_of(env.tamias(&closed_url()).arg("today"));
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
        .code(1)
        .stderr(predicate::str::contains("no Claude credentials"));
    env.tamias(&url)
        .arg("today")
        .assert()
        .success()
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
            .code(2)
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
        .code(2)
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
        .code(2)
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
        .code(1)
        .stderr(predicate::str::contains(
            "ignoring TAMIAS_CLAUDE_USAGE_URL: only http loopback URLs are allowed",
        ));
}
