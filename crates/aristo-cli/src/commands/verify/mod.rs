//! `aristo verify` — orchestrate per-entry verification dispatch.
//!
//! Slice 22 ships the dispatcher skeleton plus the `verify = false`
//! no-op arm. The other arms (`"neural"` slice 23, `"test"` slice 24,
//! `"full"` slice 26) currently return [`CliError::NotImplemented`]
//! with their target slice pointer so the user sees a precise ETA
//! rather than a silent skip.
//!
//! Flags follow J2 (mockup 11): `--filter` reuses the unified J2
//! grammar from `aristo list` / `graph` / `review`; `--rerun` is the
//! orthogonal force-flag that re-processes entries already in a clean
//! verified state; `--check` is the CI no-write mode; `--strict` is
//! reserved for warn-severity outcomes (no warnings yet — slice 22 has
//! only skip / not-implemented arms).

use std::path::PathBuf;

use aristo_core::config::ConfigFile;
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Status, VerifyLevel, VerifyMethod};
use aristo_core::proof::ProofFile;

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::filter::Filter;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

pub(crate) mod apply;
pub(crate) mod pending;
pub(crate) mod validator;

pub(crate) fn run(
    filter_strings: &[String],
    rerun: bool,
    check: bool,
    strict: bool,
    apply_verdicts: bool,
    rewrite_hashes: bool,
) -> CliResult<()> {
    let _ = (check, strict); // wired for forward-compat; no behavior yet (see module doc)
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    if apply_verdicts {
        return apply::run_apply_verdicts(&ws, &index, rewrite_hashes);
    }

    let filters = parse_filters(filter_strings)?;
    let cfg = ws.load_config();

    let mut stats = Stats::default();
    let mut pending_neural: Vec<&AnnotationId> = Vec::new();
    let mut pending_test: usize = 0;
    let mut pending_full: usize = 0;

    for (id, entry) in index.entries.iter() {
        if !matches_all(id, entry, &filters) {
            continue;
        }
        if !rerun && skip_because_proof_still_holds(id, entry, &ws, &index) {
            stats.skipped_clean += 1;
            continue;
        }
        match resolve_verify_level(entry, &cfg) {
            VerifyLevel::Bool(false) => {
                aristo::intent_stmt!(
                    "Invokes no skill, runs no test, writes no signed \
                     outcome. This is intentional, not incomplete — \
                     verify=false means documentation only. A \
                     contributor 'completing' this arm by adding a \
                     skill call defeats the opt-out semantic.",
                    verify = "neural",
                    id = "verify_false_arm_is_intentional_skip"
                );
                stats.skipped_doc_only += 1;
            }
            VerifyLevel::Bool(true) => {
                unreachable!("resolve_verify_level returns Method(..) for Bool(true)")
            }
            VerifyLevel::Method(VerifyMethod::Neural) => {
                pending_neural.push(id);
            }
            VerifyLevel::Method(VerifyMethod::Test) => pending_test += 1,
            VerifyLevel::Method(VerifyMethod::Full) => pending_full += 1,
        }
    }

    if !pending_neural.is_empty() {
        pending::write_pending_neural(&ws, &index, &pending_neural)?;
    }

    emit_summary(&stats, pending_neural.len(), pending_test, pending_full);

    // Slice 23 ships neural via the in-agent skill route. test and full
    // were originally milestone E slices 24/26 but are deferred to post-MVP
    // pending the spec-schema + injection-mechanism design (see
    // docs/deferred/verify-test-design.md). If the user has entries needing
    // those methods, surface a NotImplemented at the end with the deferred-
    // design pointer so the message is actionable rather than blocking-on-
    // a-slice-number-that-isn't-coming.
    if pending_test > 0 {
        return Err(CliError::NotImplemented {
            what: "aristo verify (verify=\"test\")",
            slice: "post-MVP (see docs/deferred/verify-test-design.md)",
        });
    }
    if pending_full > 0 {
        return Err(CliError::NotImplemented {
            what: "aristo verify (verify=\"full\")",
            slice: "post-MVP (depends on verify=\"test\"; see docs/deferred/verify-test-design.md)",
        });
    }
    Ok(())
}

