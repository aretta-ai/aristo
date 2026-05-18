//! Directory-backed task queue with atomic-claim semantics.
//!
//! Layout per pipeline:
//!
//! ```text
//! .aristo/<pipeline>-queue/
//!   pending/       <- tasks waiting to be claimed
//!     <id>.toml
//!   claimed/       <- tasks claimed by a worker; in-flight
//!     <id>.toml
//! ```
//!
//! Claim semantics use POSIX rename atomicity. Two workers calling
//! `pop_next` concurrently against the same pending entry are race-safe:
//! `fs::rename(pending/<id>.toml → claimed/<id>.toml)` either succeeds
//! (the worker holds the claim) or fails with `ENOENT` (another worker
//! got there first). The loser retries with another entry.
//!
//! Successfully submitted tasks are removed from `claimed/`. A worker
//! that crashes mid-task leaves its entry in `claimed/`; the reaper
//! (`reap_stale_claims`) moves entries older than a threshold back to
//! `pending/` for re-dispatch.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use aristo_core::index::AnnotationId;

use crate::commands::index::atomic_write;
use crate::{CliError, CliResult, Workspace};

/// Per-pipeline queue paths. Constructed once per command invocation;
/// holds no mutable state itself.
pub(crate) struct QueueDir {
    root: PathBuf,
}

impl QueueDir {
    /// Locate the queue for `<pipeline>` under `.aristo/`. Does NOT create
    /// the directories — callers that may write must call
    /// [`ensure_dirs`] first.
    pub(crate) fn for_pipeline(ws: &Workspace, pipeline: &str) -> Self {
        Self {
            root: ws.aristo_dir().join(format!("{pipeline}-queue")),
        }
    }

    #[allow(dead_code, reason = "scheduled use in future status/reap commands")]
    pub(crate) fn root(&self) -> &PathBuf {
        &self.root
    }

    pub(crate) fn pending_dir(&self) -> PathBuf {
        self.root.join("pending")
    }

    pub(crate) fn claimed_dir(&self) -> PathBuf {
        self.root.join("claimed")
    }

    /// Create `pending/` and `claimed/` if missing. Idempotent. Call
    /// before any writes; reads can skip this — a missing directory
    /// is treated as an empty queue.
    pub(crate) fn ensure_dirs(&self) -> CliResult<()> {
        fs::create_dir_all(self.pending_dir())?;
        fs::create_dir_all(self.claimed_dir())?;
        Ok(())
    }

    fn task_filename(id: &AnnotationId) -> String {
        // Same convention as `.aristo/proofs/<id>.proof`: `:` is illegal
        // in filenames on some hosts, so we use `__`.
        format!("{}.toml", id.as_str().replace(':', "__"))
    }

    pub(crate) fn pending_path(&self, id: &AnnotationId) -> PathBuf {
        self.pending_dir().join(Self::task_filename(id))
    }

    pub(crate) fn claimed_path(&self, id: &AnnotationId) -> PathBuf {
        self.claimed_dir().join(Self::task_filename(id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueueStatus {
    pub pending: usize,
    pub claimed: usize,
}

/// One claimed task: the path to the task file in `claimed/` and the
/// task content. Callers process the task and then call
/// [`submit_done`] (success) or [`requeue`] (retry).
#[derive(Debug)]
pub(crate) struct ClaimedTask {
    #[allow(
        dead_code,
        reason = "consumed by reaper / status commands in later slices"
    )]
    pub id: AnnotationId,
    #[allow(
        dead_code,
        reason = "consumed by reaper / status commands in later slices"
    )]
    pub path: PathBuf,
    pub content: String,
}

#[aristo::intent(
    "Each pipeline's queue lives at `.aristo/<pipeline>-queue/` — a sibling \
     directory per pipeline name. Verify and critique get distinct \
     subdirectories; a worker for one pipeline cannot accidentally claim \
     a task from the other. A refactor that consolidated to a single \
     shared queue would lose this isolation; per-pipeline workers would \
     need additional tagging at every pop site, and a mis-tag would \
     dispatch the wrong validator at submit time.",
    verify = "test",
    id = "queue_dir_is_per_pipeline_namespace"
)]
pub(crate) fn enqueue(qdir: &QueueDir, id: &AnnotationId, task_toml: &str) -> CliResult<()> {
    qdir.ensure_dirs()?;
    let path = qdir.pending_path(id);
    atomic_write(&path, task_toml)
}

