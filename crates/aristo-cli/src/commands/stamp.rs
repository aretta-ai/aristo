//! `aristo stamp` — refresh the local index cache from source + proofs.
//!
//! Builds on top of [`crate::commands::index`]: same walk, same cycle
//! detection, same atomic write. Under the index-as-local-cache model
//! (Option B), `.aristo/index.toml` is a gitignored, regenerable cache and
//! `aristo stamp` is OPTIONAL — it just refreshes that cache so readers hit a
//! fast path. It produces exactly what `regenerate_index` produces for a
//! reader. Adds, over `aristo index`:
//!
//! - **Status from proofs.** Status is sourced from `.aristo/proofs/` via
//!   [`merge_status_from_proofs`] (anchor check + validator), not carried from
//!   a prior committed index. A drifted/refuted/absent proof yields
//!   Stale/Counterexample/Unknown respectively.
//!
//! - **Orphan-proof archival.** A `.proof` whose annotation was removed from
//!   source (set-difference of proof files minus walked entries) is MOVED to
//!   `.aristo/archive/proofs/`, never deleted; `--gc` is the only purge.
//!
//! - **`--check` CI mode.** Computes everything but does NOT write the index.
//!   Exits non-zero if the regenerated entries differ from the committed
//!   cache, or — once the index is in sync — if a dry-run of the
//!   canon-matches reconcile finds `.aristo/canon-matches.toml` out of sync
//!   with source. (The canonical freshness gate is
//!   `aristo verify --audit --strict`, which does not need a committed index
//!   at all.)
//!
//! Slice 17 deferred the offer-rename UX (interactive promotion of opaque ids
//! to readable ones); opaque ids assigned by `aristo index` stay opaque until
//! the user runs `aristo rename` (slice 32).

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

    // Status is sourced from `.aristo/proofs/`, so the written index matches
    // exactly what `regenerate_index` produces for a reader — `aristo stamp`
    // only refreshes the (now optional, gitignored) local cache.
    merge_status_from_proofs(&mut entries, &ws)?;
    if !check {
        // Skip in --check mode; CI must not mutate the workspace.
        archive_orphan_proofs(&ws, &entries)?;
    }
    print_status_summary(&entries);

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
            // Index is in sync — now dry-run the canon-matches
            // reconcile (same authority, same skip conditions, on a
            // clone) so canon-matches drift fails CI the same way
            // index drift does. Never writes.
            check_canon_matches_drift(&ws, &index, &discovered)?;
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

    // ── Source-authoritative reconcile of .aristo/canon-matches.toml
    //    against the RAW walk (never against a possibly stale on-disk
    //    index, and never against the post-validation entry set).
    //    Runs before the canon step so its early returns
    //    (--skip-canon, disabled config, cache hit, free tier,
    //    degraded) can't let stale rows survive; under --check the
    //    early return above runs the same reconcile as a read-only
    //    drift check instead (check_canon_matches_drift). ────────────
    reconcile_canon_matches(&ws, &index, &discovered)?;

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
    "The canon-matches cache is reconciled on every mutating stamp, at \
     a point the canon step's early returns cannot skip: rows for \
     annotations deleted from source are pruned and accepted matches \
     on ids that are local in source are dropped, with a stderr note \
     per category (a prune that discards accepted bindings warns \
     louder — that's re-accept work destroyed if it was a \
     misconfiguration). The live-id authority is deliberately WIDER \
     than the freshly-built index: ids come from the RAW walk before \
     build_entries validation (a warning-skipped annotation still \
     counts as live), and when aristo.toml [index].exclude is set the \
     walk is re-run WITHOUT the excludes (an excluded annotation still \
     counts as live), because pruning from a walk that is known to be \
     partial would destroy accepted bindings for annotations that \
     still exist in source. When the authority itself is unreliable — \
     an explicit id fails to parse, or the unexcluded walk fails — the \
     reconcile is skipped with a note instead of mispruning. The file \
     is rewritten ONLY when the reconcile changed something — repeated \
     stamps must not churn a committed file. Hanging this off the \
     canon step instead would leave the stale rows in place forever: \
     --skip-canon / disabled-config return before the cache read, and \
     a deleted annotation makes every surviving row a cache hit, \
     which also returns without writing.",
    verify = "test",
    id = "stamp_reconciles_canon_matches_with_source"
)]
fn reconcile_canon_matches(
    ws: &crate::Workspace,
    index: &IndexFile,
    discovered: &[aristo_core::walk::DiscoveredAnnotation],
) -> CliResult<()> {
    let cache_path = ws.canon_matches_path();
    let mut cache = CanonMatchesFile::read(&cache_path).map_err(|e| CliError::Other {
        message: format!("reading {}: {e}", cache_path.display()),
        exit_code: 1,
    })?;
    if cache.entries.is_empty() {
        // Nothing to prune or demote — skip the (possibly second)
        // walk entirely.
        return Ok(());
    }

    let Some(live_ids) = reconcile_live_ids(ws, index, discovered) else {
        // Authority unreliable — the skip note was already printed.
        return Ok(());
    };

    let report = aristo_core::canon::reconcile(&mut cache, &live_ids);
    print_reconcile_notes(&report);
    if report.changed() {
        cache
            .write_atomic(&cache_path)
            .map_err(|e| CliError::Other {
                message: format!("writing {}: {e}", cache_path.display()),
                exit_code: 1,
            })?;
    }
    Ok(())
}