#[derive(Default, Debug)]
struct Stats {
    /// Annotations with `verify = false`: documentation only, skipped
    /// without invoking any skill or updating status.
    skipped_doc_only: usize,
    /// Annotations already in a clean verified state, skipped because
    /// `--rerun` was not passed (default policy; --rerun overrides).
    skipped_clean: usize,
}

fn emit_summary(stats: &Stats, pending_neural: usize, pending_test: usize, pending_full: usize) {
    println!(
        "ok: 0 annotations verified, {} skipped (documentation only).",
        stats.skipped_doc_only + stats.skipped_clean
    );
    if pending_neural > 0 {
        let word = if pending_neural == 1 {
            "entry"
        } else {
            "entries"
        };
        println!(
            "→ {pending_neural} {word} pending neural verification — wrote .aristo/pending-neural.toml."
        );
        println!(
            "  In Claude Code (or another agent with the aristo-neural-verify skill installed), run:"
        );
        println!("    /aristo-neural-verify");
        println!(
            "  to produce verdicts for each pending entry. The skill writes .aristo/proofs/<id>.proof"
        );
        println!(
            "  files; run `aristo verify --apply-verdicts` to validate and apply them to the index."
        );
    }
    let _ = (pending_test, pending_full); // surfaced by the NotImplemented error after this fn
}

fn parse_filters(filter_strings: &[String]) -> CliResult<Vec<Filter>> {
    let mut out = Vec::with_capacity(filter_strings.len());
    for raw in filter_strings {
        let f: Filter = raw.parse().map_err(|e| CliError::Other {
            message: format!("{e}"),
            exit_code: 2,
        })?;
        out.push(f);
    }
    Ok(out)
}

fn matches_all(id: &AnnotationId, entry: &IndexEntry, filters: &[Filter]) -> bool {
    filters.iter().all(|f| matches_filter(id, entry, f))
}

fn matches_filter(id: &AnnotationId, entry: &IndexEntry, f: &Filter) -> bool {
    match f {
        Filter::Id(want) => id.as_str() == want,
        Filter::File(want) => file_of(entry) == want,
        Filter::Parent(want) => match parent_ids(entry) {
            Some(ids) => ids.any_match(want),
            None => false,
        },
        Filter::Status(want) => crate::commands::show::status_label(status_of(entry)) == want,
    }
}

fn file_of(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

fn status_of(entry: &IndexEntry) -> Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

trait ParentMatch {
    fn any_match(&self, want: &str) -> bool;
}

impl ParentMatch for aristo_core::index::ParentLink {
    fn any_match(&self, want: &str) -> bool {
        self.iter().any(|p| p.as_str() == want)
    }
}

fn parent_ids(entry: &IndexEntry) -> Option<&aristo_core::index::ParentLink> {
    match entry {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }
}

#[aristo::intent(
    "`assume` entries have no `verify` field by design — they describe \
     external trust (OS guarantees, library invariants, environment \
     contracts), not properties of THIS code, so there is no internal \
     method that could verify them. They resolve to Bool(false) here \
     (the same arm as opt-out intents) so the dispatcher's single \
     skip-without-skill path handles both. A refactor that tries to \
     verify assumes would either invent a verification semantic the \
     design rejects or fail trying.",
    verify = "neural",
    id = "verify_assumes_are_documentation_only_by_design"
)]
#[aristo::intent(
    "Bool(true) resolves through the project's [verify].default_method \
     and falls back to the free-tier default (\"test\") when absent. \
     A refactor that hard-codes either side would silently change \
     verification depth for every annotation that opted into the \
     project default — those are precisely the entries where the \
     author deferred to project policy, so a silent override defeats \
     the deferral.",
    verify = "neural",
    id = "verify_bool_true_resolves_through_project_default"
)]
fn resolve_verify_level(entry: &IndexEntry, cfg: &ConfigFile) -> VerifyLevel {
    let raw = match entry {
        IndexEntry::Intent(e) => e.verify,
        IndexEntry::Assume(_) => return VerifyLevel::Bool(false),
    };
    match raw {
        VerifyLevel::Bool(true) => match cfg.verify.default_method {
            Some(m) => VerifyLevel::Method(m),
            None => VerifyLevel::Method(VerifyMethod::Test), // free-tier default
        },
        other => other,
    }
}

