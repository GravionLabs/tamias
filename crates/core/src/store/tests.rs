use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use rusqlite::Connection;
use tempfile::TempDir;

use super::{DEFAULT_BUSY_TIMEOUT, Error, MIGRATIONS, SourceStatus, Store};
use crate::snapshot::{Provider, Snapshot, Source, Unit, Window};

fn temp_db() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("tamias.db");
    (dir, path)
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn snapshot(observed_at: &str, used: f64) -> Snapshot {
    Snapshot {
        provider: Provider::Claude,
        window: Window::SevenDay,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at: ts("2026-11-07T18:00:00.123456789Z"),
        observed_at: ts(observed_at),
        source: Source::Poll,
    }
}

fn all_claude_seven_day(store: &Store) -> Vec<Snapshot> {
    store
        .snapshots(
            Provider::Claude,
            Window::SevenDay,
            ts("2000-01-01T00:00:00Z"),
            ts("2100-01-01T00:00:00Z"),
        )
        .unwrap()
}

/// Raw connection for inspecting the file the way another process would.
fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

fn user_version(path: &Path) -> i64 {
    raw(path)
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn tables(path: &Path) -> Vec<String> {
    let conn = raw(path);
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// --- open and migrations ---

#[test]
fn fresh_path_creates_the_database_with_all_migrations() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    assert!(path.exists());
    assert_eq!(user_version(&path), 1);
    assert_eq!(tables(&path), ["snapshot", "source_status"]);
}

#[test]
fn database_is_in_wal_mode() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    let mode: String = raw(&path)
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn reopening_is_a_no_op_and_keeps_the_data() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    store
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 12.0)])
        .unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(user_version(&path), 1);
    assert_eq!(all_claude_seven_day(&store).len(), 1);
}

#[test]
fn failed_migration_on_a_fresh_database_leaves_version_zero() {
    let (_dir, path) = temp_db();
    let err = Store::open_with_migrations(
        &path,
        DEFAULT_BUSY_TIMEOUT,
        &[MIGRATIONS[0], "CREATE TABLE extra (a); THIS IS NOT SQL;"],
    )
    .unwrap_err();
    assert!(matches!(err, Error::Migration { version: 2, .. }), "{err}");
    assert_eq!(user_version(&path), 0);
    assert!(
        tables(&path).is_empty(),
        "half-migrated: {:?}",
        tables(&path)
    );
}

#[test]
fn failed_migration_rolls_back_to_the_previous_version() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    store
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 12.0)])
        .unwrap();
    drop(store);

    let err = Store::open_with_migrations(
        &path,
        DEFAULT_BUSY_TIMEOUT,
        &[MIGRATIONS[0], "CREATE TABLE extra (a); THIS IS NOT SQL;"],
    )
    .unwrap_err();
    assert!(matches!(err, Error::Migration { version: 2, .. }), "{err}");
    assert_eq!(user_version(&path), 1);
    assert_eq!(tables(&path), ["snapshot", "source_status"]);
    assert_eq!(all_claude_seven_day(&Store::open(&path).unwrap()).len(), 1);
}

#[test]
fn newer_schema_is_rejected() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    raw(&path).pragma_update(None, "user_version", 99).unwrap();

    let err = Store::open(&path).unwrap_err();
    assert!(
        matches!(
            err,
            Error::SchemaTooNew {
                found: 99,
                supported: 1
            }
        ),
        "{err}"
    );
    assert_eq!(user_version(&path), 99);
}

#[test]
fn no_column_holds_credentials() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    let conn = raw(&path);
    for table in tables(&path) {
        let mut stmt = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        let columns: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!columns.is_empty());
        for column in columns {
            let lower = column.to_lowercase();
            for word in [
                "token",
                "secret",
                "password",
                "credential",
                "key",
                "auth",
                "cookie",
            ] {
                assert!(
                    !lower.contains(word),
                    "{table}.{column} looks like a credential"
                );
            }
        }
    }
}

// --- snapshots ---

