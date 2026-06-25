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
use crate::commands::verify::apply::derived_status;
use crate::commands::verify::pending::proof_path_for;
use crate::commands::verify::validator::validate;
use crate::{CliError, CliResult};
use aristo_core::proof::ProofFile;

#[aristo::intent(
    "When `--check` is set, `aristo stamp` never writes the index. CI \
     relies on this for drift detection: a regression that mutates the \
     index under `--check` would silently mask the drift it was meant \
     to catch.",
    verify = "test",
    id = "stamp_check_never_writes"
)]
pub(crate) fn run(check: bool, skip_canon: bool, refresh_canon: bool, gc: bool) -> CliResult<()> {
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
        // Skip in --check mode; CI shouldn't mutate the workspace. The
        // summary still surfaces what would move.
        archive_orphan_proofs(&ws, &summary)?;
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

    if gc {
        gc_archived_proofs(&ws)?;
    }

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
     <id>.proof` file (if any) is MOVED to `.aristo/archive/proofs/ \
     <id>.proof` — archived, not deleted. The proof is verdict-ABOUT-id, \
     so it must leave the active proof set; otherwise re-introducing the \
     id would re-attach a stale verdict to a fresh definition. But \
     hard-deleting on every stamp silently destroyed verification work \
     whenever an id legitimately changed (a reword or rename re-anchors \
     the deterministic id), so the proof is retained where it can be \
     recovered — `aristo stamp --gc` is the only path that purges the \
     archive. Skipped in --check mode (CI must not mutate the \
     workspace); the summary still reports what would move.",
    verify = "test",
    id = "stamp_archives_orphan_proofs_on_removed_annotations"
)]
fn archive_orphan_proofs(ws: &crate::Workspace, summary: &StampSummary) -> CliResult<()> {
    let proofs_dir = ws.aristo_dir().join("proofs");
    if !proofs_dir.is_dir() {
        return Ok(());
    }
    let archive_dir = ws.aristo_dir().join("archive").join("proofs");
    for change in &summary.notable {
        if !matches!(change.kind, NotableKind::Removed) {
            continue;
        }
        let stem = change.id.as_str().replace(':', "__");
        let from = proofs_dir.join(format!("{stem}.proof"));
        if from.is_file() {
            fs::create_dir_all(&archive_dir).map_err(|e| CliError::Other {
                message: format!("creating proof archive {}: {e}", archive_dir.display()),
                exit_code: 1,
            })?;
            // Never clobber a previously-archived verdict for the same id (an
            // id can be orphaned more than once — e.g. removed, re-added, and
            // removed again). Uniquify with a counter, keeping the `.proof`
            // extension so `--gc` still collects it. This upholds the ID-D5
            // promise that archiving never loses a verdict.
            let mut to = archive_dir.join(format!("{stem}.proof"));
            let mut n = 1u32;
            while to.exists() {
                to = archive_dir.join(format!("{stem}.{n}.proof"));
                n += 1;
            }
            fs::rename(&from, &to).map_err(|e| CliError::Other {
                message: format!("archiving orphan proof {}: {e}", from.display()),
                exit_code: 1,
            })?;
            eprintln!(
                "  • {}: archived orphan proof → {}",
                change.id,
                to.strip_prefix(&ws.root).unwrap_or(&to).display()
            );
        }
    }
    Ok(())
}

#[aristo::intent(
    "Archiving makes `aristo stamp` non-destructive: an orphaned proof is \
     moved aside, never deleted, so a stray stamp can't lose a verdict. The \
     archive at `.aristo/archive/proofs/` is reclaimed only by the explicit, \
     opt-in `aristo stamp --gc`; nothing purges it implicitly or \
     automatically.",
    verify = "test",
    id = "stamp_gc_is_the_only_purge_of_archived_proofs"
)]
fn gc_archived_proofs(ws: &crate::Workspace) -> CliResult<()> {
    let archive_dir = ws.aristo_dir().join("archive").join("proofs");
    if !archive_dir.is_dir() {
        println!("→ gc: no archived proofs to remove.");
        return Ok(());
    }
    let mut removed = 0usize;
    for entry in fs::read_dir(&archive_dir).map_err(CliError::Io)? {
        let path = entry.map_err(CliError::Io)?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("proof") {
            fs::remove_file(&path).map_err(|e| CliError::Other {
                message: format!("gc removing {}: {e}", path.display()),
                exit_code: 1,
            })?;
            removed += 1;
        }
    }
    let noun = if removed == 1 { "proof" } else { "proofs" };
    println!("→ gc: removed {removed} archived {noun}.");
    Ok(())
}

#[aristo::intent(
    "Every `aristo stamp` run that finds a Counterexample-status entry \
     emits a prominent warning enumerating each id, file, and \
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
pub(crate) struct StampSummary {
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
pub(crate) fn derive_bindings_from_cache(
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

#[aristo::intent(
    "`regenerate_index` rebuilds the full index in memory from source + \
     `.aristo/proofs/` + `.aristo/canon-matches.toml`, with no dependency on a \
     committed `.aristo/index.toml`. Same walk, cycle check, and binding \
     derivation as `aristo stamp`, but status is sourced from proofs \
     (`merge_status_from_proofs`), not carried from a prior index file. This is \
     what lets the index become a gitignored local cache: any reader can call \
     `load_index` and get correct, fresh status without the file existing.",
    verify = "neural",
    id = "regenerate_index_rebuilds_in_memory_without_a_committed_index"
)]
pub(crate) fn regenerate_index(ws: &crate::Workspace) -> CliResult<IndexFile> {
    let walk_opts = walk_options_from_workspace(ws)?;
    let discovered = walk_directory_with(&ws.root, &walk_opts).map_err(|e| CliError::Other {
        message: format!("walk failed: {e}"),
        exit_code: 1,
    })?;
    let (mut entries, parents_map) = build_entries(&discovered, &ws.root)?;
    detect_cycles(&parents_map).map_err(|e| CliError::Other {
        message: format!("{e}\n\nFix the cycle and re-run."),
        exit_code: 2,
    })?;
    let cache = CanonMatchesFile::read(&ws.canon_matches_path()).map_err(|e| CliError::Other {
        message: format!("reading {}: {e}", ws.canon_matches_path().display()),
        exit_code: 1,
    })?;
    derive_bindings_from_cache(&mut entries, &cache);
    merge_status_from_proofs(&mut entries, ws)?;
    Ok(IndexFile {
        meta: Meta {
            schema_version: 1,
            generated_by: Some(format!("aristo regenerate {}", env!("CARGO_PKG_VERSION"))),
            generated_at: Some(now_rfc3339()),
            source_root: Some(".".to_string()),
        },
        entries,
    })
}

/// Phase-1 focal status for one entry given its already-parsed proof.
/// Anchors valid -> the proof's verdict status; drifted -> Stale. Pure: no
/// validator, no cross-entry dependency, no I/O. A `None`/no-proof case is
/// handled by the caller (left as the build_entries `Unknown` default).
fn focal_status_from_proof(
    entry_text_hash: &aristo_core::index::Sha256,
    entry_body_hash: &aristo_core::index::Sha256,
    pf: &ProofFile,
) -> Status {
    let anchors_ok = &pf.verdict.produced_at_text_hash == entry_text_hash
        && &pf.verdict.produced_at_body_hash == entry_body_hash;
    if anchors_ok {
        derived_status(pf)
    } else {
        Status::Stale
    }
}

#[aristo::intent(
    "Status is sourced from `.aristo/proofs/`, not carried from a prior \
     committed index. For each entry the matching `.proof` is loaded and its \
     `produced_at_text_hash`/`produced_at_body_hash` anchors checked against \
     the entry's current hashes: anchors valid -> the proof's verdict; \
     drifted -> Stale; no proof -> Unknown (left as built). A second pass \
     re-runs the full validator against a snapshot carrying the first pass's \
     statuses, so the refuted-sibling-ground guard fires and a clean status \
     is demoted to Stale when the proof no longer validates. This is the \
     source of truth once the index becomes a gitignored cache; running it \
     while every entry is still Unknown would let a proof launder grounding \
     in a refuted sibling, so the two-phase ordering is load-bearing.",
    verify = "neural",
    id = "merge_status_from_proofs_sources_status_from_proof_files"
)]
pub(crate) fn merge_status_from_proofs(
    entries: &mut std::collections::BTreeMap<AnnotationId, IndexEntry>,
    ws: &crate::Workspace,
) -> CliResult<()> {
    // PHASE 1 — focal anchor check only (no cross-entry dependency).
    let mut focal: Vec<(AnnotationId, Status)> = Vec::new();
    for (id, entry) in entries.iter() {
        let path = proof_path_for(ws, id);
        let Ok(raw) = fs::read_to_string(&path) else {
            // No proof on disk -> leave the Unknown that build_entries set.
            continue;
        };
        let Ok(pf) = ProofFile::parse(&raw) else {
            // Unparseable proof -> treat as no verdict; leave Unknown.
            continue;
        };
        let (body_h, text_h, _) = entry_facets(entry);
        focal.push((id.clone(), focal_status_from_proof(text_h, body_h, &pf)));
    }
    for (id, st) in &focal {
        if let Some(e) = entries.get_mut(id) {
            set_status(e, *st);
        }
    }

    // PHASE 2 — full validate against a snapshot that now carries the
    // Phase-1 statuses, so `check_index_ground` can see a refuted sibling.
    let snapshot = IndexFile {
        meta: Meta {
            schema_version: 1,
            generated_by: None,
            generated_at: None,
            source_root: None,
        },
        entries: entries.clone(),
    };
    for (id, st) in &focal {
        // Drifted entries are already Stale; only re-validate the ones a
        // clean/verdict status was assigned to.
        if matches!(st, Status::Stale) {
            continue;
        }
        let path = proof_path_for(ws, id);
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(pf) = ProofFile::parse(&raw) else {
            continue;
        };
        // A clean Phase-1 status is demoted to Stale if the proof no longer
        // validates (drift caught here is structural — refuted-sibling ground,
        // attempts budget, method mismatch, body shape — not the anchor drift
        // already handled in Phase 1).
        if !validate(id, &pf, &snapshot, &ws.root).is_empty() {
            if let Some(e) = entries.get_mut(id) {
                set_status(e, Status::Stale);
            }
        }
    }

    Ok(())
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
mod proofs_join_tests {
    use super::*;
    use aristo_core::index::{
        AnnotationKind, BindingState, CoveredRegion, IntentEntry, Sha256, VerifyLevel, VerifyMethod,
    };
    use aristo_core::proof::{
        CounterexampleBody, Gap, Ground, GroundRelation, InconclusiveBody, Proof, ProofStep,
        PropertyKind, RelationKind, SuggestedAnnotation, VerdictMeta, VerdictType, VerifiedBody,
        Violation,
    };
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn aid(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    fn intent(text_h: Sha256, body_h: Sha256) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify: VerifyLevel::Method(VerifyMethod::Neural),
            status: Status::Unknown,
            text_hash: text_h,
            body_hash: body_h,
            file: "src/lib.rs".into(),
            site: "fn foo (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn proof(t: VerdictType, text_h: Sha256, body_h: Sha256, attempts: u32) -> ProofFile {
        ProofFile {
            verdict: VerdictMeta {
                r#type: t,
                method: VerifyMethod::Neural,
                produced_at_text_hash: text_h,
                produced_at_body_hash: body_h,
                produced_by: "test".into(),
                verifier_model: None,
                attempts,
                property_kind: PropertyKind::Invariant,
            },
            verified: matches!(t, VerdictType::Verified).then(|| VerifiedBody {
                proof: Proof {
                    conclusion: "x".into(),
                    steps: vec![],
                },
            }),
            counterexample: None,
            inconclusive: matches!(t, VerdictType::Inconclusive).then(|| InconclusiveBody {
                partial_proof: None,
                gap: Gap {
                    description: "x".into(),
                    unfilled_path: "0".into(),
                    // One suggestion whose text does not collide with any index
                    // entry, so a fresh inconclusive body passes the validator.
                    suggested_annotations: vec![SuggestedAnnotation {
                        kind: AnnotationKind::Assume,
                        suggested_text: "a brand new invariant to close the gap".into(),
                        at_site: "fn foo (line 1)".into(),
                        rationale: "needed".into(),
                        would_close_path: None,
                    }],
                },
            }),
        }
    }

    /// A valid Verified proof whose single step grounds in `sibling`'s intent.
    /// Passes the validator unless the cited sibling is refuted/docs-only.
    fn verified_grounding_in(
        sibling: &str,
        sibling_text_hash: Sha256,
        text_h: Sha256,
        body_h: Sha256,
    ) -> ProofFile {
        ProofFile {
            verdict: VerdictMeta {
                r#type: VerdictType::Verified,
                method: VerifyMethod::Neural,
                produced_at_text_hash: text_h,
                produced_at_body_hash: body_h,
                produced_by: "test".into(),
                verifier_model: None,
                attempts: 1,
                property_kind: PropertyKind::Invariant,
            },
            verified: Some(VerifiedBody {
                proof: Proof {
                    conclusion: "holds".into(),
                    steps: vec![ProofStep {
                        path: "0".into(),
                        claim: "by the cited sibling".into(),
                        relation_to_parent: RelationKind::Decomposes,
                        grounds: vec![Ground::Intent {
                            id: aid(sibling),
                            at_text_hash: Some(sibling_text_hash),
                            relation: GroundRelation::Instantiates,
                            reason: None,
                        }],
                        subgoal_paths: vec![],
                        proposed_promotion: false,
                    }],
                },
            }),
            counterexample: None,
            inconclusive: None,
        }
    }

    /// A structurally valid Counterexample proof (one trigger step with a
    /// self-contained ground), so it survives Phase 2 and stays Counterexample.
    fn valid_counterexample(text_h: Sha256, body_h: Sha256) -> ProofFile {
        ProofFile {
            verdict: VerdictMeta {
                r#type: VerdictType::Counterexample,
                method: VerifyMethod::Neural,
                produced_at_text_hash: text_h,
                produced_at_body_hash: body_h,
                produced_by: "test".into(),
                verifier_model: None,
                attempts: 1,
                property_kind: PropertyKind::Invariant,
            },
            verified: None,
            counterexample: Some(CounterexampleBody {
                violation: Violation {
                    description: "refuted".into(),
                    violated_step_path: "0".into(),
                    trigger_steps: vec![ProofStep {
                        path: "0".into(),
                        claim: "trigger".into(),
                        relation_to_parent: RelationKind::Decomposes,
                        grounds: vec![Ground::Composition {
                            reason: "trivial".into(),
                        }],
                        subgoal_paths: vec![],
                        proposed_promotion: false,
                    }],
                    refuted_grounds: vec![],
                },
            }),
            inconclusive: None,
        }
    }

    fn make_ws() -> (TempDir, crate::Workspace) {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("aristo.toml"),
            "[verify]\ndefault_method = \"neural\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join(".aristo").join("proofs")).unwrap();
        std::fs::write(
            tmp.path().join(".aristo").join("index.toml"),
            "[__meta__]\nschema_version = 1\n",
        )
        .unwrap();
        let ws = crate::Workspace::find(Some(tmp.path())).unwrap();
        (tmp, ws)
    }

    fn write_proof(ws: &crate::Workspace, id: &str, pf: &ProofFile) {
        let filename = format!("{}.proof", id.replace(':', "__"));
        let path = ws.aristo_dir().join("proofs").join(filename);
        std::fs::write(path, pf.to_toml().unwrap()).unwrap();
    }

    // ─── Phase 1 (pure anchor logic) ────────────────────────────────────

    #[test]
    fn focal_status_anchors_match_maps_each_verdict() {
        let (th, bh) = (sha('a'), sha('b'));
        assert_eq!(
            focal_status_from_proof(
                &th,
                &bh,
                &proof(VerdictType::Verified, th.clone(), bh.clone(), 1)
            ),
            Status::Neural
        );
        assert_eq!(
            focal_status_from_proof(
                &th,
                &bh,
                &proof(VerdictType::Counterexample, th.clone(), bh.clone(), 1)
            ),
            Status::Counterexample
        );
        assert_eq!(
            focal_status_from_proof(
                &th,
                &bh,
                &proof(VerdictType::Inconclusive, th.clone(), bh.clone(), 1)
            ),
            Status::Inconclusive
        );
    }

    #[test]
    fn focal_status_text_or_body_drift_is_stale() {
        let (th, bh) = (sha('a'), sha('b'));
        // text drift (sha chars must be valid lowercase hex)
        assert_eq!(
            focal_status_from_proof(
                &th,
                &bh,
                &proof(VerdictType::Verified, sha('c'), bh.clone(), 1)
            ),
            Status::Stale
        );
        // body drift
        assert_eq!(
            focal_status_from_proof(
                &th,
                &bh,
                &proof(VerdictType::Verified, th.clone(), sha('c'), 1)
            ),
            Status::Stale
        );
    }

    // ─── Two-phase, on disk ─────────────────────────────────────────────

    #[test]
    fn no_proof_leaves_unknown() {
        let (_tmp, ws) = make_ws();
        let mut entries = BTreeMap::new();
        entries.insert(aid("foo"), intent(sha('a'), sha('b')));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, st) = entry_facets(&entries[&aid("foo")]);
        assert_eq!(st, Status::Unknown);
    }

    #[test]
    fn anchors_match_and_valid_proof_survives_phase2() {
        // Guards against an "always demote" regression: a valid, anchor-matching
        // proof must keep its verdict through Phase 2. A no-partial-proof
        // inconclusive with one non-colliding suggestion is the lightest body
        // that passes the full validator (no proof tree / grounds to satisfy).
        let (_tmp, ws) = make_ws();
        let (th, bh) = (sha('a'), sha('b'));
        write_proof(
            &ws,
            "foo",
            &proof(VerdictType::Inconclusive, th.clone(), bh.clone(), 1),
        );
        let mut entries = BTreeMap::new();
        entries.insert(aid("foo"), intent(th, bh));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, st) = entry_facets(&entries[&aid("foo")]);
        assert_eq!(st, Status::Inconclusive);
    }

    #[test]
    fn anchors_match_but_proof_invalid_demotes_to_stale() {
        // attempts=0 is a guaranteed validator rejection. Phase 1 alone would
        // map this to Neural (see focal_status test); the full two-phase join
        // must demote it to Stale.
        let (_tmp, ws) = make_ws();
        let (th, bh) = (sha('a'), sha('b'));
        let invalid = proof(VerdictType::Verified, th.clone(), bh.clone(), 0);
        assert_eq!(
            focal_status_from_proof(&th, &bh, &invalid),
            Status::Neural,
            "precondition: Phase 1 would call this clean"
        );
        write_proof(&ws, "foo", &invalid);
        let mut entries = BTreeMap::new();
        entries.insert(aid("foo"), intent(th, bh));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, st) = entry_facets(&entries[&aid("foo")]);
        assert_eq!(st, Status::Stale);
    }

    #[test]
    fn anchor_drift_is_stale_on_disk() {
        let (_tmp, ws) = make_ws();
        // proof produced against different hashes than the entry now has.
        write_proof(
            &ws,
            "foo",
            &proof(VerdictType::Verified, sha('c'), sha('d'), 1),
        );
        let mut entries = BTreeMap::new();
        entries.insert(aid("foo"), intent(sha('a'), sha('b')));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, st) = entry_facets(&entries[&aid("foo")]);
        assert_eq!(st, Status::Stale);
    }

    #[test]
    fn join_is_terminal_clean_only_via_valid_matching_proof() {
        // The load-bearing safety invariant (absolute, not a differential
        // refinement vs the prior-index path): the only route to a terminal-clean
        // status is a valid, anchor-matching proof. Drift, absence, and invalid
        // proofs all yield non-clean statuses.
        let (_tmp, ws) = make_ws();
        write_proof(
            &ws,
            "drift",
            &proof(VerdictType::Verified, sha('c'), sha('d'), 1),
        ); // anchor drift
        write_proof(
            &ws,
            "invalid",
            &proof(VerdictType::Verified, sha('a'), sha('b'), 0),
        ); // invalid
           // "absent" has no proof file.
        let mut entries = BTreeMap::new();
        entries.insert(aid("drift"), intent(sha('a'), sha('b')));
        entries.insert(aid("invalid"), intent(sha('a'), sha('b')));
        entries.insert(aid("absent"), intent(sha('a'), sha('b')));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        for id in ["drift", "invalid", "absent"] {
            let (_, _, st) = entry_facets(&entries[&aid(id)]);
            assert!(
                !st.is_terminal_clean(),
                "{id} must not be terminal-clean, got {st:?}"
            );
        }
    }

    #[test]
    fn method_mismatch_demotes_to_stale() {
        // Entry declares verify=neural; a proof claiming method=test fails the
        // validator's method check -> Phase 2 demotes to Stale even though the
        // anchors match.
        let (_tmp, ws) = make_ws();
        let (th, bh) = (sha('a'), sha('b'));
        let mut pf = proof(VerdictType::Verified, th.clone(), bh.clone(), 1);
        pf.verdict.method = VerifyMethod::Test;
        write_proof(&ws, "foo", &pf);
        let mut entries = BTreeMap::new();
        entries.insert(aid("foo"), intent(th, bh));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, st) = entry_facets(&entries[&aid("foo")]);
        assert_eq!(st, Status::Stale);
    }

    #[test]
    fn phase2_demotes_focal_grounded_in_refuted_sibling() {
        // The raison d'être of the two-phase join: a proof that grounds in a
        // sibling whose own proof is a Counterexample must be demoted to Stale.
        // Phase 1 sets the sibling to Counterexample BEFORE the snapshot, so
        // check_index_ground sees the refuted sibling during Phase 2.
        let (_tmp, ws) = make_ws();
        let (a_t, a_b) = (sha('a'), sha('b'));
        let (b_t, b_b) = (sha('c'), sha('d'));
        write_proof(
            &ws,
            "sibling_b",
            &valid_counterexample(b_t.clone(), b_b.clone()),
        );
        write_proof(
            &ws,
            "focal_a",
            &verified_grounding_in("sibling_b", b_t.clone(), a_t.clone(), a_b.clone()),
        );
        let mut entries = BTreeMap::new();
        entries.insert(aid("focal_a"), intent(a_t, a_b));
        entries.insert(aid("sibling_b"), intent(b_t, b_b));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, b_status) = entry_facets(&entries[&aid("sibling_b")]);
        let (_, _, a_status) = entry_facets(&entries[&aid("focal_a")]);
        assert_eq!(b_status, Status::Counterexample, "sibling stays refuted");
        assert_eq!(
            a_status,
            Status::Stale,
            "focal grounded in a refuted sibling must demote"
        );
    }

    #[test]
    #[ignore = "known gap: a deleted .proof loses the sibling's Counterexample, so \
                the refuted-sibling guard cannot fire and the focal proof reaches \
                Neural. Closed by the slice-4/7 `verify --audit --strict` \
                proof-deletion red (ADR index-as-local-cache.md §3/§4); enable then."]
    fn deleted_counterexample_proof_must_not_let_sibling_relax() {
        // Same as the sibling-demotion test, but sibling_b's Counterexample
        // proof is absent (deleted). Asserts the DESIRED behavior — which fails
        // today, hence #[ignore]. It documents the one direction in which the
        // proofs-join is weaker than the durable committed-index status.
        let (_tmp, ws) = make_ws();
        let (a_t, a_b) = (sha('a'), sha('b'));
        let (b_t, b_b) = (sha('c'), sha('d'));
        // sibling_b has NO proof on disk (deleted).
        write_proof(
            &ws,
            "focal_a",
            &verified_grounding_in("sibling_b", b_t.clone(), a_t.clone(), a_b.clone()),
        );
        let mut entries = BTreeMap::new();
        entries.insert(aid("focal_a"), intent(a_t, a_b));
        entries.insert(aid("sibling_b"), intent(b_t, b_b));
        merge_status_from_proofs(&mut entries, &ws).unwrap();
        let (_, _, a_status) = entry_facets(&entries[&aid("focal_a")]);
        assert!(
            !a_status.is_terminal_clean(),
            "focal grounded in a sibling with no surviving proof must not be clean, got {a_status:?}"
        );
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
