//! `aristo stamp` — index + body-drift detection (+ B5b classification in Phase 2).
//!
//! Builds on top of [`crate::commands::index`]: same walk, same cycle
//! detection, same atomic write. Adds:
//!
//! - **Status drift detection.** Reads the existing `.aristo/index.toml`
//!   first. For each entry that exists in both old and new index, if the
//!   body_hash changed and the old status was a verified state
//!   (Verified / Tested / Neural), the new status flips to Stale —
//!   signaling that the prior verification is no longer valid for the
//!   current code. Status is preserved when body_hash is unchanged.
//!
//! - **Per-annotation summary.** Lists each entry's transition (new /
//!   stale / unchanged / removed) so the developer sees what stamp
//!   did at a glance.
//!
//! - **`--check` CI mode.** Computes everything but does NOT write the
//!   index. Exits non-zero if changes would be made — gates pre-merge CI
//!   on the index being committed in sync with source.
//!
//! Slice 17 explicitly excludes B5b classification (server-issued
//! certificates, Phase 2). Slice 17 also defers the offer-rename UX
//! (interactive promotion of opaque ids to readable ones); for now,
//! opaque ids assigned by `aristo index` stay opaque until the user runs
//! `aristo rename` (slice 32).

use std::fs;

use aristo_core::canon::CanonMatchesFile;
use aristo_core::cycle::detect_cycles;
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Meta, Status};
use aristo_core::walk::walk_directory_with;

use crate::commands::canon::runner::{
    print_stamp_summary, run_canon_step, CanonStepOutcome, RunnerArgs,
};
use crate::commands::index::{
    atomic_write, build_entries, now_rfc3339, walk_options_from_workspace, workspace_or_error,
};
use crate::{CliError, CliResult};

#[aristo::intent(
    "When `--check` is set, `aristo stamp` never writes the index. CI \
     relies on this for drift detection: a regression that mutates the \
     index under `--check` would silently mask the drift it was meant \
     to catch.",
    verify = "test",
    id = "stamp_check_never_writes"
)]
pub(crate) fn run(check: bool, skip_canon: bool, refresh_canon: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    // `--check` is read-only (no writes), so it bypasses the
    // session guard. Mutating runs refuse while a session is active.
    if !check {
        crate::session::guard::ensure_no_active_session(&ws, "aristo stamp")?;
    }

    // Snapshot the previous index BEFORE writing — needed for status drift.
    let prev_index = read_existing_index(&ws.index_path())?;

    println!("→ Walking source from {} …", ws.root.display());
    let walk_opts = walk_options_from_workspace(&ws)?;
    let discovered = walk_directory_with(&ws.root, &walk_opts).map_err(|e| CliError::Other {
        message: format!("walk failed: {e}"),
        exit_code: 1,
    })?;
    println!("→ Found {} annotations", discovered.len());

    let (mut entries, parents_map) = build_entries(&discovered, &ws.root)?;

    println!("→ Checking for parent-link cycles");
    detect_cycles(&parents_map).map_err(|e| CliError::Other {
        message: format!("{e}\n\nNo files modified. Fix the cycle and re-run `aristo stamp`."),
        exit_code: 2,
    })?;

    // Derive canon binding from the canon-matches cache before merging
    // prior status. `build_entries` walks source — which carries the
    // prefixed `id =` but not the server-issued binding handle — and
    // emits every entry as `BindingState::Local`. The cache's
    // `accepted_matches[<prefixed_id>]` is the authoritative store
    // for the `linked` ref; this step joins source-projection with
    // cache-fact so `aristo stamp` is idempotent w.r.t. canon binding
    // and no full-regen of `.aristo/index.toml` erases prior accepts.
    let cache = CanonMatchesFile::read(&ws.canon_matches_path()).map_err(|e| CliError::Other {
        message: format!("reading {}: {e}", ws.canon_matches_path().display()),
        exit_code: 1,
    })?;
    derive_bindings_from_cache(&mut entries, &cache);

    let summary = merge_status_from_prev(&mut entries, prev_index.as_ref());
    if !check {
        // Skip the cascade in --check mode; CI shouldn't mutate the
        // workspace. The summary still surfaces what would be deleted.
        cascade_delete_orphan_proofs(&ws, &summary)?;
    }
    print_summary(&summary);

    let index = IndexFile {
        meta: Meta {
            schema_version: 1,
            generated_by: Some(format!("aristo stamp {}", env!("CARGO_PKG_VERSION"))),
            generated_at: Some(now_rfc3339()),
            source_root: Some(".".to_string()),
        },
        entries,
    };
    let toml_text = toml::to_string_pretty(&index).map_err(|e| CliError::Other {
        message: format!("serializing index.toml: {e}"),
        exit_code: 1,
    })?;

    if check {
        // Compare ENTRIES + schema_version only — generated_at and
        // generated_by churn every run and would always make --check report
        // "out of sync" otherwise.
        let prev_entries = prev_index.as_ref().map(|p| &p.entries);
        let entries_unchanged = match prev_entries {
            None => index.entries.is_empty(),
            Some(prev) => prev == &index.entries,
        };
        if entries_unchanged {
            println!();
            println!("ok: index is up to date (no rewrite needed).");
            warn_on_counterexamples(&index);
            return Ok(());
        }
        println!();
        return Err(CliError::Other {
            message: "index is out of sync with source. Run `aristo stamp` (without --check) to update it, then commit."
                .to_string(),
            exit_code: 2,
        });
    }

    atomic_write(&ws.index_path(), &toml_text)?;

    // ── Canon-match step (post-index write so the cache is keyed
    //    against the freshly-stamped ids). Skipped under --check so
    //    CI doesn't make outbound network calls. ─────────────────────
    run_canon_step_for_stamp(&ws, &index, skip_canon, refresh_canon)?;

    println!();
    let n = index.entries.len();
    let path = ws.index_path();
    let rel = path.strip_prefix(&ws.root).unwrap_or(&path).display();
    let noun = if n == 1 { "annotation" } else { "annotations" };
    println!("ok: stamped {n} {noun} into {rel}");
    warn_on_counterexamples(&index);
    Ok(())
}

