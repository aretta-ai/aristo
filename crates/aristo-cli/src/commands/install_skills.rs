//! `aristo install-skills` and `aristo uninstall-skills`.
//!
//! Per K4, each agent has its own install path layout. The two install
//! backends (`file_copy_install`, `agents_md_install`) live in
//! `crate::skills::install`; this module is the per-agent dispatch + the
//! user-facing output.

use std::path::{Path, PathBuf};

use crate::skills::install::{
    agents_md_install, agents_md_state, agents_md_uninstall, file_copy_install, file_copy_state,
    file_copy_uninstall, InstallOutcome, SkillState,
};
use crate::skills::{self, Skill};
use crate::{CliError, CliResult};

/// Every agent we know how to install for. Drives the staleness audit
/// (`--check`, the post-command notice, `aristo status`) and the
/// no-`--agent` `--update` heal.
const ALL_AGENTS: [Agent; 5] = [
    Agent::ClaudeCode,
    Agent::Cursor,
    Agent::Codex,
    Agent::OpenCode,
    Agent::Antigravity,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agent {
    ClaudeCode,
    Cursor,
    Codex,
    OpenCode,
    Antigravity,
}

impl Agent {
    fn parse(s: &str) -> Result<Self, CliError> {
        match s {
            "claude-code" => Ok(Agent::ClaudeCode),
            "cursor" => Ok(Agent::Cursor),
            "codex" => Ok(Agent::Codex),
            "opencode" => Ok(Agent::OpenCode),
            "antigravity" => Ok(Agent::Antigravity),
            other => Err(CliError::Other {
                message: format!(
                    "unknown agent `{other}`; supported: claude-code, cursor, \
                     codex, opencode, antigravity (try --list-agents)"
                ),
                exit_code: 2,
            }),
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude-code",
            Agent::Cursor => "cursor",
            Agent::Codex => "codex",
            Agent::OpenCode => "opencode",
            Agent::Antigravity => "antigravity",
        }
    }

    fn pretty(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "Claude Code",
            Agent::Cursor => "Cursor",
            Agent::Codex => "Codex",
            Agent::OpenCode => "OpenCode",
            Agent::Antigravity => "Antigravity",
        }
    }
}

#[aristo::intent(
    "The per-skill install progression and the `ok: N skill(s) installed \
     for <slug>.` success line are identical at project scope and user \
     scope, modulo the on-disk target path. The post-success scope-tip \
     line legitimately differs: project scope prints a hint pointing \
     at `--user`; user scope prints nothing (the user already chose \
     the broader scope, so the cross-scope hint would be noise). A \
     refactor that changes the install progression or success-summary \
     wording for only one scope would break the symmetric core; a \
     refactor that adds the project-scope tip to user scope would \
     re-introduce the noise the asymmetry exists to avoid.",
    verify = "neural",
    id = "install_skills_scope_symmetry"
)]
pub(crate) fn install(
    agent: Option<String>,
    list_agents: bool,
    user: bool,
    update: bool,
    check: bool,
    hook_format: bool,
) -> CliResult<()> {
    if hook_format {
        // SessionStart hook entry point: emit a staleness `additionalContext`
        // block (or nothing) and exit 0. Must never error out of a hook.
        return run_session_start_hook();
    }
    if list_agents {
        emit_agent_list();
        return Ok(());
    }
    if check {
        return run_check(agent, user);
    }
    if update {
        // `--update --agent X` re-pins X; `--update` alone heals every
        // already-installed agent in place. The healing path the staleness
        // notice points at — distinct from the fresh-install progression below.
        return match agent {
            Some(name) => repin_agent(&name, user),
            None => update_all_installed(user),
        };
    }
    let agent = agent.ok_or_else(|| CliError::Other {
        message: "missing --agent flag (try `aristo install-skills --list-agents`)".to_string(),
        exit_code: 2,
    })?;
    let agent = Agent::parse(&agent)?;

    let root = scope_root(user)?;
    let scope_label = if user { "user-level" } else { "project-level" };

    println!();
    println!(
        "→ Installing Aristo skills for {} ({}) …",
        agent.pretty(),
        scope_label
    );

    match agent {
        Agent::ClaudeCode | Agent::Cursor | Agent::Antigravity => {
            install_file_copy_agent(agent, &root)?;
        }
        Agent::Codex | Agent::OpenCode => {
            install_agents_md_agent(agent, &root)?;
        }
    }

    let count = skills::bundled().len();
    println!();
    println!(
        "ok: {count} skill{plural} installed for {slug}.",
        plural = if count == 1 { "" } else { "s" },
        slug = agent.slug()
    );

    print_install_tip(agent, user);
    Ok(())
}

