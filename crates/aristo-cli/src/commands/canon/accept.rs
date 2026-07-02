//! `aristo canon accept <annotation_id> <canon_id>` — apply a pending
//! canonicalize finding to source.
//!
//! The user has reviewed a pending canon match (surfaced by `aristo
//! stamp` or `aristo critique`) and wants to accept it. This command
//! atomically:
//!
//! 1. Rewrites the source attribute body via
//!    [`aristo_core::canon::rewrite::compute_rewrite`] — positional
//!    text → `text = "<canonical_text>"`, existing `id` (if any) →
//!    canon-prefixed form (`aristos:<canon_id>` or `kanon:<canon_id>`
//!    per the pending match's `prefix_tier`).
//! 2. Rewrites the index: rekeys the entry under the prefixed id,
//!    updates `text` to the canonical text, transitions the
//!    [`BindingState`] from `Local` to `Bound { linked }`.
//! 3. Moves the pending match from
//!    `canon-matches.toml::<ann_id>::pending_matches[..]` to
//!    `canon-matches.toml::<prefixed_id>::accepted_matches[..]`,
//!    recording `accepted_at` + `bound_at` timestamps. In Phase 1
//!    both are equal (accept-and-bind happen atomically); Phase 2
//!    splits them when verification execution lands.
//!
//! ## Ordering & recovery
//!
//! Mirrors `aristo rename`'s contract: source files first (each one
//! atomic temp-then-rename), then the index, then the cache. Each
//! write is individually atomic; the cross-step window is small but
//! non-zero. On partial failure:
//!
//! - **Source-write failed:** nothing else has changed; user can retry.
//! - **Index-write failed after source:** `aristo stamp` reconciles
//!   by discovering the canon-prefixed id in source and creating a
//!   fresh `BindingState::Local` entry. The user re-runs
//!   `aristo canon accept` — it picks up the still-pending cache
//!   entry and finishes the binding.
//! - **Cache-write failed after source+index:** the binding is now
//!   correct in source + index; the cache just lacks the
//!   accepted_matches entry. The next `aristo canon refresh` (PR #8)
//!   re-fetches.
//!
//! ## Phase-1 scope
//!
//! No verification execution. Per
//! `_deferred/verification-execution.md`, accept lands the binding
//! at `BindingState::Bound { linked }`; the `Certified` variant is
//! Phase 2. No `verified_outcome` write; no test-binary dispatch;
//! no signed certificate read-back.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use aristo_core::canon::{
    cache::{AcceptedMatch, CacheEntry, CanonMatchesFile, Disposition, PendingMatch},
    rewrite::{compute_rewrite, AcceptRewriteRequest, AttributeRewrite, RewriteError},
};
use aristo_core::index::{AnnotationId, ArtaId, BindingState, IndexEntry};
use aristo_core::walk::extract_from_source;

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult, Workspace};

/// Entry point invoked from `lib::dispatch`.
pub(crate) fn run(annotation_id: &str, canon_id: &str) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let now = now_rfc3339();
    apply_acceptance(&ws, annotation_id, canon_id, &now)
}

