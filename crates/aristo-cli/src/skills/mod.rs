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
/// the agent-facing skill manifest with the canonical principles from
/// `aristo-authoring-philosophy.md` (durable principles + case links)
/// so the bundled skill cannot drift from them — no manual sync step.
///
/// The principles file lives in-crate, next to this module
/// (`src/skills/aristo-authoring-philosophy.md`), so it ships in the
/// published crate tarball. (It was previously read from the repo's
/// `.aristo/feedback/` dogfood tree, which broke `cargo publish`:
/// `include_str!` can't reach outside the crate root.)
const AUTHORING_BODY: &str = concat!(
    include_str!("aristo-authoring.md"),
    "\n\n---\n\n## Canonical principles (verbatim from PHILOSOPHY.md)\n\n\
     The section below is `include_str!`'d at build time from \
     `aristo-authoring-philosophy.md` so the bundled skill cannot \
     drift from the project's distilled principles. Edit the source \
     file, not this section.\n\n",
    include_str!("aristo-authoring-philosophy.md"),
);

const AUTHORING: Skill = Skill {
    name: "aristo-authoring",
    content: AUTHORING_BODY,
};

const VERIFY: Skill = Skill {
    name: "aristo-verify",
    content: include_str!("aristo-verify.md"),
};

const CRITIQUE: Skill = Skill {
    name: "aristo-critique",
    content: include_str!("aristo-critique.md"),
};

const INTENT_SUGGESTIONS: Skill = Skill {
    name: "aristo-intent-suggestions",
    content: include_str!("aristo-intent-suggestions.md"),
};

const AUTHORED_REVIEW: Skill = Skill {
    name: "aristo-authored-review",
    content: include_str!("aristo-authored-review.md"),
};

const STATUS: Skill = Skill {
    name: "aristo-status",
    content: include_str!("aristo-status.md"),
};

const HELP: Skill = Skill {
    name: "aristo-help",
    content: include_str!("aristo-help.md"),
};

const INSTRUMENTING: Skill = Skill {
    name: "aristo-instrumenting",
    content: include_str!("aristo-instrumenting.md"),
};

