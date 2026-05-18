//! Verify pipeline's per-entry task production.
//!
//! `aristo verify` (without `--apply-verdicts` / `--submit-verdict` /
//! `--pop-next`) iterates the index, decides which entries need neural
//! verification, and writes one task file per id under
//! `.aristo/verify-queue/pending/<id>.toml` via [`pipeline::queue::enqueue`].
//!
//! Workers (subagents spawned by `aristo-neural-verify`) call
//! `aristo verify --pop-next` to atomically claim one task at a time.
//! See [`crate::pipeline::queue`] for the directory layout + race-safety
//! guarantees.

use std::fs;

use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Sha256};
use aristo_core::proof::ProofFile;
use serde::{Deserialize, Serialize};

use crate::commands::verify::validator::MAX_REPAIR_ATTEMPTS;
use crate::pipeline::queue::{self, QueueDir};
use crate::{CliError, CliResult, Workspace};

/// The verify pipeline's name in `.aristo/<pipeline>-queue/`. Used by
/// every per-pipeline call site so a typo here doesn't fragment the queue
/// across siblings.
pub(crate) const PIPELINE_NAME: &str = "verify";

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct VerifyTask {
    pub id: String,
    pub text: String,
    pub file: String,
    pub site: String,
    pub text_hash: Sha256,
    pub body_hash: Sha256,
    /// Carried-over attempts count from a prior rejected `.proof` file
    /// for this id, if any. The skill orchestrator instructs subagents
    /// to emit `attempts = prior_attempts + 1`, so the validator's
    /// K-bounded repair budget actually accumulates across re-spawns
    /// rather than resetting to 1 every run.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prior_attempts: u32,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

#[aristo::intent(
    "Each pending verify task is a REQUEST from the SDK to the in-agent \
     skill, enqueued at `.aristo/verify-queue/pending/<id>.toml`. Workers \
     pop one at a time via `aristo verify --pop-next`. The SDK never reads \
     these task files back as verdicts — verdicts arrive via \
     `aristo verify --submit-verdict` and land at `.aristo/proofs/<id>.proof` \
     after the mechanical validator gates them. A refactor that has the \
     SDK auto-process its own queue (e.g., to call an LLM directly) would \
     conflate the CLI with the agent and break the design split: the CLI \
     never makes LLM calls; the agent never bypasses the SDK validator.",
    verify = "neural",
    id = "pending_neural_file_is_sdk_to_agent_request_not_a_result"
)]
pub(crate) fn enqueue_pending(
    ws: &Workspace,
    index: &IndexFile,
    ids: &[&AnnotationId],
) -> CliResult<usize> {
    let qdir = QueueDir::for_pipeline(ws, PIPELINE_NAME);
    qdir.ensure_dirs()?;
    let mut budget_exhausted: Vec<&AnnotationId> = Vec::new();
    let mut enqueued = 0usize;
    for id in ids {
        let Some(entry) = index.entries.get(*id) else {
            continue;
        };
        let prior_attempts = read_prior_attempts(ws, id);
        if prior_attempts >= MAX_REPAIR_ATTEMPTS {
            budget_exhausted.push(id);
            continue;
        }
        // Single-deep backup: move any existing .proof to .proof.bak so the
        // user can compare a rejected re-attempt against the prior verdict.
        // Overwrites any pre-existing .bak — the system tracks only the
        // most-recent prior attempt, not full history.
        backup_existing_proof(ws, id);
        let task = VerifyTask {
            id: id.as_str().to_string(),
            text: entry_text(entry).to_string(),
            file: entry_file(entry).to_string(),
            site: entry_site(entry).to_string(),
            text_hash: entry_text_hash(entry).clone(),
            body_hash: entry_body_hash(entry).clone(),
            prior_attempts,
        };
        let task_toml = toml::to_string_pretty(&task).map_err(|e| CliError::Other {
            message: format!("serializing verify-queue task {}: {e}", id.as_str()),
            exit_code: 1,
        })?;
        queue::enqueue(&qdir, id, &task_toml)?;
        enqueued += 1;
    }
    if !budget_exhausted.is_empty() {
        warn_budget_exhausted(&budget_exhausted);
    }
    Ok(enqueued)
}

