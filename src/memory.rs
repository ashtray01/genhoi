use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::simulation::SimulationReport;
use crate::state::GameState;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseInfo {
    pub schema_version: i64,
    pub sessions: u64,
    pub states: u64,
    pub decisions: u64,
    pub episodes: u64,
    pub lessons: u64,
    pub q_values: u64,
    pub size_bytes: u64,
}

pub struct MemoryStore {
    connection: Connection,
}

impl MemoryStore {
    /// Opens or creates a `GenHOI` SQLite database and applies its schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created, SQLite
    /// cannot open the file, or schema initialization fails.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create data directory {}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    /// Creates an isolated in-memory store for tests and ephemeral analysis.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite cannot initialize the schema.
    pub fn in_memory() -> Result<Self> {
        let store = Self {
            connection: Connection::open_in_memory()?,
        };
        store.initialize()?;
        Ok(store)
    }

    #[allow(clippy::too_many_lines)] // Keeping the declarative schema together aids auditing.
    fn initialize(&self) -> Result<()> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS schema_info (
                 version INTEGER NOT NULL
             );
             INSERT INTO schema_info(version)
             SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM schema_info);

             CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY,
                 started_at INTEGER NOT NULL,
                 ended_at INTEGER,
                 mode TEXT NOT NULL,
                 app_version TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS games (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 started_at INTEGER NOT NULL,
                 ended_at INTEGER,
                 result TEXT
             );
             CREATE TABLE IF NOT EXISTS states (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 game_hour INTEGER NOT NULL,
                 observed_at INTEGER NOT NULL,
                 state_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS states_session_hour
                 ON states(session_id, game_hour, id);
             CREATE TABLE IF NOT EXISTS decisions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 state_id INTEGER NOT NULL REFERENCES states(id),
                 source TEXT NOT NULL,
                 decision_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS actions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 decision_id INTEGER NOT NULL REFERENCES decisions(id),
                 action_json TEXT NOT NULL,
                 status TEXT NOT NULL,
                 result_json TEXT
             );
             CREATE TABLE IF NOT EXISTS outcomes (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 action_id INTEGER REFERENCES actions(id),
                 reward REAL NOT NULL,
                 outcome_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS episodes (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 feature_json TEXT NOT NULL,
                 action TEXT NOT NULL,
                 reward REAL NOT NULL,
                 outcome_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS lessons (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 observation TEXT NOT NULL,
                 evidence_json TEXT NOT NULL,
                 proposed_doctrine TEXT NOT NULL,
                 confidence REAL NOT NULL,
                 status TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS doctrines (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 lesson_id INTEGER REFERENCES lessons(id),
                 text TEXT NOT NULL,
                 confidence REAL NOT NULL,
                 status TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS q_values (
                 state_key TEXT NOT NULL,
                 action TEXT NOT NULL,
                 value REAL NOT NULL,
                 visits INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 PRIMARY KEY(state_key, action)
             );
             CREATE TABLE IF NOT EXISTS metrics (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT REFERENCES sessions(id),
                 name TEXT NOT NULL,
                 value REAL NOT NULL,
                 recorded_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS reports (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 kind TEXT NOT NULL,
                 report_text TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );",
        )?;
        let version: i64 =
            self.connection
                .query_row("SELECT version FROM schema_info LIMIT 1", [], |row| {
                    row.get(0)
                })?;
        anyhow::ensure!(
            version == SCHEMA_VERSION,
            "unsupported database schema version {version}"
        );
        Ok(())
    }

    /// Starts a recording session and returns its stable identifier.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite cannot insert the session.
    pub fn begin_session(&self, mode: &str) -> Result<String> {
        let now = unix_millis()?;
        let id = format!("session-{now}-{}", std::process::id());
        self.connection.execute(
            "INSERT INTO sessions(id, started_at, mode, app_version) VALUES (?1, ?2, ?3, ?4)",
            params![id, now, mode, env!("CARGO_PKG_VERSION")],
        )?;
        Ok(id)
    }

    /// Persists one normalized state and its deterministic decision atomically.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or any SQLite statement fails.
    pub fn record_report(&mut self, session_id: &str, report: &SimulationReport) -> Result<()> {
        let state_json = serde_json::to_string(&report.state)?;
        let decision_json = serde_json::to_string(&report.recommended_actions)?;
        let now = unix_millis()?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO states(session_id, game_hour, observed_at, state_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, report.state.game_hour, now, state_json],
        )?;
        let state_id = transaction.last_insert_rowid();
        transaction.execute(
            "INSERT INTO decisions(session_id, state_id, source, decision_json, created_at)
             VALUES (?1, ?2, 'deterministic', ?3, ?4)",
            params![session_id, state_id, decision_json, now],
        )?;
        let decision_id = transaction.last_insert_rowid();
        for action in &report.recommended_actions {
            transaction.execute(
                "INSERT INTO actions(decision_id, action_json, status)
                 VALUES (?1, ?2, 'recommended')",
                params![decision_id, serde_json::to_string(action)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Marks a session as finished.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist or SQLite cannot update it.
    pub fn finish_session(&self, session_id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
            params![unix_millis()?, session_id],
        )?;
        anyhow::ensure!(changed == 1, "unknown session {session_id}");
        Ok(())
    }

    /// Loads normalized observations for deterministic replay.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist, the query fails, or a
    /// stored state is no longer valid JSON.
    pub fn load_session(&self, session_id: &str) -> Result<Vec<GameState>> {
        let exists = self
            .connection
            .query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| {
                Ok(())
            })
            .optional()?
            .is_some();
        anyhow::ensure!(exists, "unknown session {session_id}");
        let mut statement = self.connection.prepare(
            "SELECT state_json FROM states WHERE session_id = ?1 ORDER BY game_hour, id",
        )?;
        let rows = statement.query_map([session_id], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    /// Returns database counters and on-disk size.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite cannot query schema metadata or counters.
    pub fn info(&self) -> Result<DatabaseInfo> {
        Ok(DatabaseInfo {
            schema_version: self.connection.query_row(
                "SELECT version FROM schema_info LIMIT 1",
                [],
                |row| row.get(0),
            )?,
            sessions: self.count("sessions")?,
            states: self.count("states")?,
            decisions: self.count("decisions")?,
            episodes: self.count("episodes")?,
            lessons: self.count("lessons")?,
            q_values: self.count("q_values")?,
            size_bytes: self
                .connection
                .path()
                .and_then(|path| fs::metadata(path).ok())
                .map_or(0, |metadata| metadata.len()),
        })
    }

    fn count(&self, table: &str) -> Result<u64> {
        // Callers use only fixed internal table names; no user input reaches SQL.
        let query = format!("SELECT COUNT(*) FROM {table}");
        Ok(self.connection.query_row(&query, [], |row| row.get(0))?)
    }
}

fn unix_millis() -> Result<i64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis();
    i64::try_from(millis).context("system timestamp exceeds SQLite integer range")
}

#[cfg(test)]
mod tests {
    use crate::adapter::MockScenario;
    use crate::config::AppConfig;
    use crate::simulation;

    use super::*;

    #[test]
    fn records_and_reloads_a_session() {
        let mut store = MemoryStore::in_memory().expect("memory database");
        let session = store.begin_session("test").expect("session");
        let report =
            simulation::run(&AppConfig::default(), MockScenario::DeepSalient).expect("simulation");
        store.record_report(&session, &report).expect("record");
        store.finish_session(&session).expect("finish");
        let states = store.load_session(&session).expect("replay states");
        assert_eq!(states, vec![report.state]);
        let info = store.info().expect("database info");
        assert_eq!(info.sessions, 1);
        assert_eq!(info.states, 1);
        assert_eq!(info.decisions, 1);
    }

    #[test]
    fn unknown_session_is_rejected() {
        let store = MemoryStore::in_memory().expect("memory database");
        assert!(store.load_session("missing").is_err());
    }
}
