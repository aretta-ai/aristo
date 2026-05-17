//! `aristo install-skills` and `aristo uninstall-skills`.
//!
//! Per K4, each agent has its own install path layout. The two install
//! backends (`file_copy_install`, `agents_md_install`) live in
//! `crate::skills::install`; this module is the per-agent dispatch + the
//! user-facing output.

use std::path::PathBuf;

use crate::skills::install::{
    agents_md_install, agents_md_uninstall, file_copy_install, file_copy_uninstall, InstallOutcome,
};
use crate::skills::{self, Skill};
use crate::{CliError, CliResult};

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
) -> CliResult<()> {
    let _ = update; // --update changes idempotence semantics in slice 19+; for
                    // slice 13 every install is effectively idempotent.
    if list_agents {
        emit_agent_list();
        return Ok(());
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
}