const BUNDLED: &[Skill] = &[
    AUTHORING,
    VERIFY,
    CRITIQUE,
    INTENT_SUGGESTIONS,
    AUTHORED_REVIEW,
    STATUS,
    HELP,
    INSTRUMENTING,
];

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
        assert!(find("aristo-mine-assertions").is_none()); // slice 24 (deferred)
    }

    #[test]
    fn intent_suggestions_skill_is_bundled() {
        let s = find("aristo-intent-suggestions")
            .expect("aristo-intent-suggestions must be bundled (§17 Slice 4)");
        let body = s.resolved_content();
        // §17/D3/D9: the loop is owned by the intent-review session kind.
        assert!(
            body.contains("intent-review"),
            "skill body must name the `intent-review` session kind it drives"
        );
        // §6A: the decision card is reused, not reinvented.
        assert!(
            body.contains("aristo canon show"),
            "skill body must use `aristo canon show` as the decision card (reuse, not reinvent)"
        );
        // §6A: same-kind items are presented as a navigable batch.
        assert!(
            body.contains("batch"),
            "skill body must teach the batch multi-Q decision pattern"
        );
        // §6A: the picker caps at 4 questions/page → larger sets paginate.
        assert!(
            body.contains("4 questions per page") || body.contains("4/page"),
            "skill body must call out the ≤4-questions-per-page cap (paginate larger sets)"
        );
        // §6A: two-phase batch (decide → agent finds sites → confirm).
        assert!(
            body.contains("PHASE 1") && body.contains("PHASE 2") && body.contains("PHASE 3"),
            "skill body must teach the two-phase batch (decide → find sites → confirm placements)"
        );
        // §6A snippet rule: Stage A rewrite = −/+ diff; Stage B adopt = pure +.
        assert!(
            body.contains("REWRITE") && body.contains("pure"),
            "skill body must encode the −/+ rewrite vs pure-+ adopt snippet rule"
        );
        // §6B modes: backlog + status must both be taught.
        assert!(
            body.contains("backlog"),
            "skill body must teach the `backlog` mode (skip the match, no canon refresh)"
        );
        assert!(
            body.contains("status"),
            "skill body must teach the `status` mode (--counts, no session, no writes)"
        );
        // §6B: matches / suggestions stage selectors.
        assert!(
            body.contains("Stage A only") && body.contains("Stage B only"),
            "skill body must teach the matches-only / suggestions-only stage selectors"
        );
        // §6B: subject scope via the reused filter grammar.
        assert!(
            body.contains("--filter parent=") && body.contains("--filter file="),
            "skill body must teach the `cluster <objective>` / `for <file>` scope filters"
        );
        // §6B: zero-arg slash command (no /skill --flag).
        assert!(
            body.contains("zero-argument slash command"),
            "skill body must declare it is a zero-arg slash command (NL/menu routing, no flags)"
        );
        // D4/D6: write path + parent-reject cascade discipline.
        assert!(
            body.contains("aristo canon refresh") && body.contains("aristo canon accept"),
            "skill body must funnel adoption through canon refresh → canon accept (no new source mutator)"
        );
        assert!(
            body.contains("parent-first") || body.contains("once per cluster"),
            "skill body must encode the parent-first, once-per-cluster decision (D6)"
        );
        assert!(
            body.contains("DRAGGED-IN") && body.contains("KEEP"),
            "skill body must encode the D6 discard-dragged-in-only / keep-independent rule"
        );
        // Layer-3 session-guard discipline (mirrors critique/neural-verify).
        assert!(
            body.contains("aristo session active"),
            "skill body's step 0 must check for an active review session (Layer 3 enforcement)"
        );
        assert!(
            body.contains("aristo session start intent-review"),
            "skill body must open an intent-review session before interactive review"
        );
        assert!(
            body.contains("aristo session decide --item"),
            "skill body must record per-item decisions via the substrate"
        );
        assert!(
            body.contains("aristo session exit --defer-undecided"),
            "skill body must offer defer-undecided so open items go to backlog, not silently dropped"
        );
        // Frontmatter sanity (mirrors the authoring-skill check).
        assert!(
            body.starts_with("---"),
            "skill must start with YAML frontmatter"
        );
        assert!(
            body.contains("name: aristo-intent-suggestions"),
            "frontmatter must include the skill name"
        );
    }

    #[test]
    fn authored_review_skill_is_bundled() {
        let s = find("aristo-authored-review")
            .expect("aristo-authored-review must be bundled (Phase 18 #7)");
        let body = s.resolved_content();
        // Frontmatter sanity.
        assert!(
            body.starts_with("---"),
            "skill must start with YAML frontmatter"
        );
        assert!(
            body.contains("name: aristo-authored-review"),
            "frontmatter must include the skill name"
        );
        // The two CLI surfaces this skill drives.
        assert!(
            body.contains("aristo review --json"),
            "skill body must teach listing the backlog via `aristo review --json`"
        );
        assert!(
            body.contains("aristo review --mark"),
            "skill body must teach marking reviewed via `aristo review --mark`"
        );
        // D9: the two review paths.
        assert!(
            body.contains("Critique-first") && body.contains("Review now"),
            "skill body must offer both the Critique-first and Review-now paths (D9)"
        );
        // Critique-first reuses the existing critique command + skill.
        assert!(
            body.contains("aristo critique --filter id="),
            "Critique-first must kick `aristo critique --filter id=` over the new intents"
        );
        // D11 guard ordering: kick critique BEFORE opening a session.
        assert!(
            body.contains("BEFORE") && body.contains("Guard ordering"),
            "skill body must encode the D11 guard ordering (kick critique before a session)"
        );
        // Layer-3 session-guard discipline (mirrors the other review skills).
        assert!(
            body.contains("aristo session active"),
            "skill body's step 0 must check for an active review session (Layer 3)"
        );
        // Menu-driven, paginated.
        assert!(
            body.contains("AskUserQuestion"),
            "skill body must drive the review via AskUserQuestion menus"
        );
        assert!(
            body.contains("4 questions per page") || body.contains("4/page"),
            "skill body must call out the ≤4-questions-per-page cap (paginate larger sets)"
        );
        // Subject-only rule (never reference the model).
        assert!(
            body.contains("Subject-only") || body.contains("subject-only"),
            "skill body must encode the subject-only rule"
        );
        // Source edits funnel through stamp (no direct .aristo/ state writes).
        assert!(
            body.contains("aristo stamp"),
            "skill body must funnel source edits through `aristo stamp`"
        );
    }

    #[test]
    fn critique_skill_is_bundled() {
        let s = find("aristo-critique").expect("aristo-critique must be bundled");
        let body = s.resolved_content();
        assert!(
            body.contains("aristo critique --submit-findings"),
            "skill body must teach the SDK CLI as the single write path"
        );
        assert!(
            body.contains("aristo critique --pop-next"),
            "skill body must teach the worker-loop pop pattern"
        );
        assert!(
            body.contains("model=\"sonnet\""),
            "skill body must specify the Sonnet model for critique workers \
             (Opus is overkill for shallow prose work)"
        );
        assert!(
            body.contains("Bash tools only"),
            "skill body must restrict critique workers to Bash only \
             (task body is self-contained; no Read needed)"
        );
        assert!(
            body.contains("rephrasing")
                && body.contains("parent-shape")
                && body.contains("vocabulary")
                && body.contains("scope")
                && body.contains("clarity"),
            "skill body must enumerate all five v0 finding categories"
        );
        assert!(
            body.contains("strong-suggest") && body.contains("severity"),
            "skill body must enumerate the severity scale"
        );
        assert!(
            body.contains("self-contained"),
            "skill body must teach that the task body is self-contained \
             (no source reads needed)"
        );
        assert!(
            body.contains("aristo session active"),
            "skill body's step 0 must check for an active review session \
             (slice 27.5 Layer 3 enforcement)"
        );
        assert!(
            body.contains("aristo session start critique-review"),
            "skill body's step 5 must open a session before interactive review"
        );
        assert!(
            body.contains("aristo session decide --item"),
            "skill body's step 5 must record per-finding decisions via the substrate"
        );
        assert!(
            body.contains("aristo session exit --defer-undecided"),
            "skill body must offer defer-undecided as the early-stop path \
             so open items go to backlog, not silently dropped"
        );
    }

    #[test]
    fn verify_skill_is_bundled() {
        // #1/#2: aristo-neural-verify was generalized + renamed to aristo-verify
        // (scope×mode menu + full server arm + card-driven results review). The
        // neural worker contract below is PRESERVED verbatim — that's the
        // load-bearing part the SDK validator depends on.
        let s = find("aristo-verify").expect("aristo-verify must be bundled (Phase 18 #1/#2)");
        let body = s.resolved_content();
        assert!(
            body.contains("name: aristo-verify"),
            "frontmatter must carry the renamed skill name"
        );
        assert!(
            !body.contains("name: aristo-neural-verify"),
            "the skill must be RENAMED, not aliased — no stale neural-verify name in frontmatter"
        );

        // ── #1: opening scope × mode menu (recipes over existing flags) ──
        assert!(
            body.contains("Changed · neural"),
            "skill must open with the scope×mode menu (Changed·neural is the default)"
        );
        assert!(
            body.contains("aristo verify --rerun"),
            "skill must map 'Everything' to the existing --rerun flag"
        );
        assert!(
            body.contains("aristo verify --view"),
            "skill must teach re-attaching to the background full (server) session via --view"
        );
        assert!(
            body.contains("aristo auth login"),
            "skill must gate full/both on sign-in and offer `aristo auth login`"
        );

        // ── #2: card-driven results review, SUBJECT-ONLY, render-then-menu ──
        assert!(
            body.contains("SUBJECT-ONLY"),
            "cards must be SUBJECT-ONLY (never reference the internal verification model)"
        );
        assert!(
            body.contains("render the card first"),
            "skill must encode RENDER-THEN-MENU (the user sees the card before the action menu)"
        );
        assert!(
            body.contains("✗ Refuted") && body.contains("✓ Verified"),
            "skill must define both the failure (DifferentialReport) and success cards"
        );
        assert!(
            body.contains("Successes only"),
            "results-review opening must include the Successes-only lane"
        );
        assert!(
            body.contains("aristo verify --accept") && body.contains("--because"),
            "the failure card's Waive action must use `aristo verify --accept … --because`"
        );

        // ── neural worker contract (PRESERVED verbatim from aristo-neural-verify) ──
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
            body.contains("`Bash` and `Read` tools only"),
            "skill body must restrict subagent tools to Bash + Read (no Write)"
        );
        assert!(
            body.contains("ONE-SHOT") || body.contains("one-shot"),
            "skill body must teach the one-shot-per-worker pattern \
             (context pollution risk if workers loop)"
        );
        assert!(
            body.contains("aristo verify --queue-status"),
            "skill body must teach the orchestrator to use --queue-status \
             as the non-destructive peek between worker dispatches"
        );
        assert!(
            body.contains("run_in_background"),
            "skill body must teach continuous dispatch via Agent(run_in_background=true) \
             — waves of N workers waste time waiting on the slowest in each batch"
        );
        assert!(
            body.contains("accepted: sha256:"),
            "skill body must teach the subagent how to parse the SDK's accept stdout"
        );
        assert!(
            body.contains("compare with the reported hash"),
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
            body.contains("AskUserQuestion"),
            "skill body must drive the review via AskUserQuestion menus"
        );
        assert!(
            body.contains("Every suggested annotation gets surfaced as an actionable question"),
            "skill body must encode the GSD-style interactive-suggestions rule"
        );
        assert!(
            body.contains("second `AskUserQuestion`"),
            "skill body must require two-step confirmation before any source edit \
             (no silent source mutation from skill orchestration)"
        );
        assert!(
            body.contains("aristo session active"),
            "skill body's step 0 must check for an active review session \
             (slice 27.5 Layer 3 enforcement)"
        );
        assert!(
            body.contains("aristo session start proof-review"),
            "skill body's step 7 must open a session before interactive review"
        );
        assert!(
            body.contains("aristo session decide --item"),
            "skill body's step 7 must record per-proof decisions via the substrate"
        );
        assert!(
            body.contains("aristo session exit --defer-undecided"),
            "skill body must offer defer-undecided as the early-stop path \
             so un-decided proofs go to backlog, not silently dropped"
        );
    }

    #[test]
    fn help_skill_is_bundled_and_covers_the_whole_suite() {
        let s = find("aristo-help").expect("aristo-help must be bundled (Phase 18 #11)");
        let body = s.resolved_content();
        assert!(body.starts_with("---"), "skill must start with frontmatter");
        assert!(
            body.contains("name: aristo-help"),
            "frontmatter must carry the skill name"
        );
        // Anti-drift: aristo-help must name EVERY bundled skill, so adding a
        // skill without documenting it here fails this test (forces the sync).
        for skill in bundled() {
            assert!(
                body.contains(skill.name),
                "aristo-help must mention `{}` — a bundled skill is undocumented in the suite overview",
                skill.name
            );
        }
        // It must teach scenario flows, not just a skill list.
        assert!(
            body.contains("scenario flows") || body.contains("Example scenario"),
            "aristo-help must give example end-to-end scenario flows"
        );
        assert!(
            body.contains("fix-or-waive") || body.contains("fix or waive"),
            "aristo-help must cover the failure → fix-or-waive flow"
        );
    }

    #[test]
    fn status_skill_is_bundled() {
        let s = find("aristo-status").expect("aristo-status must be bundled (Phase 18 #12)");
        let body = s.resolved_content();
        assert!(body.starts_with("---"), "skill must start with frontmatter");
        assert!(
            body.contains("name: aristo-status"),
            "frontmatter must carry the skill name"
        );
        // Reads the real reporting commands (one source of truth; no recompute).
        assert!(
            body.contains("aristo metrics --json"),
            "skill must read metrics via `aristo metrics --json`"
        );
        assert!(
            body.contains("aristo status"),
            "skill must read the full breakdown via `aristo status`"
        );
        assert!(
            body.contains("aristo review --json"),
            "skill must surface the review backlog via `aristo review --json`"
        );
        // Next-action recommendation routes through the #9 engine, not a parallel nudge.
        assert!(
            body.contains("aristo nudge"),
            "skill must take its recommended next action from `aristo nudge` (the #9 engine)"
        );
        // Read-only + subject-only discipline.
        assert!(
            body.contains("read-only") || body.contains("Read-only"),
            "skill must declare it is read-only (no mutation)"
        );
        assert!(
            body.contains("Subject-only") || body.contains("subject-only"),
            "skill must encode the subject-only rule"
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
    fn authoring_skill_teaches_diff_mode() {
        // #5: the deliberate retro pass over uncommitted changes — the one
        // sanctioned exception to "never batch", guarded by in-context + hard
        // gate + confirm-before-write.
        let s = find("aristo-authoring").unwrap();
        let body = s.content;
        assert!(
            body.contains("Diff mode"),
            "authoring skill must teach the diff (backfill) mode (#5)"
        );
        assert!(
            body.contains("uncommitted"),
            "diff mode must scope to uncommitted changes"
        );
        assert!(
            body.contains("git diff"),
            "diff mode must use git diff as the change source"
        );
        assert!(
            body.contains("IN-CONTEXT") || body.contains("in-context"),
            "diff mode must require running in-context (rationale still recoverable)"
        );
        assert!(
            body.contains("AskUserQuestion"),
            "diff mode must propose + confirm (no silent backfill edits)"
        );
        assert!(
            body.contains("body-drift") || body.contains("body-drifted"),
            "diff mode must reconcile body-drifted existing intents (stamp --check)"
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
