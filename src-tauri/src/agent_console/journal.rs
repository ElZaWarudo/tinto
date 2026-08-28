use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};

use crate::bus::contract::{
    AgentJournalSessionSummary, AgentSession, AgentSessionContextSummary, AgentSessionFeedback,
    AgentSessionGoal, AgentSessionPermissionMode, AgentSessionPersonality, AgentSessionPlanMode,
    AgentSessionStatus, AgentSessionTimelineItem, AgentSessionTurnStatus, AgentSubagentThread,
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
              provider_session_id TEXT,
              permission_mode TEXT,
              source_kind TEXT NOT NULL DEFAULT 'local',
              distro TEXT,
              status TEXT NOT NULL,
              started_at_ms INTEGER NOT NULL,
              ended_at_ms INTEGER,
              updated_at_ms INTEGER NOT NULL,
              restored_to_turn_index INTEGER,
              goal_text TEXT,
              goal_updated_at_ms INTEGER,
              goal_json TEXT,
              personality_name TEXT,
              personality_updated_at_ms INTEGER,
              plan_mode_enabled INTEGER,
              plan_mode_updated_at_ms INTEGER,
              feedback_json TEXT,
              context_summary_text TEXT,
              context_summary_created_at_ms INTEGER,
              context_summary_source_events INTEGER,
              context_summary_source_turns INTEGER,
              subagents_json TEXT
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
        self.ensure_agent_sessions_column("provider_session_id", "TEXT")?;
        self.ensure_agent_sessions_column("permission_mode", "TEXT")?;
        self.ensure_agent_sessions_column("goal_text", "TEXT")?;
        self.ensure_agent_sessions_column("goal_updated_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("goal_json", "TEXT")?;
        self.ensure_agent_sessions_column("personality_name", "TEXT")?;
        self.ensure_agent_sessions_column("personality_updated_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("plan_mode_enabled", "INTEGER")?;
        self.ensure_agent_sessions_column("plan_mode_updated_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("feedback_json", "TEXT")?;
        self.ensure_agent_sessions_column("context_summary_text", "TEXT")?;
        self.ensure_agent_sessions_column("context_summary_created_at_ms", "INTEGER")?;
        self.ensure_agent_sessions_column("context_summary_source_events", "INTEGER")?;
        self.ensure_agent_sessions_column("context_summary_source_turns", "INTEGER")?;
        self.ensure_agent_sessions_column("subagents_json", "TEXT")?;
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
        let goal_json = session
            .goal
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
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
        let permission_mode = session
            .agent_type
            .eq_ignore_ascii_case("codex")
            .then_some(session.permission_mode)
            .flatten()
            .map(permission_mode_name);
        let subagents_json = if session.subagents.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&session.subagents)?)
        };
        let updated_at_ms = now_ms() as i64;
        self.conn.execute(
            r#"
            INSERT INTO agent_sessions (
              id, repo, agent_type, provider_session_id, source_kind, distro, status,
              started_at_ms, ended_at_ms, updated_at_ms, restored_to_turn_index,
              goal_text, goal_updated_at_ms, goal_json, personality_name,
              personality_updated_at_ms, plan_mode_enabled, plan_mode_updated_at_ms,
              feedback_json, context_summary_text, context_summary_created_at_ms,
              context_summary_source_events, context_summary_source_turns, permission_mode,
              subagents_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
            ON CONFLICT(id) DO UPDATE SET
              repo = excluded.repo,
              agent_type = excluded.agent_type,
              provider_session_id = excluded.provider_session_id,
              source_kind = excluded.source_kind,
              distro = excluded.distro,
              status = excluded.status,
              ended_at_ms = excluded.ended_at_ms,
              updated_at_ms = excluded.updated_at_ms,
              restored_to_turn_index = excluded.restored_to_turn_index,
              goal_text = excluded.goal_text,
              goal_updated_at_ms = excluded.goal_updated_at_ms,
              goal_json = excluded.goal_json,
              personality_name = excluded.personality_name,
              personality_updated_at_ms = excluded.personality_updated_at_ms,
              plan_mode_enabled = excluded.plan_mode_enabled,
              plan_mode_updated_at_ms = excluded.plan_mode_updated_at_ms,
              feedback_json = excluded.feedback_json,
              context_summary_text = excluded.context_summary_text,
              context_summary_created_at_ms = excluded.context_summary_created_at_ms,
              context_summary_source_events = excluded.context_summary_source_events,
              context_summary_source_turns = excluded.context_summary_source_turns,
              permission_mode = excluded.permission_mode,
              subagents_json = excluded.subagents_json
            "#,
            params![
                &session.id,
                repo,
                &session.agent_type,
                session.provider_session_id.as_deref(),
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
                goal_json,
                personality_name,
                personality_updated_at_ms,
                plan_mode_enabled,
                plan_mode_updated_at_ms,
                feedback_json,
                context_summary_text,
                context_summary_created_at_ms,
                context_summary_source_events,
                context_summary_source_turns,
                permission_mode,
                subagents_json,
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

    /// Store descendant items under the root session row while preserving the
    /// provider thread identity. The normalized child timeline remains in the
    /// session graph JSON, and the event wrapper keeps legacy root timeline
    /// readers from accidentally displaying child output.
    pub fn record_subagent_timeline_item(
        &self,
        root_session_id: &str,
        thread_id: &str,
        item: &AgentSessionTimelineItem,
    ) -> Result<(), AgentJournalError> {
        let seq = next_event_seq(&self.conn, root_session_id)?;
        let kind = serde_json::to_value(item.kind)?
            .as_str()
            .unwrap_or("activity")
            .to_string();
        let payload = serde_json::to_string(&serde_json::json!({
            "thread_id": thread_id,
            "item": item,
        }))?;
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO agent_events (
              id, session_id, turn_id, seq, kind, payload_json, timestamp_ms
            ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)
            "#,
            params![
                format!("{root_session_id}:{thread_id}:{}", item.id),
                root_session_id,
                seq,
                kind,
                payload,
                item.timestamp_ms as i64,
            ],
        )?;
        self.conn.execute(
            "UPDATE agent_sessions SET updated_at_ms = ?1 WHERE id = ?2",
            params![item.timestamp_ms as i64, root_session_id],
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
              AND (json_extract(payload_json, '$.thread_id') IS NULL)
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
              ) AS last_event_json,
              (
                SELECT payload_json
                FROM agent_events e
                WHERE e.session_id = s.id
                  AND json_extract(e.payload_json, '$.kind') = 'user_message'
                ORDER BY e.seq ASC
                LIMIT 1
              ) AS first_user_event_json
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
            let first_user_event_json: Option<String> = row.get(10)?;
            let first_user_message = first_user_event_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<AgentSessionTimelineItem>(json).ok())
                .map(|event| event.text);
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
                first_user_message,
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
                  id, repo, agent_type, provider_session_id, distro, status, started_at_ms, ended_at_ms,
                  restored_to_turn_index, goal_text, goal_updated_at_ms, goal_json,
                  personality_name, personality_updated_at_ms,
                  plan_mode_enabled, plan_mode_updated_at_ms,
                  feedback_json,
                  context_summary_text, context_summary_created_at_ms,
                  context_summary_source_events, context_summary_source_turns,
                  permission_mode, subagents_json
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
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<i64>>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, Option<String>>(12)?,
                        row.get::<_, Option<i64>>(13)?,
                        row.get::<_, Option<i64>>(14)?,
                        row.get::<_, Option<i64>>(15)?,
                        row.get::<_, Option<String>>(16)?,
                        row.get::<_, Option<String>>(17)?,
                        row.get::<_, Option<i64>>(18)?,
                        row.get::<_, Option<i64>>(19)?,
                        row.get::<_, Option<i64>>(20)?,
                        row.get::<_, Option<String>>(21)?,
                        row.get::<_, Option<String>>(22)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            id,
            repo,
            agent_type,
            provider_session_id,
            wsl_distro,
            status,
            started_at_ms,
            ended_at_ms,
            restored_to_turn_index,
            goal_text,
            goal_updated_at_ms,
            goal_json,
            personality_name,
            personality_updated_at_ms,
            plan_mode_enabled,
            plan_mode_updated_at_ms,
            feedback_json,
            context_summary_text,
            context_summary_created_at_ms,
            context_summary_source_events,
            context_summary_source_turns,
            permission_mode,
            subagents_json,
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
        let permission_mode = if agent_type.eq_ignore_ascii_case("codex") {
            Some(permission_mode_from_name(permission_mode.as_deref()))
        } else {
            None
        };
        let subagents = subagents_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<Vec<AgentSubagentThread>>(json).ok())
            .unwrap_or_default()
            .into_iter()
            .map(stale_subagent)
            .collect();
        Ok(Some(AgentSession {
            id,
            repo: PathBuf::from(repo),
            agent_type,
            permission_mode,
            permission_mode_change_supported: false,
            provider_session_id,
            acp_runtime: None,
            acp_permissions: Vec::new(),
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
            subagents,
            runtime_options: Default::default(),
            goal: goal_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<AgentSessionGoal>(json).ok())
                .or_else(|| {
                    goal_text.map(|text| AgentSessionGoal {
                        text,
                        status: crate::bus::contract::AgentSessionGoalStatus::Active,
                        token_budget: None,
                        tokens_used: 0,
                        time_used_seconds: 0,
                        created_at_ms: goal_updated_at_ms.unwrap_or(started_at_ms) as u64,
                        updated_at_ms: goal_updated_at_ms.unwrap_or(started_at_ms) as u64,
                    })
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
            context_usage: None,
            turn_interrupt_supported: false,
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

fn permission_mode_name(mode: AgentSessionPermissionMode) -> &'static str {
    match mode {
        AgentSessionPermissionMode::Workspace => "workspace",
        AgentSessionPermissionMode::FullAccess => "full_access",
    }
}

fn permission_mode_from_name(mode: Option<&str>) -> AgentSessionPermissionMode {
    match mode {
        Some("full_access") => AgentSessionPermissionMode::FullAccess,
        _ => AgentSessionPermissionMode::Workspace,
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

fn stale_subagent(mut subagent: AgentSubagentThread) -> AgentSubagentThread {
    if matches!(
        subagent.thread_status.as_str(),
        "starting" | "running" | "active"
    ) || matches!(
        subagent.turn_status.as_str(),
        "working" | "inProgress" | "in_progress"
    ) {
        subagent.thread_status = "interrupted".to_string();
        subagent.turn_status = "waiting".to_string();
        subagent.runtime_state = Some("stale".to_string());
        if matches!(
            subagent.collaboration_status.as_deref(),
            Some("in_progress" | "inProgress" | "waiting_on_approval" | "waitingOnApproval")
        ) {
            subagent.collaboration_status = Some("interrupted".to_string());
        }
        subagent.capabilities.direct_input = false;
        subagent.capabilities.steer = false;
        subagent.capabilities.interrupt = false;
        subagent.capabilities.wait = false;
        subagent.capabilities.close = false;
    }
    subagent
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

    use crate::bus::contract::{
        AgentSession, AgentSessionAcpMode, AgentSessionAcpPermission,
        AgentSessionAcpPermissionState, AgentSessionAcpRuntime, AgentSessionAcpState,
        AgentSessionPermissionMode, AgentSessionTimelineKind, AgentSubagentCapabilities,
        AgentSubagentThread,
    };

    fn session(id: &str) -> AgentSession {
        AgentSession {
            id: id.to_string(),
            repo: PathBuf::from("/repo"),
            agent_type: "codex".to_string(),
            permission_mode: None,
            permission_mode_change_supported: false,
            provider_session_id: Some("thread-1".to_string()),
            acp_runtime: None,
            acp_permissions: Vec::new(),
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
            subagents: Vec::new(),
            runtime_options: Default::default(),
            goal: None,
            personality: None,
            plan_mode: None,
            feedback: Vec::new(),
            context_summary: None,
            context_usage: None,
            turn_interrupt_supported: false,
            reverted_at_ms: None,
            restored_to_turn_index: None,
            active_sessions: 1,
            age_ms: 10,
            output_bytes_per_second: None,
        }
    }

    fn subagent(id: &str, parent_id: Option<&str>, status: &str) -> AgentSubagentThread {
        AgentSubagentThread {
            id: id.into(),
            parent_id: parent_id.map(str::to_string),
            source_kind: "subAgentThreadSpawn".into(),
            depth: u32::from(parent_id.is_some()),
            agent_path: vec!["root".into(), id.into()],
            nickname: Some(id.into()),
            role: Some("worker".into()),
            model: Some("gpt-5.6-sol".into()),
            reasoning_effort: Some("high".into()),
            runtime: Some("codex".into()),
            approval_policy: Some("never".into()),
            permission_mode: Some("workspace".into()),
            capacity: Some(8),
            thread_status: status.into(),
            turn_status: if status == "running" {
                "working"
            } else {
                "waiting"
            }
            .into(),
            collaboration_status: None,
            collaboration_tool: None,
            consolidation_id: None,
            runtime_state: Some(status.into()),
            approval_request_id: None,
            prompt: None,
            preview: None,
            capabilities: AgentSubagentCapabilities {
                inspect: true,
                direct_input: status == "running",
                steer: status == "running",
                interrupt: status == "running",
                wait: true,
                close: true,
            },
            activities: Vec::new(),
            result: None,
            timeline: Vec::new(),
            updated_at_ms: 12,
        }
    }

    #[test]
    fn journal_roundtrips_nested_subagents_and_marks_live_children_stale() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut root = session("root-session");
        let mut child = subagent("child-thread", Some("thread-1"), "running");
        let grandchild = subagent("grandchild-thread", Some("child-thread"), "completed");
        child.timeline.push(AgentSessionTimelineItem {
            session_id: child.id.clone(),
            id: "child-message".into(),
            kind: AgentSessionTimelineKind::AgentMessage,
            text: "child result".into(),
            timestamp_ms: 20,
            attachments: Vec::new(),
        });
        root.subagents = vec![child, grandchild];
        journal.record_session(&root).expect("root graph");
        journal
            .record_subagent_timeline_item(
                "root-session",
                "child-thread",
                &root.subagents[0].timeline[0],
            )
            .expect("child event");

        let restored = journal
            .session_from_journal("root-session")
            .expect("load")
            .expect("root exists");
        assert!(
            restored.timeline.is_empty(),
            "child output must not pollute root"
        );
        assert_eq!(restored.subagents.len(), 2);
        assert_eq!(restored.subagents[0].timeline[0].text, "child result");
        assert_eq!(restored.subagents[0].thread_status, "interrupted");
        assert_eq!(
            restored.subagents[0].runtime_state.as_deref(),
            Some("stale")
        );
        assert!(!restored.subagents[0].capabilities.direct_input);
        assert!(!restored.subagents[0].capabilities.wait);
        assert_eq!(restored.subagents[1].thread_status, "completed");
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
                attachments: Vec::new(),
            })
            .expect("event 2");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::UserMessage,
                text: "hola".to_string(),
                timestamp_ms: 200,
                attachments: Vec::new(),
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
                id: "event-user".to_string(),
                kind: AgentSessionTimelineKind::UserMessage,
                text: "Revisa la autenticación del proyecto".to_string(),
                timestamp_ms: 200,
                attachments: Vec::new(),
            })
            .expect("user event");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::AgentMessage,
                text: "preview".to_string(),
                timestamp_ms: 300,
                attachments: Vec::new(),
            })
            .expect("event");

        let summaries = journal.session_summaries(10).expect("summaries");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "sess-1");
        assert_eq!(summaries[0].event_count, 2);
        assert_eq!(
            summaries[0].first_user_message.as_deref(),
            Some("Revisa la autenticación del proyecto")
        );
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
                attachments: Vec::new(),
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
                attachments: Vec::new(),
            })
            .expect("event 1");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-2".to_string(),
                kind: AgentSessionTimelineKind::AgentMessage,
                text: "second".to_string(),
                timestamp_ms: 300,
                attachments: Vec::new(),
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
            status: crate::bus::contract::AgentSessionGoalStatus::Paused,
            token_budget: Some(200_000),
            tokens_used: 45_000,
            time_used_seconds: 321,
            created_at_ms: 200,
            updated_at_ms: 200,
        });
        journal.record_session(&with_goal).expect("session");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        let goal = archived.goal.expect("goal");
        assert_eq!(goal.text, "Build the host harness");
        assert_eq!(
            goal.status,
            crate::bus::contract::AgentSessionGoalStatus::Paused
        );
        assert_eq!(goal.token_budget, Some(200_000));
        assert_eq!(goal.tokens_used, 45_000);
        assert_eq!(goal.time_used_seconds, 321);
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
        let mut live = session("sess-1");
        live.context_usage = Some(crate::bus::contract::AgentSessionContextUsage {
            used_tokens: 80_000,
            model_context_window: 128_000,
        });
        live.turn_interrupt_supported = true;
        live.acp_runtime = Some(AgentSessionAcpRuntime {
            state: AgentSessionAcpState::AcpReady,
            mode: Some(AgentSessionAcpMode::Acp),
            detail: Some("TINTO_SECRET_CANARY".to_owned()),
            lost_capabilities: Vec::new(),
            retry_available: false,
            image_attachments: false,
            config_options: Vec::new(),
        });
        live.acp_permissions = vec![AgentSessionAcpPermission {
            id: "permission".to_owned(),
            generation: 1,
            provider_session_id: "thread-1".to_owned(),
            turn_id: "turn".to_owned(),
            tool_call_id: "tool".to_owned(),
            title: "TINTO_SECRET_CANARY".to_owned(),
            options: Vec::new(),
            state: AgentSessionAcpPermissionState::Pending,
            reason: None,
            expires_at_ms: 500,
        }];
        journal.record_session(&live).expect("session");
        journal
            .record_timeline_item(&AgentSessionTimelineItem {
                session_id: "sess-1".to_string(),
                id: "event-1".to_string(),
                kind: AgentSessionTimelineKind::Lifecycle,
                text: "Session started".to_string(),
                timestamp_ms: 200,
                attachments: Vec::new(),
            })
            .expect("event");

        let archived = journal
            .session_from_journal("sess-1")
            .expect("read")
            .expect("session");

        assert!(!format!("{archived:?}").contains("TINTO_SECRET_CANARY"));
        assert_eq!(archived.id, "sess-1");
        assert_eq!(archived.provider_session_id.as_deref(), Some("thread-1"));
        assert!(archived.acp_runtime.is_none());
        assert!(archived.acp_permissions.is_empty());
        assert_eq!(archived.status, AgentSessionStatus::Exited);
        assert_eq!(archived.timeline.len(), 1);
        assert_eq!(archived.active_sessions, 0);
        assert!(archived.checkpoint.is_none());
        assert!(archived.context_usage.is_none());
        assert!(!archived.turn_interrupt_supported);
    }

    #[test]
    fn journal_roundtrips_codex_permission_mode_but_archived_capability_stays_false() {
        for (index, mode) in [
            AgentSessionPermissionMode::Workspace,
            AgentSessionPermissionMode::FullAccess,
        ]
        .into_iter()
        .enumerate()
        {
            let journal = AgentJournal::open_in_memory().expect("journal");
            let mut live = session(&format!("sess-{index}"));
            live.permission_mode = Some(mode);
            journal.record_session(&live).expect("session");

            let archived = journal
                .session_from_journal(&format!("sess-{index}"))
                .expect("read")
                .expect("session");

            assert_eq!(archived.permission_mode, Some(mode));
            assert!(!archived.permission_mode_change_supported);
        }
    }

    #[test]
    fn journal_uses_workspace_for_legacy_codex_rows_and_hides_non_codex_modes() {
        let journal = AgentJournal::open_in_memory().expect("journal");
        let mut legacy = session("legacy-codex");
        legacy.permission_mode = None;
        journal.record_session(&legacy).expect("session");
        journal
            .conn
            .execute(
                "UPDATE agent_sessions SET permission_mode = NULL WHERE id = ?1",
                params!["legacy-codex"],
            )
            .expect("legacy mode");

        let archived = journal
            .session_from_journal("legacy-codex")
            .expect("read")
            .expect("session");
        assert_eq!(
            archived.permission_mode,
            Some(AgentSessionPermissionMode::Workspace)
        );
        assert!(!archived.permission_mode_change_supported);

        let mut acp = session("acp");
        acp.agent_type = "kimi".to_string();
        acp.permission_mode = Some(AgentSessionPermissionMode::FullAccess);
        journal.record_session(&acp).expect("acp session");
        let archived_acp = journal
            .session_from_journal("acp")
            .expect("read acp")
            .expect("acp session");
        assert_eq!(archived_acp.permission_mode, None);
        assert!(!archived_acp.permission_mode_change_supported);
    }
}