#[aristo::intent(
    "`install_skills` followed by `uninstall_skills` leaves the project's \
     relevant on-disk state identical to before either ran (modulo files \
     the user hand-modified). File-copy agents: the per-skill files we \
     wrote are removed. AGENTS.md agents: the marker-delimited block is \
     stripped; surrounding content preserved. Idempotent on \
     uninstall-while-uninstalled.",
    verify = "test",
    id = "uninstall_skills_reverses_install"
)]
pub(crate) fn uninstall(agent: String, user: bool, force: bool) -> CliResult<()> {
    let _ = force; // --force overrides the "skip locally-modified" check in
                   // slice 19+; for slice 13 we always remove cleanly.
    let agent = Agent::parse(&agent)?;
    let root = scope_root(user)?;
    let scope_label = if user { "user-level" } else { "project-level" };

    println!();
    println!(
        "→ Uninstalling Aristo skills for {} ({}) …",
        agent.pretty(),
        scope_label
    );

    let removed = match agent {
        Agent::ClaudeCode | Agent::Cursor | Agent::Antigravity => {
            uninstall_file_copy_agent(agent, &root)?
        }
        Agent::Codex | Agent::OpenCode => uninstall_agents_md_agent(agent, &root)?,
    };

    println!();
    if removed > 0 {
        println!(
            "ok: {removed} skill{plural} removed for {slug}.",
            plural = if removed == 1 { "" } else { "s" },
            slug = agent.slug()
        );
    } else {
        println!(
            "ok: nothing to do (no Aristo skills installed for {}).",
            agent.slug()
        );
    }
    Ok(())
}

fn scope_root(user: bool) -> CliResult<PathBuf> {
    if user {
        dirs::home_dir().ok_or_else(|| CliError::Other {
            message: "could not resolve user home directory for --user install".to_string(),
            exit_code: 1,
        })
    } else {
        Ok(std::env::current_dir()?)
    }
}

fn install_file_copy_agent(agent: Agent, root: &std::path::Path) -> CliResult<()> {
    for skill in skills::bundled() {
        let target = file_copy_target(agent, root, skill);
        let outcome = file_copy_install(&target, skill).map_err(CliError::Io)?;
        let display = relative_display(root, &target);
        match outcome {
            InstallOutcome::Created => println!("  • Wrote: {display}"),
            InstallOutcome::Updated => println!("  • Updated: {display}"),
            InstallOutcome::Unchanged => println!("  • Unchanged: {display}"),
        }
    }
    // Claude Code also gets the review-session UserPromptSubmit hook
    // entry; other file-copy agents (Cursor, Antigravity) don't have
    // an equivalent prompt-injection surface.
    if matches!(agent, Agent::ClaudeCode) {
        let settings_display = relative_display(root, &root.join(".claude").join("settings.json"));
        let inserted = super::install_skills_hook::install_claude_hook(root)?;
        if inserted {
            println!("  • Wrote review-session hook to: {settings_display}");
        } else {
            println!("  • Review-session hook already present: {settings_display}");
        }
        // SessionStart hook: nudges the agent to refresh stale skills.
        let hook_inserted = super::install_skills_hook::install_session_start_hook(root)?;
        if hook_inserted {
            println!("  • Wrote skill-staleness SessionStart hook to: {settings_display}");
        } else {
            println!("  • Skill-staleness SessionStart hook already present: {settings_display}");
        }
        // Nudge-engine surface (Phase 18 #9): PostToolUse + UserPromptSubmit +
        // SessionStart hooks + the ambient statusLine. Idempotent; gated at
        // runtime by `[nudges] aggressiveness` (off → silent).
        super::install_skills_hook::install_nudge_surface(root)?;
        println!("  • Wrote nudge-engine hooks + statusLine to: {settings_display}");
    }
    Ok(())
}

