use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::doctrine::{AfterActionReview, LessonDraft, LessonStatus, StoredLesson};
use crate::learning::{EpisodeSummary, QValue, SimilarEpisode, cosine_similarity};
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

    /// Reads a contextual action value, returning zero for an unseen pair.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite cannot query the Q-value table.
    pub fn q_value(&self, state_key: &str, action: &str) -> Result<QValue> {
        Ok(self
            .connection
            .query_row(
                "SELECT value, visits FROM q_values WHERE state_key = ?1 AND action = ?2",
                params![state_key, action],
                |row| {
                    Ok(QValue {
                        value: row.get(0)?,
                        visits: row.get(1)?,
                    })
                },
            )
            .optional()?
            .unwrap_or(QValue {
                value: 0.0,
                visits: 0,
            }))
    }

    /// Inserts or replaces one contextual Q-value.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite cannot persist the value.
    pub fn set_q_value(&self, state_key: &str, action: &str, q: QValue) -> Result<()> {
        self.connection.execute(
            "INSERT INTO q_values(state_key, action, value, visits, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(state_key, action) DO UPDATE SET
                 value = excluded.value,
                 visits = excluded.visits,
                 updated_at = excluded.updated_at",
            params![state_key, action, q.value, q.visits, unix_millis()?],
        )?;
        Ok(())
    }

    /// Stores one compact state/action/reward episode.
    ///
    /// # Errors
    ///
    /// Returns an error if features cannot be serialized or SQLite cannot
    /// insert the episode.
    pub fn record_episode(
        &self,
        session_id: &str,
        features: &[f32],
        action: &str,
        reward: f32,
        outcome_json: &str,
    ) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO episodes(
                 session_id, feature_json, action, reward, outcome_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id,
                serde_json::to_string(features)?,
                action,
                reward,
                outcome_json,
                unix_millis()?
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Retrieves the numerically closest historical episodes.
    ///
    /// At most 1,000 recent episodes are scanned to keep runtime bounded.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite querying or feature deserialization fails.
    pub fn similar_episodes(&self, features: &[f32], limit: usize) -> Result<Vec<SimilarEpisode>> {
        let mut statement = self.connection.prepare(
            "SELECT id, feature_json, action, reward, outcome_json
             FROM episodes ORDER BY id DESC LIMIT 1000",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f32>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut episodes = Vec::new();
        for row in rows {
            let (id, feature_json, action, reward, outcome_json) = row?;
            let historical: Vec<f32> = serde_json::from_str(&feature_json)?;
            episodes.push(SimilarEpisode {
                id,
                action,
                reward,
                outcome_json,
                similarity: cosine_similarity(features, &historical),
            });
        }
        episodes.sort_by(|left, right| right.similarity.total_cmp(&left.similarity));
        episodes.truncate(limit.min(20));
        Ok(episodes)
    }

    /// Loads all episodes belonging to one session in chronological order.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite querying or feature deserialization fails.
    pub fn session_episodes(&self, session_id: &str) -> Result<Vec<EpisodeSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, feature_json, action, reward, outcome_json
             FROM episodes WHERE session_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map([session_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f32>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut episodes = Vec::new();
        for row in rows {
            let (id, features, action, reward, outcome_json) = row?;
            episodes.push(EpisodeSummary {
                id,
                features: serde_json::from_str(&features)?,
                action,
                reward,
                outcome_json,
            });
        }
        Ok(episodes)
    }

    /// Stores a proposed lesson. It is never activated automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if evidence serialization or SQLite insertion fails.
    pub fn save_lesson(&self, lesson: &LessonDraft) -> Result<i64> {
        anyhow::ensure!(
            lesson.status == LessonStatus::Proposed,
            "new lessons must start as proposed"
        );
        let now = unix_millis()?;
        self.connection.execute(
            "INSERT INTO lessons(
                 observation, evidence_json, proposed_doctrine, confidence,
                 status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                lesson.observation,
                serde_json::to_string(&lesson.evidence)?,
                lesson.proposed_doctrine,
                lesson.confidence,
                lesson.status.to_string(),
                now
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Lists lessons, optionally filtering by status.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite querying, status parsing or evidence
    /// deserialization fails.
    pub fn lessons(&self, status: Option<LessonStatus>) -> Result<Vec<StoredLesson>> {
        let mut statement = self.connection.prepare(
            "SELECT id, observation, evidence_json, proposed_doctrine, confidence, status
             FROM lessons WHERE (?1 IS NULL OR status = ?1) ORDER BY id DESC",
        )?;
        let status = status.map(|value| value.to_string());
        let rows = statement.query_map([status], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f32>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut lessons = Vec::new();
        for row in rows {
            let (id, observation, evidence, proposed_doctrine, confidence, status) = row?;
            lessons.push(StoredLesson {
                id,
                draft: LessonDraft {
                    observation,
                    evidence: serde_json::from_str(&evidence)?,
                    proposed_doctrine,
                    confidence,
                    status: parse_lesson_status(&status)?,
                },
            });
        }
        Ok(lessons)
    }

    /// Explicitly changes a lesson status and mirrors active doctrine entries.
    ///
    /// Activation requires at least ten comparable episodes and confidence of
    /// 0.75. This prevents freshly generated lessons from becoming hard rules.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown lessons, insufficient evidence, a proposed
    /// target status, or SQLite failures.
    pub fn set_lesson_status(&mut self, id: i64, status: LessonStatus) -> Result<()> {
        anyhow::ensure!(
            status != LessonStatus::Proposed,
            "use save_lesson for proposals"
        );
        let (evidence_json, doctrine, confidence) = self.connection.query_row(
            "SELECT evidence_json, proposed_doctrine, confidence FROM lessons WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f32>(2)?,
                ))
            },
        )?;
        let evidence: crate::doctrine::LessonEvidence = serde_json::from_str(&evidence_json)?;
        if status == LessonStatus::Active {
            anyhow::ensure!(
                confidence >= 0.75 && evidence.comparable_episodes >= 10,
                "lesson needs confidence >= 0.75 and at least 10 comparable episodes"
            );
        }
        let now = unix_millis()?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE lessons SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.to_string(), now, id],
        )?;
        if status == LessonStatus::Active {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM doctrines WHERE lesson_id = ?1)",
                [id],
                |row| row.get(0),
            )?;
            if !exists {
                transaction.execute(
                    "INSERT INTO doctrines(lesson_id, text, confidence, status, created_at)
                     VALUES (?1, ?2, ?3, 'active', ?4)",
                    params![id, doctrine, confidence, now],
                )?;
            }
        } else {
            transaction.execute(
                "UPDATE doctrines SET status = ?1 WHERE lesson_id = ?2",
                params![status.to_string(), id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Stores a generated after-action report.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite cannot persist the report.
    pub fn save_review(&self, review: &AfterActionReview) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO reports(session_id, kind, report_text, created_at)
             VALUES (?1, 'after_action', ?2, ?3)",
            params![review.session_id, review.report, unix_millis()?],
        )?;
        Ok(self.connection.last_insert_rowid())
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

fn parse_lesson_status(value: &str) -> Result<LessonStatus> {
    match value {
        "proposed" => Ok(LessonStatus::Proposed),
        "active" => Ok(LessonStatus::Active),
        "rejected" => Ok(LessonStatus::Rejected),
        "obsolete" => Ok(LessonStatus::Obsolete),
        _ => anyhow::bail!("unknown lesson status {value}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::MockScenario;
    use crate::config::AppConfig;
    use crate::doctrine::{LessonEvidence, LessonStatus};
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

    #[test]
    fn doctrine_activation_requires_accumulated_evidence() {
        let mut store = MemoryStore::in_memory().expect("memory database");
        let weak = LessonDraft {
            observation: "weak evidence".to_owned(),
            evidence: LessonEvidence {
                comparable_episodes: 5,
                successes: 0,
                failures: 5,
                mean_reward: -1.0,
            },
            proposed_doctrine: "hold".to_owned(),
            confidence: 0.8,
            status: LessonStatus::Proposed,
        };
        let id = store.save_lesson(&weak).expect("proposal");
        assert!(store.set_lesson_status(id, LessonStatus::Active).is_err());
    }
}
