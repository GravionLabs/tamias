//! `SQLite` snapshot store.
//!
//! One file per user (see [`crate::paths::database_path`]), WAL mode so readers never wait for
//! the collector, and a busy timeout that bounds every wait for the write lock. The schema is
//! built from the numbered scripts in `store/migrations/`; `PRAGMA user_version` holds the number
//! of applied scripts.

use std::path::Path;
use std::time::Duration;

use jiff::Timestamp;
use rusqlite::{
    Connection, ErrorCode, OpenFlags, OptionalExtension, Row, TransactionBehavior, params,
};

use crate::snapshot::{Provider, Snapshot, Window};

#[cfg(test)]
mod tests;

/// Migration scripts; script `i` (0-based) brings the schema to `user_version` `i + 1`.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_unavailable.sql"),
];

/// Default bound for a wait on the database lock.
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Error of the snapshot store.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The directory of the database could not be created.
    #[error("cannot create the database directory: {0}")]
    Io(#[from] std::io::Error),
    /// `SQLite` reported an error.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A migration script failed; the database stays at its previous version.
    #[error("migration {version} failed: {source}")]
    Migration {
        /// Version of the migration that failed.
        version: usize,
        #[source]
        /// The database error.
        source: rusqlite::Error,
    },
    /// The database was written by a newer tamias.
    #[error("database schema version {found} is newer than the supported version {supported}")]
    SchemaTooNew {
        /// Schema version of the database.
        found: i64,
        /// Newest schema version this build supports.
        supported: usize,
    },
    /// The database cannot use WAL mode (for example on a network file system).
    #[error("cannot switch the database to WAL mode (journal mode is {0})")]
    NoWal(String),
    /// A timestamp before year 0 cannot be stored in sortable form.
    #[error("timestamp {0} is out of the storable range")]
    TimestampOutOfRange(Timestamp),
    /// A stored value could not be read back.
    #[error("invalid value in column {column}: {message}")]
    InvalidValue {
        /// Column that could not be read.
        column: &'static str,
        /// What was wrong with the value.
        message: String,
    },
}

impl Error {
    /// Whether the error is a lock wait that hit the busy timeout.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        let (Error::Sqlite(inner) | Error::Migration { source: inner, .. }) = self else {
            return false;
        };
        matches!(
            inner.sqlite_error_code(),
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
        )
    }
}

/// Result of the store.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Health of one provider as seen by the collector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStatus {
    /// Provider the status is of.
    pub provider: Provider,
    /// Last successful collect.
    pub last_ok_at: Option<Timestamp>,
    /// Last failed collect.
    pub last_error_at: Option<Timestamp>,
    /// Message of the last failure, when one was recorded.
    pub last_error: Option<String>,
    /// Why the provider has no countable quota (for example `no premium quota`), set by the
    /// last successful poll; `None` when the last poll produced snapshots.
    pub unavailable: Option<String>,
}