#[aristo::intent(
    "If a legacy `.aristo/pending-neural.toml` (single-file format from \
     v0.0.6) is present, expand each entry into per-id queue files under \
     `.aristo/verify-queue/pending/` and delete the legacy file. Runs at \
     the start of every `aristo verify` invocation. Idempotent: a second \
     run with no legacy file is a no-op. Single-pass migration — there \
     is no compat shim that re-reads the legacy format on subsequent \
     runs.",
    verify = "test",
    id = "verify_migrates_legacy_pending_neural_toml_to_queue"
)]
pub(crate) fn migrate_legacy_pending_if_present(ws: &Workspace) -> CliResult<()> {
    let legacy_path = ws.aristo_dir().join("pending-neural.toml");
    if !legacy_path.is_file() {
        return Ok(());
    }
    let raw = fs::read_to_string(&legacy_path)?;
    #[derive(Deserialize)]
    struct LegacyFile {
        #[allow(dead_code)]
        schema_version: u32,
        pending: Vec<VerifyTask>,
    }
    let parsed: LegacyFile = match toml::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "warning: legacy {} failed to parse ({e}); deleting it. \
                 Run `aristo verify` again to re-populate the new queue.",
                legacy_path.display()
            );
            let _ = fs::remove_file(&legacy_path);
            return Ok(());
        }
    };
    let qdir = QueueDir::for_pipeline(ws, PIPELINE_NAME);
    qdir.ensure_dirs()?;
    for task in parsed.pending {
        let Ok(id) = AnnotationId::parse(&task.id) else {
            continue;
        };
        let task_toml = toml::to_string_pretty(&task).map_err(|e| CliError::Other {
            message: format!("re-serializing migrated task {}: {e}", task.id),
            exit_code: 1,
        })?;
        queue::enqueue(&qdir, &id, &task_toml)?;
    }
    let _ = fs::remove_file(&legacy_path);
    eprintln!(
        "note: migrated legacy `.aristo/pending-neural.toml` to \
         `.aristo/verify-queue/pending/` (one task per file)."
    );
    Ok(())
}

#[aristo::intent(
    "Prior attempts for an id come from the existing `.aristo/proofs/\
     <id>.proof` file (if any), parsed once to extract verdict.attempts. \
     Carrying this across re-spawns activates the K-bounded repair \
     budget that would otherwise be dead code: each fresh subagent \
     invocation writing attempts=1 means a hard-to-verify intent can \
     re-spawn indefinitely without ever hitting the cap. Reading from \
     the rejected proof on disk is the only persistence channel \
     available — the SDK doesn't track per-entry attempt history \
     elsewhere.",
    verify = "test",
    id = "pending_carries_prior_attempts_from_existing_proof"
)]
fn read_prior_attempts(ws: &Workspace, id: &AnnotationId) -> u32 {
    let filename = format!("{}.proof", id.as_str().replace(':', "__"));
    let path = ws.aristo_dir().join("proofs").join(filename);
    if !path.is_file() {
        return 0;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return 0;
    };
    let Ok(pf) = ProofFile::parse(&raw) else {
        return 0;
    };
    pf.verdict.attempts
}

pub(crate) fn proof_path_for(ws: &Workspace, id: &AnnotationId) -> std::path::PathBuf {
    let filename = format!("{}.proof", id.as_str().replace(':', "__"));
    ws.aristo_dir().join("proofs").join(filename)
}

pub(crate) fn proof_bak_path_for(ws: &Workspace, id: &AnnotationId) -> std::path::PathBuf {
    let filename = format!("{}.proof.bak", id.as_str().replace(':', "__"));
    ws.aristo_dir().join("proofs").join(filename)
}

#[aristo::intent(
    "When `aristo verify` re-enqueues an entry that already has a .proof \
     on disk, move the existing proof to <id>.proof.bak before the next \
     attempt overwrites it. Single-deep backup — overwrites any prior \
     .bak. Lets the user diff a rejected re-attempt against the prior \
     verdict. The .bak is auto-deleted on successful --apply-verdicts.",
    verify = "test",
    id = "pending_backs_up_existing_proof_on_rerun"
)]
fn backup_existing_proof(ws: &Workspace, id: &AnnotationId) {
    let src = proof_path_for(ws, id);
    if !src.is_file() {
        return;
    }
    let bak = proof_bak_path_for(ws, id);
    // Best-effort: failures here shouldn't abort the verify pipeline.
    let _ = std::fs::rename(&src, &bak);
}

fn warn_budget_exhausted(ids: &[&AnnotationId]) {
    eprintln!();
    eprintln!(
        "⚠  {} annotation(s) have exhausted the repair budget ({} attempts) and \
         will not be re-dispatched until you intervene:",
        ids.len(),
        MAX_REPAIR_ATTEMPTS
    );
    for id in ids {
        eprintln!("    {id}");
    }
    eprintln!();
    eprintln!(
        "    The proof file on disk records why each attempt failed. Either fix \
         the underlying issue and `aristo verify --rerun --filter id=<id>`, or \
         delete `.aristo/proofs/<id>.proof` to start the budget fresh."
    );
}

fn entry_text(e: &IndexEntry) -> &str {
    match e {
        IndexEntry::Intent(x) => &x.text,
        IndexEntry::Assume(x) => &x.text,
    }
}
fn entry_file(e: &IndexEntry) -> &str {
    match e {
        IndexEntry::Intent(x) => &x.file,
        IndexEntry::Assume(x) => &x.file,
    }
}
fn entry_site(e: &IndexEntry) -> &str {
    match e {
        IndexEntry::Intent(x) => &x.site,
        IndexEntry::Assume(x) => &x.site,
    }
}
fn entry_text_hash(e: &IndexEntry) -> &Sha256 {
    match e {
        IndexEntry::Intent(x) => &x.text_hash,
        IndexEntry::Assume(x) => &x.text_hash,
    }
}
fn entry_body_hash(e: &IndexEntry) -> &Sha256 {
    match e {
        IndexEntry::Intent(x) => &x.body_hash,
        IndexEntry::Assume(x) => &x.body_hash,
    }
}
