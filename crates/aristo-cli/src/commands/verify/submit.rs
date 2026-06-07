//! `aristo verify --submit-verdict --id <id> --json <json>` — single-
//! verdict write path used by the `aristo-verify` skill's neural arm.
//!
//! Subagents construct a JSON-serialized [`ProofFile`] body and invoke
//! this command to land it on disk as `.aristo/proofs/<id>.proof`. The
//! SDK is the sole writer of `.proof` files; agents have no direct
//! filesystem write capability for the proofs directory. This subcommand
//! is the only path that creates a `.proof` file.
//!
//! Validation is identical to `--apply-verdicts`: same [`validate`]
//! function, same ground-resolution rules, same staleness anchoring. A
//! verdict that fails submit-time validation would also fail apply-time
//! validation; running the validator at write time gives the subagent a
//! tight feedback loop (one CLI call → structured errors → retry in the
//! same context) instead of waiting until `--apply-verdicts` time to
//! discover the problem.
//!
//! On success the SDK prints `accepted: sha256:<hex>` to stdout — the
//! sha256 of the on-disk TOML body. The orchestrator can compare this
//! against `body_hash(returned_text)` to verify the subagent's reported
//! text matches what hit the file (cheap integrity check; the SDK is
//! the writer so divergence implies subagent cache corruption).

use std::path::PathBuf;

use aristo_core::hash::body_hash;
use aristo_core::index::{AnnotationId, IndexFile};
use aristo_core::proof::ProofFile;

use crate::commands::verify::apply::{stamp_ground_hashes, write_proof_atomic};
use crate::commands::verify::pending::PIPELINE_NAME;
use crate::commands::verify::validator::validate;
use crate::pipeline::queue::{self, QueueDir};
use crate::{CliError, CliResult, Workspace};

#[aristo::intent(
    "`aristo verify --submit-verdict` is the SINGLE *creation* path \
     for `.aristo/proofs/<id>.proof` files (subagents have no \
     Write-tool access). `--apply-verdicts` may re-write an existing \
     proof in-place to stamp computed ground hashes, but only after \
     running the same `validate()` schema gate. A refactor that added \
     a third writer bypassing `validate()` would let unvalidated \
     proofs land — defeating the schema gate that catches invalid \
     enum variants, child-as-prior-step, and out-of-range line \
     citations.",
    verify = "neural",
    id = "submit_verdict_is_only_write_path_for_proofs"
)]
#[aristo::intent(
    "Submit-time validation runs the EXACT SAME `validate()` function \
     as `--apply-verdicts`. Schema rules, ground resolution, and \
     hash-staleness checks must not diverge between the write gate and \
     the apply gate. A verdict the validator accepts at submit MUST be \
     a verdict the validator would accept at apply (modulo intervening \
     index/source drift). A divergence — even subtly different rules \
     in the two paths — would let proofs land that later fail apply, \
     wasting the subagent's repair budget on unfixable schema mismatches.",
    verify = "neural",
    id = "submit_validation_matches_apply_validation"
)]
#[aristo::intent(
    "On accept, the SDK prints `accepted: sha256:<hex>` to stdout where \
     <hex> is the sha256 of the on-disk TOML body. The orchestrator \
     can compare this against body_hash(text_returned_by_subagent) for \
     a cheap integrity check: SDK is the sole writer, so a mismatch \
     means the subagent's reported text diverged from what hit disk \
     (corrupted cache, fabricated response). The hash anchors the \
     write-acknowledgement so the orchestrator does not have to re-read \
     the file to validate the subagent's word.",
    verify = "neural",
    id = "submit_returns_sha256_of_written_file"
)]
pub(crate) fn run_submit_verdict(
    ws: &Workspace,
    index: &IndexFile,
    id_str: &str,
    json_str: &str,
) -> CliResult<()> {
    let id = AnnotationId::parse(id_str).map_err(|e| CliError::Other {
        message: format!("--id {id_str:?}: {e}"),
        exit_code: 2,
    })?;
    if !index.entries.contains_key(&id) {
        return Err(CliError::Other {
            message: format!(
                "--id {id_str}: no such annotation in current index (run `aristo stamp` if you just added it)"
            ),
            exit_code: 2,
        });
    }
    let mut pf: ProofFile = serde_json::from_str(json_str).map_err(|e| CliError::Other {
        message: format!("--json: parse error: {e}"),
        exit_code: 2,
    })?;

    let report = validate(&id, &pf, index, &ws.root);
    if !report.is_empty() {
        eprintln!("error: {}", report.render());
        return Err(CliError::Silent { exit_code: 1 });
    }

    // Validator passed → stamp computed ground hashes (intent/assume id
    // lookups + code-region sha256s) so the on-disk proof carries a
    // freshness anchor for the next --apply-verdicts staleness check.
    stamp_ground_hashes(&mut pf, index, &ws.root);

    let path = proof_path_for(ws, &id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_proof_atomic(&path, &pf)?;

    let written = std::fs::read_to_string(&path).map_err(|e| CliError::Other {
        message: format!("read-back {}: {e}", path.display()),
        exit_code: 1,
    })?;
    let h = body_hash(&written);
    // Clear the queue entry now that the artifact has landed. Idempotent —
    // submit-done is a no-op if the entry isn't in claimed/ (e.g., submitted
    // out-of-band by --apply-verdicts replay, or never popped from the queue).
    let qdir = QueueDir::for_pipeline(ws, PIPELINE_NAME);
    queue::submit_done(&qdir, &id)?;
    println!("accepted: {h}");
    Ok(())
}

fn proof_path_for(ws: &Workspace, id: &AnnotationId) -> PathBuf {
    let filename = format!("{}.proof", id.as_str().replace(':', "__"));
    ws.aristo_dir().join("proofs").join(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        BindingState, CoveredRegion, IndexEntry, IntentEntry, Sha256, Status, VerifyLevel,
    };
    use std::collections::BTreeMap;

    fn workspace_with_index(entries: Vec<(AnnotationId, IndexEntry)>) -> (Workspace, IndexFile) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::fs::create_dir_all(root.join(".aristo")).unwrap();
        Box::leak(Box::new(tmp));
        let ws = Workspace { root };
        let mut map = BTreeMap::new();
        for (id, e) in entries {
            map.insert(id, e);
        }
        let index = IndexFile {
            meta: aristo_core::index::Meta {
                schema_version: 1,
                generated_by: Some("test".into()),
                generated_at: None,
                source_root: None,
            },
            entries: map,
        };
        (ws, index)
    }

    fn intent_entry(file: &str, site: &str) -> IndexEntry {
        let zero = Sha256::from_bytes(b"");
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify: VerifyLevel::Method(aristo_core::index::VerifyMethod::Neural),
            status: Status::Unknown,
            text_hash: zero.clone(),
            body_hash: zero,
            file: file.into(),
            site: site.into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    #[test]
    fn rejects_unknown_id() {
        let (ws, index) = workspace_with_index(vec![]);
        let err = run_submit_verdict(&ws, &index, "no_such_thing", "{}").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("no such annotation"),
            "expected unknown-id error, got: {msg}"
        );
    }

    #[test]
    fn rejects_malformed_json() {
        let id = AnnotationId::parse("foo").unwrap();
        let (ws, index) =
            workspace_with_index(vec![(id.clone(), intent_entry("src/x.rs", "fn x"))]);
        let err = run_submit_verdict(&ws, &index, "foo", "{this is not json").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("parse error"), "got: {msg}");
    }
}