/// Library-level orchestration — public to this crate so tests can
/// drive it without spawning a subprocess.
pub(crate) fn apply_acceptance(
    ws: &Workspace,
    annotation_id_raw: &str,
    canon_id: &str,
    now: &str,
) -> CliResult<()> {
    // 1. Parse the annotation id.
    let ann_id = AnnotationId::parse(annotation_id_raw).map_err(|e| CliError::Other {
        message: format!(
            "annotation id `{annotation_id_raw}` is not valid ({e}).\n\
             Run `aristo list` to see indexed ids."
        ),
        exit_code: 2,
    })?;

    // 2. Read the cache + locate the pending match.
    let cache_path = ws.canon_matches_path();
    let mut cache = CanonMatchesFile::read(&cache_path).map_err(CliError::Io)?;
    let pending = locate_pending(&cache, &ann_id, canon_id)?;

    // 3. Read the index + validate the source entry.
    let index_path = ws.index_path();
    let mut index = crate::commands::show::load_index(ws)?;
    let entry = index.entries.get(&ann_id).ok_or_else(|| CliError::Other {
        message: format!(
            "annotation id `{}` not found in .aristo/index.toml.\n\
             Run `aristo stamp` if you have just edited source.",
            ann_id.as_str()
        ),
        exit_code: 1,
    })?;
    let intent = match entry {
        IndexEntry::Intent(e) => e.clone(),
        IndexEntry::Assume(_) => {
            return Err(CliError::Other {
                message: format!(
                    "annotation id `{}` is an `assume`, not an `intent`. Canon \
                     matches only apply to intents — see the §13 design archive.",
                    ann_id.as_str()
                ),
                exit_code: 1,
            });
        }
    };
    if !matches!(intent.binding, BindingState::Local) {
        return Err(CliError::Other {
            message: format!(
                "annotation `{}` is already canon-bound. Run `aristo canon unbind \
                 {}` first if you want to re-bind it.",
                ann_id.as_str(),
                ann_id.as_str()
            ),
            exit_code: 1,
        });
    }
    // Resolve the ArtaId for the binding. Phase 1 carve-out: the dev /
    // prod proxy may omit `linked` from /canon/match (see
    // canon::types::CanonMatch::linked rationale). When that happens
    // we synthesize a deterministic placeholder; Phase 2's
    // verified_outcome plumbing will reject placeholders and force
    // a rebind via a future migration step.
    let linked: ArtaId = match &pending.linked {
        Some(s) => ArtaId::parse(s).map_err(|e| CliError::Other {
            message: format!(
                "pending match's `linked` field `{s}` is not a valid arta_* id ({e}). \
                 The canon API returned a malformed identifier; rerun \
                 `aristo stamp` to refresh, or report the bug.",
            ),
            exit_code: 1,
        })?,
        None => aristo_core::canon::synthesize_phase1_linked(&pending.canon_id, &pending.version),
    };
    // Snapshot the user-facing form before `linked` is moved into the
    // BindingState below.
    let linked_str = linked.as_str().to_string();

    // 4. Compute the prefixed id + check it isn't already taken.
    let prefixed_str = match pending.prefix_tier {
        aristo_core::canon::PrefixTier::Aristos => format!("aristos:{canon_id}"),
        aristo_core::canon::PrefixTier::Kanon => format!("kanon:{canon_id}"),
    };
    let prefixed_id = AnnotationId::parse(&prefixed_str).map_err(|e| CliError::Other {
        message: format!("internal: failed to build prefixed annotation id `{prefixed_str}`: {e}"),
        exit_code: 1,
    })?;
    if prefixed_id != ann_id && index.entries.contains_key(&prefixed_id) {
        return Err(CliError::Other {
            message: format!(
                "id `{}` is already in the index — would collide with the canon \
                 binding for `{}`. Delete the conflicting annotation first.",
                prefixed_str,
                ann_id.as_str()
            ),
            exit_code: 1,
        });
    }

    // 5. Compute the source rewrite via the pure primitive.
    let item_line = parse_line_from_site(&intent.site).ok_or_else(|| CliError::Other {
        message: format!(
            "internal: cannot parse line number from index entry's site `{}` \
             — re-run `aristo stamp` to refresh.",
            intent.site
        ),
        exit_code: 1,
    })?;
    let src_path = ws.root.join(&intent.file);
    let source = fs::read_to_string(&src_path).map_err(|e| CliError::Other {
        message: format!(
            "cannot read `{}`: {e}\n\
             hint: the index references this file; run `aristo stamp` if you \
             have moved or removed it.",
            src_path.display()
        ),
        exit_code: 1,
    })?;
    let rewrite_request = AcceptRewriteRequest {
        item_line,
        canon_id: canon_id.to_string(),
        canonical_text: pending.canonical_text.clone(),
        prefix_tier: pending.prefix_tier,
    };
    let rewrite = compute_rewrite(&source, &rewrite_request).map_err(rewrite_error_to_cli)?;

    // 6. Apply source rewrite (atomic temp-then-rename).
    let new_source = splice_source(&source, &rewrite);
    atomic_write_bytes(&src_path, new_source.as_bytes())?;

    // 6b. Refresh sibling annotation sites in the affected file.
    //
    // The rewrite collapses the target annotation's attribute span
    // (e.g., multi-line → one-liner), which shifts the line numbers
    // of every annotation that follows it in the same file. Without
    // this refresh, the next `canon accept` (or any other operation
    // that looks up an annotation by `parse_line_from_site`) finds
    // a stale line and fails with "no attribute found at line N".
    //
    // Approach: walk the in-memory `new_source` (no second disk
    // read), build a map from each annotation's stripped site
    // (e.g., `"fn be_one"`) to its current line number, then update
    // every index entry that points at this file. The match uses
    // stripped site (function name + kind) which is unique per file
    // and stable under canon's id-prefix rewrites — `id =` changes
    // but the surrounding `fn name() {...}` doesn't.
    let fresh = extract_from_source(&new_source).map_err(|e| CliError::Other {
        message: format!(
            "internal: failed to re-walk `{}` after rewrite: {e}\n\
             The source file may be unparseable after the rewrite; report \
             this as a canon-accept bug.",
            src_path.display()
        ),
        exit_code: 1,
    })?;
    let mut line_by_site: BTreeMap<String, usize> = BTreeMap::new();
    for ann in &fresh {
        line_by_site.insert(ann.site.clone(), ann.line);
    }

    // 7. Rewrite the index entry.
    let mut new_intent = intent.clone();
    new_intent.text = pending.canonical_text.clone();
    new_intent.binding = BindingState::Bound { linked };
    // last_critiqued_at_text_hash + last_critique_finding_count are tied
    // to the old text; the canon binding has a new text, so clear them
    // so the next critique session re-runs against the canonical text.
    new_intent.last_critiqued_at_text_hash = None;
    new_intent.last_critique_finding_count = None;
    // Refresh the just-rewritten entry's own site too — even though
    // the attribute starts at the same line, the human-readable
    // "fn name (line N)" needs the post-shift N. (For a one-and-only
    // annotation at the top of the file the line stays put, but a
    // sibling above could have shifted it.)
    new_intent.site = refresh_site(&new_intent.site, &line_by_site);
    index.entries.remove(&ann_id);
    index
        .entries
        .insert(prefixed_id.clone(), IndexEntry::Intent(new_intent));

    // Refresh sibling annotations' sites in the same file.
    for (_id, entry) in index.entries.iter_mut() {
        let same_file = entry_file(entry) == intent.file;
        if !same_file {
            continue;
        }
        let new_site = refresh_site(entry_site(entry), &line_by_site);
        set_entry_site(entry, new_site);
    }

    let index_toml = toml::to_string_pretty(&index).map_err(|e| CliError::Other {
        message: format!("serializing .aristo/index.toml: {e}"),
        exit_code: 1,
    })?;
    atomic_write_bytes(&index_path, index_toml.as_bytes())?;

    // 8. Move the pending match → accepted_matches under the new id.
    //    Persist the (resolved or synthesized) `linked` ref on the
    //    accepted match — the cache is the authoritative store for
    //    the binding handle, and `aristo stamp` derives the index
    //    entry's BindingState::Bound from this field on every run.
    let accepted = AcceptedMatch {
        canon_id: pending.canon_id.clone(),
        version: pending.version.clone(),
        canonical_text: pending.canonical_text.clone(),
        canon_version: pending.canon_version.clone(),
        confidence: pending.confidence,
        prefix_tier: pending.prefix_tier,
        backed_by: pending.backed_by.clone(),
        linked: Some(linked_str.clone()),
        // P-008 carry (SLICE23-SPEC aristo item 2): the verification
        // metadata — coverage level, routed test binaries, and the
        // optional instrumentation bundle — survives accept. The
        // accepted_matches bucket is what the bundle union
        // (`aristo_core::canon::union_accepted_bundles`) and the S2
        // presence probe read. `None` for pre-P-008 pending rows.
        verification: pending.verification.clone(),
        accepted_at: now.to_string(),
        bound_at: now.to_string(),
    };
    remove_pending(&mut cache, &ann_id, canon_id);
    let bound_entry = cache
        .entries
        .entry(prefixed_id.clone())
        .or_insert_with(|| CacheEntry {
            last_match_text_hash: intent.text_hash.as_str().to_string(),
            canon_fetched_at: now.to_string(),
            pending_matches: vec![],
            accepted_matches: vec![],
            rejected_matches: vec![],
        });
    bound_entry.accepted_matches.push(accepted);
    cache.write_atomic(&cache_path).map_err(CliError::Io)?;

    // Per canon-strategy.md §CS10, the opaque `linked` ref is hidden
    // from user-facing surfaces. `linked_str` is still computed +
    // persisted in the index, but it doesn't appear in the success
    // message — Phase 1 it's a synthesized placeholder, Phase 2 it'll
    // be a server-issued handle, and neither is useful to surface to
    // the user during accept.
    let _ = linked_str;
    println!(
        "ok: accepted canon match for `{}` → `{}`.",
        ann_id.as_str(),
        prefixed_id.as_str(),
    );
    Ok(())
}

