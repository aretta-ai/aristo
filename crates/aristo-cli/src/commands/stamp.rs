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
use aristo_core::walk::walk_directory;

use crate::commands::index::{atomic_write, build_entries, now_rfc3339, workspace_or_error};
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
    let discovered = walk_directory(&ws.root).map_err(|e| CliError::Other {
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
    Ok(())
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
    TextChangedBodyHeld,
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
        } else if body_changed {
            summary.body_changed_count += 1;
            // Body drift: previously verified states become Stale.
            let next = match prev_status {
                Status::Verified | Status::Tested | Status::Neural => Status::Stale,
                other => other, // other prior states (Unknown, Stale, Orphan, etc.) carry through
            };
            set_status(new_entry, next);
            if matches!(
                prev_status,
                Status::Verified | Status::Tested | Status::Neural
            ) {
                summary.notable.push(NotableChange {
                    id: id.clone(),
                    kind: NotableKind::BodyDrifted {
                        old_status: prev_status,
                    },
                });
            }
        } else {
            // text changed, body held — preserve status (re-review concern,
            // not re-verify), surface a notable line so the user knows the
            // review cache should be invalidated.
            set_status(new_entry, prev_status);
            summary.text_changed_count += 1;
            summary.notable.push(NotableChange {
                id: id.clone(),
                kind: NotableKind::TextChangedBodyHeld,
            });
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
            NotableKind::TextChangedBodyHeld => {
                println!(
                    "  • {}: text changed, body unchanged — status held; review cache invalidated",
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