#[aristo::intent(
    "Canon-match runs AFTER index write so the freshly-stamped ids \
     are what get cached. Running it before would mean re-stamping \
     a body-drifted entry could cache against the *prior* id (the \
     stamp-assigned opaque). Order is load-bearing — flipping it \
     would silently surface canon findings on stale ids.",
    verify = "test",
    id = "stamp_runs_canon_step_after_index_write"
)]
fn run_canon_step_for_stamp(
    ws: &crate::Workspace,
    index: &IndexFile,
    skip_canon: bool,
    refresh_canon: bool,
) -> CliResult<()> {
    let config = ws.load_config();
    let outcome = run_canon_step(RunnerArgs {
        ws,
        index,
        config: &config.canon,
        threshold: config.canon.threshold_stamp,
        skip_flag: skip_canon,
        refresh_flag: refresh_canon,
        found_by: "aristo stamp",
    })?;
    // Re-read cache for the summary printer (it may have been
    // re-written by the step; this read is a tiny disk hit but
    // keeps the printer pure).
    let cache = CanonMatchesFile::read(&ws.canon_matches_path()).map_err(|e| CliError::Other {
        message: format!("re-read {}: {e}", ws.canon_matches_path().display()),
        exit_code: 1,
    })?;
    print_stamp_summary(&outcome, &cache, ws);
    if matches!(outcome, CanonStepOutcome::Degraded { .. }) {
        // Non-fatal — daily loop continues.
    }
    Ok(())
}

#[aristo::intent(
    "When an annotation is removed from source, its `.aristo/proofs/ \
     <id>.proof` file (if any) is also deleted as part of `aristo \
     stamp`. The proof is verdict-ABOUT-id; without the id it's an \
     orphan that would either rot silently or — if the id is ever \
     re-introduced under the same name — re-attach a stale verdict to \
     a fresh definition. Skipped in --check mode (CI must not mutate \
     the workspace); the summary still reports what would be removed.",
    verify = "test",
    id = "stamp_cascades_proof_deletion_on_removed_annotations"
)]
fn cascade_delete_orphan_proofs(ws: &crate::Workspace, summary: &StampSummary) -> CliResult<()> {
    let proofs_dir = ws.aristo_dir().join("proofs");
    if !proofs_dir.is_dir() {
        return Ok(());
    }
    for change in &summary.notable {
        if !matches!(change.kind, NotableKind::Removed) {
            continue;
        }
        let filename = format!("{}.proof", change.id.as_str().replace(':', "__"));
        let path = proofs_dir.join(filename);
        if path.is_file() {
            fs::remove_file(&path).map_err(|e| CliError::Other {
                message: format!("removing orphan proof {}: {e}", path.display()),
                exit_code: 1,
            })?;
            eprintln!(
                "  • {}: also removed orphan proof {}",
                change.id,
                path.strip_prefix(&ws.root).unwrap_or(&path).display()
            );
        }
    }
    Ok(())
}

