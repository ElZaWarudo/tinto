//! Claude Code's turns, from its own hooks.
//!
//! Tinto launches `claude` with hooks (passed in `--settings`) that run
//! `tinto.exe --agent-hook <file>`. That mode appends each hook's JSON as one
//! line to the session's events file, and the session reads the new lines
//! into turn events. Claude in WSL runs the same Windows executable through
//! its `/mnt/c/…` path, so the file is always named by its Windows path.
//!
//! Claude reports no event when the user interrupts with Esc: an interrupted
//! turn ends with the next prompt, the idle notice or the end of the session.

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::pty::AgentProcessEvent;
use super::AgentConsoleError;

/// `tinto.exe --agent-hook <file>` appends the hook JSON read on stdin.
pub const AGENT_HOOK_FLAG: &str = "--agent-hook";

/// The hook mode: reads one hook's JSON on stdin and appends it as a line.
pub fn append_event(path: &str) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 1;
    }
    let Ok(event) = serde_json::from_str::<Value>(&input) else {
        return 1;
    };
    let line = format!("{event}\n");
    let appended = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(line.as_bytes()));
    i32::from(appended.is_err())
}

#[derive(Debug)]
pub struct ClaudeHooks {
    path: PathBuf,
    offset: u64,
    /// Claude ran a hook: it only does after the folder's trust dialog.
    active: bool,
    prompt_open: bool,
    interrupted: bool,
    /// The conversation to resume: the resumed one, then the last one that
    /// took a prompt (`/clear` starts another, empty until its first).
    session_id: Option<String>,
}

impl ClaudeHooks {
    pub fn create(resumed: Option<&str>) -> Result<Self, AgentConsoleError> {
        let dir = crate::runtime_paths::tinto_config_dir()
            .ok_or_else(|| hooks_error("no hay carpeta de configuración de Tinto"))?
            .join("agent-hooks");
        fs::create_dir_all(&dir).map_err(|error| hooks_error(error.to_string()))?;
        let path = dir.join(format!("{}.jsonl", uuid::Uuid::new_v4().simple()));
        fs::File::create(&path).map_err(|error| hooks_error(error.to_string()))?;
        let mut hooks = Self::at(path);
        hooks.session_id = resumed.map(str::to_owned);
        Ok(hooks)
    }

    fn at(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            active: false,
            prompt_open: false,
            interrupted: false,
            session_id: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// The `--settings` value: each hook runs `command --agent-hook <file>`
    /// in the background, without a shell.
    pub fn settings(&self, command: &str) -> String {
        let handler = json!([{
            "type": "command",
            "command": command,
            "args": [AGENT_HOOK_FLAG, forward_slashes(&self.path)],
            "async": true,
        }]);
        let entry = json!([{ "hooks": handler }]);
        json!({
            "hooks": {
                "SessionStart": entry,
                "UserPromptSubmit": entry,
                "Stop": entry,
                "StopFailure": entry,
                "SessionEnd": entry,
                "Notification": [{ "matcher": "idle_prompt", "hooks": handler }],
            }
        })
        .to_string()
    }

    /// Turn events from the lines appended since the last call. A line still
    /// being written is left for the next one.
    pub fn drain(&mut self) -> Vec<AgentProcessEvent> {
        let mut events = Vec::new();
        if std::mem::take(&mut self.interrupted) {
            events.extend(self.end_prompt());
        }
        let Ok(mut file) = fs::File::open(&self.path) else {
            return events;
        };
        let mut appended = Vec::new();
        if file.seek(SeekFrom::Start(self.offset)).is_err()
            || file.read_to_end(&mut appended).is_err()
        {
            return events;
        }
        let Some(end) = appended.iter().rposition(|byte| *byte == b'\n') else {
            return events;
        };
        self.offset += end as u64 + 1;
        for line in appended[..end].split(|byte| *byte == b'\n') {
            if let Ok(hook) = serde_json::from_slice::<Value>(line) {
                events.extend(self.apply(&hook));
            }
        }
        events
    }

    /// Tinto interrupted the turn itself (it sent Esc): the next drain ends it.
    pub fn mark_interrupted(&mut self) {
        self.interrupted = true;
    }

    fn apply(&mut self, hook: &Value) -> Vec<AgentProcessEvent> {
        self.active = true;
        match hook.get("hook_event_name").and_then(Value::as_str) {
            Some("UserPromptSubmit") => {
                if let Some(session_id) = hook.get("session_id").and_then(Value::as_str) {
                    self.session_id = Some(session_id.to_owned());
                }
                // A prompt still open was interrupted.
                let mut events = self.end_prompt();
                self.prompt_open = true;
                events.push(AgentProcessEvent::TurnStarted {
                    timestamp_ms: now_ms(),
                });
                events
            }
            Some("SessionStart" | "Stop" | "StopFailure" | "SessionEnd") => self.end_prompt(),
            Some("Notification")
                if hook.get("notification_type").and_then(Value::as_str) == Some("idle_prompt") =>
            {
                self.end_prompt()
            }
            _ => Vec::new(),
        }
    }

    fn end_prompt(&mut self) -> Vec<AgentProcessEvent> {
        if !std::mem::take(&mut self.prompt_open) {
            return Vec::new();
        }
        vec![AgentProcessEvent::TurnCompleted {
            timestamp_ms: now_ms(),
        }]
    }
}

impl Drop for ClaudeHooks {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn hooks_error(message: impl Into<String>) -> AgentConsoleError {
    AgentConsoleError::new("claude_hooks_unavailable", message)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooks_in(dir: &tempfile::TempDir) -> ClaudeHooks {
        let path = dir.path().join("events.jsonl");
        fs::File::create(&path).unwrap();
        ClaudeHooks::at(path)
    }

