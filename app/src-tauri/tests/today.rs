//! The `today` command against a temp data dir: seeded snapshots, an empty dir, the config.

use std::path::{Path, PathBuf};

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use tamias_app_lib::{CommandError, today};
use tamias_core::config::{self, Config};
use tamias_core::paths::DATABASE_FILE;
use tamias_core::report::{Report, Status};
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};
use tamias_core::store::{Error, Store};
use tempfile::TempDir;

const NOW: &str = "2026-11-03T10:05:00Z";

fn now() -> Timestamp {
    NOW.parse().unwrap()
}

fn at(offset_secs: i64) -> Timestamp {
    now()
        .checked_add(SignedDuration::from_secs(offset_secs))
        .unwrap()
}

/// A temp data directory and the path of its database.
fn data_dir() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join(DATABASE_FILE);
    (dir, database)
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
        resets_at: at(resets_in_hours * 3600),
        observed_at: at(-60),
        source: Source::Poll,
    }
}

fn seed_claude(database: &Path) {
    Store::open(database)
        .unwrap()
        .record_run(
            Provider::Claude,
            &[
                snap(Provider::Claude, Window::SevenDay, 34.0, 72),
                snap(Provider::Claude, Window::FiveHour, 12.0, 2),
            ],
            at(-60),
        )
        .unwrap();
}

/// The default config on a machine without `gh`, whatever this machine has: Copilot `auto` is
/// `not set up` (the explicit `gh_path` is not a file and does not fall back).
fn no_gh() -> Config {
    config::parse("gh_path = \"/nonexistent/tamias-test/gh\"\n").unwrap()
}

/// `copilot = true`: Copilot is set up whatever this machine has.
fn copilot_on() -> Config {
    config::parse("[providers]\ncopilot = true\n").unwrap()
}

fn read(database: &Path, config: &Config) -> Report {
    today::read(database, config, now(), &TimeZone::UTC).unwrap()
}

fn statuses(report: &Report) -> Vec<(Provider, Status)> {
    report
        .providers
        .iter()
        .map(|p| (p.provider, p.status))
        .collect()
}

#[test]
fn seeded_claude_is_ok_with_its_windows() {
    let (_dir, database) = data_dir();
    seed_claude(&database);

    let report = read(&database, &no_gh());

    assert_eq!(report.schema_version, 1);
    assert_eq!(report.generated_at, now());
    let claude = &report.providers[0];
    assert_eq!(claude.provider, Provider::Claude);
    assert_eq!(claude.status, Status::Ok);
    assert_eq!(claude.stale_since, None);
    let windows: Vec<(Window, f64)> = claude.windows.iter().map(|w| (w.window, w.used)).collect();
    assert_eq!(
        windows,
        [(Window::SevenDay, 34.0), (Window::FiveHour, 12.0)]
    );
    assert!(claude.windows[0].budget.is_some(), "{report:#?}");
    assert_eq!(claude.windows[1].budget, None);
}

