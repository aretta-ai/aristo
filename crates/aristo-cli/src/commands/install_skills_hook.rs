//! Claude Code hook integration for `aristo install-skills` /
//! `aristo uninstall-skills`.
//!
//! `aristo install-skills --agent claude-code` writes two hook entries to
//! `<scope>/.claude/settings.json` (workspace scope by default; `--user`
//! writes to `~/.claude/settings.json`):
//!
//! 1. A `UserPromptSubmit` hook running `aristo session active
//!    --hook-format` (slice 27.5 Layer 2) — injects the active review
//!    session into every prompt so the agent can't drift while a review
//!    is open.
//! 2. A `SessionStart` hook running `aristo install-skills --hook-format`
//!    — a best-effort staleness check that, when the installed skills lag
//!    the binary, emits a SessionStart `additionalContext` block telling
//!    the agent to offer the user a refresh (`aristo install-skills
//!    --update`). Notify-only: it never rewrites skills itself.
//!
//! Both entries are identified by a marker substring of their command, so
//! install is idempotent and uninstall removes exactly our entries while
//! preserving any other hooks the user configured.

use std::path::Path;

use serde_json::{Map, Value};

use crate::{CliError, CliResult};

/// The `UserPromptSubmit` command. The trailing `|| true` is load-bearing:
/// if the user installs the hook before updating their on-PATH `aristo` to
/// a version that knows `session`, the hook would otherwise exit non-zero
/// and Claude Code would BLOCK every prompt with an "unrecognized
/// subcommand" error. The `|| true` makes the fallback exit 0, so the hook
/// silently injects nothing instead of breaking the user's workflow.
const HOOK_COMMAND: &str = "aristo session active --hook-format 2>/dev/null || true";

/// Marker substring inside [`HOOK_COMMAND`] uniquely identifying the aristo
/// session hook. Kept narrow (just the subcommand triple) so changes to
/// flags or shell wrapping don't break uninstall's matching.
const HOOK_MARKER: &str = "aristo session active --hook-format";

/// The `SessionStart` command: a best-effort skill-staleness check that
/// prints a SessionStart `additionalContext` block when installed skills
/// lag the binary. Same tolerant `2>/dev/null || true` fallback — a stale
/// or absent on-PATH aristo must never break session start.
const SESSION_START_COMMAND: &str = "aristo install-skills --hook-format 2>/dev/null || true";

/// Marker substring identifying the SessionStart staleness hook.
const SESSION_START_MARKER: &str = "aristo install-skills --hook-format";

// ─── nudge surface v2 (Phase 18 #9): the engine's hook surfaces ──────────────
// All emit via `hookSpecificOutput.additionalContext` (the only channel that
// reaches the agent — the S0a spike). Same tolerant `2>/dev/null || true`
// fallback so a stale on-PATH aristo never breaks a session. Behaviour is
// gated at runtime by `[nudges] aggressiveness` (off → silent), so these
// install unconditionally for Claude Code.

/// PostToolUse: the authoring-debt nudge (edit counter → "annotate" reminder).
const POST_TOOL_USE_COMMAND: &str = "aristo nudge --event post-tool-use 2>/dev/null || true";
const POST_TOOL_USE_MARKER: &str = "aristo nudge --event post-tool-use";

/// UserPromptSubmit: the consolidated per-turn review/verify/score nudge (the
/// Stop replacement — Stop's output never reached the agent).
const USER_PROMPT_NUDGE_COMMAND: &str =
    "aristo nudge --event user-prompt-submit 2>/dev/null || true";
const USER_PROMPT_NUDGE_MARKER: &str = "aristo nudge --event user-prompt-submit";

/// SessionStart: compute-only baseline capture (surfaces nothing itself).
const SESSION_START_NUDGE_COMMAND: &str = "aristo nudge --event session-start 2>/dev/null || true";
const SESSION_START_NUDGE_MARKER: &str = "aristo nudge --event session-start";

/// statusLine: the ambient user-facing status segment.
const STATUSLINE_COMMAND: &str = "aristo statusline 2>/dev/null || true";
const STATUSLINE_MARKER: &str = "aristo statusline";

/// Settings file path under `<root>/.claude/`.
fn settings_path(root: &Path) -> std::path::PathBuf {
    root.join(".claude").join("settings.json")
}