#[aristo::intent(
    "Every `aristo stamp` run that finds a Counterexample-status entry \
     emits a loud, unmissable warning enumerating each id, file, and \
     site. There is no `aristo accept-counterexample` to silence this; \
     a counterexample is a definite refutation and stays visible until \
     either the code is fixed (→ body drift → Status::Stale → re-verify) \
     or the intent text is changed to exclude the counterexample case. \
     Treating counterexamples as a quiet status badge would let a \
     refuted invariant sit in the index unnoticed and erode the trust \
     calibration of `aristo status`.",
    verify = "test",
    id = "stamp_surfaces_counterexamples_loudly"
)]
fn warn_on_counterexamples(index: &IndexFile) {
    let counterexamples: Vec<(&AnnotationId, &IndexEntry)> = index
        .entries
        .iter()
        .filter(|(_, e)| entry_status(e) == Status::Counterexample)
        .collect();
    if counterexamples.is_empty() {
        return;
    }
    eprintln!();
    eprintln!(
        "⚠  {} annotation(s) refuted by counterexample — verdicts stand until \
         code or intent text changes:",
        counterexamples.len()
    );
    for (id, entry) in counterexamples {
        let (file, site) = entry_location(entry);
        eprintln!("    {id}");
        eprintln!("      at {file}:{site}");
    }
    eprintln!();
    eprintln!(
        "    Inspect: aristo show <id>     |     Re-verify after fixing: \
         aristo verify --rerun --filter id=<id>"
    );
}

fn entry_status(entry: &IndexEntry) -> Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

fn entry_location(entry: &IndexEntry) -> (&str, &str) {
    match entry {
        IndexEntry::Intent(e) => (&e.file, &e.site),
        IndexEntry::Assume(e) => (&e.file, &e.site),
    }
}

fn read_existing_index(path: &std::path::Path) -> CliResult<Option<IndexFile>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(CliError::Io)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    toml::from_str(&text)
        .map(Some)
        .map_err(|e| CliError::Other {
            message: format!("existing {} did not parse: {e}", path.display()),
            exit_code: 1,
        })
}

/// Outcomes per-id after merging old/new statuses. Reported in stamp's
/// human-readable summary.
#[derive(Debug, Default)]
struct StampSummary {
    new_count: usize,
    unchanged_count: usize,
    body_changed_count: usize,
    text_changed_count: usize,
    removed_count: usize,
    /// Per-id transitions worth surfacing line-by-line. Keeps the summary
    /// short by only listing entries whose status actually changed.
    notable: Vec<NotableChange>,
}

#[derive(Debug)]
struct NotableChange {
    id: AnnotationId,
    kind: NotableKind,
}

#[derive(Debug)]
enum NotableKind {
    BodyDrifted { old_status: Status },
    Removed,
}

