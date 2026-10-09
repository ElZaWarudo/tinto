//! Delivery state in its own SQLite file (`delivery.sqlite`), separate from
//! the Agents journal so the two never share migrations. Each record is
//! stored as JSON next to the few columns that are queried.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use super::model::{
    DeliveryApproval, DeliveryDecision, DeliveryJob, DeliveryLease, DeliveryRepoSettings,
    DeliveryRun, DeliverySettings, DeliveryTask,
};
use super::tasks::plain_path;
use super::DeliveryError;
use crate::agent_console::checkpoint::WorktreeSnapshot;

pub struct DeliveryStore {
    conn: Connection,
}

impl DeliveryStore {
    pub fn open_default() -> Result<Self, DeliveryError> {
        let dir = crate::runtime_paths::tinto_config_dir().ok_or_else(|| {
            DeliveryError::new("delivery_store_unavailable", "config directory unavailable")
        })?;
        std::fs::create_dir_all(&dir).map_err(DeliveryError::io)?;
        Self::open(&dir.join("delivery.sqlite"))
    }

    pub fn open(path: &Path) -> Result<Self, DeliveryError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, DeliveryError> {
        let store = Self {
            conn: Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), DeliveryError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS repo_settings (repo TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS tasks (
              id TEXT PRIMARY KEY, repo TEXT NOT NULL, removed INTEGER NOT NULL, data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS jobs (
              id TEXT PRIMARY KEY, task_id TEXT NOT NULL, status TEXT NOT NULL, data TEXT NOT NULL,
              start_snapshot TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_task ON jobs(task_id);
            CREATE TABLE IF NOT EXISTS leases (name TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS approvals (
              id TEXT PRIMARY KEY, task_id TEXT NOT NULL, data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS decisions (
              id TEXT PRIMARY KEY, task_id TEXT NOT NULL, data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
              seq INTEGER PRIMARY KEY AUTOINCREMENT, at_ms INTEGER NOT NULL, kind TEXT NOT NULL,
              task_id TEXT, job_id TEXT, detail TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    // ---- settings ----

    pub fn settings(&self) -> Result<DeliverySettings, DeliveryError> {
        Ok(self.setting("settings")?.unwrap_or_default())
    }

    pub fn set_settings(&self, settings: &DeliverySettings) -> Result<(), DeliveryError> {
        self.set_setting("settings", settings)
    }

    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, DeliveryError> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        value.map(|value| decode(&value)).transpose()
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, encode(value)?],
        )?;
        Ok(())
    }

    pub fn repo_settings(&self, repo: &Path) -> Result<DeliveryRepoSettings, DeliveryError> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM repo_settings WHERE repo = ?1",
                [path_key(repo)],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value
            .map(|value| decode(&value))
            .transpose()?
            .unwrap_or_default())
    }

    pub fn set_repo_settings(
        &self,
        repo: &Path,
        settings: &DeliveryRepoSettings,
    ) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO repo_settings (repo, data) VALUES (?1, ?2)
             ON CONFLICT(repo) DO UPDATE SET data = excluded.data",
            params![path_key(repo), encode(settings)?],
        )?;
        Ok(())
    }

    // ---- runs ----

