//! §17 proof-tree suggestions queue + `aristo canon suggestions`.
//!
//! The suggestions channel is **parallel** to `canon-matches.toml`: a
//! primary match keeps its existing accept/reject flow; the proof-tree
//! siblings dragged in alongside it are queued here for a separate
//! review pass (the `intent-review` session, Slice 3).
//!
//! ## Store
//!
//! The queue lives at `.aristo/canon-suggestions-queue/` and reuses the
//! generic atomic-claim [`pipeline::queue`](crate::pipeline::queue).
//! **One task = one cluster** (`{ objective?, siblings[] }`) — the
//! parent-reject cascade (D6) needs the cluster grouped, so the unit of
//! work is the cluster, not the individual sibling.
//!
//! The task is keyed by its **objective** canon_id when present (so
//! several primaries that roll up to the same objective collapse into
//! one task — "dedup ③"), else by the seeding primary's `for_canon_id`
//! (siblings-only / pre-0b mode).
//!
//! ## Read paths
//!
//! - `aristo canon suggestions` — list the queued clusters (read-only).
//! - `aristo canon suggestions <objective>` — show one cluster's
//!   detail.
//! - `aristo canon suggestions --counts` — machine-readable
//!   `{matches:{new,pending}, suggestions:{new,pending}}` for the
//!   menu/skill entry Q&A (§6A).

use serde::{Deserialize, Serialize};

use aristo_core::canon::cache::CanonMatchesFile;
use aristo_core::canon::{ClusterSuggestion, Disposition, PrefixTier, SuggestedEntry};
use aristo_core::index::AnnotationId;

use crate::commands::index::workspace_or_error;
use crate::filter::Filter;
use crate::pipeline::queue::{self, QueueDir};
use crate::session::types::ItemRef;
use crate::{CliError, CliResult, Workspace};

/// Pipeline name for the suggestions queue (under `.aristo/`).
pub(crate) const PIPELINE: &str = "canon-suggestions";

/// One queued proof-objective cluster awaiting review.
///
/// Mirrors a [`ClusterSuggestion`] from the wire, lifted into the
/// client store: the objective + siblings become [`SuggestedMatch`]es
/// (entry fields + a local [`Disposition`]), and the task records which
/// primaries dragged this cluster in (`for_canon_ids`, plural because
/// dedup ③ may collapse several into one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SuggestionTask {
    /// The primary matched canon_ids whose clusters collapsed into this
    /// task (dedup ③). At least one; more when several primaries in a
    /// batch share an objective.
    pub for_canon_ids: Vec<String>,
    /// The kanon: proof-objective parent. `None` in siblings-only mode
    /// (pre-Slice-0b — no objective entry authored yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<SuggestedMatch>,
    /// The co-member leaf suggestions, after dedup ② filtering.
    pub siblings: Vec<SuggestedMatch>,
    /// RFC 3339 timestamp the cluster was first queued.
    pub discovered_at: String,
}

/// A suggested canon entry in the queue — entry fields carried verbatim
/// from the wire [`SuggestedEntry`], plus a local review [`Disposition`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SuggestedMatch {
    pub canon_id: String,
    pub version: String,
    pub canonical_text: String,
    pub scope: String,
    pub prefix_tier: PrefixTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backed_by: Option<String>,
    /// Review state — `"open"` until the user decides in `intent-review`.
    pub disposition: Disposition,
}

impl SuggestedMatch {
    fn from_entry(entry: &SuggestedEntry) -> Self {
        Self {
            canon_id: entry.canon_id.clone(),
            version: entry.version.clone(),
            canonical_text: entry.canonical_text.clone(),
            scope: entry.scope.clone(),
            prefix_tier: entry.prefix_tier,
            backed_by: entry.backed_by.clone(),
            disposition: Disposition::Open,
        }
    }
}

impl SuggestionTask {
    /// The task's stable queue key: the objective canon_id when present,
    /// else the seeding primary's id. Dedup ③ collapses clusters that
    /// share an objective by sharing this key.
    pub(crate) fn key(&self) -> &str {
        match &self.objective {
            Some(obj) => &obj.canon_id,
            None => self
                .for_canon_ids
                .first()
                .map(String::as_str)
                .unwrap_or(""),
        }
    }

