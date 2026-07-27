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
use crate::commands::show::load_index;
use crate::filter::Filter;
use crate::pipeline;
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

pub(crate) mod apply;
pub(crate) mod canon_dispatch;
pub(crate) mod card;
pub(crate) mod pending;
pub(crate) mod session_kind;
pub(crate) mod submit;
pub(crate) mod validator;
pub(crate) mod waiver;

#[allow(
    clippy::too_many_arguments,
    reason = "flag-shaped dispatcher; collapsing into a struct adds indirection without simplifying the call site"
)]
pub(crate) fn run(
    filter_strings: &[String],
    rerun: bool,
    check: bool,
    strict: bool,
    audit: bool,
    apply_verdicts: bool,
    rewrite_hashes: bool,
    submit_verdict: bool,
    pop_next: bool,
    queue_status: bool,
    id: Option<String>,
    json: Option<String>,
    wait: bool,
    require_dispatch: bool,
    view: Option<String>,
    tags: &[String],
    accept: Option<String>,
    because: Option<String>,
    tracking: Option<String>,
) -> CliResult<()> {
    let _ = check; // wired for forward-compat; no behavior yet (see module doc)

    // E3: `--view <session_id>` attaches to an existing session.
    // Read-only — no session guard, no workspace mutation. Runs
    // before workspace resolution since the operation only needs
    // an HTTP client (auth + server URL).
    if let Some(session_id) = view {
        if !tags.is_empty() {
            return Err(CliError::Other {
                message: "--tags is not compatible with --view (the session was already \
                          dispatched with its tag set)"
                    .into(),
                exit_code: 2,
            });
        }
        return canon_dispatch::run_view_session(&session_id, wait);
    }

    let ws = workspace_or_error()?;

    // `--audit` is the CI freshness gate: regenerate from source + proofs and
    // report fresh / stale / refuted / orphan, exiting non-zero under --strict.
    // Read-only, no dispatch, no network. Replaces `aristo stamp --check`.
    if audit {
        return run_audit(&ws, strict);
    }

    let index = load_index(&ws)?;

    // Phase 16 (c): `--accept <canon-id> --because <reason>` records a
    // user-side known-failure waiver in `.aristo/expectations.toml` and
    // returns. Write-only — no dispatch, and no session guard: an
    // accepted gap is an independent sidecar, not an index/proof
    // mutation, so it shouldn't block on an open critique session.
    if let Some(canon_id) = accept {
        let reason =
            because.expect("--because is required with --accept (enforced by clap `requires`)");
        return waiver::run_accept(&ws, &index, &canon_id, &reason, tracking.as_deref());
    }

    // Reads (pop_next, queue_status) bypass the session guard;
    // workers must keep functioning so an open critique-review
    // doesn't strand an in-flight verify dispatch. Writes block.
    if pop_next {
        return run_pop_next(&ws);
    }
    if queue_status {
        return run_queue_status(&ws);
    }

    if submit_verdict {
        crate::session::guard::ensure_no_active_session(&ws, "aristo verify --submit-verdict")?;
        // clap's `requires = ...` ensures id/json are Some here; the
        // .expect() documents the invariant rather than introducing a
        // user-facing error mode that can't actually trigger.
        let id_str = id.expect("--id is required with --submit-verdict (enforced by clap)");
        let json_str = json.expect("--json is required with --submit-verdict (enforced by clap)");
        return submit::run_submit_verdict(&ws, &index, &id_str, &json_str);
    }

    if apply_verdicts {
        crate::session::guard::ensure_no_active_session(&ws, "aristo verify --apply-verdicts")?;
        return apply::run_apply_verdicts(&ws, &index, rewrite_hashes);
    }

    // The dispatch path (enqueuing new work for workers) is also a
    // mutation — it writes pending entries that downstream apply will
    // commit to the index.
    crate::session::guard::ensure_no_active_session(&ws, "aristo verify")?;

    // Migrate any legacy single-file pending queue before this run produces
    // new work. Idempotent.
    pending::migrate_legacy_pending_if_present(&ws)?;

    let filters = parse_filters(filter_strings)?;
    let cfg = ws.load_config();

    let mut stats = Stats::default();
    let mut pending_neural: Vec<&AnnotationId> = Vec::new();
    let mut pending_test: usize = 0;
    let mut pending_full: Vec<&AnnotationId> = Vec::new();

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
            VerifyLevel::Method(VerifyMethod::Full) => pending_full.push(id),
        }
    }

    let enqueued = if !pending_neural.is_empty() {
        pending::enqueue_pending(&ws, &index, &pending_neural)?
    } else {
        0
    };
    let _ = enqueued; // count surfaced via emit_summary below

    // Partition `verify="full"` entries into canon-bound (kanon: /
    // aristos:) vs. non-canon. Canon-bound entries dispatch through
    // §14 canon-verify; non-canon-bound entries hit the deferred
    // verify=full design path (see docs/deferred/verify-test-design.md).
    let (canon_full, other_full) = canon_dispatch::partition_full(&index, &pending_full);

    emit_summary(
        &stats,
        pending_neural.len(),
        pending_test,
        canon_full.len() + other_full.len(),
    );

    // §14 canon-verify dispatch. Auth required (user-confirmed in
    // session): without an arta_* token, server-side authorization
    // would reject anyway, so we skip with an actionable hint rather
    // than POSTing a guaranteed-401.
    let tags_filter = if tags.is_empty() { None } else { Some(tags) };
    let dispatched = canon_dispatch::run_canon_dispatch(
        &ws.root,
        &ws.canon_matches_path(),
        &ws.expectations_path(),
        &canon_full,
        tags_filter,
        wait,
    )?;

    // CI guard against vacuous green (--require-dispatch): an empty
    // dispatch set means the server never saw this commit, so a 0 exit
    // would read as "verified" with nothing run.
    if require_dispatch && dispatched == 0 {
        return Err(CliError::Other {
            message: "--require-dispatch: the canon-verify dispatch set is empty — nothing \
                      was sent to the server.\n  \
                      Likely causes: no canon-bound `verify=\"full\"` entries matched; every \
                      matching entry was skipped as already verified and fresh (pass --rerun \
                      to force); or the canon-matches cache is missing/stale (run \
                      `aristo canon refresh` and commit .aristo/canon-matches.toml)."
                .into(),
            exit_code: 1,
        });
    }

    // Even without the guard, a zero-dispatch run must not pass
    // silently: say why nothing reached the server and what to check.
    if let Some(warning) =
        canon_dispatch::zero_dispatch_warning(dispatched, canon_full.len(), stats.skipped_clean)
    {
        eprintln!("{warning}");
    }

    // Slice 23 ships neural via the in-agent skill route. test +
    // non-canon-bound full were originally milestone E slices 24/26
    // but are deferred to post-MVP pending the spec-schema +
    // injection-mechanism design. Canon-bound full is now live (§14);
    // the deferred design only governs the non-canon path.
    if pending_test > 0 {
        return Err(CliError::NotImplemented {
            what: "aristo verify (verify=\"test\")",
            hint: "post-MVP — see docs/deferred/verify-test-design.md",
        });
    }
    if !other_full.is_empty() {
        return Err(CliError::NotImplemented {
            what: "aristo verify (verify=\"full\") for non-canon-bound entries",
            hint: "post-MVP — see docs/deferred/verify-test-design.md. \
                   Canon-bound (`kanon:` / `aristos:`) entries dispatch via §14.",
        });
    }
    Ok(())
}