#[aristo::intent(
    "`pop_next` atomically claims one pending entry by renaming \
     `pending/<id>.toml` → `claimed/<id>.toml`. Two workers racing on the \
     same entry cannot both succeed: POSIX rename guarantees the source \
     path disappears after the first call returns, so the loser sees \
     ENOENT and tries the next entry from a freshly-listed pending/. The \
     function returns Ok(None) ONLY when a fresh listing of pending/ \
     turns up empty (queue genuinely drained); a non-empty listing where \
     every entry was claimed by others triggers a re-list, not a None \
     return. A refactor that short-circuits to None on first ENOENT \
     would falsely report 'queue drained' under concurrent load.",
    verify = "neural",
    id = "pop_next_uses_atomic_rename_for_race_safety"
)]
#[aristo::assume(
    "POSIX rename(2) is atomic on a single local filesystem (APFS, ext4, \
     NTFS via Windows). On networked filesystems (NFS) atomicity is not \
     guaranteed by POSIX, and our race-safety claim does not hold there. \
     Aristo runs on a developer's local workspace; NFS-mounted .aristo/ \
     is out of scope.",
    id = "posix_rename_is_atomic_on_local_filesystem"
)]
#[aristo::assume(
    "The pending/ and claimed/ subdirectories share the same parent (the \
     queue root under `.aristo/<pipeline>-queue/`), so cross-filesystem \
     rename (EXDEV) is structurally impossible. A refactor that moved \
     claimed/ to a different filesystem (e.g., tmpfs) would break the \
     atomic claim by silently changing rename(2) semantics to copy+delete.",
    id = "pending_and_claimed_share_parent_filesystem"
)]
pub(crate) fn pop_next(qdir: &QueueDir) -> CliResult<Option<ClaimedTask>> {
    let pending = qdir.pending_dir();
    let claimed = qdir.claimed_dir();
    if !pending.is_dir() {
        return Ok(None);
    }
    qdir.ensure_dirs()?;
    loop {
        let read = match fs::read_dir(&pending) {
            Ok(r) => r,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut saw_any = false;
        for entry in read {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let src = entry.path();
            if !src.is_file() {
                continue;
            }
            saw_any = true;
            let Some(fname) = src.file_name() else {
                continue;
            };
            let dst = claimed.join(fname);
            match fs::rename(&src, &dst) {
                Ok(()) => {
                    let content = fs::read_to_string(&dst)?;
                    let id = parse_id_from_filename(fname.to_string_lossy().as_ref())?;
                    return Ok(Some(ClaimedTask {
                        id,
                        path: dst,
                        content,
                    }));
                }
                Err(e) if e.kind() == ErrorKind::NotFound => continue, // lost the race; try next
                Err(e) => return Err(e.into()),
            }
        }
        if !saw_any {
            return Ok(None);
        }
        // Every entry we saw was claimed by another worker mid-iteration.
        // Re-list — the queue may now be genuinely empty or may have grown
        // since the iterator snapshot.
    }
}

#[aristo::intent(
    "Once a task's artifact has been validated and written, `submit_done` \
     removes the entry from `claimed/`. A double-call is safe (idempotent: \
     NotFound is treated as success — the task is already done). A refactor \
     that errored on missing claimed file would make repeat submits visible \
     as failures when in fact the work landed cleanly.",
    verify = "test",
    id = "submit_done_is_idempotent_on_missing_claimed"
)]
pub(crate) fn submit_done(qdir: &QueueDir, id: &AnnotationId) -> CliResult<()> {
    let path = qdir.claimed_path(id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[aristo::intent(
    "`requeue` moves a claimed entry back to `pending/` so it can be \
     re-popped by the next available worker. Used by the stale-claim \
     reaper and by the submit path when a worker explicitly cancels. \
     Overwrites any existing `pending/<id>.toml` (which shouldn't exist \
     in normal flow but may if the reaper ran while another worker was \
     also re-enqueuing) — last-write-wins is acceptable because the \
     payload is the same task description, not per-attempt state.",
    verify = "test",
    id = "requeue_moves_claimed_back_to_pending"
)]
#[allow(
    dead_code,
    reason = "consumed by reap_stale_claims / submit cancel paths"
)]
pub(crate) fn requeue(qdir: &QueueDir, id: &AnnotationId) -> CliResult<()> {
    let src = qdir.claimed_path(id);
    if !src.is_file() {
        return Ok(());
    }
    qdir.ensure_dirs()?;
    let dst = qdir.pending_path(id);
    // fs::rename overwrites the destination on POSIX; that's what we want.
    fs::rename(&src, &dst).map_err(Into::into)
}

