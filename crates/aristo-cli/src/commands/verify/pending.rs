//! Pending-neural-verification request file at `.aristo/pending-neural.toml`.
//!
//! When `aristo verify` encounters `verify="neural"` entries needing
//! work, it writes this file as the contract with the in-agent
//! `aristo-neural-verify` skill. The skill reads it, dispatches one
//! subagent per entry to produce a verdict, writes the verdicts as
//! `.aristo/proofs/<id>.proof`, and calls `aristo verify --apply-verdicts`
//! to validate + apply.
//!
//! The file is transient: it's the request, not the result. The skill
//! is expected to delete or truncate it after a successful run; the
//! SDK does not require this for correctness (the next `aristo verify`
//! invocation rewrites it from scratch) but it keeps the workspace
//! tidy.

use std::fs;

use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Sha256};
use aristo_core::proof::ProofFile;
use serde::{Deserialize, Serialize};

use crate::commands::index::atomic_write;
use crate::commands::verify::validator::MAX_REPAIR_ATTEMPTS;
use crate::{CliError, CliResult, Workspace};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PendingFile {
    pub schema_version: u32,
    pub pending: Vec<PendingEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PendingEntry {
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
    "The pending file is a REQUEST from the SDK to the in-agent skill, \
     not a result. The skill reads it, dispatches per-entry verification \
     work, and writes proofs to `.aristo/proofs/<id>.proof` for the SDK \
     to validate via `--apply-verdicts`. A refactor that has the SDK \
     auto-read its own pending file (e.g., to call an LLM directly) \
     would conflate the CLI with the agent and break the design split: \
     the CLI never makes LLM calls; the agent never bypasses the SDK \
     validator.",
    verify = "neural",
    id = "pending_neural_file_is_sdk_to_agent_request_not_a_result"
)]
pub(crate) fn write_pending_neural(
    ws: &Workspace,
    index: &IndexFile,
    ids: &[&AnnotationId],
) -> CliResult<()> {
    let mut entries = Vec::with_capacity(ids.len());
    let mut budget_exhausted: Vec<&AnnotationId> = Vec::new();
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
        entries.push(PendingEntry {
            id: id.as_str().to_string(),
            text: entry_text(entry).to_string(),
            file: entry_file(entry).to_string(),
            site: entry_site(entry).to_string(),
            text_hash: entry_text_hash(entry).clone(),
            body_hash: entry_body_hash(entry).clone(),
            prior_attempts,
        });
    }
    if !budget_exhausted.is_empty() {
        warn_budget_exhausted(&budget_exhausted);
    }
    let pf = PendingFile {
        schema_version: 1,
        pending: entries,
    };
    let toml_text = toml::to_string_pretty(&pf).map_err(|e| CliError::Other {
        message: format!("serializing pending-neural.toml: {e}"),
        exit_code: 1,
    })?;
    let path = ws.aristo_dir().join("pending-neural.toml");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(&path, &toml_text)
}

#[aristo::intent(
    "Prior attempts for an id come from the existing `.aristo/proofs/ \
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
    "When `aristo verify` re-pends an entry that already has a .proof on \
     disk, move the existing proof to <id>.proof.bak before the next \
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