#[test]
fn inserted_snapshots_round_trip_with_nanosecond_utc_timestamps() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let copilot = Snapshot {
        provider: Provider::Copilot,
        window: Window::Monthly,
        used: 312.5,
        limit: 300.0,
        unit: Unit::Requests,
        resets_at: ts("2026-12-01T00:00:00Z"),
        observed_at: ts("2026-11-03T10:00:00.000000001Z"),
        source: Source::Statusline,
    };
    let claude = snapshot("2026-11-03T10:00:00.5Z", 41.0);
    assert_eq!(
        store
            .insert_snapshots(&[claude.clone(), copilot.clone()])
            .unwrap(),
        2
    );

    assert_eq!(all_claude_seven_day(&store), [claude]);
    let got = store
        .snapshots(
            Provider::Copilot,
            Window::Monthly,
            ts("2026-11-03T10:00:00Z"),
            ts("2026-11-03T10:00:01Z"),
        )
        .unwrap();
    assert_eq!(got, [copilot]);

    let stored: String = raw(&path)
        .query_row(
            "SELECT observed_at FROM snapshot WHERE provider = 'copilot'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, "2026-11-03T10:00:00.000000001Z");
}

#[test]
fn inserting_the_same_snapshot_twice_stores_it_once() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let s = snapshot("2026-11-03T10:00:00Z", 12.0);
    assert_eq!(store.insert_snapshots(std::slice::from_ref(&s)).unwrap(), 1);
    assert_eq!(store.insert_snapshots(std::slice::from_ref(&s)).unwrap(), 0);
    assert_eq!(store.insert_snapshots(&[s.clone(), s.clone()]).unwrap(), 0);
    assert_eq!(all_claude_seven_day(&store), [s]);
}

#[test]
fn duplicate_key_keeps_the_first_values() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let first = snapshot("2026-11-03T10:00:00Z", 12.0);
    let second = snapshot("2026-11-03T10:00:00Z", 99.0);
    assert_eq!(store.insert_snapshots(&[first.clone(), second]).unwrap(), 1);
    assert_eq!(all_claude_seven_day(&store), [first]);
}

#[test]
fn same_instant_from_another_source_is_a_separate_snapshot() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let poll = snapshot("2026-11-03T10:00:00Z", 12.0);
    let statusline = Snapshot {
        source: Source::Statusline,
        ..poll.clone()
    };
    assert_eq!(
        store
            .insert_snapshots(&[poll.clone(), statusline.clone()])
            .unwrap(),
        2
    );
    assert_eq!(all_claude_seven_day(&store), [poll, statusline]);
}

#[test]
fn empty_insert_stores_nothing() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.insert_snapshots(&[]).unwrap(), 0);
    assert!(all_claude_seven_day(&store).is_empty());
}

#[test]
fn range_is_half_open_ordered_and_filtered_by_provider_and_window() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let five_hour = Snapshot {
        window: Window::FiveHour,
        ..snapshot("2026-11-03T11:00:00Z", 1.0)
    };
    store
        .insert_snapshots(&[
            snapshot("2026-11-03T12:00:00Z", 3.0),
            snapshot("2026-11-03T10:00:00Z", 1.0),
            snapshot("2026-11-03T11:00:00Z", 2.0),
            snapshot("2026-11-03T09:59:59.999999999Z", 0.5),
            five_hour,
        ])
        .unwrap();

    let got = store
        .snapshots(
            Provider::Claude,
            Window::SevenDay,
            ts("2026-11-03T10:00:00Z"),
            ts("2026-11-03T12:00:00Z"),
        )
        .unwrap();
    let used: Vec<f64> = got.iter().map(|s| s.used).collect();
    assert_eq!(used, [1.0, 2.0]);

    let none = store
        .snapshots(
            Provider::Copilot,
            Window::SevenDay,
            ts("2000-01-01T00:00:00Z"),
            ts("2100-01-01T00:00:00Z"),
        )
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn empty_or_inverted_range_returns_nothing() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    store
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 1.0)])
        .unwrap();
    let at = ts("2026-11-03T10:00:00Z");
    assert!(
        store
            .snapshots(Provider::Claude, Window::SevenDay, at, at)
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .snapshots(
                Provider::Claude,
                Window::SevenDay,
                ts("2026-11-04T00:00:00Z"),
                at
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_snapshot_fails_the_whole_batch() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let err = store
        .insert_snapshots(&[
            snapshot("2026-11-03T10:00:00Z", 1.0),
            snapshot("2026-11-03T11:00:00Z", -1.0),
        ])
        .unwrap_err();
    assert!(matches!(err, Error::Sqlite(_)), "{err}");
    assert!(all_claude_seven_day(&store).is_empty());
}

#[test]
fn nan_usage_is_rejected() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    assert!(
        store
            .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", f64::NAN)])
            .is_err()
    );
    assert!(all_claude_seven_day(&store).is_empty());
}

