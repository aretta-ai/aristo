//! Bundled-skill registry.
//!
//! Each skill is a markdown manifest embedded in the binary via
//! `include_str!`. Slice 12 ships only the authoring skill; the mining,
//! neural-verify, and review skills get added in their consuming slices
//! (24, 23, 27) — adding a new skill is a one-line edit to `BUNDLED`.
//!
//! Per K4 (mockup 12), each agent gets the skill installed via that
//! agent's standard mechanism — file copy for Claude Code / Cursor /
//! Antigravity (different paths and extensions per agent), AGENTS.md
//! section-injection for Codex / OpenCode. The two install backends live
//! in [`install`] and accept a `&Skill` so the per-agent dispatch in
//! slice 13 stays a thin shell.

pub(crate) mod install;

/// One bundled skill. Content is the raw markdown body (the entire SKILL.md
/// file, frontmatter + body — agents are expected to handle frontmatter).
pub(crate) struct Skill {
    /// Slug used in install paths, e.g. `aristo-authoring` →
    /// `.claude/skills/aristo-authoring/SKILL.md`. Must be a stable
    /// identifier; renames break installations.
    pub(crate) name: &'static str,
    /// The full markdown content shipped to disk verbatim.
    pub(crate) content: &'static str,
}

#[aristo::intent(
    "Skill names in this set are part of the public install surface. \
     Renaming or removing one is a breaking change — users on the old \
     name have it on disk under that path; agents match by exact name.",
    verify = "neural",
    id = "bundled_skills_is_stable_set"
)]
pub(crate) fn bundled() -> &'static [Skill] {
    BUNDLED
}

const AUTHORING: Skill = Skill {
    name: "aristo-authoring",
    content: include_str!("aristo-authoring.md"),
};

const BUNDLED: &[Skill] = &[AUTHORING];

#[cfg(test)]
mod tests {
    use super::*;

    fn find(name: &str) -> Option<&'static Skill> {
        bundled().iter().find(|s| s.name == name)
    }

    #[test]
    fn authoring_skill_is_bundled() {
        assert!(find("aristo-authoring").is_some());
    }

    #[test]
    fn future_skill_names_not_yet_bundled() {
        // Sentinels: these skills land in their consuming slices.
        assert!(find("aristo-mining").is_none()); // slice 24
        assert!(find("aristo-neural-verify").is_none()); // slice 23
        assert!(find("aristo-review-skill").is_none()); // slice 27
    }

    #[test]
    fn bundled_skill_names_are_unique() {
        let mut names: Vec<_> = bundled().iter().map(|s| s.name).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            len_before,
            "duplicate skill name in BUNDLED — would clobber on install"
        );
    }

    #[test]
    fn authoring_skill_references_intent_stmt_not_intent_bang() {
        // Regression guard mirroring the `aristo lang` cheat-sheet test.
        // The skill teaches agents the macro names; they MUST match what
        // aristo-macros actually exports. Slice 6 ships intent_stmt!.
        let s = find("aristo-authoring").unwrap();
        assert!(
            s.content.contains("intent_stmt!"),
            "authoring skill must teach intent_stmt! (the actual macro name)"
        );
        assert!(
            !s.content.contains("aristo::intent!("),
            "authoring skill must NOT teach intent!() — that name doesn't exist (E0428)"
        );
    }

    #[test]
    fn authoring_skill_references_aristos_namespace_warning() {
        let s = find("aristo-authoring").unwrap();
        assert!(
            s.content.contains("aristos:"),
            "skill must warn agents not to write the aristos: prefix"
        );
        assert!(
            s.content.contains("aret_"),
            "skill must warn agents not to write the aret_ prefix"
        );
    }

    #[test]
    fn authoring_skill_has_yaml_frontmatter() {
        let s = find("aristo-authoring").unwrap();
        let mut lines = s.content.lines();
        assert_eq!(
            lines.next(),
            Some("---"),
            "skill must start with frontmatter"
        );
        assert!(
            s.content.contains("name: aristo-authoring"),
            "frontmatter must include the skill name"
        );
    }
}