    /// Build a task from a wire [`ClusterSuggestion`]. Siblings are
    /// taken verbatim (dedup ② is applied by the caller before this).
    fn from_cluster(cluster: &ClusterSuggestion, siblings: &[SuggestedEntry], now: &str) -> Self {
        Self {
            for_canon_ids: vec![cluster.for_canon_id.clone()],
            objective: cluster.objective.as_ref().map(SuggestedMatch::from_entry),
            siblings: siblings.iter().map(SuggestedMatch::from_entry).collect(),
            discovered_at: now.to_string(),
        }
    }
}

// ─── Merge routing (dedup ②③) ─────────────────────────────────────────────

/// Set of canon_ids the client already "knows about" — used by dedup ②
/// to drop suggestions the user has already bound, surfaced, or rejected.
pub(crate) struct LocalState {
    /// canon_ids bound in the index (`aristos:`/`kanon:` entries).
    pub bound: std::collections::BTreeSet<String>,
    /// canon_ids pending or accepted in `canon-matches.toml`.
    pub in_cache: std::collections::BTreeSet<String>,
    /// canon_ids fingerprinted in the rejection log.
    pub rejected: std::collections::BTreeSet<String>,
}

impl LocalState {
    fn contains(&self, canon_id: &str) -> bool {
        self.bound.contains(canon_id)
            || self.in_cache.contains(canon_id)
            || self.rejected.contains(canon_id)
    }
}

/// Filter a wire cluster's siblings + objective through dedup ②: drop
/// any member already bound in the index, pending/accepted in the cache,
/// or in the rejection log. Returns the surviving sibling entries (the
/// objective is kept regardless — its adoption is decided in-session).
fn dedup_two<'a>(cluster: &'a ClusterSuggestion, local: &LocalState) -> Vec<&'a SuggestedEntry> {
    cluster
        .siblings
        .iter()
        .filter(|s| !local.contains(&s.canon_id))
        .collect()
}

/// Route a match response's `suggestions` into the queue, applying
/// dedup ② (vs local state) and dedup ③ (collapse-by-objective).
///
/// Returns the number of distinct cluster tasks written. A cluster whose
/// siblings are entirely consumed by dedup ② and that has no objective
/// is dropped (nothing actionable to review).
pub(crate) fn route_suggestions_into_queue(
    qdir: &QueueDir,
    suggestions: &[Option<ClusterSuggestion>],
    local: &LocalState,
    now: &str,
) -> CliResult<usize> {
    // Dedup ③: collapse clusters that share an objective into one task,
    // keyed by the task key (objective canon_id, else seeding primary).
    let mut tasks: std::collections::BTreeMap<String, SuggestionTask> =
        std::collections::BTreeMap::new();

    for cluster in suggestions.iter().flatten() {
        // Dedup ② — drop already-known siblings.
        let siblings: Vec<&SuggestedEntry> = dedup_two(cluster, local);
        // Nothing left to review for this cluster: no surviving siblings
        // and no objective to adopt.
        if siblings.is_empty() && cluster.objective.is_none() {
            continue;
        }

        let owned: Vec<SuggestedEntry> = siblings.into_iter().cloned().collect();
        let task = SuggestionTask::from_cluster(cluster, &owned, now);
        let key = task.key().to_string();

        match tasks.get_mut(&key) {
            Some(existing) => {
                // Collapse: merge the seeding primary + any new siblings.
                if !existing.for_canon_ids.contains(&cluster.for_canon_id) {
                    existing.for_canon_ids.push(cluster.for_canon_id.clone());
                }
                for s in task.siblings {
                    if !existing.siblings.iter().any(|e| e.canon_id == s.canon_id) {
                        existing.siblings.push(s);
                    }
                }
            }
            None => {
                tasks.insert(key, task);
            }
        }
    }

    qdir.ensure_dirs()?;
    let mut written = 0usize;
    for (key, task) in tasks {
        let id = AnnotationId::parse(&key).map_err(|e| CliError::Other {
            message: format!("suggestion cluster key `{key}` is not a valid id: {e}"),
            exit_code: 1,
        })?;
        let toml_text = toml::to_string_pretty(&task).map_err(|e| CliError::Other {
            message: format!("serialize suggestion task: {e}"),
            exit_code: 1,
        })?;
        queue::enqueue(qdir, &id, &toml_text)?;
        written += 1;
    }
    Ok(written)
}