#[test]
fn timestamp_before_year_zero_is_rejected() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let early = Snapshot {
        observed_at: ts("-000001-06-01T00:00:00Z"),
        ..snapshot("2026-11-03T10:00:00Z", 1.0)
    };
    let err = store.insert_snapshots(&[early]).unwrap_err();
    assert!(matches!(err, Error::TimestampOutOfRange(_)), "{err}");
}

#[test]
fn unreadable_stored_value_is_reported_with_its_column() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    store
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 1.0)])
        .unwrap();
    raw(&path)
        .execute("UPDATE snapshot SET resets_at = 'tomorrow'", [])
        .unwrap();
    let err = store
        .snapshots(
            Provider::Claude,
            Window::SevenDay,
            ts("2000-01-01T00:00:00Z"),
            ts("2100-01-01T00:00:00Z"),
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::InvalidValue {
                column: "resets_at",
                ..
            }
        ),
        "{err}"
    );
}

// --- source status ---

#[test]
fn status_is_none_before_the_first_run() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.status(Provider::Claude).unwrap(), None);
}

#[test]
fn ok_and_error_are_recorded_independently() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    store
        .record_ok(Provider::Claude, ts("2026-11-03T10:00:00Z"))
        .unwrap();
    store
        .record_error(Provider::Claude, ts("2026-11-03T10:05:00Z"), "HTTP 429")
        .unwrap();
    store
        .record_ok(Provider::Claude, ts("2026-11-03T10:10:00Z"))
        .unwrap();

    assert_eq!(
        store.status(Provider::Claude).unwrap(),
        Some(SourceStatus {
            provider: Provider::Claude,
            last_ok_at: Some(ts("2026-11-03T10:10:00Z")),
            last_error_at: Some(ts("2026-11-03T10:05:00Z")),
            last_error: Some("HTTP 429".to_owned()),
        })
    );
    assert_eq!(store.status(Provider::Copilot).unwrap(), None);
}

#[test]
fn error_before_any_success_has_no_ok_time() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    store
        .record_error(Provider::Copilot, ts("2026-11-03T10:00:00Z"), "")
        .unwrap();
    let status = store.status(Provider::Copilot).unwrap().unwrap();
    assert_eq!(status.last_ok_at, None);
    assert_eq!(status.last_error.as_deref(), Some(""));
}

// --- one collector run in one transaction ---

#[test]
fn record_run_stores_snapshots_and_ok_time_together() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let s = snapshot("2026-11-03T10:00:00Z", 12.0);
    let at = ts("2026-11-03T10:00:01Z");
    assert_eq!(
        store
            .record_run(Provider::Claude, std::slice::from_ref(&s), at)
            .unwrap(),
        1
    );
    assert_eq!(all_claude_seven_day(&store), [s]);
    assert_eq!(
        store.status(Provider::Claude).unwrap().unwrap().last_ok_at,
        Some(at)
    );
}

