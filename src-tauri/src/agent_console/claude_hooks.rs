//! The hook mode older Claude Code conversations still call.
//!
//! Tinto used to launch `claude` in a terminal with hooks that ran
//! `tinto.exe --agent-hook <file>` to learn its turns. Claude Code now
//! speaks stream-json (see `claude_stream`), but a conversation started by
//! an older Tinto keeps calling the hook until it ends: without this mode
//! each call would open Tinto itself.

use std::fs::OpenOptions;
use std::io::{Read, Write};

use serde_json::Value;

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
