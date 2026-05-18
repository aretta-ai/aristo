//! Bundled-skill registry.
//!
//! Each skill is a markdown manifest embedded in the binary via
//! `include_str!`. Slice 12 ships only the authoring skill; the mining,
//! neural-verify, and critique skills get added in their consuming slices
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
        "Install paths MUST call resolved_content, never read .content \
         directly. The template needs the build-time SDK version; \
         writing .content to disk would ship a literal `{{SDK_VERSION}}` \
         placeholder to user-installed SKILL.md files. The install \
         outcome would look successful but the version pin would be \
         garbage — silent staleness on every release.",
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

const NEURAL_VERIFY: Skill = Skill {
    name: "aristo-neural-verify",
    content: include_str!("aristo-neural-verify.md"),
};

const BUNDLED: &[Skill] = &[AUTHORING, NEURAL_VERIFY];

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
        assert!(find("aristo-critique").is_none()); // slice 27
    }

    #[test]
    fn neural_verify_skill_is_bundled() {
        let s = find("aristo-neural-verify").expect("aristo-neural-verify must be bundled");
        // The skill body must teach the validator's hard contract; spot-check
        // a couple of load-bearing lines.
        let body = s.resolved_content();
        assert!(
            body.contains("`.aristo/pending-neural.toml`"),
            "skill body must reference the pending-request file the SDK writes"
        );
        assert!(
            body.contains("aristo verify --apply-verdicts"),
            "skill body must teach the agent to call the SDK validator after producing proofs"
        );
        assert!(
            body.contains("aristo verify --submit-verdict"),
            "skill body must teach the subagent to submit verdicts via the SDK CLI \
             (single write path; no direct file writes from agents)"
        );
        assert!(
            body.contains("`Bash` and `Read` tools allowed"),
            "skill body must restrict subagent tools to Bash + Read (no Write)"
        );
        assert!(
            body.contains("accepted: sha256:"),
            "skill body must teach the subagent how to parse the SDK's accept stdout"
        );
        assert!(
            body.contains("compares with the reported hash"),
            "skill body must teach the orchestrator's hash-comparison integrity check"
        );
        assert!(
            body.contains("do NOT inline it as a discovered ground"),
            "skill body must encode the strict-on-discovered-assumptions rule"
        );
        assert!(
            body.contains("DO NOT write hash fields"),
            "skill body must instruct the agent to omit hash fields (SDK stamps them)"
        );
        assert!(
            body.contains("Cited id discipline"),
            "skill body must encode the cited-id-discipline rule (open #2 fix)"
        );
        assert!(
            body.contains("`supports` is NOT a valid variant"),
            "skill body must explicitly disallow the `supports` enum variant \
             (broke 2/4 of the first submit-flow dogfood run)"
        );
        assert!(
            body.contains("CANNOT cite"),
            "skill body must explicitly disallow child-as-prior-step \
             (broke verify_bool_true on first dogfood run)"
        );
        assert!(
            body.contains("\"1-200\""),
            "skill body must call out the over-broad line-range failure mode \
             (broke pending_neural on first dogfood run)"
        );
        assert!(
            body.contains("prior_attempts + 1") || body.contains("prior_attempts}} + 1"),
            "skill body must teach the agent to compute attempts as prior_attempts + 1 \
             (GAP-9: repair budget accumulates across re-spawns)"
        );
        assert!(
            body.contains("AskUserQuestion") && body.contains("interactive review"),
            "skill body must teach the post-apply interactive review flow via AskUserQuestion"
        );
        assert!(
            body.contains("Every suggested annotation gets surfaced as an actionable question"),
            "skill body must encode the GSD-style interactive-suggestions rule"
        );
        assert!(
            body.contains("confirmed via a second `AskUserQuestion`"),
            "skill body must require two-step confirmation before any source edit \
             (no silent source mutation from skill orchestration)"
        );
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