#[aristo::intent(
    "`aristo verify --pop-next` is the worker-facing API: it atomically \
     claims one pending task from `.aristo/verify-queue/pending/`, prints \
     the task body (TOML) to stdout, and exits 0. When the queue is \
     genuinely drained, it prints nothing and still exits 0 — the caller \
     distinguishes 'drained' from 'task body' by checking whether stdout \
     is empty. A refactor that printed a sentinel string (e.g., 'empty') \
     on a drained queue would collide with any task content; a refactor \
     that exited non-zero would force every worker to special-case the \
     happy path. Print-or-empty is the contract.",
    verify = "neural",
    id = "verify_pop_next_prints_task_or_empty_exit_zero"
)]
#[aristo::intent(
    "`aristo verify --audit` regenerates the index from source and \
     `.aristo/proofs/` rather than trusting the committed cache. Under \
     `--strict` it exits non-zero on any STALE, COUNTEREXAMPLE, or ORPHAN \
     proof, but never on an `unknown` entry — never-verified is a legitimate \
     starting state, not a regression.",
    verify = "neural",
    id = "verify_audit_reds_on_stale_refuted_or_orphan_not_unknown"
)]
fn run_audit(ws: &Workspace, strict: bool) -> CliResult<()> {
    let index = crate::commands::stamp::regenerate_index(ws)?;

    let mut fresh = 0usize;
    let mut unknown = 0usize;
    let mut stale: Vec<String> = Vec::new();
    let mut refuted: Vec<String> = Vec::new();
    for (id, entry) in &index.entries {
        let IndexEntry::Intent(e) = entry else {
            continue; // assumes are documentation-only; not a verification target
        };
        if !matches!(e.verify, VerifyLevel::Method(_)) {
            continue; // verify=true/false carry no proof obligation
        }
        match e.status {
            Status::Stale => stale.push(id.as_str().to_string()),
            Status::Counterexample => refuted.push(id.as_str().to_string()),
            s if s.is_terminal_clean() => fresh += 1,
            _ => unknown += 1,
        }
    }
    let orphans = orphan_proof_ids(ws, &index);

    println!(
        "→ Audited {} annotation(s) from source + .aristo/proofs/",
        index.entries.len()
    );
    println!(
        "  fresh: {fresh}, unknown (unverified): {unknown}, stale: {}, refuted: {}, orphan proofs: {}",
        stale.len(),
        refuted.len(),
        orphans.len()
    );
    for id in &stale {
        eprintln!("  • stale: {id} — code drifted since verification; re-verify");
    }
    for id in &refuted {
        eprintln!("  • refuted: {id} — counterexample on record; fix code or intent");
    }
    for id in &orphans {
        eprintln!("  • orphan proof: {id} — .proof on disk has no source annotation");
    }

    let problems = stale.len() + refuted.len() + orphans.len();
    if strict && problems > 0 {
        return Err(CliError::Other {
            message: format!(
                "{problems} freshness problem(s): {} stale, {} refuted, {} orphan proof(s). \
                 Re-verify the drifted entries or remove the orphaned proofs.",
                stale.len(),
                refuted.len(),
                orphans.len()
            ),
            exit_code: 1,
        });
    }
    if problems == 0 {
        println!();
        println!("ok: all verification-bearing annotations are fresh.");
    }
    Ok(())
}

