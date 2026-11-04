//! `tamias today --json` against a temp data dir with seeded snapshots, validated against
//! `docs/schema/today-v1.schema.json`. No network, no Keychain.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};
use tamias_core::store::Store;
use tempfile::TempDir;

fn repo_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn read_json(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(repo_file(path)).unwrap()).unwrap()
}

/// The published schema as is: it allows unknown fields (compatibility rule of docs/consumers.md).
fn published_validator() -> jsonschema::Validator {
    jsonschema::draft202012::new(&read_json("docs/schema/today-v1.schema.json"))
        .expect("the schema is valid draft 2020-12")
}

/// Sets `additionalProperties: false` on every object schema, so a field the CLI adds without
/// the schema knowing it fails the tests.
fn strictify(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            if map.get("type") == Some(&json!("object")) {
                map.insert("additionalProperties".into(), json!(false));
            }
            map.values_mut().for_each(strictify);
        }
        Value::Array(items) => items.iter_mut().for_each(strictify),
        _ => {}
    }
}

/// The strict copy used for the CLI output and the negative cases.
fn validator() -> jsonschema::Validator {
    let mut schema = read_json("docs/schema/today-v1.schema.json");
    strictify(&mut schema);
    jsonschema::draft202012::new(&schema).expect("the strict schema is valid draft 2020-12")
}

fn assert_valid(document: &Value) {
    let errors: Vec<String> = validator()
        .iter_errors(document)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}\n{document:#}");
}

fn assert_invalid(document: &Value) {
    assert!(!validator().is_valid(document), "{document:#}");
}

struct Env {
    dir: TempDir,
}

impl Env {
    /// Claude set to `claude` (`true` or `false`) so no credential is probed.
    ///
    /// Copilot is `auto` and there is no `gh` (see [`Env::tamias`]): `unavailable`, not set up.
    fn new(claude: bool) -> Self {
        Self::with_copilot(claude, "\"auto\"")
    }

    /// Like [`Env::new`] with the `copilot` setting `copilot` (a TOML value).
    fn with_copilot(claude: bool, copilot: &str) -> Self {
        let env = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        std::fs::write(
            env.path("config.toml"),
            format!("[providers]\nclaude = {claude}\ncopilot = {copilot}\n"),
        )
        .unwrap();
        env
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn store(&self) -> Store {
        Store::open(self.path("data").join("tamias.db")).unwrap()
    }

    fn tamias(&self) -> Command {
        let mut cmd = Command::cargo_bin("tamias").expect("binary tamias is built");
        cmd.env("TAMIAS_DATA_DIR", self.path("data"))
            .env("TAMIAS_CLAUDE_CREDENTIALS_FILE", self.path("missing.json"))
            .env("TAMIAS_GH", self.path("no-such-gh"))
            .env("TZ", "UTC")
            .env_remove("NO_COLOR")
            .arg("--config")
            .arg(self.path("config.toml"));
        cmd
    }

    /// `tamias today --json`: exits 0, stdout is one JSON document.
    fn json(&self) -> Value {
        self.json_with_code(0)
    }

    /// `tamias today --json`: exits `code` (3 when a provider is stale), stdout is one JSON
    /// document.
    fn json_with_code(&self, code: i32) -> Value {
        let output = self.tamias().args(["today", "--json"]).output().unwrap();
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains('\x1b'), "no ANSI codes: {stdout:?}");
        serde_json::from_str(&stdout).unwrap()
    }
}

fn at(offset: SignedDuration) -> Timestamp {
    Timestamp::now().checked_add(offset).unwrap()
}

fn snap(provider: Provider, window: Window, used: f64, resets_in_hours: i64) -> Snapshot {
    let (limit, unit) = match window {
        Window::Monthly => (300.0, Unit::Requests),
        _ => (100.0, Unit::Percent),
    };
    Snapshot {
        provider,
        window,
        used,
        limit,
        unit,
        resets_at: at(SignedDuration::from_hours(resets_in_hours)),
        observed_at: at(SignedDuration::from_secs(-60)),
        source: Source::Poll,
    }
}

fn seed_claude(store: &mut Store) {
    let snapshots = [
        snap(Provider::Claude, Window::SevenDay, 34.0, 72),
        snap(Provider::Claude, Window::FiveHour, 12.0, 2),
    ];
    store
        .record_run(
            Provider::Claude,
            &snapshots,
            at(SignedDuration::from_secs(-60)),
        )
        .unwrap();
}

