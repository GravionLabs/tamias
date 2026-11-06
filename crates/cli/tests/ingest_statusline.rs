//! `tamias ingest-statusline` against a temp data dir. No network: the usage endpoint and `gh`
//! point at nothing, and a test fails if the command needed them.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use jiff::{SignedDuration, Timestamp};
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};
use tamias_core::store::Store;
use tempfile::TempDir;

/// Runtime bound of one call: 100 ms in a release build (CI), generous in debug where the
/// process start and the unoptimised SQLite dominate.
const BUDGET: Duration = if cfg!(debug_assertions) {
    Duration::from_millis(1500)
} else {
    Duration::from_millis(100)
};

struct Env {
    dir: TempDir,
}

impl Env {
    fn new(config: &str) -> Self {
        let env = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        std::fs::write(env.path("config.toml"), config).unwrap();
        env
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn store(&self) -> Store {
        Store::open(self.path("data").join("tamias.db")).unwrap()
    }

    fn ingest(&self, stdin: &str, args: &[&str]) -> (String, i32) {
        let output = Command::cargo_bin("tamias")
            .unwrap()
            .env("TAMIAS_DATA_DIR", self.path("data"))
            .env("TAMIAS_CLAUDE_USAGE_URL", "http://127.0.0.1:1/never")
            .env("TAMIAS_GH", self.path("no-gh"))
            .env("TAMIAS_COPILOT_USER_URL", "http://127.0.0.1:1/never")
            .env("TZ", "UTC")
            .env_remove("NO_COLOR")
            .env_remove("CLAUDE_CONFIG_DIR")
            .arg("--config")
            .arg(self.path("config.toml"))
            .arg("ingest-statusline")
            .args(args)
            .write_stdin(stdin.to_owned())
            .output()
            .unwrap();
        (
            String::from_utf8(output.stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn statusline_snapshots(&self, window: Window) -> Vec<Snapshot> {
        let now = Timestamp::now();
        let from = now.checked_sub(SignedDuration::from_hours(48)).unwrap();
        let to = now.checked_add(SignedDuration::from_hours(48)).unwrap();
        self.store()
            .snapshots(Provider::Claude, window, from, to)
            .unwrap()
            .into_iter()
            .filter(|s| s.source == Source::Statusline)
            .collect()
    }
}

const CLAUDE_ON: &str = "[providers]\nclaude = true\ncopilot = false\n";

fn body(five: f64, seven: f64) -> String {
    let now = Timestamp::now().as_second();
    format!(
        r#"{{"session_id":"x","rate_limits":{{"five_hour":{{"used_percentage":{five},"resets_at":{}}},"seven_day":{{"used_percentage":{seven},"resets_at":{}}}}}}}"#,
        now + 3600,
        now + 3 * 86_400
    )
}

#[test]
fn stores_two_snapshots_sets_last_ok_and_prints_the_line_with_ansi() {
    let env = Env::new(CLAUDE_ON);
    let (out, code) = env.ingest(&body(12.0, 34.0), &[]);
    assert_eq!(code, 0);
    assert!(out.contains("C 34%"), "{out:?}");
    assert!(out.contains("\x1b["), "ansi by default: {out:?}");
    assert_eq!(env.statusline_snapshots(Window::FiveHour).len(), 1);
    let seven = env.statusline_snapshots(Window::SevenDay);
    assert_eq!(seven.len(), 1);
    assert_eq!((seven[0].used, seven[0].limit), (34.0, 100.0));
    assert_eq!(seven[0].unit, Unit::Percent);
    let status = env.store().status(Provider::Claude).unwrap().unwrap();
    assert!(status.last_ok_at.is_some());
}

#[test]
fn color_never_prints_plain_text() {
    let env = Env::new(CLAUDE_ON);
    let (out, code) = env.ingest(&body(12.0, 34.0), &["--color", "never"]);
    assert_eq!((out.as_str(), code), ("C 34%\n", 0));
}

#[test]
fn identical_call_within_five_minutes_stores_nothing_and_changed_value_stores() {
    let env = Env::new(CLAUDE_ON);
    let input = body(12.0, 34.0);
    env.ingest(&input, &[]);
    let ok_before = env.store().status(Provider::Claude).unwrap().unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    env.ingest(&input, &[]);
    assert_eq!(env.statusline_snapshots(Window::SevenDay).len(), 1);
    let ok_after = env.store().status(Provider::Claude).unwrap().unwrap();
    assert_eq!(ok_before.last_ok_at, ok_after.last_ok_at, "no write at all");
    env.ingest(&body(12.0, 35.0), &[]);
    assert_eq!(env.statusline_snapshots(Window::SevenDay).len(), 2);
    assert_eq!(env.statusline_snapshots(Window::FiveHour).len(), 1);
}

#[test]
fn garbage_and_empty_stdin_print_a_line_and_exit_zero() {
    let env = Env::new(CLAUDE_ON);
    for input in [
        "not json",
        "",
        "[]",
        r#"{"rate_limits":{"five_hour":{"used_percentage":500,"resets_at":1}}}"#,
    ] {
        let (out, code) = env.ingest(input, &["--color", "never"]);
        assert_eq!(code, 0, "{input:?}");
        assert!(out.ends_with('\n'), "{input:?}: {out:?}");
    }
    assert_eq!(
        env.statusline_snapshots(Window::FiveHour),
        [] as [tamias_core::snapshot::Snapshot; 0]
    );
}

#[test]
fn no_rate_limits_stores_nothing() {
    let env = Env::new(CLAUDE_ON);
    let (_, code) = env.ingest(r#"{"rate_limits_available":false,"rate_limits":null}"#, &[]);
    assert_eq!(code, 0);
    assert_eq!(
        env.statusline_snapshots(Window::SevenDay),
        [] as [tamias_core::snapshot::Snapshot; 0]
    );
}

#[test]
fn disabled_claude_stores_nothing() {
    let env = Env::new("[providers]\nclaude = false\ncopilot = false\n");
    let (out, code) = env.ingest(&body(12.0, 34.0), &["--color", "never"]);
    assert_eq!((out.as_str(), code), ("\n", 0));
    assert_eq!(
        env.statusline_snapshots(Window::SevenDay),
        [] as [tamias_core::snapshot::Snapshot; 0]
    );
    assert!(env.store().status(Provider::Claude).unwrap().is_none());
}

#[test]
fn poll_and_statusline_snapshot_of_one_window_count_once() {
    let env = Env::new(CLAUDE_ON);
    let now = Timestamp::now();
    let resets_at = now.checked_add(SignedDuration::from_hours(72)).unwrap();
    let poll = Snapshot {
        provider: Provider::Claude,
        window: Window::SevenDay,
        used: 30.0,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at,
        observed_at: now.checked_sub(SignedDuration::from_mins(10)).unwrap(),
        source: Source::Poll,
    };
    env.store()
        .record_run(Provider::Claude, &[poll], poll_time(now))
        .unwrap();
    let json = |env: &Env| {
        let output = Command::cargo_bin("tamias")
            .unwrap()
            .env("TAMIAS_DATA_DIR", env.path("data"))
            .env("TZ", "UTC")
            .arg("--config")
            .arg(env.path("config.toml"))
            .args(["today", "--json"])
            .output()
            .unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let seven = doc["providers"][0]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["window"] == "seven_day")
            .unwrap()
            .clone();
        seven["usage"].as_f64().or(seven["used"].as_f64()).unwrap()
    };
    let before = json(&env);
    let body = format!(
        r#"{{"rate_limits":{{"seven_day":{{"used_percentage":30,"resets_at":{}}}}}}}"#,
        resets_at.as_second()
    );
    env.ingest(&body, &[]);
    assert_eq!(env.statusline_snapshots(Window::SevenDay).len(), 1);
    assert_eq!(
        json(&env).to_bits(),
        before.to_bits(),
        "the same value from two sources counts once"
    );
}

fn poll_time(now: Timestamp) -> Timestamp {
    now.checked_sub(SignedDuration::from_mins(10)).unwrap()
}

#[test]
fn earlier_poll_error_stays_and_later_ingest_makes_claude_ok() {
    let env = Env::new(CLAUDE_ON);
    let error_at = Timestamp::now()
        .checked_sub(SignedDuration::from_mins(2))
        .unwrap();
    env.store()
        .record_error(Provider::Claude, error_at, "expired")
        .unwrap();
    env.ingest(&body(12.0, 34.0), &[]);
    let status = env.store().status(Provider::Claude).unwrap().unwrap();
    assert_eq!(status.last_error.as_deref(), Some("expired"));
    assert!(status.last_ok_at > status.last_error_at);
}

#[test]
fn finishes_fast_on_a_store_with_a_month_of_snapshots() {
    let env = Env::new(CLAUDE_ON);
    let now = Timestamp::now();
    let resets_at = now.checked_add(SignedDuration::from_hours(72)).unwrap();
    let mut snapshots = Vec::new();
    for minute in 1..=(30 * 24 * 60 / 5) {
        let observed_at = now
            .checked_sub(SignedDuration::from_mins(5 * minute))
            .unwrap();
        for window in [Window::FiveHour, Window::SevenDay] {
            snapshots.push(Snapshot {
                provider: Provider::Claude,
                window,
                used: 10.0 + f64::from(i32::try_from(minute % 50).unwrap()),
                limit: 100.0,
                unit: Unit::Percent,
                resets_at,
                observed_at,
                source: Source::Poll,
            });
        }
    }
    env.store()
        .record_run(Provider::Claude, &snapshots, now)
        .unwrap();
    let input = body(12.0, 34.0);
    env.ingest(&input, &[]); // warm up the file cache, not the code path under test
    let started = Instant::now();
    let (out, code) = env.ingest(&body(12.0, 35.0), &[]);
    let elapsed = started.elapsed();
    assert_eq!(code, 0);
    // The high-water mark of the polled history (up to 59 %) wins over the new 35 %.
    assert!(out.contains("C 59%"), "{out:?}");
    assert!(elapsed < BUDGET, "took {elapsed:?}, budget {BUDGET:?}");
}
