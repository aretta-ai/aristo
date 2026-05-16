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

/// One bundled skill. `content` is a TEMPLATE — call
/// [`Skill::resolved_content`] before writing to disk so placeholders
/// like `{{SDK_VERSION}}` get substituted.
pub(crate) struct Skill {
    /// Slug used in install paths, e.g. `aristo-authoring` →
    /// `.claude/skills/aristo-authoring/SKILL.md`. Must be a stable
    /// identifier; renames break installations.
    pub(crate) name: &'static str,
    /// The raw markdown template — frontmatter + body. May contain
    /// placeholders (currently only `{{SDK_VERSION}}`). Direct callers
    /// should use [`Skill::resolved_content`] unless they specifically
    /// need the unresolved form (e.g. drift checks against
    /// `aristo-authoring.md` on disk).
    pub(crate) content: &'static str,
}

impl Skill {
    /// Render the skill's template into the form that ships to disk.
    /// Currently substitutes one placeholder (`{{SDK_VERSION}}`) with
    /// the binary's compile-time version.
    #[aristo::intent(
        "Install paths MUST go through resolved_content, never .content \
         directly. The template-vs-resolved split exists because skill \
         text needs values (SDK version, future bundle hash) that are \
         only available at build time of THIS binary, not at template \
         authoring time. Writing .content to disk would ship a literal \
         `{{SDK_VERSION}}` to user-installed SKILL.md files; the install \
         outcome would look successful but the version pin would be \
         garbage.",
        verify = "neural",
        id = "skill_install_must_use_resolved_content"
    )]
    pub(crate) fn resolved_content(&self) -> String {
        self.content
            .replace("{{SDK_VERSION}}", env!("CARGO_PKG_VERSION"))
    }
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

/// The authoring skill body shipped to disk on install. Concatenates
/// the agent-facing skill manifest with the canonical PHILOSOPHY.md
/// (durable principles + case links) so updates to PHILOSOPHY.md
/// auto-propagate into the bundled skill — no manual sync step.
///
/// PHILOSOPHY.md lives at `.aristo/feedback/aristo-authoring/`; it's
/// human-curated and tracked in git (`.gitignore` whitelists the
/// `feedback/` subtree). The relative path here climbs four parents
/// from this file (`crates/aristo-cli/src/skills/` → repo root) before
/// descending; if the layout changes the build breaks loudly rather
/// than silently shipping a skill without principles.
const AUTHORING_BODY: &str = concat!(
    include_str!("aristo-authoring.md"),
    "\n\n---\n\n## Canonical principles (verbatim from PHILOSOPHY.md)\n\n\
     The section below is `include_str!`'d at build time from \
     `.aristo/feedback/aristo-authoring/PHILOSOPHY.md` so the bundled \
     skill cannot drift from the project's distilled principles. Edit \
     the source file, not this section.\n\n",
    include_str!("../../../../.aristo/feedback/aristo-authoring/PHILOSOPHY.md"),
);

const AUTHORING: Skill = Skill {
    name: "aristo-authoring",
    content: AUTHORING_BODY,
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
        assert!(find("aristo-mine-assertions").is_none()); // slice 24
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
    fn authoring_skill_embeds_philosophy_principles_verbatim() {
        let s = find("aristo-authoring").unwrap();
        // Every distilled-principle heading from PHILOSOPHY.md must
        // appear in the bundled skill body. The build-time `concat!` +
        // `include_str!` guarantees this; the test makes the contract
        // explicit so a future refactor that breaks the include path
        // fails here (instead of silently shipping a skill without the
        // canonical principles).
        for principle in [
            "P-SPEC-STYLE",
            "P-CHECK-TYPE-SYSTEM-FIRST",
            "P-NO-DOUBLE-INTENT",
            "P-INVARIANT-AT-LOAD-BEARING-SITE",
            "P-INVARIANT-NOT-IMPL",
            "P-WHY-AS-INVARIANT",
            "P-NAME-THE-REFACTOR-TRAP",
            "P-AGENT-PROOFING",
            "P-VERIFY-MATCHES-SHAPE",
        ] {
            assert!(
                s.content.contains(principle),
                "bundled skill is missing `{principle}` — did the \
                 PHILOSOPHY.md include path break?"
            );
        }
        // The "include marker" line proves the wiring (vs. someone
        // having pasted PHILOSOPHY's body into the .md by hand).
        assert!(
            s.content.contains("`include_str!`'d at build time from"),
            "missing include-marker phrase — skill may not be \
             auto-wiring from PHILOSOPHY.md"
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