#[aristo::intent(
    "Canon binding state is derived from `.aristo/canon-matches.toml`, \
     not preserved across stamp runs. For every index entry whose id \
     carries a canon prefix (`aristos:` / `kanon:`) and has a matching \
     row in the cache's `accepted_matches`, set the binding to \
     `BindingState::Bound { linked }` (or `AssumeEntry::linked = \
     Some(...)`). If the cache row's `linked` field is absent — older \
     caches written before the field was added, or Phase 1 carve-outs \
     where the server omits it — synthesize a deterministic placeholder \
     from `(canon_id, version)`, identical to what `canon accept` \
     would have written. Source has a canon prefix but no cache row → \
     leave Local and emit a diagnostic; the binding was orphaned \
     (cache deleted or never fetched) and the user must re-run with \
     `--refresh-canon` or re-accept.",
    verify = "test",
    id = "stamp_derives_canon_binding_from_cache"
)]
fn derive_bindings_from_cache(
    entries: &mut std::collections::BTreeMap<AnnotationId, IndexEntry>,
    cache: &CanonMatchesFile,
) {
    use aristo_core::canon::synthesize_phase1_linked;
    use aristo_core::index::{ArtaId, BindingState};

    for (id, entry) in entries.iter_mut() {
        // Only canon-prefixed ids are eligible for binding; bare ids
        // (e.g. opaque `aret_*` or user-written snake_case) are
        // unambiguously Local.
        if !is_canon_prefixed(id) {
            continue;
        }
        let Some(cache_row) = cache.entries.get(id) else {
            eprintln!(
                "warning: `{}` has a canon prefix in source but no row in \
                 .aristo/canon-matches.toml — binding orphaned. Re-run \
                 `aristo stamp --refresh-canon` or re-accept to restore.",
                id.as_str()
            );
            continue;
        };
        // Take the most recent accepted match (BTreeMap iteration is
        // ordered, and accept appends; the last entry wins).
        let Some(accepted) = cache_row.accepted_matches.last() else {
            eprintln!(
                "warning: `{}` has a canon prefix in source and a cache row, \
                 but no accepted_matches[]. Binding cannot be derived — \
                 re-accept the canon match.",
                id.as_str()
            );
            continue;
        };
        let linked: ArtaId = match accepted.linked.as_deref() {
            Some(raw) => match ArtaId::parse(raw) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!(
                        "warning: `{}` cache row has a malformed `linked` field \
                         (`{raw}`: {e}); falling back to synthesized handle.",
                        id.as_str()
                    );
                    synthesize_phase1_linked(&accepted.canon_id, &accepted.version)
                }
            },
            None => synthesize_phase1_linked(&accepted.canon_id, &accepted.version),
        };
        match entry {
            IndexEntry::Intent(e) => {
                e.binding = BindingState::Bound { linked };
            }
            IndexEntry::Assume(e) => {
                e.linked = Some(linked);
            }
        }
    }
}

fn is_canon_prefixed(id: &AnnotationId) -> bool {
    let s = id.as_str();
    s.starts_with("aristos:") || s.starts_with("kanon:")
}

#[aristo::intent(
    "Status after stamp reflects the current code, not any prior \
     version. Body-unchanged entries keep their prior status. \
     Body-drifted entries with verified-class status (Verified, Tested, \
     Neural) flip to Stale. Other prior statuses pass through.",
    verify = "test",
    id = "merge_status_preserves_when_body_unchanged"
)]
fn merge_status_from_prev(
    entries: &mut std::collections::BTreeMap<AnnotationId, IndexEntry>,
    prev: Option<&IndexFile>,
) -> StampSummary {
    let mut summary = StampSummary::default();
    let prev_entries = prev.map(|p| &p.entries);

    for (id, new_entry) in entries.iter_mut() {
        let Some(prev_entries) = prev_entries else {
            summary.new_count += 1;
            continue;
        };
        let Some(prev_entry) = prev_entries.get(id) else {
            summary.new_count += 1;
            continue;
        };

        let (new_body, new_text, new_status) = entry_facets(new_entry);
        let (prev_body, prev_text, prev_status) = entry_facets(prev_entry);

        let body_changed = new_body != prev_body;
        let text_changed = new_text != prev_text;

        if !body_changed && !text_changed {
            // Pure unchanged — preserve prior status outright.
            set_status(new_entry, prev_status);
            summary.unchanged_count += 1;
        } else {
            // GAP-1 + GAP-8 (strict policy): any verdict-bearing prior status,
            // body OR text drift, transitions to Stale. Includes Inconclusive —
            // a queued-suggestions verdict against text the agent never saw is
            // a stale verdict. Counterexample → Stale matches the Status enum
            // docstring's promise (the prior implementation handled only the
            // positive arms). Text-drift treated as semantic-rewrite by default
            // (strict) rather than prose-level: the system has no way to tell
            // "fixed a typo" from "narrowed the claim to exclude the failure
            // case"; safer to force re-verify and let the user explicitly opt
            // back in via --rerun on no-op text edits.
            if body_changed {
                summary.body_changed_count += 1;
            } else {
                summary.text_changed_count += 1;
            }
            let next = match prev_status {
                Status::Verified
                | Status::Tested
                | Status::Neural
                | Status::Counterexample
                | Status::Inconclusive => Status::Stale,
                other => other, // Unknown, Stale, Orphan, etc. carry through
            };
            set_status(new_entry, next);
            if matches!(
                prev_status,
                Status::Verified
                    | Status::Tested
                    | Status::Neural
                    | Status::Counterexample
                    | Status::Inconclusive
            ) {
                summary.notable.push(NotableChange {
                    id: id.clone(),
                    kind: NotableKind::BodyDrifted {
                        old_status: prev_status,
                    },
                });
            }
        }
        let _ = new_status;
    }

    if let Some(prev_entries) = prev_entries {
        for id in prev_entries.keys() {
            if !entries.contains_key(id) {
                summary.removed_count += 1;
                summary.notable.push(NotableChange {
                    id: id.clone(),
                    kind: NotableKind::Removed,
                });
            }
        }
    }

    summary
}