/// Read the existing settings.json (or an empty JSON object if missing).
/// Bubbles up parse errors so the user notices a manually-corrupted
/// settings file rather than us silently overwriting it.
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
    "Installing a hook is idempotent — running install twice leaves \
     settings.json with exactly one entry for that hook's marker, not two. \
     Existing entries are found by command substring and left in place. A \
     refactor that appends unconditionally would compound on every \
     reinstall. Applies uniformly to every hook event (UserPromptSubmit, \
     SessionStart).",
    verify = "neural",
    id = "install_claude_hook_is_idempotent"
)]
fn install_hook_into(root: &Path, event: &str, marker: &str, entry: Value) -> CliResult<bool> {
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

    let array = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| CliError::Other {
            message: format!(
                "`hooks.{event}` in {} is not a JSON array; refusing to overwrite",
                path.display()
            ),
            exit_code: 1,
        })?;

    let already_present = array.iter().any(|entry| entry_mentions_hook(entry, marker));
    if !already_present {
        array.push(entry);
    }
    write_settings(&path, &value)?;
    Ok(!already_present)
}

#[aristo::intent(
    "Uninstall removes ONLY the aristo entry matching the given marker from \
     its hook-event array; any other hooks the user configured are \
     preserved. After removal an empty array is left in place rather than \
     removed — the user may have intentional structure around the key. \
     Idempotent: uninstalling-when-not-installed is a no-op return.",
    verify = "neural",
    id = "uninstall_claude_hook_preserves_other_hooks"
)]
fn uninstall_hook_from(root: &Path, event: &str, marker: &str) -> CliResult<bool> {
    let path = settings_path(root);
    if !path.exists() {
        return Ok(false);
    }
    let mut value = read_settings(&path)?;
    let Some(root_obj) = value.as_object_mut() else {
        return Ok(false);
    };
    let Some(hooks) = root_obj.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let Some(arr) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let before = arr.len();
    arr.retain(|entry| !entry_mentions_hook(entry, marker));
    let removed = arr.len() < before;
    if removed {
        write_settings(&path, &value)?;
    }
    Ok(removed)
}

/// Install the `UserPromptSubmit` review-session hook. Returns `true` if a
/// new entry was written, `false` if one was already present.
pub fn install_claude_hook(root: &Path) -> CliResult<bool> {
    install_hook_into(root, "UserPromptSubmit", HOOK_MARKER, session_hook_entry())
}

/// Install the `SessionStart` skill-staleness hook.
pub fn install_session_start_hook(root: &Path) -> CliResult<bool> {
    install_hook_into(
        root,
        "SessionStart",
        SESSION_START_MARKER,
        session_start_hook_entry(),
    )
}

/// Remove the `UserPromptSubmit` review-session hook.
pub fn uninstall_claude_hook(root: &Path) -> CliResult<bool> {
    uninstall_hook_from(root, "UserPromptSubmit", HOOK_MARKER)
}

/// Remove the `SessionStart` skill-staleness hook.
pub fn uninstall_session_start_hook(root: &Path) -> CliResult<bool> {
    uninstall_hook_from(root, "SessionStart", SESSION_START_MARKER)
}

// ─── nudge surface v2 install/uninstall ─────────────────────────────────────

/// Install all nudge-engine surfaces for Claude Code: the PostToolUse,
/// UserPromptSubmit, and SessionStart hooks plus the statusLine. Idempotent.
pub fn install_nudge_surface(root: &Path) -> CliResult<()> {
    install_hook_into(
        root,
        "PostToolUse",
        POST_TOOL_USE_MARKER,
        post_tool_use_entry(),
    )?;
    install_hook_into(
        root,
        "UserPromptSubmit",
        USER_PROMPT_NUDGE_MARKER,
        user_prompt_nudge_entry(),
    )?;
    install_hook_into(
        root,
        "SessionStart",
        SESSION_START_NUDGE_MARKER,
        session_start_nudge_entry(),
    )?;
    install_statusline(root)?;
    Ok(())
}

/// Remove all nudge-engine surfaces. Each removal preserves any non-aristo
/// hooks and only touches a statusLine that is ours.
pub fn uninstall_nudge_surface(root: &Path) -> CliResult<()> {
    uninstall_hook_from(root, "PostToolUse", POST_TOOL_USE_MARKER)?;
    uninstall_hook_from(root, "UserPromptSubmit", USER_PROMPT_NUDGE_MARKER)?;
    uninstall_hook_from(root, "SessionStart", SESSION_START_NUDGE_MARKER)?;
    uninstall_statusline(root)?;
    Ok(())
}

#[aristo::intent(
    "Installing the statusLine NEVER clobbers an existing one: settings.json's \
     `statusLine` is a single value (not an append-safe array), so a user who \
     already configured a status line keeps it — aristo only sets it when the \
     field is absent.",
    verify = "test",
    id = "statusline_install_never_clobbers_user_config"
)]
fn install_statusline(root: &Path) -> CliResult<bool> {
    let path = settings_path(root);
    let mut value = read_settings(&path)?;
    let obj = ensure_object(&mut value);
    if obj.contains_key("statusLine") {
        return Ok(false); // respect the user's existing statusLine
    }
    obj.insert(
        "statusLine".to_string(),
        serde_json::json!({ "type": "command", "command": STATUSLINE_COMMAND }),
    );
    write_settings(&path, &value)?;
    Ok(true)
}

