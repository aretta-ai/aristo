//! Claude Code hook integration for `aristo install-skills` /
//! `aristo uninstall-skills` (slice 27.5, step 4).
//!
//! Layer 2 of the three-layer enforcement described in
//! `docs/decisions/review-sessions.md` D2: a `UserPromptSubmit` hook
//! that runs `aristo session active --hook-format` and injects the
//! resulting system-reminder block into every prompt. The hook makes
//! the active session visible in every turn — the agent can't drift
//! into other work while a review is open.
//!
//! `aristo install-skills --agent claude-code` writes the hook entry
//! to `<scope>/.claude/settings.json` (workspace scope by default;
//! `--user` writes to `~/.claude/settings.json`). Uninstall removes
//! the entry cleanly while preserving any other hooks the user has.
//!
//! We identify our hook by the command string — any
//! `UserPromptSubmit` entry whose command contains
//! `aristo session active --hook-format` is ours.

use std::path::Path;

use serde_json::{Map, Value};

use crate::{CliError, CliResult};

/// Marker substring uniquely identifying the aristo session hook.
/// Uninstall finds and removes any `UserPromptSubmit` entry whose
/// command contains this string.
const HOOK_COMMAND: &str = "aristo session active --hook-format";

/// Settings file path under `<root>/.claude/`.
fn settings_path(root: &Path) -> std::path::PathBuf {
    root.join(".claude").join("settings.json")
}

/// Read the existing settings.json (or an empty JSON object if
/// missing). Bubbles up parse errors so the user notices a manually-
/// corrupted settings file rather than us silently overwriting it.
fn read_settings(path: &Path) -> CliResult<Value> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Value::Object(Map::new())),
        Err(e) => return Err(e.into()),
    };
    serde_json::from_str(&text).map_err(|e| CliError::Other {
        message: format!("could not parse {}: {e}", path.display()),
        exit_code: 1,
    })
}

fn write_settings(path: &Path, value: &Value) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| CliError::Other {
        message: format!("settings serialize: {e}"),
        exit_code: 1,
    })?;
    std::fs::write(path, text)?;
    Ok(())
}

#[aristo::intent(
    "Installing the session hook is idempotent — running install twice \
     leaves the settings.json with exactly one `aristo session active \
     --hook-format` entry, not two. We find existing entries by command \
     substring and replace in place. A refactor that appends \
     unconditionally would compound on every reinstall.",
    verify = "neural",
    id = "install_claude_hook_is_idempotent"
)]
pub fn install_claude_hook(root: &Path) -> CliResult<bool> {
    let path = settings_path(root);
    let mut value = read_settings(&path)?;
    let root_obj = ensure_object(&mut value);

    let hooks = root_obj
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| CliError::Other {
            message: format!(
                "`hooks` key in {} is not a JSON object; refusing to overwrite",
                path.display()
            ),
            exit_code: 1,
        })?;

    let prompt_submit_array = hooks
        .entry("UserPromptSubmit".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| CliError::Other {
            message: format!(
                "`hooks.UserPromptSubmit` in {} is not a JSON array; refusing to overwrite",
                path.display()
            ),
            exit_code: 1,
        })?;

    let already_present = prompt_submit_array
        .iter()
        .any(|entry| entry_mentions_hook(entry, HOOK_COMMAND));
    if !already_present {
        prompt_submit_array.push(aristo_hook_entry());
    }
    write_settings(&path, &value)?;
    Ok(!already_present)
}

#[aristo::intent(
    "Uninstall removes ONLY aristo's session hook entry from \
     UserPromptSubmit; any other hooks the user configured are \
     preserved. After removal, if UserPromptSubmit becomes empty we \
     leave the empty array in place rather than removing it — the \
     user may have intentional structure around the key. Idempotent: \
     uninstalling-when-not-installed is a no-op return.",
    verify = "neural",
    id = "uninstall_claude_hook_preserves_other_hooks"
)]
pub fn uninstall_claude_hook(root: &Path) -> CliResult<bool> {
    let path = settings_path(root);
    let mut value = match read_settings(&path) {
        Ok(v) => v,
        // Missing settings file → nothing to uninstall.
        Err(_) if !path.exists() => return Ok(false),
        Err(e) => return Err(e),
    };
    if !path.exists() {
        return Ok(false);
    }
    let Some(root_obj) = value.as_object_mut() else {
        return Ok(false);
    };
    let Some(hooks) = root_obj.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let Some(arr) = hooks
        .get_mut("UserPromptSubmit")
        .and_then(Value::as_array_mut)
    else {
        return Ok(false);
    };
    let before = arr.len();
    arr.retain(|entry| !entry_mentions_hook(entry, HOOK_COMMAND));
    let removed = arr.len() < before;
    if removed {
        write_settings(&path, &value)?;
    }
    Ok(removed)
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("just set to object")
}