/// Given an existing `"<site> (line N)"` string and a fresh
/// `stripped_site -> line` map, return a new site string with the
/// post-rewrite line. Falls back to the original string when the
/// stripped site isn't in the map — covers the corner where a
/// sibling annotation is unparseable post-rewrite (shouldn't happen
/// but is harmless to no-op).
fn refresh_site(old: &str, line_by_site: &BTreeMap<String, usize>) -> String {
    // Site format is `"<stripped> (line N)"`. Strip the trailing
    // ` (line N)` to get the stripped site for map lookup.
    let stripped = match old.rfind(" (line ") {
        Some(idx) => &old[..idx],
        None => old,
    };
    match line_by_site.get(stripped) {
        Some(line) => format!("{stripped} (line {line})"),
        None => old.to_string(),
    }
}

/// Per-entry site accessor — index.rs has one for read; this is the
/// write side.
fn set_entry_site(entry: &mut IndexEntry, site: String) {
    match entry {
        IndexEntry::Intent(e) => e.site = site,
        IndexEntry::Assume(e) => e.site = site,
    }
}

/// Read accessor mirroring `set_entry_site` for the entry-site refresh.
fn entry_site(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.site,
        IndexEntry::Assume(e) => &e.site,
    }
}

/// Per-entry file accessor.
fn entry_file(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

/// Locate the pending match by `(annotation_id, canon_id)`. Returns a
/// clone so the caller can continue using the cache; the actual
/// removal happens later in [`remove_pending`].
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
    let mut candidates: Vec<&PendingMatch> = entry
        .pending_matches
        .iter()
        .filter(|m| m.canon_id == canon_id)
        .collect();
    // Multiple matches for the same canon_id should never happen (the
    // server returns one match per (annotation, canon_id) pair), but
    // be defensive: take the highest-confidence one and warn.
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let pending = candidates.first().ok_or_else(|| CliError::Other {
        message: format!(
            "no pending canon match `{canon_id}` for annotation `{}` in \
             .aristo/canon-matches.toml.\n\
             hint: list pending matches with `aristo critique --apply-findings`.",
            ann_id.as_str()
        ),
        exit_code: 1,
    })?;
    if matches!(pending.disposition, Disposition::Accepted) {
        // Idempotent: a second accept call is allowed only if the
        // index is still un-bound (i.e. the first call was interrupted
        // before the index write). For the simple case we just
        // continue and re-apply.
    }
    Ok((*pending).clone())
}