#[aristo::intent(
    "An entry is skipped iff (1) its status is in the terminal set \
     {Verified, Tested, Neural, Counterexample, Inconclusive} AND \
     (2) its on-disk .proof file still passes the mechanical \
     validator against the current index + source. The validator \
     is the source of truth for 'still applicable'; the status flag \
     is a cache. Reading only the flag would miss ground drift in \
     existing proofs (cited code rewritten, cited intent's text \
     changed) — invisible until the next --apply-verdicts cycle. \
     Re-running the validator at list time is bounded by the count \
     of terminal entries, which is the workload we are trying to skip.",
    verify = "test",
    id = "verify_skip_consults_validator_not_just_status"
)]
fn skip_because_proof_still_holds(
    id: &AnnotationId,
    entry: &IndexEntry,
    ws: &Workspace,
    index: &IndexFile,
) -> bool {
    if !is_terminal_status(status_of(entry)) {
        return false;
    }
    let path = proof_path_for(ws, id);
    if !path.is_file() {
        // Terminal status but no proof file → inconsistency. Force re-verify.
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(pf) = ProofFile::parse(&raw) else {
        return false;
    };
    validator::validate(id, &pf, index, &ws.root).is_empty()
}

fn is_terminal_status(s: Status) -> bool {
    matches!(
        s,
        Status::Verified
            | Status::Tested
            | Status::Neural
            | Status::Counterexample
            | Status::Inconclusive
    )
}

fn proof_path_for(ws: &Workspace, id: &AnnotationId) -> PathBuf {
    let filename = format!("{}.proof", id.as_str().replace(':', "__"));
    ws.aristo_dir().join("proofs").join(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{IntentEntry, Sha256};

    fn intent(id: &str, verify: VerifyLevel, status: Status) -> (AnnotationId, IndexEntry) {
        let zero = Sha256::from_bytes(b"");
        (
            AnnotationId::parse(id).unwrap(),
            IndexEntry::Intent(IntentEntry {
                text: "x".to_string(),
                verify,
                status,
                text_hash: zero.clone(),
                body_hash: zero,
                file: "src/x.rs".to_string(),
                site: "fn x (line 1)".to_string(),
                covered_region: aristo_core::index::CoveredRegion::Function,
                binding: aristo_core::index::BindingState::Local,
                parent: None,
            }),
        )
    }

    #[test]
    fn bool_true_resolves_to_project_default_method_when_set() {
        let (_, entry) = intent("foo", VerifyLevel::Bool(true), Status::Unknown);
        let mut cfg = ConfigFile::default();
        cfg.verify.default_method = Some(VerifyMethod::Full);
        assert_eq!(
            resolve_verify_level(&entry, &cfg),
            VerifyLevel::Method(VerifyMethod::Full)
        );
    }

    #[test]
    fn bool_true_falls_back_to_test_when_no_project_default() {
        let (_, entry) = intent("foo", VerifyLevel::Bool(true), Status::Unknown);
        let cfg = ConfigFile::default();
        assert_eq!(
            resolve_verify_level(&entry, &cfg),
            VerifyLevel::Method(VerifyMethod::Test)
        );
    }

    #[test]
    fn bool_false_resolves_to_bool_false() {
        let (_, entry) = intent("foo", VerifyLevel::Bool(false), Status::Unknown);
        let cfg = ConfigFile::default();
        assert_eq!(resolve_verify_level(&entry, &cfg), VerifyLevel::Bool(false));
    }

    #[test]
    fn terminal_statuses_include_counterexample_and_inconclusive() {
        // Both terminal verdict statuses must be in the terminal set —
        // otherwise the skip logic re-runs them every verify cycle
        // (GAPs 2 and 3 of the proof-lifecycle audit).
        for s in [
            Status::Verified,
            Status::Tested,
            Status::Neural,
            Status::Counterexample,
            Status::Inconclusive,
        ] {
            assert!(
                is_terminal_status(s),
                "{s:?} must be considered terminal-clean"
            );
        }
    }

    #[test]
    fn non_terminal_statuses_force_reverify() {
        for s in [
            Status::Unknown,
            Status::Stale,
            Status::Orphan,
            Status::Forged,
            Status::PendingDeepen,
        ] {
            assert!(
                !is_terminal_status(s),
                "{s:?} must NOT be terminal-clean — needs (re-)verify"
            );
        }
    }
}