fn install_agents_md_agent(agent: Agent, root: &std::path::Path) -> CliResult<()> {
    let target = root.join("AGENTS.md");
    let bundled = skills::bundled();
    let refs: Vec<&Skill> = bundled.iter().collect();
    let outcome = agents_md_install(&target, &refs).map_err(CliError::Io)?;
    let display = relative_display(root, &target);
    match outcome {
        InstallOutcome::Created => println!("  • Created: {display}"),
        InstallOutcome::Updated => {
            println!("  • Updated: {display} (block replaced; user content preserved)")
        }
        InstallOutcome::Unchanged => println!("  • Unchanged: {display}"),
    }
    let _ = agent; // codex / opencode share the same AGENTS.md handling
    Ok(())
}

fn uninstall_file_copy_agent(agent: Agent, root: &std::path::Path) -> CliResult<usize> {
    let mut removed = 0;
    for skill in skills::bundled() {
        let target = file_copy_target(agent, root, skill);
        if file_copy_uninstall(&target).map_err(CliError::Io)? {
            println!("  • Removed: {}", relative_display(root, &target));
            removed += 1;
        }
    }
    if matches!(agent, Agent::ClaudeCode) {
        let settings_display = relative_display(root, &root.join(".claude").join("settings.json"));
        if super::install_skills_hook::uninstall_claude_hook(root)? {
            println!("  • Removed review-session hook from: {settings_display}");
            removed += 1;
        }
        if super::install_skills_hook::uninstall_session_start_hook(root)? {
            println!("  • Removed skill-staleness SessionStart hook from: {settings_display}");
            removed += 1;
        }
        // Nudge-engine surface (hooks + our statusLine, never a user's).
        super::install_skills_hook::uninstall_nudge_surface(root)?;
    }
    Ok(removed)
}

fn uninstall_agents_md_agent(_agent: Agent, root: &std::path::Path) -> CliResult<usize> {
    let target = root.join("AGENTS.md");
    if agents_md_uninstall(&target).map_err(CliError::Io)? {
        println!(
            "  • Stripped Aristo block from: {}",
            relative_display(root, &target)
        );
        Ok(1)
    } else {
        Ok(0)
    }
}

fn file_copy_target(agent: Agent, root: &std::path::Path, skill: &Skill) -> PathBuf {
    match agent {
        Agent::ClaudeCode => root
            .join(".claude")
            .join("skills")
            .join(skill.name)
            .join("SKILL.md"),
        Agent::Cursor => root
            .join(".cursor")
            .join("rules")
            .join(format!("{}.mdc", skill.name)),
        Agent::Antigravity => root
            .join(".antigravity")
            .join("skills")
            .join(format!("{}.md", skill.name)),
        Agent::Codex | Agent::OpenCode => unreachable!("AGENTS.md agents take a different path"),
    }
}

fn relative_display(root: &std::path::Path, target: &std::path::Path) -> String {
    target
        .strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| target.display().to_string())
}

fn emit_agent_list() {
    println!("Supported agents:");
    println!("  • claude-code   — file copy to .claude/skills/<skill>/SKILL.md");
    println!("  • cursor        — file copy to .cursor/rules/<skill>.mdc");
    println!("  • codex         — AGENTS.md section injection");
    println!("  • opencode      — AGENTS.md section injection (shares block with codex)");
    println!("  • antigravity   — file copy to .antigravity/skills/<skill>.md (format TBD)");
    println!();
    println!("Default install scope: project-level. Pass --user to install at user level");
    println!("(skills available across all projects on this machine).");
}

