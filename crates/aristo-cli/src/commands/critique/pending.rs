//! Critique-pipeline task production.
//!
//! The critique queue entry is **self-contained**: it embeds the focal
//! annotation text + sibling and parent annotation texts so the worker
//! never needs to read source files. The worker's tooling is Bash-only
//! (no Read, no Write) — pop a task, decide findings from the embedded
//! context, submit.
//!
//! See `docs/decisions/critique-and-pipeline-architecture.md` §D2 for
//! the rationale (kills the "agent goes exploring" failure mode + slashes
//! token spend).

use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Sha256, VerifyLevel};
use serde::{Deserialize, Serialize};

use crate::pipeline::queue::{self, QueueDir};
use crate::{CliError, CliResult, Workspace};

pub(crate) const PIPELINE_NAME: &str = "critique";

/// Max siblings to embed per task. Larger sets blow up the worker prompt
/// without much marginal value for vocabulary alignment — five siblings
/// already give a decent corpus. If users need more, they can opt in
/// later via a config knob (deferred).
const MAX_SIBLINGS: usize = 5;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CritiqueTask {
    pub id: String,
    pub text: String,
    pub verify: String,
    pub file: String,
    pub site: String,
    pub text_hash: Sha256,
    pub body_hash: Sha256,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prior_attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<EmbeddedAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub siblings: Vec<EmbeddedAnnotation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EmbeddedAnnotation {
    pub id: String,
    pub text: String,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

#[aristo::intent(
    "Critique queue entries embed the focal annotation text PLUS sibling \
     and parent annotation texts as a self-contained TOML body under \
     `.aristo/critique-queue/pending/<id>.toml`. Workers get Bash-only \
     tooling (no Read, no Write) and decide findings purely from the \
     embedded context — they cannot wander into the repo. A refactor \
     that left the queue entry thin (id + hash only, agent reads source \
     itself) would re-introduce the very failure mode this design \
     defends against: agents spending tokens on unrelated reads and \
     producing critique grounded in irrelevant code.",
    verify = "neural",
    id = "critique_queue_entries_are_self_contained"
)]
pub(crate) fn enqueue_pending(
    ws: &Workspace,
    index: &IndexFile,
    ids: &[&AnnotationId],
) -> CliResult<usize> {
    let qdir = QueueDir::for_pipeline(ws, PIPELINE_NAME);
    qdir.ensure_dirs()?;
    let mut enqueued = 0usize;
    for id in ids {
        let Some(entry) = index.entries.get(*id) else {
            continue;
        };
        let task = build_task(id, entry, index);
        let task_toml = toml::to_string_pretty(&task).map_err(|e| CliError::Other {
            message: format!("serializing critique task {}: {e}", id.as_str()),
            exit_code: 1,
        })?;
        queue::enqueue(&qdir, id, &task_toml)?;
        enqueued += 1;
    }
    Ok(enqueued)
}

fn build_task(focal_id: &AnnotationId, focal: &IndexEntry, index: &IndexFile) -> CritiqueTask {
    let parent = parent_annotation(focal, index);
    let siblings = sibling_annotations(focal_id, focal, index);
    CritiqueTask {
        id: focal_id.as_str().to_string(),
        text: entry_text(focal).to_string(),
        verify: verify_label(focal),
        file: entry_file(focal).to_string(),
        site: entry_site(focal).to_string(),
        text_hash: entry_text_hash(focal).clone(),
        body_hash: entry_body_hash(focal).clone(),
        prior_attempts: 0, // critique doesn't carry attempts in v0
        parent,
        siblings,
    }
}

fn parent_annotation(focal: &IndexEntry, index: &IndexFile) -> Option<EmbeddedAnnotation> {
    let parent = match focal {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }?;
    // ParentLink may be Single or Multiple; embed the first (siblings
    // under multi-parent are uncommon and v1 can extend).
    let first_parent_id = parent.iter().next()?;
    let parent_entry = index.entries.get(first_parent_id)?;
    Some(EmbeddedAnnotation {
        id: first_parent_id.as_str().to_string(),
        text: entry_text(parent_entry).to_string(),
    })
}

#[aristo::intent(
    "Sibling context for a critique task is limited to annotations \
     sharing the focal annotation's parent, capped at five and taken \
     in deterministic order. A larger cap raises per-worker token cost \
     for diminishing vocabulary-alignment value; a smaller one misses \
     the cross-sibling consistency findings that the parent-shape and \
     vocabulary categories exist to surface.",
    verify = "neural",
    id = "critique_sibling_embedding_capped_at_five_per_parent"
)]
fn sibling_annotations(
    focal_id: &AnnotationId,
    focal: &IndexEntry,
    index: &IndexFile,
) -> Vec<EmbeddedAnnotation> {
    let Some(parent_link) = (match focal {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }) else {
        return Vec::new();
    };
    let parent_ids: Vec<&AnnotationId> = parent_link.iter().collect();
    let mut out = Vec::new();
    for (sibling_id, sibling_entry) in index.entries.iter() {
        if sibling_id == focal_id {
            continue;
        }
        if out.len() >= MAX_SIBLINGS {
            break;
        }
        let shares_parent = match sibling_entry {
            IndexEntry::Intent(e) => e.parent.as_ref(),
            IndexEntry::Assume(e) => e.parent.as_ref(),
        }
        .is_some_and(|sp| sp.iter().any(|sp_id| parent_ids.contains(&sp_id)));
        if !shares_parent {
            continue;
        }
        out.push(EmbeddedAnnotation {
            id: sibling_id.as_str().to_string(),
            text: entry_text(sibling_entry).to_string(),
        });
    }
    out
}

fn verify_label(entry: &IndexEntry) -> String {
    match entry {
        IndexEntry::Intent(e) => match e.verify {
            VerifyLevel::Bool(b) => b.to_string(),
            VerifyLevel::Method(m) => format!("{m:?}").to_lowercase(),
        },
        IndexEntry::Assume(_) => "—".into(),
    }
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

pub(crate) fn critique_path_for(ws: &Workspace, id: &AnnotationId) -> std::path::PathBuf {
    let filename = format!("{}.critique", id.as_str().replace(':', "__"));
    ws.aristo_dir().join("critiques").join(filename)
}