#[aristo::intent(
    "Uninstall deletes the status line only when its command carries aristo's \
     status-line marker; a status line the user configured themselves is left \
     in place. Unconditionally clearing the status line on uninstall would \
     silently destroy one aristo never owned, since the status line is a \
     single slot with no marker-filtered array to preserve a foreign entry.",
    verify = "test",
    id = "uninstall_statusline_only_removes_aristo_owned"
)]
fn uninstall_statusline(root: &Path) -> CliResult<bool> {
    let path = settings_path(root);
    if !path.exists() {
        return Ok(false);
    }
    let mut value = read_settings(&path)?;
    let Some(obj) = value.as_object_mut() else {
        return Ok(false);
    };
    let is_ours = obj
        .get("statusLine")
        .map(|v| entry_mentions_hook(v, STATUSLINE_MARKER))
        .unwrap_or(false);
    if is_ours {
        obj.remove("statusLine");
        write_settings(&path, &value)?;
        return Ok(true);
    }
    Ok(false)
}

/// `PostToolUse` entry: matched to the edit-like tools (the nudge emitter
/// also re-checks the tool name from stdin).
fn post_tool_use_entry() -> Value {
    serde_json::json!({
        "matcher": "Edit|Write|MultiEdit|NotebookEdit",
        "hooks": [ { "type": "command", "command": POST_TOOL_USE_COMMAND } ]
    })
}

fn user_prompt_nudge_entry() -> Value {
    serde_json::json!({
        "matcher": ".*",
        "hooks": [ { "type": "command", "command": USER_PROMPT_NUDGE_COMMAND } ]
    })
}

fn session_start_nudge_entry() -> Value {
    serde_json::json!({
        "hooks": [ { "type": "command", "command": SESSION_START_NUDGE_COMMAND } ]
    })
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("just set to object")
}

/// `UserPromptSubmit` entry shape: `{matcher, hooks: [{type, command}]}`.
fn session_hook_entry() -> Value {
    serde_json::json!({
        "matcher": ".*",
        "hooks": [ { "type": "command", "command": HOOK_COMMAND } ]
    })
}

/// `SessionStart` entry shape: `{hooks: [{type, command}]}` (no matcher —
/// SessionStart fires once per session, not per tool/prompt).
fn session_start_hook_entry() -> Value {
    serde_json::json!({
        "hooks": [ { "type": "command", "command": SESSION_START_COMMAND } ]
    })
}