#[test]
fn failed_run_leaves_neither_snapshots_nor_ok_time() {
    let (_dir, path) = temp_db();
    let mut store = Store::open(&path).unwrap();
    let before = ts("2026-11-03T09:00:00Z");
    store.record_ok(Provider::Claude, before).unwrap();

    let err = store
        .record_run(
            Provider::Claude,
            &[
                snapshot("2026-11-03T10:00:00Z", 12.0),
                snapshot("2026-11-03T10:00:01Z", -5.0),
            ],
            ts("2026-11-03T10:00:02Z"),
        )
        .unwrap_err();
    assert!(matches!(err, Error::Sqlite(_)), "{err}");
    assert!(all_claude_seven_day(&store).is_empty());
    assert_eq!(
        store.status(Provider::Claude).unwrap().unwrap().last_ok_at,
        Some(before)
    );
}

#[test]
fn failed_run_on_a_busy_database_leaves_neither() {
    let (_dir, path) = temp_db();
    let mut store = Store::open_with_busy_timeout(&path, Duration::from_millis(100)).unwrap();
    let blocker = raw(&path);
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    let err = store
        .record_run(
            Provider::Claude,
            &[snapshot("2026-11-03T10:00:00Z", 12.0)],
            ts("2026-11-03T10:00:01Z"),
        )
        .unwrap_err();
    assert!(err.is_busy(), "{err}");
    blocker.execute_batch("ROLLBACK").unwrap();

    assert!(all_claude_seven_day(&store).is_empty());
    assert_eq!(store.status(Provider::Claude).unwrap(), None);
}

// --- read-only ---

#[test]
fn read_only_reads_what_the_writer_stored() {
    let (_dir, path) = temp_db();
    let mut writer = Store::open(&path).unwrap();
    writer
        .record_run(
            Provider::Claude,
            &[snapshot("2026-11-03T10:00:00Z", 12.0)],
            ts("2026-11-03T10:00:01Z"),
        )
        .unwrap();

    let reader = Store::open_read_only(&path).unwrap();
    assert_eq!(all_claude_seven_day(&reader).len(), 1);
    let status = reader.status(Provider::Claude).unwrap().unwrap();
    assert_eq!(status.last_ok_at, Some(ts("2026-11-03T10:00:01Z")));

    // Later commits of the collector are seen by the open reader.
    writer
        .insert_snapshots(&[snapshot("2026-11-03T11:00:00Z", 14.0)])
        .unwrap();
    assert_eq!(all_claude_seven_day(&reader).len(), 2);
}

#[test]
fn read_only_does_not_create_a_missing_database() {
    let (dir, path) = temp_db();
    assert!(Store::open_read_only(&path).is_err());
    assert!(!path.exists());
    assert!(!dir.path().join("nested").exists());
}

#[test]
fn read_only_cannot_write() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    let mut reader = Store::open_read_only(&path).unwrap();
    let err = reader
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 12.0)])
        .unwrap_err();
    assert!(matches!(err, Error::Sqlite(_)), "{err}");
    assert!(all_claude_seven_day(&reader).is_empty());
}

#[test]
fn read_only_does_not_migrate_and_rejects_a_newer_schema() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    raw(&path).pragma_update(None, "user_version", 99).unwrap();

    let err = Store::open_read_only(&path).unwrap_err();
    assert!(
        matches!(
            err,
            Error::SchemaTooNew {
                found: 99,
                supported: 1
            }
        ),
        "{err}"
    );
    assert_eq!(user_version(&path), 99);
}

#[test]
fn read_only_leaves_an_older_schema_unmigrated() {
    let (_dir, path) = temp_db();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    raw(&path).execute_batch("CREATE TABLE other (a)").unwrap();

    let reader = Store::open_read_only(&path).unwrap();
    assert!(reader.status(Provider::Claude).is_err());
    assert_eq!(user_version(&path), 0);
    assert_eq!(tables(&path), ["other"]);
}

// --- concurrency ---