fn print_install_tip(agent: Agent, user: bool) {
    if user {
        return;
    }
    let (dir, user_dir) = match agent {
        Agent::ClaudeCode => (".claude/", "~/.claude/skills/"),
        Agent::Cursor => (".cursor/", "~/.cursor/rules/"),
        Agent::Codex | Agent::OpenCode => return,
        Agent::Antigravity => (".antigravity/", "~/.antigravity/skills/"),
    };
    println!();
    println!("Tip: commit {dir} to share skills with your team. To install globally");
    println!("instead, pass --user (writes to {user_dir}).");
}

// ─── Staleness audit ────────────────────────────────────────────────────────
// Installed skill files carry the SDK version they were rendered from
// (`sdk_version:` in frontmatter / the AGENTS.md block). The running binary
// embeds the canonical templates, so it can read those artifacts back and tell
// whether they still match — no network, no manifest. Drives `--check`, the
// post-command notice (`stale_skills_notice`), and `aristo status`.

/// Per-(agent, scope) staleness summary.
enum AuditState {
    /// No Aristo skills installed for this agent at this scope.
    NotInstalled,
    /// Installed and matching the running binary.
    UpToDate,
    /// At least one installed skill differs from the running binary.
    Stale { installed_version: Option<String> },
}

/// A render-ready audit line for one installed (agent, scope). `NotInstalled`
/// agents are omitted from [`audit_rows`], so every row is something the user
/// actually has on disk.
pub(crate) struct AuditRow {
    pub(crate) agent_pretty: &'static str,
    pub(crate) scope: &'static str,
    pub(crate) summary: String,
    pub(crate) stale: bool,
}

/// Classify one agent's installed skills under `root`.
fn audit_agent(agent: Agent, root: &Path) -> AuditState {
    let states: Vec<SkillState> = match agent {
        Agent::ClaudeCode | Agent::Cursor | Agent::Antigravity => skills::bundled()
            .iter()
            .map(|s| file_copy_state(&file_copy_target(agent, root, s), s))
            .collect(),
        Agent::Codex | Agent::OpenCode => {
            let refs: Vec<&Skill> = skills::bundled().iter().collect();
            vec![agents_md_state(&root.join("AGENTS.md"), &refs)]
        }
    };

    let mut present = false;
    let mut stale_version: Option<Option<String>> = None;
    for state in states {
        match state {
            SkillState::Missing => {}
            SkillState::UpToDate => present = true,
            SkillState::Stale { installed_version } => {
                present = true;
                stale_version.get_or_insert(installed_version);
            }
        }
    }

    match (present, stale_version) {
        (false, _) => AuditState::NotInstalled,
        (true, Some(v)) => AuditState::Stale {
            installed_version: v,
        },
        (true, None) => AuditState::UpToDate,
    }
}

/// Build render-ready rows for the given scopes. A `None` scope is skipped;
/// only installed agents produce a row.
pub(crate) fn audit_rows(project_root: Option<&Path>, user_root: Option<&Path>) -> Vec<AuditRow> {
    let current = env!("CARGO_PKG_VERSION");
    let mut rows = Vec::new();
    for (scope, maybe_root) in [("project", project_root), ("user", user_root)] {
        let Some(root) = maybe_root else { continue };
        for agent in ALL_AGENTS {
            match audit_agent(agent, root) {
                AuditState::NotInstalled => {}
                AuditState::UpToDate => rows.push(AuditRow {
                    agent_pretty: agent.pretty(),
                    scope,
                    summary: format!("v{current}"),
                    stale: false,
                }),
                AuditState::Stale { installed_version } => {
                    let from = installed_version.unwrap_or_else(|| "?".to_string());
                    rows.push(AuditRow {
                        agent_pretty: agent.pretty(),
                        scope,
                        summary: format!("v{from} (binary v{current})"),
                        stale: true,
                    });
                }
            }
        }
    }
    rows
}