#[aristo::intent(
    "Stale-claim reaper: scan `claimed/` for entries whose mtime is older \
     than `max_age` and `requeue` them. Returns the list of ids that were \
     moved so the caller can log. A crashed worker leaves its claim \
     behind; without the reaper, that entry would block forever until a \
     human noticed and intervened. The threshold is a budget: too short \
     and slow but valid work gets stolen mid-execution; too long and \
     dead claims sit unprocessed. Callers (typically the verify/critique \
     skill at startup) pick the threshold per-pipeline based on expected \
     per-task latency.",
    verify = "test",
    id = "reap_stale_claims_recovers_crashed_workers"
)]
#[allow(
    dead_code,
    reason = "scheduled use at skill startup once worker-crash recovery lands"
)]
pub(crate) fn reap_stale_claims(
    qdir: &QueueDir,
    max_age: Duration,
) -> CliResult<Vec<AnnotationId>> {
    let claimed = qdir.claimed_dir();
    if !claimed.is_dir() {
        return Ok(Vec::new());
    }
    let now = SystemTime::now();
    let mut reaped = Vec::new();
    for entry in fs::read_dir(&claimed)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let mtime = entry.metadata()?.modified()?;
        let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
        if age < max_age {
            continue;
        }
        let Some(fname) = path.file_name() else {
            continue;
        };
        let id = parse_id_from_filename(fname.to_string_lossy().as_ref())?;
        requeue(qdir, &id)?;
        reaped.push(id);
    }
    Ok(reaped)
}

pub(crate) fn queue_status(qdir: &QueueDir) -> CliResult<QueueStatus> {
    Ok(QueueStatus {
        pending: count_files(&qdir.pending_dir())?,
        claimed: count_files(&qdir.claimed_dir())?,
    })
}

fn count_files(dir: &PathBuf) -> CliResult<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut n = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            n += 1;
        }
    }
    Ok(n)
}

