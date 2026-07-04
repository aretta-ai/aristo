//! Source-authoritative reconciliation of `.aristo/canon-matches.toml`.
//!
//! The cache file is committed by default and was never garbage
//! collected: deleting an annotation from source left its entry
//! lingering forever (real incident: a client repo removed an
//! `aristos:`-bound intent but the accepted binding stayed in
//! `canon-matches.toml`, so dashboards mirroring the committed file
//! kept showing a dead binding). [`reconcile`] makes the write-path
//! scan self-healing: given the live annotation-id set, it prunes
//! rows for removed annotations and demotes rows whose source id
//! lost its canon prefix. `aristo canon unbind` remains the way to
//! unbind a LIVE annotation.
//!
//! The live-id set must be a GC-grade authority: derive it with
//! [`raw_live_ids`] from a RAW, unexcluded source walk — never from
//! the post-validation index — so an annotation that was skipped
//! with a warning or filtered by `[index].exclude` still counts as
//! live. Pruning from a partial walk would destroy accepted bindings
//! for annotations that still exist in source.
//!
//! Pure functions — no I/O, unit-testable without a repo. Callers
//! (the `aristo stamp` write path) read the cache, reconcile, and
//! write it back only when [`ReconcileReport::changed`] says
//! something actually changed.

use std::collections::btree_map::Entry;
use std::collections::{BTreeSet, HashMap};

use crate::id::{deterministic_id, id_bucket_key};
use crate::index::AnnotationId;
use crate::walk::DiscoveredAnnotation;

use super::cache::{CacheEntry, CanonMatchesFile, PendingMatch, RejectedMatch};

/// What [`reconcile`] did (or would warn about). Every bucket is
/// sorted so callers can print deterministic summaries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Rows removed whole: their key (either key form — bare or
    /// canon-prefixed) matches no live annotation id.
    pub pruned: Vec<AnnotationId>,
    /// The subset of [`Self::pruned`] whose rows still carried
    /// `accepted_matches` — a pruned binding is worth a louder
    /// warning, since re-accepting is real work.
    pub pruned_bound: Vec<AnnotationId>,
    /// Live but unprefixed ids that lost their `accepted_matches`
    /// bucket (source says local, source wins). Covers both a live
    /// bare-keyed row that carried the bucket and a dead prefixed
    /// key rekeyed onto its live bare form; the rest of the row
    /// (pending + rejected memory) is kept either way.
    pub demoted: Vec<AnnotationId>,
    /// Live canon-prefixed ids with no cache row or no
    /// `accepted_matches` — no mutation; the caller may warn and
    /// suggest `aristo canon refresh`.
    pub missing_binding: Vec<AnnotationId>,
    /// Dead bare-id rows preserved instead of pruned because a
    /// pending match's canon-prefixed form IS live in source AND
    /// that prefixed id has no accepted row in the cache — the
    /// interrupted-`canon accept` recovery state (source was
    /// rewritten, the cache rekey never completed).
    pub preserved_for_recovery: Vec<AnnotationId>,
}

impl ReconcileReport {
    /// Did the reconcile mutate the cache? Only prune and demote
    /// mutate; warnings and preserved rows do not.
    pub fn changed(&self) -> bool {
        !(self.pruned.is_empty() && self.demoted.is_empty())
    }
}

/// The GC-authority live-id set, derived from RAW walk output —
/// deliberately BEFORE `build_entries` validation, see
/// [`raw_live_ids`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawLiveIds {
    /// Every id derivable from the walk: explicit ids that parse,
    /// plus the deterministic `aret_*` id of every idless annotation.
    pub ids: BTreeSet<AnnotationId>,
    /// How many annotations carried an explicit `id = "…"` that does
    /// not parse. Such an id cannot be represented in the set, so a
    /// caller that sees a nonzero count must NOT treat the set as
    /// authoritative for pruning (a typo'd id would otherwise turn
    /// into a destructive prune of the still-existing annotation's
    /// row).
    pub unparseable_explicit_ids: usize,
}