/// Build [`LocalState`] from the workspace: index bindings, cache
/// pending/accepted, and the shared rejection log (intent-review kind).
pub(crate) fn local_state(ws: &Workspace, cache: &CanonMatchesFile) -> CliResult<LocalState> {
    use std::collections::BTreeSet;

    // Index-bound canon_ids — strip the prefix to get the bare canon_id.
    let mut bound = BTreeSet::new();
    let index_path = ws.index_path();
    if index_path.is_file() {
        let raw = std::fs::read_to_string(&index_path).map_err(CliError::Io)?;
        if let Ok(index) = toml::from_str::<aristo_core::index::IndexFile>(&raw) {
            for id in index.entries.keys() {
                if id.is_canon_bound() {
                    let bare = id
                        .as_str()
                        .strip_prefix("aristos:")
                        .or_else(|| id.as_str().strip_prefix("kanon:"))
                        .unwrap_or(id.as_str());
                    bound.insert(bare.to_string());
                }
            }
        }
    }

    // Cache pending + accepted canon_ids.
    let mut in_cache = BTreeSet::new();
    for entry in cache.entries.values() {
        for m in &entry.pending_matches {
            in_cache.insert(m.canon_id.clone());
        }
        for m in &entry.accepted_matches {
            in_cache.insert(m.canon_id.clone());
        }
    }

    // Rejection-log fingerprints for the intent-review kind. The
    // fingerprint is the bare canon_id (string).
    let mut rejected = BTreeSet::new();
    for entry in crate::session::rejections::read_for_kind(ws, INTENT_REVIEW_KIND)? {
        if let Some(canon_id) = entry.fingerprint.as_str() {
            rejected.insert(canon_id.to_string());
        }
    }

    Ok(LocalState {
        bound,
        in_cache,
        rejected,
    })
}

/// Session kind that owns the suggestions rejection fingerprints.
/// Matches the kind registered in Slice 3 (`session/kind.rs`).
pub(crate) const INTENT_REVIEW_KIND: &str = "intent-review";

// ─── Read commands ─────────────────────────────────────────────────────────