/// Build the canonical hook entry shape Claude Code expects:
/// `{matcher, hooks: [{type, command}]}`.
fn aristo_hook_entry() -> Value {
    serde_json::json!({
        "matcher": ".*",
        "hooks": [
            {
                "type": "command",
                "command": HOOK_COMMAND,
            }
        ]
    })
}

/// True if the given UserPromptSubmit entry contains a sub-hook
/// whose command mentions our marker. Tolerant of both flat
/// (`command` at top level) and nested (`hooks: [{command}]`) shapes
/// so future Claude Code schema changes don't silently leave stale
/// entries behind.
fn entry_mentions_hook(entry: &Value, marker: &str) -> bool {
    fn has(entry: &Value, marker: &str) -> bool {
        if let Some(s) = entry.get("command").and_then(Value::as_str) {
            if s.contains(marker) {
                return true;
            }
        }
        if let Some(arr) = entry.get("hooks").and_then(Value::as_array) {
            return arr.iter().any(|e| has(e, marker));
        }
        false
    }
    has(entry, marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn install_creates_settings_when_missing() {
        let tmp = TempDir::new().unwrap();
        let inserted = install_claude_hook(tmp.path()).unwrap();
        assert!(inserted);
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert!(body.contains(HOOK_COMMAND), "body: {body}");
    }

    #[test]
    fn install_twice_remains_idempotent() {
        let tmp = TempDir::new().unwrap();
        let first = install_claude_hook(tmp.path()).unwrap();
        assert!(first);
        let second = install_claude_hook(tmp.path()).unwrap();
        assert!(!second, "second install should be a no-op");
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        let count = body.matches(HOOK_COMMAND).count();
        assert_eq!(count, 1, "expected exactly one hook entry, got {count}");
    }

    #[test]
    fn install_preserves_other_existing_hooks() {
        let tmp = TempDir::new().unwrap();
        let settings = settings_path(tmp.path());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{ "hooks": { "UserPromptSubmit": [{ "matcher": ".*", "hooks": [{"type":"command","command":"someone_elses_tool --hook"}] }] } }"#,
        )
        .unwrap();
        install_claude_hook(tmp.path()).unwrap();
        let body = std::fs::read_to_string(&settings).unwrap();
        assert!(body.contains("someone_elses_tool"));
        assert!(body.contains(HOOK_COMMAND));
    }

    #[test]
    fn uninstall_removes_only_aristo_entry() {
        let tmp = TempDir::new().unwrap();
        let settings = settings_path(tmp.path());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{ "hooks": { "UserPromptSubmit": [{ "matcher": ".*", "hooks": [{"type":"command","command":"someone_elses_tool --hook"}] }] } }"#,
        )
        .unwrap();
        install_claude_hook(tmp.path()).unwrap();
        let removed = uninstall_claude_hook(tmp.path()).unwrap();
        assert!(removed);
        let body = std::fs::read_to_string(&settings).unwrap();
        assert!(!body.contains(HOOK_COMMAND));
        assert!(body.contains("someone_elses_tool"));
    }

    #[test]
    fn uninstall_when_not_installed_is_no_op() {
        let tmp = TempDir::new().unwrap();
        let removed = uninstall_claude_hook(tmp.path()).unwrap();
        assert!(!removed);
    }

    #[test]
    fn install_then_uninstall_idempotent_on_repeat() {
        let tmp = TempDir::new().unwrap();
        install_claude_hook(tmp.path()).unwrap();
        let removed1 = uninstall_claude_hook(tmp.path()).unwrap();
        assert!(removed1);
        let removed2 = uninstall_claude_hook(tmp.path()).unwrap();
        assert!(!removed2);
    }
}
