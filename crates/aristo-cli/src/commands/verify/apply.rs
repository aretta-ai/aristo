//! `aristo verify --apply-verdicts` — consume every `.aristo/proofs/<id>.proof`
//! file, validate via the mechanical validator, and (on pass) flip the
//! index entry's status.
//!
//! Slice 23 ships this as the SDK's reply to the in-agent skill: the
//! skill orchestrates per-entry subagents that write proof files; the
//! SDK enforces structural integrity before letting any verdict touch
//! the index. No LLM in this path — the rejection guarantees are
//! mechanical (see `validator.rs`).

use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Status};
use aristo_core::proof::{ProofFile, VerdictType};

use crate::commands::index::atomic_write;
use crate::commands::verify::validator::{validate, ValidatorReport};
use crate::{CliError, CliResult, Workspace};

pub(crate) fn run_apply_verdicts(ws: &Workspace, index: &IndexFile) -> CliResult<()> {
    let proofs_dir = ws.aristo_dir().join("proofs");
    if !proofs_dir.is_dir() {
        println!("ok: no pending verdict files in .aristo/proofs/.");
        return Ok(());
    }

    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut updates: Vec<(AnnotationId, Status)> = Vec::new();
    let mut rejections: Vec<(PathBuf, ValidatorReport)> = Vec::new();
    let mut parse_errors: Vec<(PathBuf, String)> = Vec::new();

    for path in collect_proof_files(&proofs_dir)? {
        let id = match id_from_filename(&path) {
            Some(id) => id,
            None => {
                parse_errors.push((path.clone(), "filename is not <id>.proof".into()));
                continue;
            }
        };
        let raw = fs::read_to_string(&path).map_err(|e| CliError::Other {
            message: format!("read {}: {e}", path.display()),
            exit_code: 1,
        })?;
        let pf = match ProofFile::parse(&raw) {
            Ok(p) => p,
            Err(e) => {
                parse_errors.push((path.clone(), format!("parse: {e}")));
                continue;
            }
        };
        let report = validate(&id, &pf, index, &ws.root);
        if !report.is_empty() {
            rejections.push((path, report));
            rejected += 1;
            continue;
        }
        let new_status = derived_status(&pf);
        updates.push((id, new_status));
        accepted += 1;
    }

    if !updates.is_empty() {
        apply_status_updates(ws, index, &updates)?;
    }

    print_summary(accepted, rejected, parse_errors.len());
    for (path, msg) in &parse_errors {
        eprintln!("error: {}: {}", path.display(), msg);
    }
    for (path, rep) in &rejections {
        eprintln!("error: {}: {}", path.display(), rep.render());
    }

    if rejected > 0 || !parse_errors.is_empty() {
        Err(CliError::Silent { exit_code: 1 })
    } else {
        Ok(())
    }
}

fn collect_proof_files(dir: &Path) -> CliResult<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| CliError::Other {
        message: format!("read_dir {}: {e}", dir.display()),
        exit_code: 1,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| CliError::Other {
            message: format!("read_dir entry: {e}"),
            exit_code: 1,
        })?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("proof") {
            out.push(path);
        }
    }
    out.sort(); // deterministic order
    Ok(out)
}

#[aristo::intent(
    "Filename ↔ AnnotationId mapping uses `:` → `__` substitution \
     (same convention as .aristo/doc/<id-safe>.md per TOOLS.md §I1). \
     A refactor that picks a different scheme (e.g., URL-encoding) \
     would break every previously-written `.proof` file on disk — \
     proof files are tracked in git, so any mapping change is a \
     migration, not a free refactor.",
    verify = "test",
    id = "proof_file_id_uses_colon_underscore_underscore"
)]
fn id_from_filename(path: &Path) -> Option<AnnotationId> {
    let stem = path.file_stem()?.to_str()?;
    let id_str = stem.replace("__", ":");
    AnnotationId::parse(&id_str).ok()
}

fn derived_status(pf: &ProofFile) -> Status {
    match pf.verdict.r#type {
        VerdictType::Verified => Status::Neural, // slice 23 only flips to Neural
        VerdictType::Counterexample => Status::Counterexample,
        VerdictType::Inconclusive => Status::Unknown, // partial info, not a status change
    }
}

fn apply_status_updates(
    ws: &Workspace,
    index: &IndexFile,
    updates: &[(AnnotationId, Status)],
) -> CliResult<()> {
    let mut new_index = index.clone();
    for (id, status) in updates {
        if let Some(entry) = new_index.entries.get_mut(id) {
            set_status(entry, *status);
        }
    }
    let toml_text = toml::to_string_pretty(&new_index).map_err(|e| CliError::Other {
        message: format!("serializing index.toml: {e}"),
        exit_code: 1,
    })?;
    atomic_write(&ws.index_path(), &toml_text)
}

fn set_status(entry: &mut IndexEntry, status: Status) {
    match entry {
        IndexEntry::Intent(e) => e.status = status,
        IndexEntry::Assume(e) => e.status = status,
    }
}

fn print_summary(accepted: usize, rejected: usize, parse_errors: usize) {
    let total = accepted + rejected + parse_errors;
    if total == 0 {
        println!("ok: no verdict files in .aristo/proofs/.");
    } else {
        println!(
            "applied: {accepted}/{total} verdict(s) ({rejected} rejected, {parse_errors} unparseable)."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_from_filename_handles_plain_id() {
        let p = PathBuf::from("/tmp/foo.proof");
        let id = id_from_filename(&p).unwrap();
        assert_eq!(id.as_str(), "foo");
    }

    #[test]
    fn id_from_filename_recovers_namespaced_id() {
        // `:` is illegal in filenames on some hosts; we use `__` instead.
        let p = PathBuf::from("/tmp/aristos__balance_no_dups.proof");
        let id = id_from_filename(&p).unwrap();
        assert_eq!(id.as_str(), "aristos:balance_no_dups");
    }

    #[test]
    fn derived_status_maps_each_verdict_type() {
        fn pf_with(t: VerdictType) -> ProofFile {
            use aristo_core::hash::text_hash;
            use aristo_core::index::VerifyMethod;
            use aristo_core::proof::{
                Gap, InconclusiveBody, PropertyKind, VerdictMeta, VerifiedBody,
            };
            ProofFile {
                verdict: VerdictMeta {
                    r#type: t,
                    method: VerifyMethod::Neural,
                    produced_at_text_hash: text_hash(""),
                    produced_at_body_hash: text_hash(""),
                    produced_by: "x".into(),
                    verifier_model: None,
                    attempts: 1,
                    property_kind: PropertyKind::Invariant,
                },
                verified: matches!(t, VerdictType::Verified).then(|| VerifiedBody {
                    proof: aristo_core::proof::Proof {
                        conclusion: "x".into(),
                        steps: vec![],
                    },
                }),
                counterexample: None,
                inconclusive: matches!(t, VerdictType::Inconclusive).then(|| InconclusiveBody {
                    partial_proof: None,
                    gap: Gap {
                        description: "x".into(),
                        unfilled_path: "0".into(),
                        suggested_annotations: vec![],
                    },
                }),
            }
        }
        assert_eq!(
            derived_status(&pf_with(VerdictType::Verified)),
            Status::Neural
        );
        assert_eq!(
            derived_status(&pf_with(VerdictType::Counterexample)),
            Status::Counterexample
        );
        assert_eq!(
            derived_status(&pf_with(VerdictType::Inconclusive)),
            Status::Unknown
        );
    }
}