    fn append(hooks: &ClaudeHooks, text: &str) {
        OpenOptions::new()
            .append(true)
            .open(hooks.path())
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }

    fn kinds(events: &[AgentProcessEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|event| match event {
                AgentProcessEvent::TurnStarted { .. } => "started",
                AgentProcessEvent::TurnCompleted { .. } => "completed",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn a_prompt_opens_a_turn_and_its_stop_closes_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut hooks = hooks_in(&dir);
        assert!(!hooks.active());

        append(
            &hooks,
            "{\"hook_event_name\":\"SessionStart\",\"source\":\"startup\"}\n",
        );
        assert!(hooks.drain().is_empty());
        assert!(hooks.active());

        append(&hooks, "{\"hook_event_name\":\"UserPromptSubmit\"}\n");
        assert_eq!(kinds(&hooks.drain()), ["started"]);
        append(&hooks, "{\"hook_event_name\":\"Stop\"}\n");
        assert_eq!(kinds(&hooks.drain()), ["completed"]);
        // A second end for the same prompt changes nothing.
        append(
            &hooks,
            "{\"hook_event_name\":\"Notification\",\"notification_type\":\"idle_prompt\"}\n",
        );
        assert!(hooks.drain().is_empty());
    }

    #[test]
    fn a_prompt_after_an_interrupted_one_ends_it_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut hooks = hooks_in(&dir);
        append(
            &hooks,
            "{\"hook_event_name\":\"UserPromptSubmit\"}\n{\"hook_event_name\":\"UserPromptSubmit\"}\n",
        );
        assert_eq!(kinds(&hooks.drain()), ["started", "completed", "started"]);
        hooks.mark_interrupted();
        assert_eq!(kinds(&hooks.drain()), ["completed"]);
        hooks.mark_interrupted();
        assert!(hooks.drain().is_empty());
    }

    #[test]
    fn the_conversation_to_resume_is_the_last_one_that_took_a_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let mut hooks = hooks_in(&dir);
        hooks.session_id = Some("resumed".into());
        append(
            &hooks,
            "{\"hook_event_name\":\"SessionStart\",\"session_id\":\"resumed\",\"source\":\"resume\"}\n",
        );
        hooks.drain();
        assert_eq!(hooks.session_id(), Some("resumed"));

        // After /clear the new conversation is empty until its first prompt.
        append(
            &hooks,
            "{\"hook_event_name\":\"SessionStart\",\"session_id\":\"cleared\",\"source\":\"clear\"}\n",
        );
        hooks.drain();
        assert_eq!(hooks.session_id(), Some("resumed"));
        append(
            &hooks,
            "{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"cleared\"}\n",
        );
        hooks.drain();
        assert_eq!(hooks.session_id(), Some("cleared"));
    }

    #[test]
    fn idle_clear_and_failures_end_an_open_prompt() {
        for end in [
            "{\"hook_event_name\":\"Notification\",\"notification_type\":\"idle_prompt\"}",
            "{\"hook_event_name\":\"SessionStart\",\"source\":\"clear\"}",
            "{\"hook_event_name\":\"StopFailure\",\"error\":\"rate_limit\"}",
            "{\"hook_event_name\":\"SessionEnd\",\"reason\":\"clear\"}",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut hooks = hooks_in(&dir);
            append(&hooks, "{\"hook_event_name\":\"UserPromptSubmit\"}\n");
            hooks.drain();
            append(&hooks, &format!("{end}\n"));
            assert_eq!(kinds(&hooks.drain()), ["completed"], "{end}");
        }
    }

    #[test]
    fn other_events_partial_and_broken_lines_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut hooks = hooks_in(&dir);
        append(&hooks, "{\"hook_event_name\":\"UserPromptSubmit\"}\n");
        hooks.drain();
        append(
            &hooks,
            "{\"hook_event_name\":\"SubagentStop\"}\nnot json\n{\"hook_event_name\":\"Notification\",\"notification_type\":\"permission_prompt\"}\n{\"hook_event_name\":\"St",
        );
        assert!(hooks.drain().is_empty());
        append(&hooks, "op\"}\n");
        assert_eq!(kinds(&hooks.drain()), ["completed"]);
    }

    #[test]
    fn settings_run_the_hook_mode_without_a_shell() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = hooks_in(&dir);
        let settings: Value =
            serde_json::from_str(&hooks.settings("/mnt/c/Tinto/tinto.exe")).unwrap();
        let stop = &settings["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(stop["command"], "/mnt/c/Tinto/tinto.exe");
        assert_eq!(stop["args"][0], AGENT_HOOK_FLAG);
        assert_eq!(stop["args"][1], forward_slashes(hooks.path()));
        assert_eq!(stop["async"], true);
        assert_eq!(
            settings["hooks"]["Notification"][0]["matcher"],
            "idle_prompt"
        );
        assert!(settings["hooks"].get("SubagentStop").is_none());
    }

    #[test]
    fn the_events_file_goes_with_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = hooks_in(&dir);
        let path = hooks.path().to_path_buf();
        drop(hooks);
        assert!(!path.exists());
    }
}