/// True if the given hook entry contains a sub-hook whose command mentions
/// our marker. Tolerant of both flat (`command` at top level) and nested
/// (`hooks: [{command}]`) shapes so Claude Code schema changes don't
/// silently leave stale entries behind.
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
    fn both_hook_commands_use_tolerant_shell_fallback() {
        // Regression guard: both hooks MUST exit 0 even when aristo is
        // missing or doesn't recognize the subcommand — otherwise installing
        // a hook before updating PATH-resident aristo would break sessions.
        for cmd in [HOOK_COMMAND, SESSION_START_COMMAND] {
            assert!(cmd.contains("|| true"), "`{cmd}` must fall back gracefully");
            assert!(cmd.contains("2>/dev/null"), "`{cmd}` must suppress stderr");
        }
        assert!(HOOK_COMMAND.contains(HOOK_MARKER));
        assert!(SESSION_START_COMMAND.contains(SESSION_START_MARKER));
    }

    #[test]
    fn install_twice_remains_idempotent() {
        let tmp = TempDir::new().unwrap();
        assert!(install_claude_hook(tmp.path()).unwrap());
        assert!(
            !install_claude_hook(tmp.path()).unwrap(),
            "second install should be a no-op"
        );
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert_eq!(body.matches(HOOK_COMMAND).count(), 1);
    }

    #[test]
    fn session_start_hook_installs_idempotently_under_its_own_event() {
        let tmp = TempDir::new().unwrap();
        assert!(install_session_start_hook(tmp.path()).unwrap());
        assert!(!install_session_start_hook(tmp.path()).unwrap());
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert!(body.contains("SessionStart"));
        assert_eq!(body.matches(SESSION_START_COMMAND).count(), 1);
    }

    #[test]
    fn both_hooks_coexist_in_one_settings_file() {
        let tmp = TempDir::new().unwrap();
        install_claude_hook(tmp.path()).unwrap();
        install_session_start_hook(tmp.path()).unwrap();
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert!(body.contains("UserPromptSubmit"));
        assert!(body.contains("SessionStart"));
        assert!(body.contains(HOOK_MARKER));
        assert!(body.contains(SESSION_START_MARKER));
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
        assert!(uninstall_claude_hook(tmp.path()).unwrap());
        let body = std::fs::read_to_string(&settings).unwrap();
        assert!(!body.contains(HOOK_COMMAND));
        assert!(body.contains("someone_elses_tool"));
    }

    #[test]
    fn uninstall_session_start_removes_only_its_entry() {
        let tmp = TempDir::new().unwrap();
        install_claude_hook(tmp.path()).unwrap();
        install_session_start_hook(tmp.path()).unwrap();
        assert!(uninstall_session_start_hook(tmp.path()).unwrap());
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        // SessionStart entry gone; UserPromptSubmit one preserved.
        assert!(!body.contains(SESSION_START_COMMAND));
        assert!(body.contains(HOOK_COMMAND));
    }

    #[test]
    fn uninstall_when_not_installed_is_no_op() {
        let tmp = TempDir::new().unwrap();
        assert!(!uninstall_claude_hook(tmp.path()).unwrap());
        assert!(!uninstall_session_start_hook(tmp.path()).unwrap());
    }

    #[test]
    fn install_then_uninstall_idempotent_on_repeat() {
        let tmp = TempDir::new().unwrap();
        install_claude_hook(tmp.path()).unwrap();
        assert!(uninstall_claude_hook(tmp.path()).unwrap());
        assert!(!uninstall_claude_hook(tmp.path()).unwrap());
    }

    #[test]
    fn nudge_surface_installs_all_three_hooks_plus_statusline() {
        let tmp = TempDir::new().unwrap();
        install_nudge_surface(tmp.path()).unwrap();
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert!(body.contains(POST_TOOL_USE_MARKER));
        assert!(body.contains(USER_PROMPT_NUDGE_MARKER));
        assert!(body.contains(SESSION_START_NUDGE_MARKER));
        assert!(body.contains(STATUSLINE_MARKER));
        // Stop is intentionally NOT installed (its output never reaches the agent).
        assert!(!body.contains("--event stop"));
        assert!(!body.contains("\"Stop\""));
    }

    #[test]
    fn nudge_surface_install_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        install_nudge_surface(tmp.path()).unwrap();
        install_nudge_surface(tmp.path()).unwrap();
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert_eq!(body.matches(POST_TOOL_USE_COMMAND).count(), 1);
        assert_eq!(body.matches(USER_PROMPT_NUDGE_COMMAND).count(), 1);
        assert_eq!(body.matches(STATUSLINE_COMMAND).count(), 1);
    }

    #[test]
    fn nudge_surface_coexists_with_the_session_and_staleness_hooks() {
        let tmp = TempDir::new().unwrap();
        install_claude_hook(tmp.path()).unwrap(); // session-active (UserPromptSubmit)
        install_session_start_hook(tmp.path()).unwrap(); // staleness (SessionStart)
        install_nudge_surface(tmp.path()).unwrap();
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        // Both the pre-existing aristo hooks and the new nudge hooks are present.
        assert!(body.contains(HOOK_MARKER)); // session active
        assert!(body.contains(SESSION_START_MARKER)); // staleness
        assert!(body.contains(USER_PROMPT_NUDGE_MARKER));
        assert!(body.contains(SESSION_START_NUDGE_MARKER));
    }

    #[test]
    fn statusline_install_does_not_clobber_an_existing_one() {
        let tmp = TempDir::new().unwrap();
        let settings = settings_path(tmp.path());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{ "statusLine": { "type": "command", "command": "my_custom_bar" } }"#,
        )
        .unwrap();
        assert!(
            !install_statusline(tmp.path()).unwrap(),
            "must not overwrite a user statusLine"
        );
        let body = std::fs::read_to_string(&settings).unwrap();
        assert!(body.contains("my_custom_bar"));
        assert!(!body.contains(STATUSLINE_COMMAND));
        // …and uninstall leaves the user's statusLine alone (not ours).
        assert!(!uninstall_statusline(tmp.path()).unwrap());
        assert!(std::fs::read_to_string(&settings)
            .unwrap()
            .contains("my_custom_bar"));
    }

    #[test]
    fn nudge_surface_uninstall_removes_only_aristo_entries() {
        let tmp = TempDir::new().unwrap();
        install_nudge_surface(tmp.path()).unwrap();
        uninstall_nudge_surface(tmp.path()).unwrap();
        let body = std::fs::read_to_string(settings_path(tmp.path())).unwrap();
        assert!(!body.contains(POST_TOOL_USE_MARKER));
        assert!(!body.contains(USER_PROMPT_NUDGE_MARKER));
        assert!(!body.contains(STATUSLINE_COMMAND));
    }
}
