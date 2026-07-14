use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};

use crate::bus::contract::{
    AgentJournalSessionSummary, AgentSession, AgentSessionContextSummary, AgentSessionFeedback,
    AgentSessionGoal, AgentSessionPersonality, AgentSessionPlanMode, AgentSessionStatus,
    AgentSessionTimelineItem, AgentSessionTurnStatus,
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
        let dir = crate::runtime_paths::tinto_config_dir()
            .ok_or(AgentJournalError::ConfigDirUnavailable)?;
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
              updated_at_ms INTEGER NOT NULL,
              restored_to_turn_index INTEGER,
              goal_text TEXT,
              goal_updated_at_ms INTEGER,
              personality_name TEXT,
              personality_updated_at_ms INTEGER,
              plan_mode_enabled INTEGER,
              plan_mode_updated_at_ms INTEGER,
              feedback_json TEXT,
              context_summary_text TEXT,
              context_summary_created_at_ms INTEGER,
              context_summary_source_events INTEGER,
              context_summary_source_turns INTEGER
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
        self.ensure_agent_sessions_column("restored_to_turn_index", "INTEGER")?;
        self.ensure_agent_sessions_column("goal_text", "TEXT")?;
        self.ensure_agent_sessions_column("goal_updated_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("personality_name", "TEXT")?;
        self.ensure_agent_sessions_column("personality_updated_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("plan_mode_enabled", "INTEGER")?;
        self.ensure_agent_sessions_column("plan_mode_updated_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("feedback_json", "TEXT")?;
        self.ensure_agent_sessions_column("context_summary_text", "TEXT")?;
        self.ensure_agent_sessions_column("context_summary_created_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("context_summary_source_events", "INTEGER")?;
        self.ensure_agent_sessions_column("context_summary_source_turns", "INTEGER")?;
        Ok(())
    }

    fn ensure_agent_sessions_column(
        &self,
        name: &str,
        definition: &str,
    ) -> Result<(), AgentJournalError> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(agent_sessions)")?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for column in columns {
            if column? == name {
                return Ok(());
            }
        }
        self.conn.execute(
            &format!("ALTER TABLE agent_sessions ADD COLUMN {name} {definition}"),
            [],
        )?;
        Ok(())
    }

    pub fn record_session(&self, session: &AgentSession) -> Result<(), AgentJournalError> {
        let repo = session.repo.to_string_lossy().to_string();
        let ended_at_ms = session.ended_at_ms.map(|value| value as i64);
        let restored_to_turn_index = session.restored_to_turn_index.map(|value| value as i64);
        let goal_text = session.goal.as_ref().map(|goal| goal.text.as_str());
        let goal_updated_at_ms = session.goal.as_ref().map(|goal| goal.updated_at_ms as i64);
        let personality_name = session
            .personality
            .as_ref()
            .map(|personality| personality.name.as_str());
        let personality_updated_at_ms = session
            .personality
            .as_ref()
            .map(|personality| personality.updated_at_ms as i64);
        let plan_mode_enabled =
            session
                .plan_mode
                .as_ref()
                .map(|plan_mode| if plan_mode.enabled { 1_i64 } else { 0_i64 });
        let plan_mode_updated_at_ms = session
            .plan_mode
            .as_ref()
            .map(|plan_mode| plan_mode.updated_at_ms as i64);
        let feedback_json = if session.feedback.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&session.feedback)?)
        };
        let context_summary_text = session
            .context_summary
            .as_ref()
            .map(|summary| summary.text.as_str());
        let context_summary_created_at_ms = session
            .context_summary
            .as_ref()
            .map(|summary| summary.created_at_ms as i64);
        let context_summary_source_events = session
            .context_summary
            .as_ref()
            .map(|summary| summary.source_events as i64);
        let context_summary_source_turns = session
            .context_summary
            .as_ref()
            .map(|summary| summary.source_turns as i64);
        let updated_at_ms = now_ms() as i64;
        self.conn.execute(
            r#"
            INSERT INTO agent_sessions (
              id, repo, agent_type, source_kind, distro, status,
              started_at_ms, ended_at_ms, updated_at_ms, restored_to_turn_index,
              goal_text, goal_updated_at_ms, personality_name,
              personality_updated_at_ms, plan_mode_enabled, plan_mode_updated_at_ms,
              feedback_json, context_summary_text, context_summary_created_at_ms,
              context_summary_source_events, context_summary_source_turns
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
            ON CONFLICT(id) DO UPDATE SET
              repo = excluded.repo,
              agent_type = excluded.agent_type,
              source_kind = excluded.source_kind,
              distro = excluded.distro,
              status = excluded.status,
              ended_at_ms = excluded.ended_at_ms,
              updated_at_ms = excluded.updated_at_ms,
              restored_to_turn_index = excluded.restored_to_turn_index,
              goal_text = excluded.goal_text,
              goal_updated_at_ms = excluded.goal_updated_at_ms,
              personality_name = excluded.personality_name,
              personality_updated_at_ms = excluded.personality_updated_at_ms,
              plan_mode_enabled = excluded.plan_mode_enabled,
              plan_mode_updated_at_ms = excluded.plan_mode_updated_at_ms,
              feedback_json = excluded.feedback_json,
              context_summary_text = excluded.context_summary_text,
              context_summary_created_at_ms = excluded.context_summary_created_at_ms,
              context_summary_source_events = excluded.context_summary_source_events,
              context_summary_source_turns = excluded.context_summary_source_turns
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
                restored_to_turn_index,
                goal_text,
                goal_updated_at_ms,
                personality_name,
                personality_updated_at_ms,
                plan_mode_enabled,
                plan_mode_updated_at_ms,
                feedback_json,
                context_summary_text,
                context_summary_created_at_ms,
                context_summary_source_events,
                context_summary_source_turns,
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

    pub fn delete_session(&self, session_id: &str) -> Result<bool, AgentJournalError> {
        let deleted = self.conn.execute(
            "DELETE FROM agent_sessions WHERE id = ?1",
            params![session_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn session_from_journal(
        &self,
        session_id: &str,
    ) -> Result<Option<AgentSession>, AgentJournalError> {
        let session = self
            .conn
            .query_row(
                r#"
                SELECT
                  id, repo, agent_type, distro, status, started_at_ms, ended_at_ms,
                  restored_to_turn_index, goal_text, goal_updated_at_ms,
                  personality_name, personality_updated_at_ms,
                  plan_mode_enabled, plan_mode_updated_at_ms,
                  feedback_json,
                  context_summary_text, context_summary_created_at_ms,
                  context_summary_source_events, context_summary_source_turns
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
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, Option<i64>>(11)?,
                        row.get::<_, Option<i64>>(12)?,
                        row.get::<_, Option<i64>>(13)?,
                        row.get::<_, Option<String>>(14)?,
                        row.get::<_, Option<String>>(15)?,
                        row.get::<_, Option<i64>>(16)?,
                        row.get::<_, Option<i64>>(17)?,
                        row.get::<_, Option<i64>>(18)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            id,
            repo,
            agent_type,
            wsl_distro,
            status,
            started_at_ms,
            ended_at_ms,
            restored_to_turn_index,
            goal_text,
            goal_updated_at_ms,
            personality_name,
            personality_updated_at_ms,
            plan_mode_enabled,
            plan_mode_updated_at_ms,
            feedback_json,
            context_summary_text,
            context_summary_created_at_ms,
            context_summary_source_events,
            context_summary_source_turns,
        )) = session
        else {
            return Ok(None);
        };
        let timeline = self.timeline_for_session(&id)?;
        let feedback = feedback_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<Vec<AgentSessionFeedback>>(json).ok())
            .unwrap_or_default();
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
            runtime_options: Default::default(),
            goal: goal_text.map(|text| AgentSessionGoal {
                text,
                updated_at_ms: goal_updated_at_ms.unwrap_or(started_at_ms) as u64,
            }),
            personality: personality_name.map(|name| AgentSessionPersonality {
                name,
                updated_at_ms: personality_updated_at_ms.unwrap_or(started_at_ms) as u64,
            }),
            plan_mode: plan_mode_enabled.map(|enabled| AgentSessionPlanMode {
                enabled: enabled != 0,
                updated_at_ms: plan_mode_updated_at_ms.unwrap_or(started_at_ms) as u64,
            }),
            feedback,
            context_summary: context_summary_text.map(|text| AgentSessionContextSummary {
                text,
                created_at_ms: context_summary_created_at_ms.unwrap_or(started_at_ms) as u64,
                source_events: context_summary_source_events.unwrap_or(0).max(0) as usize,
                source_turns: context_summary_source_turns.unwrap_or(0).max(0) as usize,
            }),
            reverted_at_ms: None,
            restored_to_turn_index: restored_to_turn_index.map(|value| value as u32),
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
            runtime_options: Default::default(),
            goal: None,
            personality: None,
            plan_mode: None,
            feedback: Vec::new(),
            context_summary: None,
            reverted_at_ms: None,
            restored_to_turn_index: None,
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
    fn journal_deletes_a_session_and_its_related_events() {
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

        assert!(journal.delete_session("sess-1").expect("delete"));
        assert!(!journal.delete_session("sess-1").expect("delete missing"));
        assert!(journal.session_summaries(10).expect("summaries").is_empty());
        assert!(journal
            .session_from_journal("sess-1")
            .expect("read deleted session")
            .is_none());
        assert!(journal
            .timeline_for_session("sess-1")
            .expect("deleted timeline")
            .is_empty());
    }

    #[test]
    fn journal_reconstructs_restored_turn_index() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut restored = session("sess-1");
        restored.restored_to_turn_index = Some(1);
        journal.record_session(&restored).expect("session");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::UserMessage,
                text: "first".to_string(),
                timestamp_ms: 200,
            })
            .expect("event 1");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-2".to_string(),
                kind: AgentSessionTimelineKind::AgentMessage,
                text: "second".to_string(),
                timestamp_ms: 300,
            })
            .expect("event 2");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        assert_eq!(archived.restored_to_turn_index, Some(1));
        assert_eq!(archived.timeline.len(), 2);
    }

    #[test]
    fn journal_reconstructs_session_goal() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut with_goal = session("sess-1");
        with_goal.goal = Some(AgentSessionGoal {
            text: "Build the host harness".to_string(),
            updated_at_ms: 200,
        });
        journal.record_session(&with_goal).expect("session");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        let goal = archived.goal.expect("goal");
        assert_eq!(goal.text, "Build the host harness");
        assert_eq!(goal.updated_at_ms, 200);
    }

    #[test]
    fn journal_reconstructs_session_personality() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut with_personality = session("sess-1");
        with_personality.personality = Some(AgentSessionPersonality {
            name: "precise".to_string(),
            updated_at_ms: 220,
        });
        journal.record_session(&with_personality).expect("session");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        let personality = archived.personality.expect("personality");
        assert_eq!(personality.name, "precise");
        assert_eq!(personality.updated_at_ms, 220);
    }

    #[test]
    fn journal_reconstructs_session_plan_mode() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut planning = session("sess-1");
        planning.plan_mode = Some(AgentSessionPlanMode {
            enabled: true,
            updated_at_ms: 225,
        });
        journal.record_session(&planning).expect("session");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        let plan_mode = archived.plan_mode.expect("plan mode");
        assert!(plan_mode.enabled);
        assert_eq!(plan_mode.updated_at_ms, 225);
    }

    #[test]
    fn journal_reconstructs_session_feedback() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut with_feedback = session("sess-1");
        with_feedback.feedback = vec![AgentSessionFeedback {
            kind: "comment".to_string(),
            text: "Make the command palette feel native.".to_string(),
            created_at_ms: 230,
        }];
        journal.record_session(&with_feedback).expect("session");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        assert_eq!(archived.feedback.len(), 1);
        assert_eq!(archived.feedback[0].kind, "comment");
        assert_eq!(
            archived.feedback[0].text,
            "Make the command palette feel native."
        );
        assert_eq!(archived.feedback[0].created_at_ms, 230);
    }

    #[test]
    fn journal_reconstructs_context_summary() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut compacted = session("sess-1");
        compacted.context_summary = Some(AgentSessionContextSummary {
            text: "Goal: Build the host harness".to_string(),
            created_at_ms: 240,
            source_events: 3,
            source_turns: 2,
        });
        journal.record_session(&compacted).expect("session");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        let summary = archived.context_summary.expect("context summary");
        assert_eq!(summary.text, "Goal: Build the host harness");
        assert_eq!(summary.created_at_ms, 240);
        assert_eq!(summary.source_events, 3);
        assert_eq!(summary.source_turns, 2);
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