    pub fn put_run(&self, run: &DeliveryRun) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO runs (id, data) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![run.id, encode(run)?],
        )?;
        Ok(())
    }

    pub fn run(&self, id: &str) -> Result<Option<DeliveryRun>, DeliveryError> {
        self.one("SELECT data FROM runs WHERE id = ?1", id)
    }

    pub fn runs(&self) -> Result<Vec<DeliveryRun>, DeliveryError> {
        self.all("SELECT data FROM runs ORDER BY rowid", [])
    }

    // ---- tasks ----

    pub fn put_task(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO tasks (id, repo, removed, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET repo = excluded.repo, removed = excluded.removed,
               data = excluded.data",
            params![
                task.id,
                path_key(&task.repo),
                task.removed_at_ms.is_some(),
                encode(task)?
            ],
        )?;
        Ok(())
    }

    pub fn task(&self, id: &str) -> Result<Option<DeliveryTask>, DeliveryError> {
        self.one("SELECT data FROM tasks WHERE id = ?1", id)
    }

    /// Tasks that still have a worktree, oldest first.
    pub fn tasks(&self) -> Result<Vec<DeliveryTask>, DeliveryError> {
        self.all(
            "SELECT data FROM tasks WHERE removed = 0 ORDER BY rowid",
            [],
        )
    }

    // ---- jobs ----

    pub fn put_job(&self, job: &DeliveryJob) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO jobs (id, task_id, status, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET status = excluded.status, data = excluded.data",
            params![job.id, job.task_id, encode(&job.status)?, encode(job)?],
        )?;
        Ok(())
    }

    pub fn job(&self, id: &str) -> Result<Option<DeliveryJob>, DeliveryError> {
        self.one("SELECT data FROM jobs WHERE id = ?1", id)
    }

    /// Every job in creation order, which is also the dispatch (FIFO) order.
    pub fn jobs(&self) -> Result<Vec<DeliveryJob>, DeliveryError> {
        self.all("SELECT data FROM jobs ORDER BY rowid", [])
    }

    pub fn task_jobs(&self, task_id: &str) -> Result<Vec<DeliveryJob>, DeliveryError> {
        self.all(
            "SELECT data FROM jobs WHERE task_id = ?1 ORDER BY rowid",
            [task_id],
        )
    }

    pub fn set_start_snapshot(
        &self,
        job_id: &str,
        snapshot: &WorktreeSnapshot,
    ) -> Result<(), DeliveryError> {
        self.conn.execute(
            "UPDATE jobs SET start_snapshot = ?2 WHERE id = ?1",
            params![job_id, encode(snapshot)?],
        )?;
        Ok(())
    }

    pub fn start_snapshot(&self, job_id: &str) -> Result<Option<WorktreeSnapshot>, DeliveryError> {
        let value: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT start_snapshot FROM jobs WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()?;
        value.flatten().map(|value| decode(&value)).transpose()
    }

    // ---- leases ----

    pub fn put_lease(&self, lease: &DeliveryLease) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO leases (name, data) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET data = excluded.data",
            params![lease.name, encode(lease)?],
        )?;
        Ok(())
    }

    pub fn lease(&self, name: &str) -> Result<Option<DeliveryLease>, DeliveryError> {
        self.one("SELECT data FROM leases WHERE name = ?1", name)
    }

    pub fn leases(&self) -> Result<Vec<DeliveryLease>, DeliveryError> {
        self.all("SELECT data FROM leases ORDER BY name", [])
    }

    // ---- approvals ----

    pub fn put_approval(&self, approval: &DeliveryApproval) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO approvals (id, task_id, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![approval.id, approval.task_id, encode(approval)?],
        )?;
        Ok(())
    }

    pub fn approval(&self, id: &str) -> Result<Option<DeliveryApproval>, DeliveryError> {
        self.one("SELECT data FROM approvals WHERE id = ?1", id)
    }

    pub fn approvals(&self) -> Result<Vec<DeliveryApproval>, DeliveryError> {
        self.all("SELECT data FROM approvals ORDER BY rowid", [])
    }

    // ---- decisions ----

    pub fn put_decision(&self, decision: &DeliveryDecision) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO decisions (id, task_id, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![decision.id, decision.task_id, encode(decision)?],
        )?;
        Ok(())
    }

    pub fn decision(&self, id: &str) -> Result<Option<DeliveryDecision>, DeliveryError> {
        self.one("SELECT data FROM decisions WHERE id = ?1", id)
    }

    pub fn decisions(&self) -> Result<Vec<DeliveryDecision>, DeliveryError> {
        self.all("SELECT data FROM decisions ORDER BY rowid", [])
    }

    pub fn task_decisions(&self, task_id: &str) -> Result<Vec<DeliveryDecision>, DeliveryError> {
        self.all(
            "SELECT data FROM decisions WHERE task_id = ?1 ORDER BY rowid",
            [task_id],
        )
    }

    // ---- events ----

    /// Append-only audit trail; never rewritten.
    pub fn record_event(
        &self,
        at_ms: u64,
        kind: &str,
        task_id: Option<&str>,
        job_id: Option<&str>,
        detail: &str,
    ) -> Result<(), DeliveryError> {
        self.conn.execute(
            "INSERT INTO events (at_ms, kind, task_id, job_id, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![at_ms as i64, kind, task_id, job_id, detail],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn event_kinds(&self) -> Result<Vec<String>, DeliveryError> {
        let mut stmt = self.conn.prepare("SELECT kind FROM events ORDER BY seq")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<String>, _>>()?)
    }

    // ---- helpers ----

    fn one<T: DeserializeOwned>(&self, sql: &str, id: &str) -> Result<Option<T>, DeliveryError> {
        let value: Option<String> = self
            .conn
            .query_row(sql, [id], |row| row.get(0))
            .optional()?;
        value.map(|value| decode(&value)).transpose()
    }

    fn all<T: DeserializeOwned, P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<T>, DeliveryError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, |row| row.get::<_, String>(0))?;
        rows.map(|row| decode(&row?)).collect()
    }
}

/// The same repo is one key whether or not its path has the `\\?\` prefix.
fn path_key(path: &Path) -> String {
    plain_path(path)
}

fn encode<T: Serialize + ?Sized>(value: &T) -> Result<String, DeliveryError> {
    serde_json::to_string(value)
        .map_err(|error| DeliveryError::new("delivery_store_encode", error.to_string()))
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, DeliveryError> {
    serde_json::from_str(value)
        .map_err(|error| DeliveryError::new("delivery_store_decode", error.to_string()))
}
