//! UI layout persistence (RDM-007). Stores the serialized dockview workspace
//! layout (plus any UI prefs) as an opaque JSON string in
//! `<config_dir>/tinto/ui-state.json`. Mirrors the workbench store's atomic
//! write (tmp-per-PID + rename) and corrupt-tolerant read: a missing or
//! unreadable file yields `None` rather than an error, so a bad UI-state file
//! never blocks startup. The frontend owns the JSON shape; the backend treats
//! it as an opaque blob.

use std::path::PathBuf;

const UI_STATE_FILE: &str = "ui-state.json";

fn ui_state_dir() -> Option<PathBuf> {
    crate::runtime_paths::tinto_config_dir()
}

/// Read the persisted UI state, or `None` when absent/unreadable.
fn load_ui_state(dir: &std::path::Path) -> Option<String> {
    let file = dir.join(UI_STATE_FILE);
    std::fs::read_to_string(file).ok()
}

/// Atomically write the UI state (tmp-per-PID + rename, like the workbench
/// store). Returns a safe error string on failure.
fn store_ui_state(dir: &std::path::Path, state: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{UI_STATE_FILE}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, state).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(UI_STATE_FILE)).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_ui_state() -> Option<String> {
    let dir = ui_state_dir()?;
    load_ui_state(&dir)
}

#[tauri::command]
pub fn set_ui_state(state: String) -> Result<(), String> {
    let dir = ui_state_dir().ok_or_else(|| "no config dir available".to_string())?;
    store_ui_state(&dir, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_ui_state(dir.path()).is_none());
    }

    #[test]
    fn store_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let payload = r#"{"layout":{"panels":["dashboard"]},"v":1}"#;
        store_ui_state(dir.path(), payload).unwrap();
        assert_eq!(load_ui_state(dir.path()).as_deref(), Some(payload));
    }

    #[test]
    fn overwrite_replaces_atomically_without_tmp_artifact() {
        let dir = tempfile::tempdir().unwrap();
        store_ui_state(dir.path(), "first").unwrap();
        store_ui_state(dir.path(), "second").unwrap();
        assert_eq!(load_ui_state(dir.path()).as_deref(), Some("second"));
        // No leftover tmp file from the atomic write.
        let leftover = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains(".tmp"));
        assert!(!leftover, "atomic write must not leave a tmp artifact");
    }

    #[test]
    fn unreadable_path_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        // Make the target a directory so read_to_string fails — must be None,
        // never a panic (the JS side falls back to the default layout).
        std::fs::create_dir(dir.path().join(UI_STATE_FILE)).unwrap();
        assert!(load_ui_state(dir.path()).is_none());
    }

    #[test]
    fn store_creates_dir_if_missing() {
        let base = tempfile::tempdir().unwrap();
        let nested = base.path().join("does/not/exist/yet");
        store_ui_state(&nested, "x").unwrap();
        assert_eq!(load_ui_state(&nested).as_deref(), Some("x"));
    }
}