#[aristo::intent(
    "The reconcile's live-id authority is derived from the RAW walk, \
     before build_entries validation: an annotation skipped with a \
     warning (invalid parent id, invalid verify value, duplicate id) \
     still contributes its id, and an idless annotation contributes \
     the same deterministic aret_* id build_entries would mint \
     (same bucket key, same ordinal assignment). Treating \
     'skipped from the index' as 'deleted from source' would prune — \
     and destroy — the accepted bindings and rejected-match memory of \
     annotations that still exist. The one id class the set cannot \
     represent is an explicit id that fails to parse; those are \
     counted so the caller can skip reconciliation instead of \
     mispruning.",
    verify = "test",
    id = "raw_live_ids_counts_skipped_annotations_as_live"
)]
pub fn raw_live_ids(discovered: &[DiscoveredAnnotation]) -> RawLiveIds {
    let mut out = RawLiveIds::default();
    // Same ordinal bookkeeping as build_entries' resolve_id (ID-D2):
    // idless duplicates of one (kind, text, site) bucket mint
    // distinct source-order ordinals.
    let mut ordinal_counter: HashMap<(crate::index::AnnotationKind, String, String), usize> =
        HashMap::new();
    for d in discovered {
        match &d.annotation.id {
            Some(s) => match AnnotationId::parse(s) {
                Ok(id) => {
                    out.ids.insert(id);
                }
                Err(_) => out.unparseable_explicit_ids += 1,
            },
            None => {
                let key = id_bucket_key(d.annotation.kind, &d.annotation.text, &d.annotation.site);
                let ordinal = ordinal_counter.entry(key).or_insert(0);
                out.ids.insert(deterministic_id(
                    d.annotation.kind,
                    &d.annotation.text,
                    &d.annotation.site,
                    *ordinal,
                ));
                *ordinal += 1;
            }
        }
    }
    out
}