/// Open snapshot database.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens or creates the database at `path` with [`DEFAULT_BUSY_TIMEOUT`] and applies
    /// pending migrations. Missing parent directories are created.
    ///
    /// # Errors
    ///
    /// I/O or `SQLite` errors, a failed migration (the database stays at its previous version),
    /// or a schema newer than this build supports.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_busy_timeout(path, DEFAULT_BUSY_TIMEOUT)
    }

    /// As [`Store::open`], with `busy_timeout` as the bound for every wait on the lock.
    ///
    /// # Errors
    ///
    /// As [`Store::open`].
    pub fn open_with_busy_timeout(path: impl AsRef<Path>, busy_timeout: Duration) -> Result<Self> {
        Self::open_with_migrations(path.as_ref(), busy_timeout, MIGRATIONS)
    }

    /// Opens the existing database at `path` read-only with [`DEFAULT_BUSY_TIMEOUT`], for readers
    /// such as the desktop app: no directory or file is created and no migration runs, so the
    /// reader never takes the write lock. Every write through it fails.
    ///
    /// A database of an older schema is opened as it is; reads of what it lacks fail.
    ///
    /// # Errors
    ///
    /// `SQLite` errors (a missing file is one), or [`Error::SchemaTooNew`] when the database was
    /// written by a newer tamias.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(DEFAULT_BUSY_TIMEOUT)?;
        let found = user_version(&conn)?;
        if usize::try_from(found).map_or(true, |version| version > MIGRATIONS.len()) {
            return Err(Error::SchemaTooNew {
                found,
                supported: MIGRATIONS.len(),
            });
        }
        Ok(Self { conn })
    }

    fn open_with_migrations(
        path: &Path,
        busy_timeout: Duration,
        migrations: &[&str],
    ) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        // Before anything that takes a lock, so every wait below is bounded too.
        conn.busy_timeout(busy_timeout)?;
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(Error::NoWal(mode));
        }
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        migrate(&mut conn, migrations)?;
        Ok(Self { conn })
    }

    /// Stores `snapshots` in one transaction; duplicates of
    /// `(provider, window, observed_at, source)` are skipped (the first stored row wins).
    ///
    /// Returns the number of newly stored snapshots.
    ///
    /// # Errors
    ///
    /// `SQLite` errors (including a busy timeout) or an invalid snapshot; nothing is stored then.
    pub fn insert_snapshots(&mut self, snapshots: &[Snapshot]) -> Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = insert_snapshots(&tx, snapshots)?;
        tx.commit()?;
        Ok(inserted)
    }

    /// Stores the result of one successful collector run: `snapshots` and `last_ok_at = at` for
    /// `provider`, in one transaction. On error neither is stored.
    ///
    /// Returns the number of newly stored snapshots.
    ///
    /// # Errors
    ///
    /// As [`Store::insert_snapshots`].
    pub fn record_run(
        &mut self,
        provider: Provider,
        snapshots: &[Snapshot],
        at: Timestamp,
    ) -> Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = insert_snapshots(&tx, snapshots)?;
        record_ok(&tx, provider, at)?;
        tx.commit()?;
        Ok(inserted)
    }

    /// Snapshots of `provider` and `window` with `from <= observed_at < to`, oldest first.
    ///
    /// # Errors
    ///
    /// `SQLite` errors or a stored value that cannot be read back.
    pub fn snapshots(
        &self,
        provider: Provider,
        window: Window,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<Snapshot>> {
        let mut stmt = self.conn.prepare_cached(
            r#"SELECT provider, "window", used, "limit", unit, resets_at, observed_at, source
               FROM snapshot
               WHERE provider = ?1 AND "window" = ?2 AND observed_at >= ?3 AND observed_at < ?4
               ORDER BY observed_at, source"#,
        )?;
        let rows = stmt.query_map(
            params![
                provider.as_str(),
                window.as_str(),
                encode_ts(from)?,
                encode_ts(to)?
            ],
            |row| Ok(read_snapshot(row)),
        )?;
        rows.map(|row| row?).collect()
    }

    /// Records a successful collector run of `provider` at `at`.
    ///
    /// # Errors
    ///
    /// `SQLite` errors (including a busy timeout).
    pub fn record_ok(&mut self, provider: Provider, at: Timestamp) -> Result<()> {
        record_ok(&self.conn, provider, at)
    }

    /// Records a successful poll of `provider` at `at` that found no countable quota: `reason`
    /// and `last_ok_at = at` are written by one statement, so a failed write leaves the previous
    /// status untouched.
    ///
    /// `reason` must not contain credentials or tokens.
    ///
    /// # Errors
    ///
    /// `SQLite` errors (including a busy timeout) or an out-of-range timestamp.
    pub fn record_unavailable(
        &mut self,
        provider: Provider,
        at: Timestamp,
        reason: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO source_status (provider, last_ok_at, unavailable) VALUES (?1, ?2, ?3)
             ON CONFLICT (provider) DO UPDATE
             SET last_ok_at = excluded.last_ok_at, unavailable = excluded.unavailable",
            params![provider.as_str(), encode_ts(at)?, reason],
        )?;
        Ok(())
    }

    /// Records a failed collector run of `provider` at `at` with `message`.
    ///
    /// `message` must not contain credentials or tokens.
    ///
    /// # Errors
    ///
    /// `SQLite` errors (including a busy timeout).
    pub fn record_error(&mut self, provider: Provider, at: Timestamp, message: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO source_status (provider, last_error_at, last_error) VALUES (?1, ?2, ?3)
             ON CONFLICT (provider) DO UPDATE
             SET last_error_at = excluded.last_error_at, last_error = excluded.last_error",
            params![provider.as_str(), encode_ts(at)?, message],
        )?;
        Ok(())
    }

    /// Health of `provider`, `None` before its first recorded run.
    ///
    /// # Errors
    ///
    /// `SQLite` errors or a stored value that cannot be read back.
    pub fn status(&self, provider: Provider) -> Result<Option<SourceStatus>> {
        self.conn
            .query_row(
                "SELECT last_ok_at, last_error_at, last_error, unavailable FROM source_status WHERE provider = ?1",
                [provider.as_str()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?
            .map(|(ok, err_at, last_error, unavailable)| {
                Ok(SourceStatus {
                    provider,
                    last_ok_at: ok.as_deref().map(|s| decode_ts("last_ok_at", s)).transpose()?,
                    last_error_at: err_at
                        .as_deref()
                        .map(|s| decode_ts("last_error_at", s))
                        .transpose()?,
                    last_error,
                    unavailable,
                })
            })
            .transpose()
    }
}

/// Applies the scripts of `migrations` beyond `user_version`, all in one transaction.
fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    let target = i64::try_from(migrations.len()).expect("migration count fits in i64");
    // Readers of an up-to-date database never take the write lock.
    if user_version(conn)? == target {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = user_version(&tx)?;
    let applied = usize::try_from(current).unwrap_or(usize::MAX);
    if applied > migrations.len() {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: migrations.len(),
        });
    }
    for (index, script) in migrations.iter().enumerate().skip(applied) {
        tx.execute_batch(script)
            .map_err(|source| Error::Migration {
                version: index + 1,
                source,
            })?;
    }
    tx.pragma_update(None, "user_version", target)?;
    tx.commit()?;
    Ok(())
}

fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn insert_snapshots(conn: &Connection, snapshots: &[Snapshot]) -> Result<usize> {
    let mut stmt = conn.prepare_cached(
        // Only the unique key is skipped; CHECK and NOT NULL violations still fail (unlike OR IGNORE).
        r#"INSERT INTO snapshot
           (provider, "window", used, "limit", unit, resets_at, observed_at, source)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
           ON CONFLICT (provider, "window", observed_at, source) DO NOTHING"#,
    )?;
    let mut inserted = 0;
    for s in snapshots {
        inserted += stmt.execute(params![
            s.provider.as_str(),
            s.window.as_str(),
            s.used,
            s.limit,
            s.unit.as_str(),
            encode_ts(s.resets_at)?,
            encode_ts(s.observed_at)?,
            s.source.as_str(),
        ])?;
    }
    Ok(inserted)
}

fn record_ok(conn: &Connection, provider: Provider, at: Timestamp) -> Result<()> {
    conn.execute(
        "INSERT INTO source_status (provider, last_ok_at) VALUES (?1, ?2)
         ON CONFLICT (provider) DO UPDATE SET last_ok_at = excluded.last_ok_at, unavailable = NULL",
        params![provider.as_str(), encode_ts(at)?],
    )?;
    Ok(())
}

fn read_snapshot(row: &Row<'_>) -> Result<Snapshot> {
    let text = |index: usize| -> Result<String> { Ok(row.get(index)?) };
    Ok(Snapshot {
        provider: parse("provider", &text(0)?)?,
        window: parse("window", &text(1)?)?,
        used: row.get(2)?,
        limit: row.get(3)?,
        unit: parse("unit", &text(4)?)?,
        resets_at: decode_ts("resets_at", &text(5)?)?,
        observed_at: decode_ts("observed_at", &text(6)?)?,
        source: parse("source", &text(7)?)?,
    })
}

fn parse<T>(column: &'static str, value: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|e: T::Err| Error::InvalidValue {
        column,
        message: e.to_string(),
    })
}

/// Unix second of 0000-01-01T00:00:00Z; earlier timestamps would not sort as text.
const MIN_STORABLE_SECOND: i64 = -62_167_219_200;

/// UTC RFC 3339 with a fixed nine-digit fraction: text order equals time order.
fn encode_ts(ts: Timestamp) -> Result<String> {
    if ts.as_second() < MIN_STORABLE_SECOND {
        return Err(Error::TimestampOutOfRange(ts));
    }
    Ok(format!("{ts:.9}"))
}

fn decode_ts(column: &'static str, value: &str) -> Result<Timestamp> {
    parse(column, value)
}