/// The GC-authority live-id set the canon-matches reconcile prunes
/// against: the RAW discovered annotations before `build_entries`
/// validation, re-walked WITHOUT `[index].exclude` when excludes are
/// configured (a partial walk must never count as deletion), unioned
/// with the freshly-built index keys. Returns `None` — after printing
/// a skip note — when the authority is unreliable (an explicit id
/// fails to parse, or the unexcluded walk fails): reconciling against
/// a lossy authority would misprune rows for annotations that still
/// exist in source. Shared by the write path
/// ([`reconcile_canon_matches`]) and the `--check` dry run
/// ([`check_canon_matches_drift`]) so both judge drift by the same
/// rules.
fn reconcile_live_ids(
    ws: &crate::Workspace,
    index: &IndexFile,
    discovered: &[aristo_core::walk::DiscoveredAnnotation],
) -> Option<std::collections::BTreeSet<AnnotationId>> {
    use aristo_core::walk::WalkOptions;

    // GC-authority live set: the RAW discovered annotations, before
    // build_entries validation. When [index].exclude is configured
    // the index walk is partial by design, so re-walk without the
    // excludes — an excluded annotation is still in source and its
    // rows (accepted bindings, rejected-match memory) must survive.
    let cfg = ws.load_config();
    let raw = if cfg.index.exclude.is_empty() {
        aristo_core::canon::raw_live_ids(discovered)
    } else {
        match walk_directory_with(&ws.root, &WalkOptions::none()) {
            Ok(all) => aristo_core::canon::raw_live_ids(&all),
            Err(e) => {
                // The excluded paths may be excluded precisely because
                // they don't walk cleanly — degrade to "no reconcile"
                // rather than failing the stamp or pruning blind.
                eprintln!(
                    "  note: canon-matches: reconcile skipped (walk without \
                     [index].exclude failed: {e})"
                );
                return None;
            }
        }
    };
    if raw.unparseable_explicit_ids > 0 {
        eprintln!(
            "  note: canon-matches: reconcile skipped ({} annotation(s) have \
             unparseable ids; fix them so stale cache entries can be pruned)",
            raw.unparseable_explicit_ids
        );
        return None;
    }
    let mut live_ids: std::collections::BTreeSet<AnnotationId> = raw.ids;
    // Belt-and-braces: anything in the freshly-built index is live by
    // definition (guards against ordinal drift between the excluded
    // and unexcluded walks for idless duplicate annotations).
    live_ids.extend(index.entries.keys().cloned());
    Some(live_ids)
}

