//! `aristo critique --submit-findings --id <id> --json '<json>'`
//! — agent write path; the SDK is the sole writer of `.critique` files.

use aristo_core::critique::CritiqueFile;
use aristo_core::hash::body_hash;
use aristo_core::index::{AnnotationId, IndexFile};

use crate::commands::critique::pending::{critique_path_for, PIPELINE_NAME};
use crate::commands::critique::validator::{stamp_derived, validate};
use crate::commands::index::atomic_write;
use crate::pipeline::queue::{self, QueueDir};
use crate::{CliError, CliResult, Workspace};

#[aristo::intent(
    "`aristo critique --submit-findings` is the SINGLE creation path \
     for `.aristo/critiques/<id>.critique` files (subagents have no \
     Write-tool access — critique workers have Bash only). On accept, \
     prints `accepted: sha256:<hex>` to stdout for the orchestrator's \
     integrity check. Validation gates schema enums + focal-id-in-index \
     + text staleness anchor + per-finding rationale presence; any \
     failure short-circuits before write_proof_atomic. Same shape as \
     submit-verdict, different payload schema.",
    verify = "neural",
    id = "submit_findings_is_only_write_path_for_critiques"
)]
pub(crate) fn run_submit_findings(
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
    let mut cf: CritiqueFile = serde_json::from_str(json_str).map_err(|e| CliError::Other {
        message: format!("--json: parse error: {e}"),
        exit_code: 2,
    })?;

    let report = validate(&id, &cf, index);
    if !report.is_empty() {
        eprintln!("error: {}", report.render());
        return Err(CliError::Silent { exit_code: 1 });
    }

    // SDK stamps derived fields (finding_count + highest_severity) so the
    // on-disk TOML has the authoritative values regardless of what the
    // agent submitted.
    stamp_derived(&mut cf);

    let path = critique_path_for(ws, &id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_text = cf.to_toml().map_err(|e| CliError::Other {
        message: format!("serializing critique for {}: {e}", id.as_str()),
        exit_code: 1,
    })?;
    atomic_write(&path, &toml_text)?;

    let written = std::fs::read_to_string(&path).map_err(|e| CliError::Other {
        message: format!("read-back {}: {e}", path.display()),
        exit_code: 1,
    })?;
    let h = body_hash(&written);
    let qdir = QueueDir::for_pipeline(ws, PIPELINE_NAME);
    queue::submit_done(&qdir, &id)?;
    println!("accepted: {h}");
    Ok(())
}