/// Audit the conventional scopes (project = cwd, user = home), best-effort.
fn standard_audit() -> Vec<AuditRow> {
    let cwd = std::env::current_dir().ok();
    let home = dirs::home_dir();
    audit_rows(cwd.as_deref(), home.as_deref())
}

/// One-line nudge for the post-command update notice. `None` when nothing
/// installed is stale. Best-effort — any resolution failure just yields `None`.
pub(crate) fn stale_skills_notice() -> Option<String> {
    standard_audit().iter().any(|r| r.stale).then(|| {
        "\nSome installed aristo skills were generated by an older aristo.\n\
         Refresh them with: aristo install-skills --update   \
         (run `aristo install-skills --check` for details)."
            .to_string()
    })
}

/// SessionStart hook entry point (`aristo install-skills --hook-format`).
/// Emits a SessionStart `additionalContext` JSON block telling the agent to
/// offer a skill refresh when installed skills are stale; prints nothing
/// when current. Always succeeds — a SessionStart hook must never error.
fn run_session_start_hook() -> CliResult<()> {
    if let Some(context) = stale_skills_hook_context() {
        let json = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": context,
            }
        });
        println!("{json}");
    }
    Ok(())
}

/// The agent-facing `additionalContext` message when installed skills are
/// stale, or `None` when everything is current. Best-effort.
fn stale_skills_hook_context() -> Option<String> {
    let stale: Vec<AuditRow> = standard_audit().into_iter().filter(|r| r.stale).collect();
    (!stale.is_empty()).then(|| build_hook_context(&stale))
}

/// Build the SessionStart `additionalContext` body (pure, testable). Reports
/// which installed skills are stale and — per the Layer 1 "notify, agent
/// acts" design — instructs the agent to surface it to the user and offer to
/// run the refresh, rather than rewriting anything itself.
fn build_hook_context(stale: &[AuditRow]) -> String {
    let current = env!("CARGO_PKG_VERSION");
    let mut list = String::new();
    for r in stale {
        list.push_str(&format!(
            "\n  - {} ({}): {}",
            r.agent_pretty, r.scope, r.summary
        ));
    }
    format!(
        "The Aristo skills installed in this workspace were generated by an older aristo than the \
         installed CLI (v{current}), so they may not match its current commands and flags:{list}\n\n\
         Tell the user their Aristo skills are out of date and offer to refresh them. If they agree, \
         run `aristo install-skills --update` (it re-pins the installed skills to v{current}); the \
         refreshed skills take effect on the next session or after `/reload-skills`. Nothing has been \
         changed yet — this is a session-start heads-up."
    )
}

/// `aristo install-skills --check`: report installed-skill staleness. Prints
/// its own report and exits non-zero (silently) when anything is stale, so it
/// works as a gate.
fn run_check(agent: Option<String>, user: bool) -> CliResult<()> {
    // `--user` narrows to user scope; otherwise report both. `--agent` filters
    // to a single agent across the chosen scopes.
    let project = if user {
        None
    } else {
        std::env::current_dir().ok()
    };
    let user_root = dirs::home_dir();
    let mut rows = audit_rows(project.as_deref(), user_root.as_deref());

    if let Some(name) = &agent {
        let want = Agent::parse(name)?;
        rows.retain(|r| r.agent_pretty == want.pretty());
    }

    println!();
    println!(
        "Aristo skills — installed vs binary v{}:",
        env!("CARGO_PKG_VERSION")
    );
    if rows.is_empty() {
        let where_ = if user {
            "user scope"
        } else {
            "this project or user scope"
        };
        println!("  (none installed in {where_})");
        println!();
        println!("Install with: aristo install-skills --agent <name>");
        return Ok(());
    }
    for r in &rows {
        let mark = if r.stale { "stale" } else { "ok" };
        println!(
            "  [{mark:>5}] {:<13} {:<8} {}",
            r.agent_pretty, r.scope, r.summary
        );
    }

    let stale = rows.iter().filter(|r| r.stale).count();
    println!();
    if stale == 0 {
        println!("ok: all installed skills are current.");
        Ok(())
    } else {
        let plural = if stale == 1 { "" } else { "s" };
        println!("{stale} installed skill set{plural} out of date. Re-pin with:");
        println!("  aristo install-skills --update");
        Err(CliError::Silent { exit_code: 1 })
    }
}