#[aristo::intent(
    "Reconciliation is source-authoritative and idempotent: the live-id \
     set from a fresh source walk is the sole authority. A cache row \
     whose key (either key form, bare or canon-prefixed) matches no \
     live annotation id is removed whole; a live but unprefixed id \
     that still carries accepted_matches loses exactly that bucket \
     (source says local, source wins). Two refinements keep memory \
     the live set says is still meaningful. A dead canon-prefixed key \
     whose BARE form is live is a demotion, not a removal: the row is \
     rekeyed under the bare id with accepted_matches dropped and \
     rejected-match memory preserved. And a dead bare id is preserved \
     untouched only while a pending match's canon-prefixed form is \
     live in source AND that prefixed id has no accepted row in the \
     cache — the interrupted-accept window, where pruning would \
     destroy the pending entry a re-run `aristo canon accept` needs \
     to finish the rekey. Once the prefixed row carries an accepted \
     match the binding completed (via this or another annotation) and \
     the dead bare row is ordinary garbage. `__meta__` is never \
     touched, and a second run over the same inputs reports no \
     changes.",
    verify = "test",
    id = "canon_reconcile_is_source_authoritative_and_idempotent"
)]
pub fn reconcile(
    cache: &mut CanonMatchesFile,
    live_ids: &BTreeSet<AnnotationId>,
) -> ReconcileReport {
    let mut report = ReconcileReport::default();

    // Rule 1 — dead keys: any row whose key matches no live
    // annotation id is pruned whole (same set-difference pattern as
    // orphan-proof archival: the source walk is the sole authority),
    // except the demote-rekey and interrupted-accept cases, see the
    // intent above.
    let dead: Vec<AnnotationId> = cache
        .entries
        .keys()
        .filter(|id| !live_ids.contains(*id))
        .cloned()
        .collect();
    for id in dead {
        // Rule 1a — DEMOTE-REKEY: a dead PREFIXED key whose bare form
        // is live means the source id lost its canon prefix (hand
        // edit, or a source-only revert after an accept). Nothing was
        // removed from source — rekey the row under the live bare id,
        // dropping only the accepted bucket, so the pending/rejected
        // memory the demote rule promises to keep survives.
        if id.is_canon_bound() {
            if let Some(bare) = bare_form(&id).filter(|b| live_ids.contains(b)) {
                let mut row = cache
                    .entries
                    .remove(&id)
                    .expect("dead key was collected from this map");
                row.accepted_matches.clear();
                match cache.entries.entry(bare.clone()) {
                    Entry::Vacant(slot) => {
                        slot.insert(row);
                    }
                    // A live bare row already exists (e.g. the
                    // leftover shell accept leaves behind): it is
                    // authoritative for the current text, so keep it
                    // and fold in only the prefixed row's
                    // rejected-match memory.
                    Entry::Occupied(mut slot) => {
                        merge_rejected(&mut slot.get_mut().rejected_matches, row.rejected_matches);
                    }
                }
                report.demoted.push(bare);
                continue;
            }
        }
        // Rule 1b — PRESERVE: the interrupted-accept recovery state.
        let is_recovery_row = !id.is_canon_bound()
            && cache
                .entries
                .get(&id)
                .is_some_and(|entry| pending_rekey_awaits_completion(entry, cache, live_ids));
        if is_recovery_row {
            report.preserved_for_recovery.push(id);
            continue;
        }
        // Rule 1c — PRUNE.
        let row = cache
            .entries
            .remove(&id)
            .expect("dead key was collected from this map");
        if !row.accepted_matches.is_empty() {
            report.pruned_bound.push(id.clone());
        }
        report.pruned.push(id);
    }

    // Rule 2 — DEMOTE: a live, unprefixed id whose row still carries
    // accepted_matches. Source says local, source wins: drop the
    // accepted bucket, keep the rest of the row (pending + rejected
    // memory survive).
    for (id, entry) in cache.entries.iter_mut() {
        if !id.is_canon_bound() && live_ids.contains(id) && !entry.accepted_matches.is_empty() {
            entry.accepted_matches.clear();
            report.demoted.push(id.clone());
        }
    }
    // Rule 1a pushes bare forms out of key order, and 1a + 2 can both
    // report the same id (rekey onto a bare row that itself carried
    // an accepted bucket) — sort + dedupe for deterministic output.
    report.demoted.sort();
    report.demoted.dedup();

    // Rule 3 — WARN (no mutation): a live canon-prefixed id with no
    // cache row or no accepted_matches cannot derive its binding.
    // Collected for the caller to warn on; reconcile never fetches
    // (scans stay offline-safe).
    for id in live_ids {
        if !id.is_canon_bound() {
            continue;
        }
        let has_accepted = cache
            .entries
            .get(id)
            .is_some_and(|e| !e.accepted_matches.is_empty());
        if !has_accepted {
            report.missing_binding.push(id.clone());
        }
    }

    report
}

/// Does any of this row's pending matches rekey to a canon-prefixed
/// id that is live in source and NOT yet bound in the cache? True
/// means the row is the documented interrupted-`canon accept`
/// transient (source→index→cache ordering died after the source
/// rewrite, so the rekeyed row never gained its accepted match) and
/// must be preserved for the re-run to complete. A live prefixed
/// target that already carries an accepted row means the binding
/// completed — via this annotation or another one matched to the
/// same canon entry — and preserving would make the dead bare row
/// immortal.
fn pending_rekey_awaits_completion(
    entry: &CacheEntry,
    cache: &CanonMatchesFile,
    live_ids: &BTreeSet<AnnotationId>,
) -> bool {
    entry.pending_matches.iter().any(|p| {
        pending_prefixed_form(p).is_some_and(|pid| {
            live_ids.contains(&pid)
                && cache
                    .entries
                    .get(&pid)
                    .is_none_or(|e| e.accepted_matches.is_empty())
        })
    })
}

/// The canon-prefixed id this pending match would rekey the row to
/// on accept (`aristos:<canon_id>` / `kanon:<canon_id>` per tier).
fn pending_prefixed_form(p: &PendingMatch) -> Option<AnnotationId> {
    AnnotationId::parse(&format!("{}{}", p.prefix_tier.as_prefix(), p.canon_id)).ok()
}