/// Counts surfaced at the menu / session entry (§6A, D9).
#[derive(Debug, Clone, PartialEq, Serialize)]
struct Counts {
    matches: Bucket,
    suggestions: Bucket,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Bucket {
    /// Items first surfaced this run (queued/pending, not yet reviewed).
    new: usize,
    /// Items carried over from prior runs (still open).
    pending: usize,
}

/// `aristo canon suggestions [<objective>] [--counts] [--filter ...]`.
pub(crate) fn run(objective: Option<String>, counts: bool, filter: Vec<Filter>) -> CliResult<()> {
    let ws = workspace_or_error()?;
    if counts {
        return run_counts(&ws);
    }
    match objective {
        Some(obj) => run_show(&ws, &obj),
        None => run_list(&ws, &filter),
    }
}

/// Strip an `aristos:` / `kanon:` prefix so a `--filter parent=<id>`
/// value (which may carry the tier prefix, e.g. the §6B `cluster
/// <objective>` recipe `parent=kanon:wal_protocol_correctness`) matches
/// the bare cluster key.
fn bare(id: &str) -> &str {
    id.strip_prefix("aristos:")
        .or_else(|| id.strip_prefix("kanon:"))
        .unwrap_or(id)
}

/// Does this cluster task pass every `--filter` clause? Only `parent=`
/// is meaningful for the suggestions queue — it scopes to one cluster by
/// objective key (the §6B `cluster <objective>` mode). Other keys
/// (`id=`/`file=`/`status=`) don't apply to a clusterized queue and pass
/// through (the seeding/match stages honor them). Clauses AND together.
fn task_passes_filter(task: &SuggestionTask, filter: &[Filter]) -> bool {
    filter.iter().all(|f| match f {
        Filter::Parent(p) => task.key() == bare(p),
        _ => true,
    })
}

fn read_all_tasks(ws: &Workspace) -> CliResult<Vec<SuggestionTask>> {
    Ok(read_all_tasks_with_paths(ws)?
        .into_iter()
        .map(|(t, _)| t)
        .collect())
}

/// Like [`read_all_tasks`] but pairs each task with its on-disk path, so
/// the intent-review session (Slice 3) can rewrite or remove a specific
/// cluster task after the parent-reject cascade (D6).
pub(crate) fn read_all_tasks_with_paths(
    ws: &Workspace,
) -> CliResult<Vec<(SuggestionTask, std::path::PathBuf)>> {
    let qdir = QueueDir::for_pipeline(ws, PIPELINE);
    let mut out = Vec::new();
    for dir in [qdir.pending_dir(), qdir.claimed_dir()] {
        if !dir.is_dir() {
            continue;
        }
        let mut paths: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        for p in paths {
            let raw = std::fs::read_to_string(&p).map_err(CliError::Io)?;
            let task: SuggestionTask = toml::from_str(&raw).map_err(|e| CliError::Other {
                message: format!("parse suggestion task {}: {e}", p.display()),
                exit_code: 1,
            })?;
            out.push((task, p));
        }
    }
    Ok(out)
}

/// Find one queued cluster task by its key (objective canon_id, else
/// seeding primary), returning the task and its on-disk path.
pub(crate) fn find_task_by_key(
    ws: &Workspace,
    key: &str,
) -> CliResult<Option<(SuggestionTask, std::path::PathBuf)>> {
    Ok(read_all_tasks_with_paths(ws)?
        .into_iter()
        .find(|(t, _)| t.key() == key))
}

/// True iff `canon_id` is held by an *independent* primary match or
/// binding — i.e. it appears bound in the index, or pending/accepted in
/// `canon-matches.toml`. This is the D6 guard: a dragged-in sibling that
/// the user has independently asserted (or accepted in Stage A) must NOT
/// be discarded when its cluster's parent is rejected.
pub(crate) fn member_independently_held(
    ws: &Workspace,
    cache: &CanonMatchesFile,
    canon_id: &str,
) -> CliResult<bool> {
    let local = local_state(ws, cache)?;
    // `bound` (index) and `in_cache` (pending/accepted) are the
    // "independently asserted" signals. `rejected` is NOT — a member the
    // user already rejected has no independent assertion to protect.
    Ok(local.bound.contains(canon_id) || local.in_cache.contains(canon_id))
}

fn run_list(ws: &Workspace, filter: &[Filter]) -> CliResult<()> {
    let tasks: Vec<SuggestionTask> = read_all_tasks(ws)?
        .into_iter()
        .filter(|t| task_passes_filter(t, filter))
        .collect();
    if tasks.is_empty() {
        println!(
            "ok: no proof-tree suggestions queued. Run `aristo stamp` (signed in) \
             to populate the queue."
        );
        return Ok(());
    }
    println!("proof-tree suggestions ({} cluster(s)):", tasks.len());
    for task in &tasks {
        let obj = task
            .objective
            .as_ref()
            .map(|o| o.canon_id.as_str())
            .unwrap_or("(siblings-only — no objective yet)");
        println!(
            "  {obj}  ({} sibling(s), for {})",
            task.siblings.len(),
            task.for_canon_ids.join(", ")
        );
    }
    println!();
    println!(
        "review with `aristo session start intent-review`, or inspect one with \
         `aristo canon suggestions <objective>`."
    );
    Ok(())
}

fn run_show(ws: &Workspace, objective: &str) -> CliResult<()> {
    let tasks = read_all_tasks(ws)?;
    let task = tasks
        .iter()
        .find(|t| t.key() == objective)
        .ok_or_else(|| CliError::Other {
            message: format!(
                "no queued suggestion cluster `{objective}`.\n\
                 hint: list clusters with `aristo canon suggestions`."
            ),
            exit_code: 1,
        })?;

    match &task.objective {
        Some(obj) => {
            println!("objective: {} {} ({} tier)", obj.canon_id, obj.version, obj.prefix_tier.as_prefix());
            println!("  {}", obj.canonical_text);
        }
        None => println!("objective: (siblings-only — no objective entry yet)"),
    }
    println!("dragged in by: {}", task.for_canon_ids.join(", "));
    println!("siblings ({}):", task.siblings.len());
    for s in &task.siblings {
        println!(
            "  {} {} ({} tier)",
            s.canon_id,
            s.version,
            s.prefix_tier.as_prefix()
        );
        println!("    {}", s.canonical_text);
        if let Some(backed_by) = &s.backed_by {
            println!("    backed by: {backed_by}");
        }
    }
    println!();
    println!("card detail for any entry: `aristo canon show <canon_id>`.");
    Ok(())
}

fn run_counts(ws: &Workspace) -> CliResult<()> {
    let cache_path = ws.canon_matches_path();
    let cache = CanonMatchesFile::read(&cache_path).map_err(CliError::Io)?;

    // Matches: pending = open primary matches in the cache. We don't
    // distinguish new-vs-carried here (no prior snapshot to diff
    // against in the read-only path), so everything open is "pending"
    // and "new" is 0 — Slice 3's session seeding does the new/carried
    // split. Reported here for the menu's at-a-glance count.
    let mut match_pending = 0usize;
    for entry in cache.entries.values() {
        for m in &entry.pending_matches {
            if matches!(m.disposition, Disposition::Open) {
                match_pending += 1;
            }
        }
    }

    let tasks = read_all_tasks(ws)?;
    let qdir = QueueDir::for_pipeline(ws, PIPELINE);
    let pending_in_queue = if qdir.pending_dir().is_dir() {
        std::fs::read_dir(qdir.pending_dir())?
            .filter_map(Result::ok)
            .filter(|e| e.path().is_file())
            .count()
    } else {
        0
    };
    let claimed_in_queue = tasks.len().saturating_sub(pending_in_queue);

    let counts = Counts {
        matches: Bucket {
            new: 0,
            pending: match_pending,
        },
        suggestions: Bucket {
            new: pending_in_queue,
            pending: claimed_in_queue,
        },
    };
    println!(
        "{}",
        serde_json::to_string(&counts).map_err(|e| CliError::Other {
            message: format!("serialize counts: {e}"),
            exit_code: 1,
        })?
    );
    Ok(())
}

/// Build a rejection-log fingerprint for a suggested canon entry. The
/// fingerprint is the bare canon_id (a JSON string); dedup ② / ④ match
/// on it. Co-located here so the producer (the intent-review reject
/// path) and the consumer ([`local_state`]) agree on the shape.
pub(crate) fn rejection_fingerprint(canon_id: &str) -> serde_json::Value {
    serde_json::Value::String(canon_id.to_string())
}

pub(crate) fn rejection_item_ref(canon_id: &str) -> ItemRef {
    ItemRef::from_opaque(canon_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::canon::{Relationship, SuggestedEntry, VerificationMetadata};
    use std::collections::BTreeSet;

    fn entry(canon_id: &str, rel: Relationship, tier: PrefixTier) -> SuggestedEntry {
        SuggestedEntry {
            canon_id: canon_id.into(),
            version: "v0.1.0".into(),
            canonical_text: format!("text for {canon_id}"),
            scope: "turso".into(),
            prefix_tier: tier,
            backed_by: match tier {
                PrefixTier::Aristos => Some("golden model + proofs".into()),
                PrefixTier::Kanon => None,
            },
            verification: VerificationMetadata::default(),
            relationship: rel,
        }
    }

    fn cluster(primary: &str, objective: Option<&str>, siblings: &[&str]) -> ClusterSuggestion {
        ClusterSuggestion {
            for_canon_id: primary.into(),
            objective: objective.map(|o| entry(o, Relationship::Parent, PrefixTier::Kanon)),
            siblings: siblings
                .iter()
                .map(|s| entry(s, Relationship::Sibling, PrefixTier::Aristos))
                .collect(),
        }
    }

    fn empty_local() -> LocalState {
        LocalState {
            bound: BTreeSet::new(),
            in_cache: BTreeSet::new(),
            rejected: BTreeSet::new(),
        }
    }

    fn fresh_qdir() -> (tempfile::TempDir, QueueDir) {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Workspace {
            root: tmp.path().to_path_buf(),
        };
        let qdir = QueueDir::for_pipeline(&ws, PIPELINE);
        (tmp, qdir)
    }

    fn read_queue(qdir: &QueueDir) -> Vec<SuggestionTask> {
        let mut out = Vec::new();
        let mut paths: Vec<_> = std::fs::read_dir(qdir.pending_dir())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        paths.sort();
        for p in paths {
            let raw = std::fs::read_to_string(p).unwrap();
            out.push(toml::from_str::<SuggestionTask>(&raw).unwrap());
        }
        out
    }

    #[test]
    fn route_writes_one_task_per_cluster() {
        let (_tmp, qdir) = fresh_qdir();
        let suggestions = vec![Some(cluster("p1", Some("obj_a"), &["s1", "s2"]))];
        let n =
            route_suggestions_into_queue(&qdir, &suggestions, &empty_local(), "2026-06-05T00:00:00Z")
                .unwrap();
        assert_eq!(n, 1);
        let tasks = read_queue(&qdir);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].objective.as_ref().unwrap().canon_id, "obj_a");
        assert_eq!(tasks[0].siblings.len(), 2);
        assert_eq!(tasks[0].for_canon_ids, vec!["p1"]);
    }