/// Per-category stderr notes for a [`ReconcileReport`] — shared
/// verbatim between the write path (what the reconcile did) and the
/// `--check` dry run (what it would do), so the affected ids read the
/// same either way.
fn print_reconcile_notes(report: &aristo_core::canon::ReconcileReport) {
    if !report.pruned.is_empty() {
        let n = report.pruned.len();
        let (noun, id_phrase) = if n == 1 {
            ("entry", "id is")
        } else {
            ("entries", "ids are")
        };
        let ids = report
            .pruned
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        // "no longer in source", not "removed annotations": a dead id
        // also arises from rewording an idless intent (the aret_* id
        // is content-addressed) or from accept's bare→prefixed rekey.
        eprintln!(
            "  note: canon-matches: pruned {n} stale {noun} whose annotation \
             {id_phrase} no longer in source: {ids}"
        );
    }
    if !report.pruned_bound.is_empty() {
        let n = report.pruned_bound.len();
        let (noun, verb) = if n == 1 {
            ("entry", "was")
        } else {
            ("entries", "were")
        };
        let ids = report
            .pruned_bound
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "  warning: canon-matches: {n} pruned {noun} {verb} carrying \
             accepted canon bindings: {ids}. If this is unexpected, restore \
             .aristo/canon-matches.toml from git; re-binding otherwise takes \
             a fresh `aristo canon accept`."
        );
    }
    for id in &report.demoted {
        eprintln!(
            "  note: canon-matches: demoted `{}` (source id lost its canon prefix). \
             Re-match with `aristo stamp --refresh-canon` and re-bind with \
             `aristo canon accept`, or leave it local.",
            id.as_str()
        );
    }
    for id in &report.preserved_for_recovery {
        eprintln!(
            "  note: canon-matches: kept `{}` (annotation no longer in source, \
             but a pending match's canon-prefixed id is live and unbound — \
             looks like an interrupted `aristo canon accept`; re-run it to \
             finish, after which this row is cleaned up).",
            id.as_str()
        );
    }
    // report.missing_binding (live prefixed id without a derivable
    // binding) is already warned about by derive_bindings_from_cache
    // earlier in this same run — no duplicate warning here.
}