/// `aristo install-skills --update --agent X`: re-pin one agent's skills to
/// this binary's version (re-installs, overwriting any stale content). Framed
/// as a re-pin, not a fresh install — no "commit .claude/" install tip.
fn repin_agent(agent: &str, user: bool) -> CliResult<()> {
    let agent = Agent::parse(agent)?;
    let root = scope_root(user)?;
    println!();
    println!(
        "→ Re-pinning Aristo skills for {} ({}) …",
        agent.pretty(),
        scope_word(user)
    );
    match agent {
        Agent::ClaudeCode | Agent::Cursor | Agent::Antigravity => {
            install_file_copy_agent(agent, &root)?
        }
        Agent::Codex | Agent::OpenCode => install_agents_md_agent(agent, &root)?,
    }
    println!();
    println!(
        "ok: re-pinned {} skills to {}.",
        agent.slug(),
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

fn update_all_installed(user_only: bool) -> CliResult<()> {
    let mut scopes: Vec<(bool, PathBuf)> = Vec::new();
    if user_only {
        scopes.push((true, scope_root(true)?));
    } else {
        if let Ok(p) = scope_root(false) {
            scopes.push((false, p));
        }
        if let Ok(h) = scope_root(true) {
            scopes.push((true, h));
        }
    }

    let healed = update_scopes(&scopes)?;

    println!();
    if healed > 0 {
        println!(
            "ok: re-pinned installed Aristo skills to {}.",
            env!("CARGO_PKG_VERSION")
        );
    } else {
        println!("ok: no Aristo skills installed — nothing to update.");
        println!("Install with: aristo install-skills --agent <name>");
    }
    Ok(())
}

/// Re-pin every installed agent across `scopes`, returning the count of
/// (agent, scope) pairs healed. Split from [`update_all_installed`] so the
/// heal-only-installed invariant is testable without touching the real
/// `$HOME` (the workspace forbids `unsafe`, hence no `set_var`).
#[aristo::intent(
    "Re-pins ONLY agents that already have skills installed under a scope — \
     it never creates a fresh install for an absent agent. It heals existing \
     state, it does not broadly install: a user who only set up Claude Code \
     must not silently gain Cursor / Codex files from running an update.",
    verify = "test",
    id = "update_all_heals_only_installed_agents"
)]
fn update_scopes(scopes: &[(bool, PathBuf)]) -> CliResult<usize> {
    let mut healed = 0usize;
    for (user, root) in scopes {
        for agent in ALL_AGENTS {
            if matches!(audit_agent(agent, root), AuditState::NotInstalled) {
                continue;
            }
            healed += 1;
            println!();
            println!("→ Re-pinning {} ({}) …", agent.pretty(), scope_word(*user));
            match agent {
                Agent::ClaudeCode | Agent::Cursor | Agent::Antigravity => {
                    install_file_copy_agent(agent, root)?
                }
                Agent::Codex | Agent::OpenCode => install_agents_md_agent(agent, root)?,
            }
        }
    }
    Ok(healed)
}

