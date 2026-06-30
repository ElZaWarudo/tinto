use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};

use crate::bus::contract::{
    AgentJournalSessionSummary, AgentSession, AgentSessionStatus, AgentSessionTimelineItem,
    AgentSessionTurnStatus,
};

#[derive(Debug)]
pub struct AgentJournal {
    conn: Connection,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentJournalError {
    #[error("no se pudo resolver el directorio de configuracion")]
    ConfigDirUnavailable,
    #[error("no se pudo preparar el directorio del diario: {0}")]
    CreateDir(#[source] std::io::Error),
    #[error("sqlite fallo: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json fallo: {0}")]
    Json(#[from] serde_json::Error),
}

impl AgentJournal {
    pub fn open_default() -> Result<Self, AgentJournalError> {
        let dir = dirs::config_dir()
            .ok_or(AgentJournalError::ConfigDirUnavailable)?
            .join("tinto");
        Self::open(dir.join("agent-journal.sqlite"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, AgentJournalError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(AgentJournalError::CreateDir)?;
        }
        let conn = Connection::open(path)?;
        let journal = Self { conn };
        journal.migrate()?;
        Ok(journal)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, AgentJournalError> {
        let conn = Connection::open_in_memory()?;
        let journal = Self { conn };
        journal.migrate()?;
        Ok(journal)
    }

    fn migrate(&self) -> Result<(), AgentJournalError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS agent_sessions (
              id TEXT PRIMARY KEY,
              repo TEXT NOT NULL,
              agent_type TEXT NOT NULL,
              source_kind TEXT NOT NULL DEFAULT 'local',
              distro TEXT,
              status TEXT NOT NULL,
              started_at_ms INTEGER NOT NULL,
              ended_at_ms INTEGER,
              updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_turns (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              turn_index INTEGER NOT NULL,
              status TEXT NOT NULL,
              started_at_ms INTEGER NOT NULL,
              completed_at_ms INTEGER,
              FOREIGN KEY(session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS agent_events (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              turn_id TEXT,
              seq INTEGER NOT NULL,
              kind TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              timestamp_ms INTEGER NOT NULL,
              FOREIGN KEY(session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE,
              FOREIGN KEY(turn_id) REFERENCES agent_turns(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_agent_events_session_seq
              ON agent_events(session_id, seq);
            CREATE INDEX IF NOT EXISTS idx_agent_sessions_repo_updated
              ON agent_sessions(repo, updated_at_ms DESC);
            "#,
        )?;
        Ok(())
    }

    pub fn record_session(&self, session: &AgentSession) -> Result<(), AgentJournalError> {
        let repo = session.repo.to_string_lossy().to_string();
        let ended_at_ms = session.ended_at_ms.map(|value| value as i64);
        let updated_at_ms = now_ms() as i64;
        self.conn.execute(
            r#"
            INSERT INTO agent_sessions (
              id, repo, agent_type, source_kind, distro, status,
              started_at_ms, ended_at_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
              repo = excluded.repo,
              agent_type = excluded.agent_type,
              source_kind = excluded.source_kind,
              distro = excluded.distro,
              status = excluded.status,
              ended_at_ms = excluded.ended_at_ms,
              updated_at_ms = excluded.updated_at_ms
            "#,
            params![
                &session.id,
                repo,
                &session.agent_type,
                if session.wsl_distro.is_some() {
                    "wsl"
                } else {
                    "local"
                },
                session.wsl_distro.as_deref(),
                status_name(session.status),
                session.started_at_ms as i64,
                ended_at_ms,
                updated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn record_timeline_item(
        &self,
        item: &AgentSessionTimelineItem,
    ) -> Result<(), AgentJournalError> {
        let seq = next_event_seq(&self.conn, &item.session_id)?;
        let kind = serde_json::to_value(item.kind)?
            .as_str()
            .unwrap_or("lifecycle")
            .to_string();
        let payload = serde_json::to_string(item)?;
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO agent_events (
              id, session_id, turn_id, seq, kind, payload_json, timestamp_ms
            ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)
            "#,
            params![
                item.id,
                item.session_id,
                seq,
                kind,
                payload,
                item.timestamp_ms as i64,
            ],
        )?;
        self.conn.execute(
            "UPDATE agent_sessions SET updated_at_ms = ?1 WHERE id = ?2",
            params![item.timestamp_ms as i64, item.session_id],
        )?;
        Ok(())
    }

    pub fn timeline_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<AgentSessionTimelineItem>, AgentJournalError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT payload_json
            FROM agent_events
            WHERE session_id = ?1
            ORDER BY seq ASC
            "#,
        )?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(serde_json::from_str(&row?)?);
        }
        Ok(items)
    }

    pub fn session_summaries(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentJournalSessionSummary>, AgentJournalError> {
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              s.id,
              s.repo,
              s.agent_type,
              s.distro,
              s.status,
              s.started_at_ms,
              s.ended_at_ms,
              s.updated_at_ms,
              (SELECT COUNT(*) FROM agent_events e WHERE e.session_id = s.id) AS event_count,
              (
                SELECT payload_json
                FROM agent_events e
                WHERE e.session_id = s.id
                ORDER BY e.seq DESC
                LIMIT 1
              ) AS last_event_json
            FROM agent_sessions s
            ORDER BY s.updated_at_ms DESC, s.started_at_ms DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let last_event_json: Option<String> = row.get(9)?;
            let last_event = last_event_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<AgentSessionTimelineItem>(json).ok());
            Ok(AgentJournalSessionSummary {
                id: row.get(0)?,
                repo: PathBuf::from(row.get::<_, String>(1)?),
                agent_type: row.get(2)?,
                wsl_distro: row.get(3)?,
                status: status_from_name(row.get::<_, String>(4)?.as_str()),
                started_at_ms: row.get::<_, i64>(5)? as u64,
                ended_at_ms: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                updated_at_ms: row.get::<_, i64>(7)? as u64,
                event_count: row.get::<_, i64>(8)? as usize,
                last_event_kind: last_event.as_ref().map(|event| event.kind),
                last_event_text: last_event.as_ref().map(|event| event.text.clone()),
                last_event_at_ms: last_event.as_ref().map(|event| event.timestamp_ms),
            })
        })?;
        let mut summaries = Vec::new();
        for row in rows {
            summaries.push(row?);
        }
        Ok(summaries)
    }

    pub fn session_from_journal(
        &self,
        session_id: &str,
    ) -> Result<Option<AgentSession>, AgentJournalError> {
        let session = self
            .conn
            .query_row(
                r#"
                SELECT id, repo, agent_type, distro, status, started_at_ms, ended_at_ms
                FROM agent_sessions
                WHERE id = ?1
                "#,
                params![session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, repo, agent_type, wsl_distro, status, started_at_ms, ended_at_ms)) = session
        else {
            return Ok(None);
        };
        let timeline = self.timeline_for_session(&id)?;
        let ended_at_ms = ended_at_ms.map(|value| value as u64);
        let status = archived_status(status_from_name(&status), ended_at_ms);
        Ok(Some(AgentSession {
            id,
            repo: PathBuf::from(repo),
            agent_type,
            wsl_distro,
            status,
            pid: None,
            started_at_ms: started_at_ms as u64,
            ended_at_ms,
            exit_code: None,
            error: None,
            checkpoint: None,
            change_log: Vec::new(),
            turn_status: AgentSessionTurnStatus::Waiting,
            turn_checkpoints: Vec::new(),
            timeline,
            reverted_at_ms: None,
            active_sessions: 0,
            age_ms: now_ms().saturating_sub(started_at_ms as u64),
            output_bytes_per_second: None,
        }))
    }
}

fn next_event_seq(conn: &Connection, session_id: &str) -> Result<i64, rusqlite::Error> {
    let seq = conn
        .query_row(
            "SELECT MAX(seq) FROM agent_events WHERE session_id = ?1",
            params![session_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten()
        .unwrap_or(0)
        + 1;
    Ok(seq)
}

fn status_name(status: AgentSessionStatus) -> &'static str {
    match status {
        AgentSessionStatus::Starting => "starting",
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::Error => "error",
        AgentSessionStatus::Exited => "exited",
        AgentSessionStatus::Reverted => "reverted",
    }
}

fn status_from_name(status: &str) -> AgentSessionStatus {
    match status {
        "starting" => AgentSessionStatus::Starting,
        "running" => AgentSessionStatus::Running,
        "completed" => AgentSessionStatus::Completed,
        "failed" => AgentSessionStatus::Failed,
        "error" => AgentSessionStatus::Error,
        "exited" => AgentSessionStatus::Exited,
        "reverted" => AgentSessionStatus::Reverted,
        _ => AgentSessionStatus::Exited,
    }
}

fn archived_status(status: AgentSessionStatus, ended_at_ms: Option<u64>) -> AgentSessionStatus {
    if ended_at_ms.is_some() {
        return status;
    }
    match status {
        AgentSessionStatus::Starting | AgentSessionStatus::Running => AgentSessionStatus::Exited,
        other => other,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::bus::contract::{AgentSession, AgentSessionTimelineKind};

    fn session(id: &str) -> AgentSession {
        AgentSession {
            id: id.to_string(),
            repo: PathBuf::from("/repo"),
            agent_type: "codex".to_string(),
            wsl_distro: None,
            status: AgentSessionStatus::Running,
            pid: Some(10),
            started_at_ms: 100,
            ended_at_ms: None,
            exit_code: None,
            error: None,
            checkpoint: None,
            change_log: Vec::new(),
            turn_status: crate::bus::contract::AgentSessionTurnStatus::Waiting,
            turn_checkpoints: Vec::new(),
            timeline: Vec::new(),
            reverted_at_ms: None,
            active_sessions: 1,
            age_ms: 10,
            output_bytes_per_second: None,
        }
    }

    #[test]
    fn journal_roundtrips_timeline_items_in_order() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        journal.record_session(&session("sess-1")).expect("session");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-2".to_string(),
                kind: AgentSessionTimelineKind::AgentMessage,
                text: "respuesta".to_string(),
                timestamp_ms: 300,
            })
            .expect("event 2");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::UserMessage,
                text: "hola".to_string(),
                timestamp_ms: 200,
            })
            .expect("event 1");

        let timeline = journal.timeline_for_session("sess-1").expect("timeline");
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].id, "event-2");
        assert_eq!(timeline[1].kind, AgentSessionTimelineKind::UserMessage);
    }

    #[test]
    fn journal_lists_session_summaries_with_last_event_preview() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        journal.record_session(&session("sess-1")).expect("session");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::AgentMessage,
                text: "preview".to_string(),
                timestamp_ms: 300,
            })
            .expect("event");

        let summaries = journal.session_summaries(10).expect("summaries");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "sess-1");
        assert_eq!(summaries[0].event_count, 1);
        assert_eq!(
            summaries[0].last_event_kind,
            Some(AgentSessionTimelineKind::AgentMessage)
        );
        assert_eq!(summaries[0].last_event_text.as_deref(), Some("preview"));
    }

    #[test]
    fn journal_reconstructs_archived_session_with_timeline() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        journal.record_session(&session("sess-1")).expect("session");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::Lifecycle,
                text: "Session started".to_string(),
                timestamp_ms: 200,
            })
            .expect("event");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        assert_eq!(archived.id, "sess-1");
        assert_eq!(archived.status, AgentSessionStatus::Exited);
        assert_eq!(archived.timeline.len(), 1);
        assert_eq!(archived.active_sessions, 0);
        assert!(archived.checkpoint.is_none());
    }
}