/// `.proof` files on disk whose decoded id has no entry in the regenerated
/// index — verification artifacts left behind by a removed or renamed
/// annotation. Mirrors the `:`→`__` filename convention.
fn orphan_proof_ids(ws: &Workspace, index: &IndexFile) -> Vec<String> {
    let dir = ws.aristo_dir().join("proofs");
    let mut orphans = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return orphans;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("proof") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id_str = stem.replace("__", ":");
        if let Ok(id) = AnnotationId::parse(&id_str) {
            if !index.entries.contains_key(&id) {
                orphans.push(id_str);
            }
        }
    }
    orphans.sort();
    orphans
}

fn run_pop_next(ws: &Workspace) -> CliResult<()> {
    let qdir = pipeline::queue::QueueDir::for_pipeline(ws, pending::PIPELINE_NAME);
    match pipeline::queue::pop_next(&qdir)? {
        Some(task) => {
            // stdout: the task body verbatim. Workers parse this as TOML
            // (matching the VerifyTask schema).
            print!("{}", task.content);
            Ok(())
        }
        None => Ok(()),
    }
}

#[aristo::intent(
    "`aristo verify --queue-status` is a non-destructive peek: it prints \
     `pending: N` and `claimed: M` to stdout and exits 0. Unlike \
     `--pop-next`, it does not claim any entry.",
    verify = "neural",
    id = "verify_queue_status_is_non_destructive_peek"
)]
fn run_queue_status(ws: &Workspace) -> CliResult<()> {
    let qdir = pipeline::queue::QueueDir::for_pipeline(ws, pending::PIPELINE_NAME);
    let status = pipeline::queue::queue_status(&qdir)?;
    println!("pending: {}", status.pending);
    println!("claimed: {}", status.claimed);
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
    let total_skipped = stats.skipped_doc_only + stats.skipped_clean;
    let reason: String = match (stats.skipped_doc_only, stats.skipped_clean) {
        (_, 0) => "documentation-only".to_string(),
        (0, _) => "already in a clean verified state".to_string(),
        (d, c) => format!("{d} documentation-only, {c} already clean"),
    };
    println!("ok: 0 annotations verified, {total_skipped} skipped ({reason}).");
    if pending_neural > 0 {
        let word = if pending_neural == 1 {
            "entry"
        } else {
            "entries"
        };
        println!(
            "→ {pending_neural} {word} pending neural verification — enqueued under .aristo/verify-queue/pending/."
        );
        println!(
            "  In Claude Code (or another agent with the aristo-verify skill installed), run:"
        );
        println!("    /aristo-verify");
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
        Filter::Id(wants) => wants.iter().any(|w| id.as_str() == w),
        // verify ignores the optional line_range (introduced by slice
        // 27.7 for critique scope; verify's "verify everything in this
        // file" semantic is unchanged).
        Filter::File { path, .. } => file_of(entry) == path,
        Filter::Parent(wants) => match parent_ids(entry) {
            Some(ids) => wants.iter().any(|w| ids.any_match(w)),
            None => false,
        },
        Filter::Status(wants) => {
            let label = crate::commands::show::status_label(status_of(entry));
            wants.iter().any(|w| label == w)
        }
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
     method that could verify them. They take the same \
     skip-without-verification path the dispatcher uses for opt-out \
     intents, so neither needs a verification skill. A refactor that \
     tries to verify assumes would either invent a verification \
     semantic the design rejects or fail trying.",
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
                last_critiqued_at_text_hash: None,
                last_critique_finding_count: None,
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