    #[test]
    fn dedup_two_drops_bound_pending_and_rejected_members() {
        let (_tmp, qdir) = fresh_qdir();
        let mut local = empty_local();
        local.bound.insert("s1".into()); // already bound in index
        local.in_cache.insert("s2".into()); // already pending/accepted
        local.rejected.insert("s3".into()); // in the rejection log
        let suggestions = vec![Some(cluster(
            "p1",
            Some("obj_a"),
            &["s1", "s2", "s3", "s4"],
        ))];
        route_suggestions_into_queue(&qdir, &suggestions, &local, "2026-06-05T00:00:00Z").unwrap();
        let tasks = read_queue(&qdir);
        assert_eq!(tasks.len(), 1);
        // Only s4 survives dedup ②.
        let surviving: Vec<&str> = tasks[0].siblings.iter().map(|s| s.canon_id.as_str()).collect();
        assert_eq!(surviving, vec!["s4"]);
    }

    #[test]
    fn dedup_three_collapses_clusters_sharing_an_objective() {
        let (_tmp, qdir) = fresh_qdir();
        // Two primaries (p1, p2) both roll up to obj_a, with disjoint
        // siblings → must collapse into ONE task.
        let suggestions = vec![
            Some(cluster("p1", Some("obj_a"), &["s1", "s2"])),
            Some(cluster("p2", Some("obj_a"), &["s2", "s3"])),
        ];
        let n =
            route_suggestions_into_queue(&qdir, &suggestions, &empty_local(), "2026-06-05T00:00:00Z")
                .unwrap();
        assert_eq!(n, 1, "two primaries, same objective → one task");
        let tasks = read_queue(&qdir);
        assert_eq!(tasks.len(), 1);
        let mut ids: Vec<&str> = tasks[0].siblings.iter().map(|s| s.canon_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["s1", "s2", "s3"], "siblings union, deduped");
        let mut primaries = tasks[0].for_canon_ids.clone();
        primaries.sort();
        assert_eq!(primaries, vec!["p1", "p2"]);
    }

