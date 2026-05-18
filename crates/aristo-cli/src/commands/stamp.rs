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

use aristo_core::cycle::detect_cycles;
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Meta, Status};
use aristo_core::walk::walk_directory_with;

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
pub(crate) fn run(check: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;

    // Snapshot the previous index BEFORE writing — needed for status drift.
    let prev_index = read_existing_index(&ws.index_path())?;

    println!("→ Walking source from {} …", ws.root.display());
    let walk_opts = walk_options_from_workspace(&ws)?;
    let discovered = walk_directory_with(&ws.root, &walk_opts).map_err(|e| CliError::Other {
        message: format!("walk failed: {e}"),
        exit_code: 1,
    })?;
    println!("→ Found {} annotations", discovered.len());

    println!("→ Building index entries");
    let (mut entries, parents_map) = build_entries(&discovered, &ws.root)?;

    println!("→ Detecting cycles in parent graph");
    detect_cycles(&parents_map).map_err(|e| CliError::Other {
        message: format!("{e}\n\nNo files modified. Fix the cycle and re-run `aristo stamp`."),
        exit_code: 2,
    })?;

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

    println!();
    println!(
        "ok: stamped {} annotations into {}",
        index.entries.len(),
        ws.index_path()
            .strip_prefix(&ws.root)
            .unwrap_or(&ws.index_path())
            .display()
    );
    warn_on_counterexamples(&index);
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
     Treating counterexamples as quiet 'just a status' would let a \
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