fn parse_id_from_filename(fname: &str) -> CliResult<AnnotationId> {
    let stem = fname.strip_suffix(".toml").unwrap_or(fname);
    let id_str = stem.replace("__", ":");
    AnnotationId::parse(&id_str).map_err(|e| CliError::Other {
        message: format!("queue file {fname:?} is not a valid annotation id: {e}"),
        exit_code: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn fresh_qdir() -> (tempfile::TempDir, QueueDir) {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Workspace {
            root: tmp.path().to_path_buf(),
        };
        let qdir = QueueDir::for_pipeline(&ws, "verify");
        qdir.ensure_dirs().unwrap();
        (tmp, qdir)
    }

    fn id(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    #[test]
    fn enqueue_writes_to_pending() {
        let (_tmp, qdir) = fresh_qdir();
        enqueue(&qdir, &id("foo"), "x = 1").unwrap();
        assert!(qdir.pending_path(&id("foo")).is_file());
        assert_eq!(queue_status(&qdir).unwrap().pending, 1);
    }

    #[test]
    fn pop_next_returns_none_on_empty_queue() {
        let (_tmp, qdir) = fresh_qdir();
        assert!(pop_next(&qdir).unwrap().is_none());
    }

    #[test]
    fn pop_next_claims_one_then_drains() {
        let (_tmp, qdir) = fresh_qdir();
        enqueue(&qdir, &id("a"), "task = 'a'").unwrap();
        enqueue(&qdir, &id("b"), "task = 'b'").unwrap();

        let task = pop_next(&qdir).unwrap().unwrap();
        assert!(["a", "b"].contains(&task.id.as_str()));
        // Source file moved out of pending/, into claimed/
        assert!(!qdir.pending_path(&task.id).is_file());
        assert!(qdir.claimed_path(&task.id).is_file());

        let task2 = pop_next(&qdir).unwrap().unwrap();
        assert_ne!(task.id.as_str(), task2.id.as_str());

        // Queue now empty
        assert!(pop_next(&qdir).unwrap().is_none());
    }

    #[test]
    fn submit_done_removes_claimed_entry() {
        let (_tmp, qdir) = fresh_qdir();
        enqueue(&qdir, &id("foo"), "x").unwrap();
        let task = pop_next(&qdir).unwrap().unwrap();
        submit_done(&qdir, &task.id).unwrap();
        assert!(!qdir.claimed_path(&task.id).is_file());
        assert_eq!(queue_status(&qdir).unwrap().claimed, 0);
    }

    #[test]
    fn submit_done_is_idempotent() {
        let (_tmp, qdir) = fresh_qdir();
        submit_done(&qdir, &id("never_existed")).unwrap();
        submit_done(&qdir, &id("never_existed")).unwrap();
    }

    #[test]
    fn requeue_moves_claimed_back_to_pending() {
        let (_tmp, qdir) = fresh_qdir();
        enqueue(&qdir, &id("foo"), "x").unwrap();
        let task = pop_next(&qdir).unwrap().unwrap();
        requeue(&qdir, &task.id).unwrap();
        assert!(qdir.pending_path(&task.id).is_file());
        assert!(!qdir.claimed_path(&task.id).is_file());
    }

    #[test]
    fn race_safe_two_threads_claim_disjoint_entries() {
        // Spawn N OS threads against a queue of M entries; assert exactly
        // M distinct entries get claimed across all threads (no duplicates,
        // no losses). The race condition flagged in the design discussion.
        let (_tmp, qdir) = fresh_qdir();
        const M: usize = 50;
        const N: usize = 8;
        for i in 0..M {
            enqueue(&qdir, &id(&format!("e{i:03}")), "x").unwrap();
        }
        let qdir = Arc::new(qdir);
        let handles: Vec<_> = (0..N)
            .map(|_| {
                let qdir = qdir.clone();
                thread::spawn(move || {
                    let mut claimed = Vec::new();
                    while let Some(t) = pop_next(&qdir).unwrap() {
                        claimed.push(t.id.as_str().to_string());
                    }
                    claimed
                })
            })
            .collect();
        let mut all = Vec::new();
        for h in handles {
            all.extend(h.join().unwrap());
        }
        assert_eq!(all.len(), M, "expected {M} total claims, got {}", all.len());
        let mut dedup = all.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(
            dedup.len(),
            M,
            "duplicate claim detected — race-safety broken"
        );
    }

    #[test]
    fn reap_returns_empty_when_no_stale_claims() {
        let (_tmp, qdir) = fresh_qdir();
        enqueue(&qdir, &id("foo"), "x").unwrap();
        let _task = pop_next(&qdir).unwrap().unwrap();
        // Just claimed; not stale
        let reaped = reap_stale_claims(&qdir, Duration::from_secs(3600)).unwrap();
        assert!(reaped.is_empty());
    }

    #[test]
    fn reap_moves_old_claimed_back_to_pending() {
        let (_tmp, qdir) = fresh_qdir();
        enqueue(&qdir, &id("foo"), "x").unwrap();
        let _task = pop_next(&qdir).unwrap().unwrap();
        // Any positive age qualifies if we pass zero
        let reaped = reap_stale_claims(&qdir, Duration::ZERO).unwrap();
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].as_str(), "foo");
        assert!(qdir.pending_path(&id("foo")).is_file());
        assert!(!qdir.claimed_path(&id("foo")).is_file());
    }
}