#[test]
fn missing_database_makes_every_provider_unavailable_and_creates_nothing() {
    let (dir, database) = data_dir();

    let report = read(&database, &no_gh());

    assert_eq!(
        statuses(&report),
        [
            (Provider::Claude, Status::Unavailable),
            (Provider::Copilot, Status::Unavailable)
        ]
    );
    assert!(report.providers.iter().all(|p| p.windows.is_empty()));
    assert!(!database.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn missing_data_dir_makes_every_provider_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("not-there").join(DATABASE_FILE);

    let report = read(&database, &no_gh());

    assert!(
        report
            .providers
            .iter()
            .all(|p| p.status == Status::Unavailable)
    );
    assert!(!dir.path().join("not-there").exists());
}

#[test]
fn copilot_without_gh_is_unavailable() {
    let (_dir, database) = data_dir();
    seed_claude(&database);

    let report = read(&database, &no_gh());

    assert_eq!(
        statuses(&report),
        [
            (Provider::Claude, Status::Ok),
            (Provider::Copilot, Status::Unavailable)
        ]
    );
}

#[test]
fn seeded_copilot_is_read_too() {
    let (_dir, database) = data_dir();
    Store::open(&database)
        .unwrap()
        .record_run(
            Provider::Copilot,
            &[snap(Provider::Copilot, Window::Monthly, 26.0, 24 * 20)],
            at(-60),
        )
        .unwrap();

    let report = read(&database, &copilot_on());

    let copilot = &report.providers[1];
    assert_eq!(copilot.status, Status::Ok);
    let windows: Vec<(Window, f64, f64)> = copilot
        .windows
        .iter()
        .map(|w| (w.window, w.used, w.limit))
        .collect();
    assert_eq!(windows, [(Window::Monthly, 26.0, 300.0)]);
    assert_eq!(report.providers[0].status, Status::Unavailable);
}

fn seed_no_quota(database: &Path, reason: &str) {
    Store::open(database)
        .unwrap()
        .record_unavailable(Provider::Copilot, at(-60), reason)
        .unwrap();
}

fn copilot_status(report: &Report) -> (Status, usize) {
    let copilot = &report.providers[1];
    assert_eq!(copilot.provider, Provider::Copilot);
    (copilot.status, copilot.windows.len())
}

#[test]
fn copilot_seat_without_quota_is_unavailable_not_ok() {
    for reason in ["no premium quota", "unlimited"] {
        let (_dir, database) = data_dir();
        seed_claude(&database);
        seed_no_quota(&database, reason);

        let report = read(&database, &copilot_on());

        assert_eq!(
            copilot_status(&report),
            (Status::Unavailable, 0),
            "{reason}"
        );
        assert_eq!(report.providers[1].stale_since, None, "{reason}");
    }
}

#[test]
fn copilot_error_recorded_after_the_no_quota_reason_is_stale() {
    let (_dir, database) = data_dir();
    seed_no_quota(&database, "unlimited");
    Store::open(&database)
        .unwrap()
        .record_error(Provider::Copilot, at(-30), "network")
        .unwrap();

    let report = read(&database, &copilot_on());

    assert_eq!(report.providers[1].status, Status::Stale);
}

#[test]
fn copilot_switched_off_is_unavailable_despite_data() {
    let (_dir, database) = data_dir();
    Store::open(&database)
        .unwrap()
        .record_run(
            Provider::Copilot,
            &[snap(Provider::Copilot, Window::Monthly, 26.0, 24 * 20)],
            at(-60),
        )
        .unwrap();
    let config = config::parse("[providers]\ncopilot = false\n").unwrap();

    let report = read(&database, &config);

    assert_eq!(copilot_status(&report), (Status::Unavailable, 0));
}

#[test]
fn copilot_with_gh_but_never_collected_is_stale() {
    let (_dir, database) = data_dir();
    seed_claude(&database);

    let report = read(&database, &copilot_on());

    assert_eq!(copilot_status(&report), (Status::Stale, 0));
    assert_eq!(report.providers[1].stale_since, None);
}

#[test]
fn failed_collect_is_stale_since_the_last_success() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    Store::open(&database)
        .unwrap()
        .record_error(Provider::Claude, at(-30), "network")
        .unwrap();

    let report = read(&database, &no_gh());

    let claude = &report.providers[0];
    assert_eq!(claude.status, Status::Stale);
    assert_eq!(claude.stale_since, Some(at(-60)));
    assert_eq!(claude.windows.len(), 2);
}

#[test]
fn claude_switched_off_is_unavailable_despite_data() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    let config = config::parse("[providers]\nclaude = false\n").unwrap();

    let report = read(&database, &config);

    assert_eq!(report.providers[0].status, Status::Unavailable);
    assert!(report.providers[0].windows.is_empty());
}

#[test]
fn claude_switched_on_but_never_collected_is_stale() {
    let (_dir, database) = data_dir();
    Store::open(&database).unwrap();
    let config =
        config::parse("gh_path = \"/nonexistent/tamias-test/gh\"\n[providers]\nclaude = true\n")
            .unwrap();

    let report = read(&database, &config);

    assert_eq!(
        statuses(&report),
        [
            (Provider::Claude, Status::Stale),
            (Provider::Copilot, Status::Unavailable)
        ]
    );
    assert_eq!(report.providers[0].stale_since, None);
}

#[test]
fn stale_after_of_the_config_is_used() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    let config = config::parse("stale_after = \"30s\"\n").unwrap();

    let report = read(&database, &config);

    assert_eq!(report.providers[0].status, Status::Stale);
}

#[test]
fn the_store_is_not_written() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    let before = std::fs::read(&database).unwrap();

    read(&database, &no_gh());

    assert_eq!(std::fs::read(&database).unwrap(), before);
}

#[test]
fn database_of_a_newer_tamias_is_an_error() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    rusqlite::Connection::open(&database)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();

    let err = today::read(&database, &no_gh(), now(), &TimeZone::UTC).unwrap_err();

    assert!(
        matches!(err, Error::SchemaTooNew { found: 99, .. }),
        "{err}"
    );
}

#[test]
fn report_is_schema_v1_json() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    let report = read(&database, &no_gh());

    let json = serde_json::to_value(&report).unwrap();
    let schema_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema/today-v1.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&json)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}\n{json:#}");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["providers"][0]["provider"], "claude");
    assert_eq!(json["providers"][1]["status"], "unavailable");
    assert_eq!(Report::parse(&json.to_string()).unwrap(), report);
}

#[test]
fn a_busy_store_is_a_busy_command_error() {
    let busy = Error::Sqlite(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
        None,
    ));

    let err = CommandError::from(today::Error::Store(busy));

    assert!(err.busy);
    assert_eq!(serde_json::to_value(&err).unwrap()["busy"], true);
}

#[test]
fn other_store_errors_are_not_busy() {
    let (_dir, database) = data_dir();
    seed_claude(&database);
    rusqlite::Connection::open(&database)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let err = today::read(&database, &no_gh(), now(), &TimeZone::UTC).unwrap_err();

    let err = CommandError::from(today::Error::Store(err));

    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        serde_json::json!({
            "message": "database schema version 99 is newer than the supported version 2",
            "busy": false,
        })
    );
}