fn scope_word(user: bool) -> &'static str {
    if user {
        "user-level"
    } else {
        "project-level"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_parse_accepts_all_phase1_names() {
        assert_eq!(Agent::parse("claude-code").unwrap(), Agent::ClaudeCode);
        assert_eq!(Agent::parse("cursor").unwrap(), Agent::Cursor);
        assert_eq!(Agent::parse("codex").unwrap(), Agent::Codex);
        assert_eq!(Agent::parse("opencode").unwrap(), Agent::OpenCode);
        assert_eq!(Agent::parse("antigravity").unwrap(), Agent::Antigravity);
    }

    #[test]
    fn agent_parse_rejects_unknown_with_helpful_message() {
        let err = Agent::parse("emacs").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("emacs"));
        assert!(msg.contains("--list-agents"));
    }

    // ---- staleness audit ----

    use crate::skills::install::file_copy_install;

    /// Install all bundled Claude Code skills under `root`.
    fn install_claude(root: &std::path::Path) {
        for s in skills::bundled() {
            file_copy_install(&file_copy_target(Agent::ClaudeCode, root, s), s).unwrap();
        }
    }

    #[test]
    fn audit_agent_reports_not_installed_then_uptodate_then_stale() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();

        assert!(matches!(
            audit_agent(Agent::ClaudeCode, root),
            AuditState::NotInstalled
        ));

        install_claude(root);
        assert!(matches!(
            audit_agent(Agent::ClaudeCode, root),
            AuditState::UpToDate
        ));

        // Make one of the three skills look like an older install.
        let first = &skills::bundled()[0];
        std::fs::write(
            file_copy_target(Agent::ClaudeCode, root, first),
            "---\nsdk_version: 0.0.1\n---\nold\n",
        )
        .unwrap();
        match audit_agent(Agent::ClaudeCode, root) {
            AuditState::Stale { installed_version } => {
                assert_eq!(installed_version.as_deref(), Some("0.0.1"));
            }
            _ => panic!("expected Stale"),
        }
    }

    #[test]
    fn audit_rows_omits_uninstalled_agents() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        install_claude(root);

        let rows = audit_rows(Some(root), None);
        // Only Claude Code is installed → exactly one row; others omitted.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].agent_pretty, "Claude Code");
        assert_eq!(rows[0].scope, "project");
        assert!(!rows[0].stale);
    }

    #[test]
    fn update_all_heals_only_installed_agents() {
        // Backs the `update_all_heals_only_installed_agents` intent.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        install_claude(&root);

        // Tamper one skill so the heal has something to do.
        let first = &skills::bundled()[0];
        std::fs::write(file_copy_target(Agent::ClaudeCode, &root, first), "stale").unwrap();

        let healed = update_scopes(&[(false, root.clone())]).unwrap();

        // Exactly one agent (Claude Code) was installed, so exactly one healed.
        assert_eq!(healed, 1);
        assert!(matches!(
            audit_agent(Agent::ClaudeCode, &root),
            AuditState::UpToDate
        ));
        // No fresh installs were created for absent agents.
        assert!(matches!(
            audit_agent(Agent::Cursor, &root),
            AuditState::NotInstalled
        ));
        assert!(
            !root.join(".cursor").exists(),
            "cursor dir must not be created by an update"
        );
        assert!(
            !root.join("AGENTS.md").exists(),
            "codex/opencode block must not be created by an update"
        );
    }

    #[test]
    fn update_scopes_no_installs_heals_nothing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let healed = update_scopes(&[(false, tmp.path().to_path_buf())]).unwrap();
        assert_eq!(healed, 0);
    }

    #[test]
    fn hook_context_instructs_agent_to_offer_refresh() {
        let rows = vec![AuditRow {
            agent_pretty: "Claude Code",
            scope: "project",
            summary: "v0.2.0 (binary v0.2.1)".to_string(),
            stale: true,
        }];
        let ctx = build_hook_context(&rows);
        assert!(ctx.contains("Claude Code"), "must name the stale agent");
        assert!(
            ctx.contains("aristo install-skills --update"),
            "must tell the agent the exact refresh command"
        );
        assert!(
            ctx.contains("offer to refresh"),
            "must instruct the agent to offer, not silently act"
        );
        assert!(
            ctx.contains("Nothing has been changed"),
            "must make clear it's notify-only"
        );
    }
}
