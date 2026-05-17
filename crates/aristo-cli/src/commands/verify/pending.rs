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
use serde::{Deserialize, Serialize};

use crate::commands::index::atomic_write;
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
    for id in ids {
        let Some(entry) = index.entries.get(*id) else {
            continue;
        };
        entries.push(PendingEntry {
            id: id.as_str().to_string(),
            text: entry_text(entry).to_string(),
            file: entry_file(entry).to_string(),
            site: entry_site(entry).to_string(),
            text_hash: entry_text_hash(entry).clone(),
            body_hash: entry_body_hash(entry).clone(),
        });
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