fn remove_pending(cache: &mut CanonMatchesFile, ann_id: &AnnotationId, canon_id: &str) {
    if let Some(entry) = cache.entries.get_mut(ann_id) {
        entry.pending_matches.retain(|m| m.canon_id != canon_id);
    }
}

fn rewrite_error_to_cli(e: RewriteError) -> CliError {
    CliError::Other {
        message: format!("source rewrite failed: {e}"),
        exit_code: 1,
    }
}

fn parse_line_from_site(site: &str) -> Option<usize> {
    let open = site.rfind("(line ")?;
    let after = &site[open + "(line ".len()..];
    let close = after.rfind(')')?;
    after[..close].trim().parse().ok()
}

fn splice_source(source: &str, rewrite: &AttributeRewrite) -> String {
    let mut bytes = source.as_bytes().to_vec();
    bytes.splice(
        rewrite.byte_start..rewrite.byte_end,
        rewrite.replacement.as_bytes().iter().copied(),
    );
    // Source rewrite preserves UTF-8 by construction (the replacement
    // is a valid Rust attribute body using only ASCII syntax + the
    // user-supplied text, which is already valid UTF-8).
    String::from_utf8(bytes).expect("rewrite preserves utf-8")
}

fn atomic_write_bytes(target: &Path, content: &[u8]) -> CliResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let tmp_name = match target.file_name() {
        Some(name) => {
            let mut s = name.to_os_string();
            s.push(".aristo-tmp");
            s
        }
        None => {
            return Err(CliError::Other {
                message: format!(
                    "canon accept: cannot atomic-write `{}` — path has no file name",
                    target.display()
                ),
                exit_code: 1,
            });
        }
    };
    let tmp = target.with_file_name(tmp_name);
    fs::write(&tmp, content).map_err(CliError::Io)?;
    fs::rename(&tmp, target).map_err(CliError::Io)?;
    Ok(())
}

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is post-1970")
        .as_secs();
    // Use the same formatter as the session substrate so timestamps
    // across the SDK look uniform.
    crate::session::id_gen::format_rfc3339(secs)
}