#[test]
fn reader_is_not_blocked_by_an_open_write_transaction() {
    let (_dir, path) = temp_db();
    let mut writer = Store::open(&path).unwrap();
    writer
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 1.0)])
        .unwrap();

    // A second connection holds the write lock with an uncommitted row.
    let collector = raw(&path);
    collector.execute_batch("BEGIN IMMEDIATE").unwrap();
    collector
        .execute(
            r#"INSERT INTO snapshot (provider, "window", used, "limit", unit, resets_at, observed_at, source)
               VALUES ('claude', 'seven_day', 2, 100, 'percent', '2026-11-07T18:00:00.000000000Z',
                       '2026-11-03T11:00:00.000000000Z', 'poll')"#,
            [],
        )
        .unwrap();

    // A tiny busy timeout: a reader that had to wait for the lock would fail here.
    let reader = Store::open_with_busy_timeout(&path, Duration::from_millis(1)).unwrap();
    let seen = all_claude_seven_day(&reader);
    assert_eq!(seen.len(), 1, "reader saw uncommitted data");

    collector.execute_batch("COMMIT").unwrap();
    assert_eq!(all_claude_seven_day(&reader).len(), 2);
}

#[test]
fn concurrent_writer_and_reader_both_succeed() {
    let (_dir, path) = temp_db();
    Store::open(&path).unwrap();
    let writer_path = path.clone();

    let writer = std::thread::spawn(move || {
        let mut store = Store::open(&writer_path).unwrap();
        let base = ts("2026-11-03T00:00:00Z");
        for i in 0..200_i64 {
            let s = Snapshot {
                observed_at: base + jiff::SignedDuration::from_secs(i),
                ..snapshot("2026-11-03T00:00:00Z", 1.0)
            };
            store
                .record_run(
                    Provider::Claude,
                    &[s],
                    base + jiff::SignedDuration::from_secs(i),
                )
                .unwrap();
        }
    });

    let reader = Store::open(&path).unwrap();
    let mut last = 0;
    while !writer.is_finished() {
        let n = all_claude_seven_day(&reader).len();
        assert!(n >= last, "reader went backwards");
        last = n;
        reader.status(Provider::Claude).unwrap();
    }
    writer.join().unwrap();
    assert_eq!(all_claude_seven_day(&reader).len(), 200);
}

#[test]
fn busy_timeout_bounds_a_write_wait() {
    let (_dir, path) = temp_db();
    let timeout = Duration::from_millis(200);
    let mut store = Store::open_with_busy_timeout(&path, timeout).unwrap();
    let blocker = raw(&path);
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    let started = Instant::now();
    let err = store
        .insert_snapshots(&[snapshot("2026-11-03T10:00:00Z", 1.0)])
        .unwrap_err();
    let waited = started.elapsed();
    assert!(err.is_busy(), "{err}");
    assert!(waited >= timeout / 2, "gave up after {waited:?}");
    assert!(waited < Duration::from_secs(5), "waited {waited:?}");

    let err = store
        .record_error(Provider::Claude, ts("2026-11-03T10:00:00Z"), "x")
        .unwrap_err();
    assert!(err.is_busy(), "{err}");
    let err = store
        .record_ok(Provider::Claude, ts("2026-11-03T10:00:00Z"))
        .unwrap_err();
    assert!(err.is_busy(), "{err}");
}

#[test]
fn busy_timeout_bounds_a_migration_wait() {
    let (_dir, path) = temp_db();
    // An unmigrated database whose write lock another connection holds.
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let blocker = raw(&path);
    blocker.execute_batch("PRAGMA journal_mode = WAL").unwrap();
    blocker
        .execute_batch("BEGIN IMMEDIATE; CREATE TABLE other (a);")
        .unwrap();

    let started = Instant::now();
    let err = Store::open_with_busy_timeout(&path, Duration::from_millis(200)).unwrap_err();
    assert!(err.is_busy(), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5));
    blocker.execute_batch("ROLLBACK").unwrap();
    assert_eq!(user_version(&path), 0);
}

#[test]
fn non_busy_errors_are_not_busy() {
    let err = Error::SchemaTooNew {
        found: 2,
        supported: 1,
    };
    assert!(!err.is_busy());
}