fn entry_facets(
    entry: &IndexEntry,
) -> (
    &aristo_core::index::Sha256,
    &aristo_core::index::Sha256,
    Status,
) {
    match entry {
        IndexEntry::Intent(e) => (&e.body_hash, &e.text_hash, e.status),
        IndexEntry::Assume(e) => (&e.body_hash, &e.text_hash, e.status),
    }
}

fn set_status(entry: &mut IndexEntry, status: Status) {
    match entry {
        IndexEntry::Intent(e) => e.status = status,
        IndexEntry::Assume(e) => e.status = status,
    }
}

fn print_summary(s: &StampSummary) {
    println!(
        "  new: {}, unchanged: {}, body-drifted: {}, text-changed: {}, removed: {}",
        s.new_count, s.unchanged_count, s.body_changed_count, s.text_changed_count, s.removed_count
    );
    for change in &s.notable {
        match &change.kind {
            NotableKind::BodyDrifted { old_status } => {
                println!(
                    "  • {}: body changed — status was {old_status:?}, now Stale",
                    change.id
                );
            }
            NotableKind::Removed => {
                println!(
                    "  • {}: source annotation removed; entry dropped from index",
                    change.id
                );
            }
        }
    }
}

#[cfg(test)]
mod derive_bindings_tests {
    use super::*;
    use aristo_core::canon::cache::{AcceptedMatch, CacheEntry};
    use aristo_core::canon::{synthesize_phase1_linked, PrefixTier};
    use aristo_core::index::{
        AssumeEntry, BindingState, CoveredRegion, IntentEntry, Sha256, VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn aid(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }
    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn local_intent(text: &str) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: text.into(),
            verify: VerifyLevel::Method(VerifyMethod::Full),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn foo (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn local_assume(text: &str) -> IndexEntry {
        IndexEntry::Assume(AssumeEntry {
            text: text.into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn foo (line 1)".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn accepted(canon_id: &str, version: &str, linked: Option<&str>) -> AcceptedMatch {
        AcceptedMatch {
            canon_id: canon_id.into(),
            version: version.into(),
            canonical_text: "x".into(),
            canon_version: "v0.2.0".into(),
            confidence: 1.0,
            prefix_tier: PrefixTier::Aristos,
            backed_by: Some("backing".into()),
            linked: linked.map(str::to_string),
            accepted_at: "2026-05-26T00:00:00Z".into(),
            bound_at: "2026-05-26T00:00:00Z".into(),
        }
    }

    fn cache_with(id: &str, am: AcceptedMatch) -> CanonMatchesFile {
        let mut c = CanonMatchesFile::default();
        c.entries.insert(
            aid(id),
            CacheEntry {
                last_match_text_hash: "blake3:x".into(),
                canon_fetched_at: "2026-05-26T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![am],
                rejected_matches: vec![],
            },
        );
        c
    }

    #[test]
    fn prefixed_id_with_cache_linked_promotes_intent_to_bound() {
        let mut entries = BTreeMap::new();
        let id = "aristos:foo";
        entries.insert(aid(id), local_intent("text"));
        let cache = cache_with(id, accepted("foo", "v0.1.0", Some("arta_op4q3z9NbV")));

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Intent(e) = &entries[&aid(id)] else {
            panic!("expected Intent");
        };
        match &e.binding {
            BindingState::Bound { linked } => {
                assert_eq!(linked.as_str(), "arta_op4q3z9NbV");
            }
            other => panic!("expected Bound, got {other:?}"),
        }
    }

    #[test]
    fn prefixed_id_without_cache_linked_synthesizes_handle() {
        // Older caches written before the `linked` field existed
        // deserialize as None. The derive step must synthesize the
        // same handle `canon accept` would have written.
        let mut entries = BTreeMap::new();
        let id = "aristos:foo";
        entries.insert(aid(id), local_intent("text"));
        let cache = cache_with(id, accepted("foo", "v0.1.0", None));

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Intent(e) = &entries[&aid(id)] else {
            panic!();
        };
        let expected = synthesize_phase1_linked("foo", "v0.1.0");
        match &e.binding {
            BindingState::Bound { linked } => assert_eq!(linked, &expected),
            other => panic!("expected Bound, got {other:?}"),
        }
    }

    #[test]
    fn prefixed_id_with_no_cache_row_stays_local() {
        // Source has a canon prefix but the cache has no row at all
        // — binding is orphaned (e.g. user deleted canon-matches.toml).
        // Stay Local; the deriver emits a stderr warning.
        let mut entries = BTreeMap::new();
        let id = "aristos:foo";
        entries.insert(aid(id), local_intent("text"));
        let cache = CanonMatchesFile::default();

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Intent(e) = &entries[&aid(id)] else {
            panic!();
        };
        assert_eq!(e.binding, BindingState::Local);
    }

    #[test]
    fn prefixed_id_with_cache_row_but_no_accepted_stays_local() {
        let mut entries = BTreeMap::new();
        let id = "aristos:foo";
        entries.insert(aid(id), local_intent("text"));
        // Cache row exists but accepted_matches is empty (e.g.
        // pending-only state).
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid(id),
            CacheEntry {
                last_match_text_hash: "blake3:x".into(),
                canon_fetched_at: "2026-05-26T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Intent(e) = &entries[&aid(id)] else {
            panic!();
        };
        assert_eq!(e.binding, BindingState::Local);
    }

    #[test]
    fn unprefixed_id_is_never_promoted_even_with_cache_hit() {
        // Bare ids (opaque `aret_*`, user snake_case) are unambiguously
        // Local — even if someone manually inserts an accepted match
        // for them, the deriver ignores it. Canon binding is signaled
        // by the prefix in source, not by cache presence.
        let mut entries = BTreeMap::new();
        let id = "aret_abc12def";
        entries.insert(aid(id), local_intent("text"));
        let cache = cache_with(id, accepted("foo", "v0.1.0", Some("arta_op4q3z9NbV")));

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Intent(e) = &entries[&aid(id)] else {
            panic!();
        };
        assert_eq!(e.binding, BindingState::Local);
    }

    #[test]
    fn assume_entry_with_cache_hit_sets_linked() {
        let mut entries = BTreeMap::new();
        let id = "aristos:some_external_invariant";
        entries.insert(aid(id), local_assume("text"));
        let cache = cache_with(
            id,
            accepted("some_external_invariant", "v0.1.0", Some("arta_op4q3z9NbV")),
        );

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Assume(e) = &entries[&aid(id)] else {
            panic!();
        };
        assert_eq!(
            e.linked.as_ref().map(|a| a.as_str()),
            Some("arta_op4q3z9NbV")
        );
    }

    #[test]
    fn malformed_cache_linked_falls_back_to_synthesized() {
        // A corrupted `linked` field shouldn't fail the entire stamp;
        // fall back to the synthesized handle and warn.
        let mut entries = BTreeMap::new();
        let id = "aristos:foo";
        entries.insert(aid(id), local_intent("text"));
        let cache = cache_with(id, accepted("foo", "v0.1.0", Some("not-an-arta-id")));

        derive_bindings_from_cache(&mut entries, &cache);

        let IndexEntry::Intent(e) = &entries[&aid(id)] else {
            panic!();
        };
        let expected = synthesize_phase1_linked("foo", "v0.1.0");
        match &e.binding {
            BindingState::Bound { linked } => assert_eq!(linked, &expected),
            other => panic!("expected Bound (synthesized fallback), got {other:?}"),
        }
    }
}