#[aristo::intent(
    "`aristo stamp --check` detects canon-matches drift the same way it \
     detects index drift: after the index sync check passes it dry-runs \
     the reconcile — the same live-id authority and skip conditions as \
     the write path, run on a CLONE of the loaded cache — and exits \
     non-zero, naming the affected ids, when the reconcile would change \
     .aristo/canon-matches.toml. Nothing is written. When the authority \
     is unreliable (unparseable explicit id, failed unexcluded walk) the \
     same skip note is printed and the check passes: the write path \
     would skip the reconcile too, so there is no drift a re-run could \
     fix — failing would be a --check false positive, which is worse \
     than a missed drift.",
    verify = "test",
    id = "stamp_check_detects_canon_matches_drift"
)]
fn check_canon_matches_drift(
    ws: &crate::Workspace,
    index: &IndexFile,
    discovered: &[aristo_core::walk::DiscoveredAnnotation],
) -> CliResult<()> {
    let cache_path = ws.canon_matches_path();
    let cache = CanonMatchesFile::read(&cache_path).map_err(|e| CliError::Other {
        message: format!("reading {}: {e}", cache_path.display()),
        exit_code: 1,
    })?;
    if cache.entries.is_empty() {
        // Nothing the reconcile could prune or demote — skip the
        // (possibly second) walk entirely, like the write path.
        return Ok(());
    }
    let Some(live_ids) = reconcile_live_ids(ws, index, discovered) else {
        // Authority unreliable — the skip note was already printed.
        // The write path would skip the reconcile for the same
        // reason, so there is no drift to fail on.
        return Ok(());
    };
    // Dry run on a clone: --check never writes.
    let mut probe = cache.clone();
    let report = aristo_core::canon::reconcile(&mut probe, &live_ids);
    if !report.changed() {
        return Ok(());
    }
    print_reconcile_notes(&report);
    let stale = report.pruned.len();
    let demotions = report.demoted.len();
    let stale_noun = if stale == 1 { "entry" } else { "entries" };
    let demotion_noun = if demotions == 1 {
        "demotion"
    } else {
        "demotions"
    };
    println!();
    Err(CliError::Other {
        message: format!(
            "canon-matches.toml is out of sync with source ({stale} stale {stale_noun}, \
             {demotions} {demotion_noun}). Run `aristo stamp` (without --check) to \
             reconcile, then commit."
        ),
        exit_code: 2,
    })
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
fn archive_orphan_proofs(
    ws: &crate::Workspace,
    entries: &std::collections::BTreeMap<AnnotationId, IndexEntry>,
) -> CliResult<()> {
    let proofs_dir = ws.aristo_dir().join("proofs");
    if !proofs_dir.is_dir() {
        return Ok(());
    }
    let archive_dir = ws.aristo_dir().join("archive").join("proofs");
    // Orphan = a `.proof` on disk whose decoded id is no longer in source
    // (set-difference of proof files minus walked entries). No prior committed
    // index is consulted — the source walk is the sole authority.
    for entry in fs::read_dir(&proofs_dir).map_err(CliError::Io)? {
        let from = entry.map_err(CliError::Io)?.path();
        if from.extension().and_then(|e| e.to_str()) != Some("proof") {
            continue;
        }
        let Some(stem) = from.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id_str = stem.replace("__", ":");
        let still_in_source = AnnotationId::parse(&id_str)
            .map(|id| entries.contains_key(&id))
            .unwrap_or(false);
        if still_in_source {
            continue;
        }
        fs::create_dir_all(&archive_dir).map_err(|e| CliError::Other {
            message: format!("creating proof archive {}: {e}", archive_dir.display()),
            exit_code: 1,
        })?;
        // Never clobber a previously-archived verdict for the same id (an id
        // can be orphaned more than once — removed, re-added, removed again).
        // Uniquify with a counter, keeping the `.proof` extension so `--gc`
        // still collects it. Upholds the ID-D5 promise: archiving never loses a
        // verdict.
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
            id_str,
            to.strip_prefix(&ws.root).unwrap_or(&to).display()
        );
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

// StampSummary / NotableChange / NotableKind were removed with
// merge_status_from_prev: status now comes from `.aristo/proofs/` via
// merge_status_from_proofs, and orphan archival is a set-difference, so no
// prior-index transition record is needed. See
// docs/decisions/index-as-local-cache.md.

#[aristo::intent(
    "Canon binding state is derived fresh from the canon-matches cache \
     on every stamp run, never carried over from a prior run — the cache \
     is the single source of truth. An entry whose id carries a canon \
     prefix and has an accepted match in the cache is marked bound to \
     that match. When the cached match omits the linked detail (an older \
     cache predating the field, or a server carve-out), a deterministic \
     placeholder is synthesized from the canon id and version, identical \
     to what an interactive accept would have written, so the binding \
     never depends on cache vintage. A canon-prefixed id with no cache \
     row is left unbound and reported as a diagnostic — an orphaned \
     binding is surfaced for the user to refresh or re-accept, never \
     silently treated as bound.",
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
    "Status is sourced from the stored proofs, never carried over from a \
     previously committed index. An entry keeps its proof's verdict only \
     while the proof's text and body anchors still match the entry's \
     current hashes; a drifted anchor demotes it to Stale, and an entry \
     with no proof stays Unknown. A second validation pass runs against a \
     snapshot that already carries the first pass's statuses, so the \
     refuted-sibling-ground guard can fire and demote a clean entry to \
     Stale. This ordering is load-bearing: running the guard while every \
     entry is still Unknown would let a proof launder its grounding \
     through a refuted sibling.",
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

fn print_status_summary(entries: &std::collections::BTreeMap<AnnotationId, IndexEntry>) {
    let mut fresh = 0usize;
    let mut stale = 0usize;
    let mut refuted = 0usize;
    let mut unverified = 0usize;
    let mut other = 0usize;
    for entry in entries.values() {
        match entry_status(entry) {
            s if s.is_terminal_clean() => fresh += 1,
            Status::Stale => stale += 1,
            Status::Counterexample => refuted += 1,
            Status::Unknown => unverified += 1,
            _ => other += 1,
        }
    }
    let tail = if other > 0 {
        format!(", other: {other}")
    } else {
        String::new()
    };
    println!(
        "  status (from .aristo/proofs/): fresh: {fresh}, stale: {stale}, refuted: {refuted}, unverified: {unverified}{tail}"
    );
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
            verification: None,
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