#[test]
fn claude_ok_is_valid() {
    let env = Env::new(true);
    seed_claude(&mut env.store());

    let doc = env.json();
    assert_valid(&doc);
    assert_eq!(doc["schema_version"], 1);
    let claude = &doc["providers"][0];
    assert_eq!(claude["provider"], "claude");
    assert_eq!(claude["status"], "ok");
    assert!(claude.get("stale_since").is_none());
    let windows = claude["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0]["window"], "seven_day");
    assert_eq!(windows[0]["binding"], true);
    assert_eq!(windows[0]["used"], 34.0);
    assert!(windows[0]["budget"].is_number(), "{doc:#}");
    assert_eq!(windows[1]["window"], "five_hour");
    assert_eq!(windows[1]["used"], 12.0);
    assert!(windows[1].get("budget").is_none());
    assert_eq!(
        doc["providers"][1],
        json!({"provider": "copilot", "status": "unavailable", "windows": []}),
        "no gh"
    );
}

#[test]
fn json_and_table_show_the_same_numbers() {
    let env = Env::new(true);
    seed_claude(&mut env.store());

    let doc = env.json();
    let seven_day = &doc["providers"][0]["windows"][0];
    let output = env.tamias().arg("today").output().unwrap();
    let table = String::from_utf8(output.stdout).unwrap();
    let row = table.lines().nth(1).unwrap();
    let expected = format!(
        "claude    seven_day  {:.0}%   {:.1}%   {:.1}%",
        seven_day["used"].as_f64().unwrap(),
        seven_day["today_usage"].as_f64().unwrap(),
        seven_day["budget"].as_f64().unwrap(),
    );
    assert!(row.starts_with(&expected), "{row:?} vs {expected:?}");
}

#[test]
fn claude_stale_is_valid_with_grey_windows() {
    let env = Env::new(true);
    let mut store = env.store();
    seed_claude(&mut store);
    store
        .record_error(Provider::Claude, Timestamp::now(), "network")
        .unwrap();

    let doc = env.json_with_code(3);
    assert_valid(&doc);
    let claude = &doc["providers"][0];
    assert_eq!(claude["status"], "stale");
    assert!(claude["stale_since"].is_string(), "{doc:#}");
    let windows = claude["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    assert!(windows.iter().all(|w| w["colour"] == "grey"), "{doc:#}");
}

#[test]
fn never_collected_claude_is_stale_without_windows() {
    let env = Env::new(true);
    let doc = env.json_with_code(3);
    assert_valid(&doc);
    assert_eq!(
        doc["providers"],
        json!([
            {"provider": "claude", "status": "stale", "windows": []},
            {"provider": "copilot", "status": "unavailable", "windows": []}
        ])
    );
}

#[test]
fn unavailable_provider_is_valid() {
    let env = Env::new(false);
    let doc = env.json();
    assert_valid(&doc);
    assert_eq!(
        doc["providers"],
        json!([
            {"provider": "claude", "status": "unavailable", "windows": []},
            {"provider": "copilot", "status": "unavailable", "windows": []}
        ])
    );
}

#[test]
fn seeded_copilot_monthly_window_is_valid() {
    let env = Env::with_copilot(true, "true");
    seed_claude(&mut env.store());
    let snapshots = [snap(Provider::Copilot, Window::Monthly, 120.0, 20 * 24)];
    env.store()
        .record_run(
            Provider::Copilot,
            &snapshots,
            at(SignedDuration::from_secs(-60)),
        )
        .unwrap();

    let doc = env.json();
    assert_valid(&doc);
    let copilot = &doc["providers"][1];
    assert_eq!(copilot["provider"], "copilot");
    assert_eq!(copilot["status"], "ok");
    assert_eq!(doc["providers"][0]["status"], "ok");
    let monthly = &copilot["windows"][0];
    assert_eq!(monthly["window"], "monthly");
    assert_eq!(monthly["unit"], "requests");
    assert_eq!(monthly["binding"], true);
    assert_eq!(monthly["used"], 120.0);
    assert_eq!(monthly["limit"], 300.0);
    assert!(monthly["budget"].is_number(), "{doc:#}");
}

#[test]
fn copilot_unavailable_from_a_stored_reason_is_valid() {
    for reason in ["no premium quota", "unlimited"] {
        let env = Env::with_copilot(true, "true");
        let mut store = env.store();
        seed_claude(&mut store);
        store
            .record_unavailable(Provider::Copilot, Timestamp::now(), reason)
            .unwrap();

        let doc = env.json();

        assert_valid(&doc);
        assert_eq!(doc["providers"][0]["status"], "ok");
        assert_eq!(
            doc["providers"][1],
            json!({"provider": "copilot", "status": "unavailable", "windows": []}),
            "{reason}"
        );
    }
}

#[test]
fn short_shows_both_providers() {
    let env = Env::with_copilot(true, "true");
    let mut store = env.store();
    seed_claude(&mut store);
    store
        .record_run(
            Provider::Copilot,
            &[snap(Provider::Copilot, Window::Monthly, 120.0, 20 * 24)],
            at(SignedDuration::from_secs(-60)),
        )
        .unwrap();

    env.tamias()
        .args(["today", "--short", "--color", "never"])
        .assert()
        .code(0)
        .stdout("C 34% · CP 40%\n");
}

#[test]
fn short_leaves_out_an_unavailable_provider() {
    let env = Env::new(true);
    seed_claude(&mut env.store());
    env.tamias()
        .args(["today", "--short", "--color", "never"])
        .assert()
        .code(0)
        .stdout("C 34%\n");
}

#[test]
fn stale_json_exits_3_and_is_valid_json_without_ansi_even_with_color_always() {
    let env = Env::new(true);
    let mut store = env.store();
    seed_claude(&mut store);
    store
        .record_error(Provider::Claude, Timestamp::now(), "network")
        .unwrap();

    let output = env
        .tamias()
        .args(["today", "--json", "--color", "always"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains('\x1b'), "no ANSI codes: {stdout:?}");
    let doc: Value = serde_json::from_str(&stdout).unwrap();
    assert_valid(&doc);
    assert_eq!(doc["providers"][0]["status"], "stale");
}

#[test]
fn json_and_short_show_the_same_used_percent() {
    let env = Env::new(true);
    seed_claude(&mut env.store());

    let doc = env.json();
    let used = doc["providers"][0]["windows"][0]["used"].as_f64().unwrap();
    env.tamias()
        .args(["today", "--short", "--color", "never"])
        .assert()
        .code(0)
        .stdout(format!("C {used:.0}%\n"));
}

#[test]
fn json_and_short_together_are_a_usage_error() {
    let env = Env::new(true);
    env.tamias()
        .args(["today", "--json", "--short"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn logs_go_to_stderr_and_stdout_is_only_json() {
    let env = Env::new(true);
    seed_claude(&mut env.store());
    let output = env
        .tamias()
        .args(["-v", "today", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("DEBUG"), "{stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: Value = serde_json::from_str(&stdout).unwrap();
    assert_valid(&doc);
    assert!(stdout.ends_with("}\n"), "{stdout:?}");
}

#[test]
fn pinned_example_document_is_valid() {
    assert_valid(&read_json("docs/schema/today-v1.example.json"));
}

/// Applies `change` to the pinned example document.
fn example_with(change: impl FnOnce(&mut Value)) -> Value {
    let mut doc = read_json("docs/schema/today-v1.example.json");
    change(&mut doc);
    doc
}

fn window(doc: &mut Value) -> &mut serde_json::Map<String, Value> {
    doc["providers"][0]["windows"][0].as_object_mut().unwrap()
}

#[test]
fn renamed_or_removed_fields_fail_the_schema() {
    for field in [
        "window",
        "binding",
        "unit",
        "used",
        "limit",
        "resets_at",
        "colour",
    ] {
        assert_invalid(&example_with(|d| {
            window(d).remove(field);
        }));
        assert_invalid(&example_with(|d| {
            let value = window(d).remove(field).unwrap();
            window(d).insert(format!("{field}_v2"), value);
        }));
    }
    for field in ["schema_version", "generated_at", "providers"] {
        assert_invalid(&example_with(|d| {
            d.as_object_mut().unwrap().remove(field);
        }));
    }
    for field in ["provider", "status", "windows"] {
        assert_invalid(&example_with(|d| {
            d["providers"][0].as_object_mut().unwrap().remove(field);
        }));
    }
    assert_invalid(&example_with(|d| {
        let value = window(d).remove("budget").unwrap();
        window(d).insert("daily_budget".to_owned(), value);
    }));
}

#[test]
fn wrong_values_fail_the_schema() {
    assert_invalid(&example_with(|d| d["schema_version"] = json!(2)));
    assert_invalid(&example_with(|d| {
        d["generated_at"] = json!("2026-11-03T11:05:00+01:00");
    }));
    assert_invalid(&example_with(|d| {
        d["providers"][0]["status"] = json!("fine");
    }));
    assert_invalid(&example_with(|d| {
        d["providers"][0]["windows"][0]["colour"] = json!("blue");
    }));
    // A stale provider shows grey windows; stale_since only on a stale provider.
    assert_invalid(&example_with(|d| {
        d["providers"][0]["status"] = json!("stale");
    }));
    assert_invalid(&example_with(|d| {
        d["providers"][0]["stale_since"] = json!("2026-11-03T10:00:00Z");
    }));
    // Budget fields only on binding windows; Copilot only has `monthly` in requests.
    assert_invalid(&example_with(|d| {
        d["providers"][0]["windows"][1]["budget"] = json!(1.0);
    }));
    assert_invalid(&example_with(|d| {
        d["providers"][1]["windows"][0]["unit"] = json!("percent");
    }));
    assert_invalid(&example_with(|d| {
        d["providers"][1]["windows"][0]["window"] = json!("seven_day");
    }));
    assert_invalid(&example_with(|d| {
        d["providers"][0]["unknown"] = json!(true);
    }));
}

#[test]
fn published_schema_allows_unknown_fields() {
    let mut document = read_json("docs/schema/today-v1.example.json");
    document["added_later"] = json!(1);
    document["providers"][0]["unknown"] = json!(true);
    document["providers"][1]["windows"][0]["extra"] = json!("x");
    let validator = published_validator();
    assert!(validator.is_valid(&document), "{document:#}");
    // the strict copy still rejects them
    assert_invalid(&document);
}