    #[test]
    fn distinct_objectives_stay_separate_tasks() {
        let (_tmp, qdir) = fresh_qdir();
        let suggestions = vec![
            Some(cluster("p1", Some("obj_a"), &["s1"])),
            Some(cluster("p2", Some("obj_b"), &["s2"])),
        ];
        let n =
            route_suggestions_into_queue(&qdir, &suggestions, &empty_local(), "2026-06-05T00:00:00Z")
                .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn cluster_fully_consumed_by_dedup_and_no_objective_is_dropped() {
        let (_tmp, qdir) = fresh_qdir();
        let mut local = empty_local();
        local.bound.insert("s1".into());
        // siblings-only cluster (no objective); its single sibling is
        // already bound → nothing to review → dropped entirely.
        let suggestions = vec![Some(cluster("p1", None, &["s1"]))];
        let n =
            route_suggestions_into_queue(&qdir, &suggestions, &local, "2026-06-05T00:00:00Z")
                .unwrap();
        assert_eq!(n, 0);
        assert!(read_queue(&qdir).is_empty());
    }

    #[test]
    fn siblings_only_cluster_keyed_by_primary() {
        let (_tmp, qdir) = fresh_qdir();
        let suggestions = vec![Some(cluster("p1", None, &["s1"]))];
        route_suggestions_into_queue(&qdir, &suggestions, &empty_local(), "2026-06-05T00:00:00Z")
            .unwrap();
        let tasks = read_queue(&qdir);
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].objective.is_none());
        assert_eq!(tasks[0].key(), "p1");
    }

    #[test]
    fn null_cluster_entries_are_skipped() {
        let (_tmp, qdir) = fresh_qdir();
        let suggestions = vec![None, Some(cluster("p1", Some("obj_a"), &["s1"]))];
        let n =
            route_suggestions_into_queue(&qdir, &suggestions, &empty_local(), "2026-06-05T00:00:00Z")
                .unwrap();
        assert_eq!(n, 1);
    }
}