/// The bare (prefix-stripped) form of a canon-prefixed id; `None`
/// for ids that carry no canon prefix.
fn bare_form(id: &AnnotationId) -> Option<AnnotationId> {
    let s = id.as_str();
    let body = s
        .strip_prefix("aristos:")
        .or_else(|| s.strip_prefix("kanon:"))?;
    AnnotationId::parse(body).ok()
}

/// Fold `from` into `into`, skipping entries whose
/// `(canon_id, text_hash)` rejection key is already present.
fn merge_rejected(into: &mut Vec<RejectedMatch>, from: Vec<RejectedMatch>) {
    for r in from {
        let dup = into
            .iter()
            .any(|x| x.canon_id == r.canon_id && x.text_hash == r.text_hash);
        if !dup {
            into.push(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::cache::{AcceptedMatch, Disposition};
    use super::super::types::PrefixTier;
    use super::*;
    use crate::index::AnnotationKind;

    fn aid(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap_or_else(|e| panic!("parse {s:?}: {e}"))
    }

    fn live(ids: &[&str]) -> BTreeSet<AnnotationId> {
        ids.iter().map(|s| aid(s)).collect()
    }

    fn pending(canon_id: &str, tier: PrefixTier) -> PendingMatch {
        PendingMatch {
            canon_id: canon_id.into(),
            version: "v0.2.1".into(),
            canonical_text: "canonical".into(),
            canon_version: "v0.2.0".into(),
            confidence: 0.92,
            prefix_tier: tier,
            backed_by: None,
            linked: None,
            verification: None,
            disposition: Disposition::Open,
            found_at: "2026-06-15T09:14:22Z".into(),
            found_by: "aristo stamp".into(),
        }
    }

    fn accepted(canon_id: &str) -> AcceptedMatch {
        AcceptedMatch {
            canon_id: canon_id.into(),
            version: "v0.2.1".into(),
            canonical_text: "canonical".into(),
            canon_version: "v0.2.0".into(),
            confidence: 1.0,
            prefix_tier: PrefixTier::Aristos,
            backed_by: None,
            linked: None,
            verification: None,
            accepted_at: "2026-06-15T09:20:00Z".into(),
            bound_at: "2026-06-15T09:20:00Z".into(),
        }
    }

    fn rejected(canon_id: &str) -> RejectedMatch {
        RejectedMatch {
            canon_id: canon_id.into(),
            version: "v0.1.2".into(),
            text_hash: "blake3:c2f7a912".into(),
            rejected_at: "2026-06-13T11:48:00Z".into(),
            reason: None,
        }
    }

    fn row(
        pending_matches: Vec<PendingMatch>,
        accepted_matches: Vec<AcceptedMatch>,
        rejected_matches: Vec<RejectedMatch>,
    ) -> CacheEntry {
        CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-15T09:14:22Z".into(),
            pending_matches,
            accepted_matches,
            rejected_matches,
        }
    }

    // ─── Rule 1: PRUNE ────────────────────────────────────────────────────

    #[test]
    fn prune_removes_bare_entry_for_removed_annotation() {
        let mut cache = CanonMatchesFile::default();
        cache
            .entries
            .insert(aid("deleted_one"), row(vec![], vec![], vec![]));
        cache
            .entries
            .insert(aid("still_here"), row(vec![], vec![], vec![]));

        let report = reconcile(&mut cache, &live(&["still_here"]));

        assert_eq!(report.pruned, vec![aid("deleted_one")]);
        assert!(report.pruned_bound.is_empty(), "no accepted bucket pruned");
        assert!(report.changed());
        assert!(!cache.entries.contains_key(&aid("deleted_one")));
        assert!(cache.entries.contains_key(&aid("still_here")));
    }

    #[test]
    fn prune_removes_prefixed_entry_for_removed_annotation() {
        // Both prefixed key forms — the turso lingering-binding case:
        // the aristos:-bound intent was deleted from source but the
        // accepted binding stayed in canon-matches.toml.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("aristos:wal_checkpoint_error_no_db_leak"),
            row(
                vec![],
                vec![accepted("wal_checkpoint_error_no_db_leak")],
                vec![],
            ),
        );
        cache.entries.insert(
            aid("kanon:checkout_total_non_negative"),
            row(
                vec![],
                vec![accepted("checkout_total_non_negative")],
                vec![],
            ),
        );

        let report = reconcile(&mut cache, &live(&["unrelated_live_id"]));

        assert_eq!(
            report.pruned,
            vec![
                aid("aristos:wal_checkpoint_error_no_db_leak"),
                aid("kanon:checkout_total_non_negative"),
            ]
        );
        // Both rows carried accepted bindings — flagged for the
        // caller's louder warning.
        assert_eq!(report.pruned_bound, report.pruned);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn prune_preserves_interrupted_accept_recovery_row() {
        // accept's ordering contract is source→index→cache. If accept
        // dies after the source rewrite, source holds `aristos:x`
        // while the cache still holds the pending match under the OLD
        // BARE id (and no accepted row exists under `aristos:x` yet)
        // — the re-run needs that row; do not prune it.
        let mut cache = CanonMatchesFile::default();
        let mut p = pending("cell_write_once", PrefixTier::Aristos);
        p.disposition = Disposition::Accepted; // the documented transient
        cache
            .entries
            .insert(aid("edit_page_invariant"), row(vec![p], vec![], vec![]));

        let report = reconcile(&mut cache, &live(&["aristos:cell_write_once"]));

        assert!(report.pruned.is_empty());
        assert_eq!(
            report.preserved_for_recovery,
            vec![aid("edit_page_invariant")]
        );
        assert!(!report.changed(), "preservation must not force a write");
        assert!(cache.entries.contains_key(&aid("edit_page_invariant")));
    }

    #[test]
    fn prune_preserves_recovery_row_when_prefixed_row_exists_without_accepted() {
        // Boundary of the recovery window: the prefixed target's row
        // exists (e.g. pending-only) but carries no accepted match —
        // the rekey has not completed, keep waiting.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("edit_page_invariant"),
            row(
                vec![pending("cell_write_once", PrefixTier::Aristos)],
                vec![],
                vec![],
            ),
        );
        cache
            .entries
            .insert(aid("aristos:cell_write_once"), row(vec![], vec![], vec![]));

        let report = reconcile(&mut cache, &live(&["aristos:cell_write_once"]));

        assert_eq!(
            report.preserved_for_recovery,
            vec![aid("edit_page_invariant")]
        );
        assert!(report.pruned.is_empty());
    }

    #[test]
    fn prune_does_not_preserve_dead_row_whose_rekey_target_is_already_bound() {
        // Two annotations matched the same popular canon entry; B
        // accepted (so `aristos:cell_write_once` is live AND carries
        // an accepted row), then A was genuinely deleted. A's dead
        // bare row is NOT an interrupted accept — the binding
        // completed via B — so it must be pruned, not preserved
        // forever.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("deleted_ann"),
            row(
                vec![pending("cell_write_once", PrefixTier::Aristos)],
                vec![],
                vec![],
            ),
        );
        cache.entries.insert(
            aid("aristos:cell_write_once"),
            row(vec![], vec![accepted("cell_write_once")], vec![]),
        );

        let report = reconcile(&mut cache, &live(&["aristos:cell_write_once"]));

        assert_eq!(report.pruned, vec![aid("deleted_ann")]);
        assert!(report.preserved_for_recovery.is_empty());
        assert!(!cache.entries.contains_key(&aid("deleted_ann")));
        assert!(
            cache.entries.contains_key(&aid("aristos:cell_write_once")),
            "the completed binding is untouched"
        );
    }

    #[test]
    fn prune_does_not_preserve_dead_row_whose_rekey_target_is_not_live() {
        // A dead bare row with pending matches whose prefixed forms
        // are NOT in source is ordinary garbage — prune it.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("gone_annotation"),
            row(
                vec![pending("some_canon_entry", PrefixTier::Kanon)],
                vec![],
                vec![],
            ),
        );

        let report = reconcile(&mut cache, &live(&["unrelated_live_id"]));

        assert_eq!(report.pruned, vec![aid("gone_annotation")]);
        assert!(report.preserved_for_recovery.is_empty());
    }

    // ─── Rule 2: DEMOTE ───────────────────────────────────────────────────

    #[test]
    fn demote_drops_accepted_matches_on_live_unprefixed_id() {
        // Source says local (no aristos:/kanon: prefix) — source
        // wins: the accepted bucket goes, everything else stays.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("hand_stripped"),
            row(
                vec![pending("other_entry", PrefixTier::Aristos)],
                vec![accepted("stripped_entry")],
                vec![rejected("rejected_entry")],
            ),
        );

        let report = reconcile(&mut cache, &live(&["hand_stripped"]));

        assert_eq!(report.demoted, vec![aid("hand_stripped")]);
        assert!(report.changed());
        let entry = &cache.entries[&aid("hand_stripped")];
        assert!(entry.accepted_matches.is_empty(), "accepted dropped");
        assert_eq!(entry.pending_matches.len(), 1, "pending kept");
        assert_eq!(entry.rejected_matches.len(), 1, "rejection memory kept");
        assert_eq!(entry.last_match_text_hash, "blake3:x", "hash kept");
    }

    #[test]
    fn demote_rekeys_dead_prefixed_row_onto_live_bare_form() {
        // The realistic post-accept demotion: accept keyed the row
        // under the PREFIXED id; the user then hand-stripped the
        // prefix in source (or reverted src/ without .aristo/). The
        // dead prefixed key must not be reported as a removed
        // annotation — it is a demotion, and the rejected-match
        // memory rekeys onto the live bare id.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("aristos:cell_write_once"),
            row(
                vec![],
                vec![accepted("cell_write_once")],
                vec![rejected("other_entry")],
            ),
        );

        let report = reconcile(&mut cache, &live(&["cell_write_once"]));

        assert!(report.pruned.is_empty(), "nothing was removed from source");
        assert_eq!(report.demoted, vec![aid("cell_write_once")]);
        assert!(report.changed());
        assert!(!cache.entries.contains_key(&aid("aristos:cell_write_once")));
        let entry = &cache.entries[&aid("cell_write_once")];
        assert!(entry.accepted_matches.is_empty(), "accepted dropped");
        assert_eq!(
            entry.rejected_matches.len(),
            1,
            "rejection memory rekeyed onto the bare id"
        );
    }

    #[test]
    fn demote_rekey_merges_rejected_memory_into_existing_bare_row() {
        // Both key forms exist (accept leaves a bare leftover shell
        // behind): the live bare row is authoritative; the dead
        // prefixed row folds its rejected memory in and disappears.
        // The id is reported as demoted exactly once.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("cell_write_once"),
            row(
                vec![pending("other_entry", PrefixTier::Kanon)],
                vec![],
                vec![rejected("already_here")],
            ),
        );
        cache.entries.insert(
            aid("aristos:cell_write_once"),
            row(
                vec![],
                vec![accepted("cell_write_once")],
                vec![rejected("already_here"), rejected("only_in_prefixed")],
            ),
        );

        let report = reconcile(&mut cache, &live(&["cell_write_once"]));

        assert_eq!(report.demoted, vec![aid("cell_write_once")]);
        assert!(report.pruned.is_empty());
        assert!(!cache.entries.contains_key(&aid("aristos:cell_write_once")));
        let entry = &cache.entries[&aid("cell_write_once")];
        assert_eq!(entry.pending_matches.len(), 1, "live row's pending kept");
        let mut names: Vec<&str> = entry
            .rejected_matches
            .iter()
            .map(|r| r.canon_id.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec!["already_here", "only_in_prefixed"],
            "rejected memory merged without duplicates"
        );
    }

    #[test]
    fn demote_ignores_live_prefixed_id_with_accepted_matches() {
        // Steady state after accept: prefixed id live in source,
        // accepted match in cache. Reconcile must not touch it.
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("aristos:cell_write_once"),
            row(vec![], vec![accepted("cell_write_once")], vec![]),
        );

        let report = reconcile(&mut cache, &live(&["aristos:cell_write_once"]));

        assert!(!report.changed());
        assert_eq!(
            cache.entries[&aid("aristos:cell_write_once")]
                .accepted_matches
                .len(),
            1
        );
    }

    // ─── Rule 3: WARN (no mutation) ───────────────────────────────────────

    #[test]
    fn warn_collects_live_prefixed_ids_without_a_derivable_binding() {
        let mut cache = CanonMatchesFile::default();
        // Row exists but has no accepted_matches.
        cache.entries.insert(
            aid("kanon:row_without_accepted"),
            row(vec![], vec![], vec![]),
        );
        // aristos:no_row_at_all has no cache row.
        // aristos:fully_bound is healthy.
        cache.entries.insert(
            aid("aristos:fully_bound"),
            row(vec![], vec![accepted("fully_bound")], vec![]),
        );

        let report = reconcile(
            &mut cache,
            &live(&[
                "aristos:no_row_at_all",
                "kanon:row_without_accepted",
                "aristos:fully_bound",
            ]),
        );

        assert_eq!(
            report.missing_binding,
            vec![
                aid("aristos:no_row_at_all"),
                aid("kanon:row_without_accepted")
            ]
        );
        assert!(!report.changed(), "warnings never mutate the cache");
        assert!(
            cache
                .entries
                .contains_key(&aid("kanon:row_without_accepted")),
            "the warned row is a legitimate state, never pruned"
        );
    }

    // ─── Idempotence + __meta__ + empties ─────────────────────────────────

    #[test]
    fn reconcile_is_idempotent() {
        let mut cache = CanonMatchesFile::default();
        cache
            .entries
            .insert(aid("deleted_one"), row(vec![], vec![], vec![]));
        cache.entries.insert(
            aid("hand_stripped"),
            row(vec![], vec![accepted("stripped_entry")], vec![]),
        );
        cache.entries.insert(
            aid("aristos:rekeyed_demote"),
            row(vec![], vec![accepted("rekeyed_demote")], vec![]),
        );
        let live_ids = live(&[
            "hand_stripped",
            "rekeyed_demote",
            "aristos:orphaned_binding",
        ]);

        let first = reconcile(&mut cache, &live_ids);
        assert!(first.changed());
        let after_first = cache.clone();

        let second = reconcile(&mut cache, &live_ids);
        assert!(!second.changed(), "second run must report no changes");
        assert!(second.pruned.is_empty());
        assert!(second.demoted.is_empty());
        assert_eq!(cache, after_first, "second run must not mutate");
        // Warnings are stable across runs (they never mutate).
        assert_eq!(first.missing_binding, second.missing_binding);
    }

    #[test]
    fn meta_is_untouched_even_when_entries_are_pruned() {
        let mut cache = CanonMatchesFile::default();
        cache.meta.canon_version = Some("v0.2.0".into());
        cache.meta.last_fetched = Some("2026-06-15T09:14:22Z".into());
        cache
            .entries
            .insert(aid("deleted_one"), row(vec![], vec![], vec![]));

        let report = reconcile(&mut cache, &live(&[]));

        assert_eq!(report.pruned, vec![aid("deleted_one")]);
        assert_eq!(cache.meta.schema_version, 1);
        assert_eq!(cache.meta.canon_version.as_deref(), Some("v0.2.0"));
        assert_eq!(
            cache.meta.last_fetched.as_deref(),
            Some("2026-06-15T09:14:22Z")
        );
    }

    #[test]
    fn empty_cache_is_a_no_op() {
        let mut cache = CanonMatchesFile::default();
        let report = reconcile(&mut cache, &live(&["anything_live"]));
        assert_eq!(report, ReconcileReport::default());
        assert!(!report.changed());
    }

    #[test]
    fn empty_live_set_prunes_every_entry() {
        // Empty index (all annotations deleted): every row goes.
        let mut cache = CanonMatchesFile::default();
        cache
            .entries
            .insert(aid("bare_one"), row(vec![], vec![], vec![]));
        cache.entries.insert(
            aid("aristos:bound_one"),
            row(vec![], vec![accepted("bound_one")], vec![]),
        );

        let report = reconcile(&mut cache, &BTreeSet::new());

        assert_eq!(
            report.pruned,
            vec![aid("aristos:bound_one"), aid("bare_one")]
        );
        assert_eq!(report.pruned_bound, vec![aid("aristos:bound_one")]);
        assert!(cache.entries.is_empty());
    }

    // ─── raw_live_ids ─────────────────────────────────────────────────────

    fn discovered(
        explicit_id: Option<&str>,
        text: &str,
        site: &str,
        parent: Option<&str>,
    ) -> DiscoveredAnnotation {
        use crate::walk::{AnnotationForm, ExtractedAnnotation, ParentRaw};
        DiscoveredAnnotation {
            file: std::path::PathBuf::from("src/lib.rs"),
            annotation: ExtractedAnnotation {
                kind: AnnotationKind::Intent,
                form: AnnotationForm::Attribute,
                text: text.to_string(),
                verify: None,
                parent: parent.map(|p| ParentRaw::Single(p.to_string())),
                id: explicit_id.map(str::to_string),
                site: site.to_string(),
                line: 1,
                covered_region: crate::index::CoveredRegion::Function,
                text_hash: crate::hash::text_hash(text),
                body_hash: crate::hash::body_hash("fn body() {}"),
            },
        }
    }

    #[test]
    fn raw_live_ids_includes_annotations_build_entries_would_skip() {
        // A typo'd parent id makes build_entries skip the annotation
        // from the index — but the annotation still exists in source,
        // so its id MUST count as live (pruning its canon-matches row
        // would destroy an accepted binding over a warning).
        let d = discovered(
            Some("aristos:cell_write_once"),
            "some invariant",
            "fn foo",
            Some("Bad Parent Typo!!"),
        );
        let raw = raw_live_ids(&[d]);
        assert_eq!(raw.unparseable_explicit_ids, 0);
        assert!(raw.ids.contains(&aid("aristos:cell_write_once")));
    }

    #[test]
    fn raw_live_ids_derives_the_same_deterministic_id_as_build_entries() {
        // Idless annotations contribute the deterministic aret_* id,
        // with the same per-bucket ordinal assignment resolve_id
        // uses — including for identical duplicates.
        let a = discovered(None, "each cell written once", "fn edit_page", None);
        let b = discovered(None, "each cell written once", "fn edit_page", None);
        let raw = raw_live_ids(&[a, b]);
        let id0 = deterministic_id(
            AnnotationKind::Intent,
            "each cell written once",
            "fn edit_page",
            0,
        );
        let id1 = deterministic_id(
            AnnotationKind::Intent,
            "each cell written once",
            "fn edit_page",
            1,
        );
        assert!(raw.ids.contains(&id0));
        assert!(raw.ids.contains(&id1));
        assert_eq!(raw.ids.len(), 2);
    }

    #[test]
    fn raw_live_ids_counts_unparseable_explicit_ids() {
        // An explicit id that doesn't parse can't be represented in
        // the set — the caller must see the count and skip the
        // reconcile rather than prune on a lossy authority.
        let bad = discovered(Some("Not A Valid Id!"), "text", "fn foo", None);
        let good = discovered(Some("fine_id"), "text2", "fn bar", None);
        let raw = raw_live_ids(&[bad, good]);
        assert_eq!(raw.unparseable_explicit_ids, 1);
        assert_eq!(raw.ids.len(), 1);
        assert!(raw.ids.contains(&aid("fine_id")));
    }
}
