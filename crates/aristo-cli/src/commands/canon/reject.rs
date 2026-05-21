//! `aristo canon reject <annotation_id> <canon_id> [--reason <text>]` —
//! move a pending canon match into the `rejected_matches[..]` bucket
//! so it doesn't re-surface on future stamps until the annotation
//! text changes. Closes Flow 7 from cli-sessions.md.
//!
//! Unlike [`super::accept`], rejection is a **cache-only** operation:
//! source bytes are not touched and the index entry is not modified.
//! The rejection is pinned by the annotation's current `text_hash`,
//! so once the user edits the annotation prose, the L5 invalidation
//! rules let the canon API re-evaluate the match against the new text.
//!
//! ## On-disk shape after reject
//!
//! ```toml
//! [my_local_invariant]
//! last_match_text_hash = "blake3:c2f7a912..."
//! canon_fetched_at     = "2026-06-13T10:15:00Z"
//!
//! [[my_local_invariant.rejected_matches]]
//! canon_id    = "some_unrelated_entry"
//! version     = "v0.1.2"
//! text_hash   = "blake3:c2f7a912..."     # PIN: lifts on text change
//! rejected_at = "2026-06-13T11:48:00Z"
//! reason      = "intentionally narrower than canon entry"   # optional
//! ```
//!
//! The `text_hash` here is the annotation's current text-hash from
//! `.aristo/index.toml::<ann_id>::text_hash` (NOT
//! `CacheEntry::last_match_text_hash`, which describes the match
//! batch and may have drifted slightly). The stamp runner consumes
//! the pin via `CacheEntry::is_rejected(canon_id, text_hash)` (cache
//! module, PR #4) when merging a fresh response — already wired.

use std::fs;

use aristo_core::canon::cache::{CanonMatchesFile, PendingMatch, RejectedMatch};
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile};

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult, Workspace};

/// Entry point invoked from `lib::dispatch`.
pub(crate) fn run(annotation_id: &str, canon_id: &str, reason: Option<String>) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let now = now_rfc3339();
    apply_rejection(&ws, annotation_id, canon_id, reason, &now)
}

/// Library-level orchestration — public to this crate so tests can
/// drive it without spawning a subprocess.
pub(crate) fn apply_rejection(
    ws: &Workspace,
    annotation_id_raw: &str,
    canon_id: &str,
    reason: Option<String>,
    now: &str,
) -> CliResult<()> {
    let ann_id = AnnotationId::parse(annotation_id_raw).map_err(|e| CliError::Other {
        message: format!(
            "annotation id `{annotation_id_raw}` is not valid ({e}).\n\
             Run `aristo list` to see indexed ids."
        ),
        exit_code: 2,
    })?;

    // Read the cache + locate the pending match.
    let cache_path = ws.canon_matches_path();
    let mut cache = CanonMatchesFile::read(&cache_path).map_err(CliError::Io)?;
    let pending = locate_pending(&cache, &ann_id, canon_id)?;

    // Read the index to recover the current text_hash. The pin uses
    // the index's value (not CacheEntry::last_match_text_hash) because
    // the index is the canonical source of truth for the annotation's
    // current text.
    let index = read_index(&ws.index_path())?;
    let intent = match index.entries.get(&ann_id) {
        Some(IndexEntry::Intent(e)) => e,
        Some(IndexEntry::Assume(_)) => {
            return Err(CliError::Other {
                message: format!(
                    "annotation id `{}` is an `assume`, not an `intent`. Canon \
                     matches only apply to intents — see the §13 design archive.",
                    ann_id.as_str()
                ),
                exit_code: 1,
            });
        }
        None => {
            return Err(CliError::Other {
                message: format!(
                    "annotation id `{}` not found in .aristo/index.toml.\n\
                     Run `aristo stamp` if you have just edited source.",
                    ann_id.as_str()
                ),
                exit_code: 1,
            });
        }
    };
    let text_hash = intent.text_hash.as_str().to_string();

    // Move pending → rejected.
    let rejected = RejectedMatch {
        canon_id: pending.canon_id.clone(),
        version: pending.version.clone(),
        text_hash,
        rejected_at: now.to_string(),
        reason,
    };
    let entry = cache
        .entries
        .get_mut(&ann_id)
        .expect("entry was located via pending lookup above");
    entry.pending_matches.retain(|m| m.canon_id != canon_id);
    entry.rejected_matches.push(rejected);

    cache.write_atomic(&cache_path).map_err(CliError::Io)?;

    println!(
        "ok: rejected canon match `{canon_id}` for `{}`. Match will not re-surface \
         until the annotation text changes.",
        ann_id.as_str()
    );
    Ok(())
}

fn locate_pending(
    cache: &CanonMatchesFile,
    ann_id: &AnnotationId,
    canon_id: &str,
) -> CliResult<PendingMatch> {
    let entry = cache.entries.get(ann_id).ok_or_else(|| CliError::Other {
        message: format!(
            "no pending canon matches for `{}` in .aristo/canon-matches.toml.\n\
             hint: run `aristo stamp` to refresh.",
            ann_id.as_str()
        ),
        exit_code: 1,
    })?;
    entry
        .pending_matches
        .iter()
        .find(|m| m.canon_id == canon_id)
        .cloned()
        .ok_or_else(|| CliError::Other {
            message: format!(
                "no pending canon match `{canon_id}` for annotation `{}` in \
                 .aristo/canon-matches.toml.\n\
                 hint: list pending matches with `aristo critique --apply-findings`.",
                ann_id.as_str()
            ),
            exit_code: 1,
        })
}

fn read_index(path: &std::path::Path) -> CliResult<IndexFile> {
    if !path.is_file() {
        return Err(CliError::Other {
            message: format!(
                "no .aristo/index.toml at {}\n\
                 hint: run `aristo stamp` to build one",
                path.display()
            ),
            exit_code: 2,
        });
    }
    let text = fs::read_to_string(path).map_err(CliError::Io)?;
    toml::from_str(&text).map_err(|e| CliError::Other {
        message: format!("parsing {}: {e}", path.display()),
        exit_code: 1,
    })
}

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is post-1970")
        .as_secs();
    crate::session::id_gen::format_rfc3339(secs)
}
