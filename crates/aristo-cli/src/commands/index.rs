//! `aristo index` — walk source, parse annotations, write `.aristo/index.toml`.
//!
//! Slice 16 ships the full-walk path: every invocation re-scans every
//! `.rs` file under the workspace, regenerates the index from scratch,
//! detects cycles, and writes atomically. The mtime cache (incremental
//! re-walk) is a slice-17+ optimization — `--all` is accepted as a no-op
//! flag in this slice so users / CI scripts that already pass it don't
//! break when the cache lands.
//!
//! Per `docs/TOOLS.md`, `aristo index` is the lower-level building block:
//! `aristo stamp` runs `aristo index` and additionally classifies B5b
//! binding state and offers id-promotion. Slice 17 layers stamp on top.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use aristo_core::cycle::detect_cycles;
use aristo_core::id;
use aristo_core::index::{
    AnnotationId, AnnotationKind, AssumeEntry, BindingState, IndexEntry, IndexFile, IntentEntry,
    Meta, ParentLink, Status, VerifyLevel, VerifyMethod,
};
use aristo_core::walk::{walk_directory, DiscoveredAnnotation, ParentRaw};

use crate::{CliError, CliResult, Workspace};

/// `(id-keyed entries, id → parent ids)` — the two parallel maps
/// `aristo index` builds in one walk: the first becomes the
/// `IndexFile.entries`, the second feeds [`detect_cycles`].
type BuiltEntries = (
    BTreeMap<AnnotationId, IndexEntry>,
    HashMap<AnnotationId, Vec<AnnotationId>>,
);

#[aristo::intent(
    "aristo index writes .aristo/index.toml ATOMICALLY: temp file in the \
     same directory + rename. A crash mid-write leaves the previous index \
     intact rather than a half-formed one — `aristo show` / `aristo list` \
     / etc. always see a consistent file or the prior version, never a \
     parser error from a truncated rewrite.",
    verify = "test",
    id = "aristo_index_writes_atomically"
)]
pub(crate) fn run(_all: bool) -> CliResult<()> {
    // _all is a slice-17+ flag (mtime cache); accepted as no-op for now.
    let ws = workspace_or_error()?;

    println!("→ Walking source from {} …", ws.root.display());
    let discovered = walk_directory(&ws.root).map_err(|e| CliError::Other {
        message: format!("walk failed: {e}"),
        exit_code: 1,
    })?;
    println!("→ Found {} annotations", discovered.len());

    println!("→ Building index entries");
    let (entries, parents_map) = build_entries(&discovered, &ws.root)?;

    println!("→ Detecting cycles in parent graph");
    detect_cycles(&parents_map).map_err(|e| CliError::Other {
        message: format!("{e}\n\nNo files modified. Fix the cycle and re-run `aristo index`."),
        exit_code: 2,
    })?;

    let index = IndexFile {
        meta: Meta {
            schema_version: 1,
            generated_by: Some(format!("aristo index {}", env!("CARGO_PKG_VERSION"))),
            generated_at: Some(now_rfc3339()),
            source_root: Some(".".to_string()),
        },
        entries,
    };

    let toml_text = toml::to_string_pretty(&index).map_err(|e| CliError::Other {
        message: format!("serializing index.toml: {e}"),
        exit_code: 1,
    })?;

    let index_path = ws.index_path();
    let bytes_written = toml_text.len();
    atomic_write(&index_path, &toml_text)?;

    let entry_count = index.entries.len();
    let rel_path = index_path
        .strip_prefix(&ws.root)
        .unwrap_or(&index_path)
        .display();
    println!("→ Writing {rel_path} … ok ({entry_count} entries, {bytes_written} bytes)");
    println!();
    println!("ok: index regenerated ({entry_count} annotations).");
    Ok(())
}

fn workspace_or_error() -> CliResult<Workspace> {
    Workspace::find(None).map_err(|e| match e {
        crate::WorkspaceError::NotFound { searched_from } => {
            CliError::NotInWorkspace { searched_from }
        }
    })
}

#[aristo::intent(
    "build_entries assigns an opaque aret_<random> id to every \
     annotation that lacks a user-written id; aristo stamp (slice 17) \
     offers to promote opaque ids to readable ones via the rename flow. \
     The IndexFile schema requires every entry to have an id — there is \
     no `unindexed` half-state.",
    verify = "test",
    id = "build_entries_assigns_opaque_ids_when_missing"
)]
fn build_entries(discovered: &[DiscoveredAnnotation], _root: &Path) -> CliResult<BuiltEntries> {
    let mut entries: BTreeMap<AnnotationId, IndexEntry> = BTreeMap::new();
    let mut parents_map: HashMap<AnnotationId, Vec<AnnotationId>> = HashMap::new();
    let mut skipped = 0usize;

    for d in discovered {
        let Some(ann_id) = resolve_id(d, &mut skipped) else {
            continue;
        };
        let Some(parent_ids) = resolve_parent_ids(d, &mut skipped) else {
            continue;
        };
        let Some(verify) = resolve_verify(d, &mut skipped) else {
            continue;
        };

        let parent_link = parent_link_from_ids(&parent_ids);
        let entry = build_index_entry(d, parent_link, verify);

        if entries.insert(ann_id.clone(), entry).is_some() {
            eprintln!(
                "warning: skipping {}:{}: duplicate id `{}` (each id must appear at most once)",
                d.file.display(),
                d.annotation.line,
                ann_id.as_str()
            );
            skipped += 1;
            continue;
        }
        parents_map.insert(ann_id, parent_ids);
    }

    if skipped > 0 {
        eprintln!("→ Skipped {skipped} annotation(s) due to validation errors above");
    }
    Ok((entries, parents_map))
}

fn resolve_id(d: &DiscoveredAnnotation, skipped: &mut usize) -> Option<AnnotationId> {
    match &d.annotation.id {
        Some(s) => match AnnotationId::parse(s) {
            Ok(id) => Some(id),
            Err(e) => {
                eprintln!(
                    "warning: skipping {}:{}: invalid id `{s}`: {e}",
                    d.file.display(),
                    d.annotation.line
                );
                *skipped += 1;
                None
            }
        },
        None => Some(id::generate_opaque_id()),
    }
}

fn resolve_parent_ids(d: &DiscoveredAnnotation, skipped: &mut usize) -> Option<Vec<AnnotationId>> {
    let raws: Vec<&str> = match &d.annotation.parent {
        None => Vec::new(),
        Some(ParentRaw::Single(s)) => vec![s.as_str()],
        Some(ParentRaw::Multiple(ss)) => ss.iter().map(String::as_str).collect(),
    };
    let mut out = Vec::with_capacity(raws.len());
    for raw in raws {
        match AnnotationId::parse(raw) {
            Ok(id) => out.push(id),
            Err(e) => {
                eprintln!(
                    "warning: skipping {}:{}: invalid parent id `{raw}`: {e}",
                    d.file.display(),
                    d.annotation.line
                );
                *skipped += 1;
                return None;
            }
        }
    }
    Some(out)
}

fn resolve_verify(d: &DiscoveredAnnotation, skipped: &mut usize) -> Option<VerifyLevel> {
    match parse_verify(&d.annotation.verify, d) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!(
                "warning: skipping {}:{}: {}",
                d.file.display(),
                d.annotation.line,
                e
            );
            *skipped += 1;
            None
        }
    }
}

fn parent_link_from_ids(ids: &[AnnotationId]) -> Option<ParentLink> {
    match ids.len() {
        0 => None,
        1 => Some(ParentLink::Single(ids[0].clone())),
        _ => Some(ParentLink::Multiple(ids.to_vec())),
    }
}

fn build_index_entry(
    d: &DiscoveredAnnotation,
    parent: Option<ParentLink>,
    verify: VerifyLevel,
) -> IndexEntry {
    let file_str = d.file.display().to_string();
    let site = format!("{} (line {})", d.annotation.site, d.annotation.line);
    let common_text = d.annotation.text.clone();
    let text_hash = d.annotation.text_hash.clone();
    let body_hash = d.annotation.body_hash.clone();
    let covered_region = d.annotation.covered_region;

    match d.annotation.kind {
        AnnotationKind::Intent => IndexEntry::Intent(IntentEntry {
            text: common_text,
            verify,
            status: Status::Unknown,
            text_hash,
            body_hash,
            file: file_str,
            site,
            covered_region,
            binding: BindingState::Local,
            parent,
        }),
        AnnotationKind::Assume => IndexEntry::Assume(AssumeEntry {
            text: common_text,
            status: Status::Unknown,
            text_hash,
            body_hash,
            file: file_str,
            site,
            covered_region,
            linked: None,
            parent,
        }),
    }
}

fn parse_verify(raw: &Option<String>, d: &DiscoveredAnnotation) -> CliResult<VerifyLevel> {
    let Some(raw) = raw else {
        // No `verify =` argument → resolves to project default at verify
        // time; in the index we record `true` as the placeholder
        // ("project default"), matching ConfigFile.verify.default_method's
        // resolution rule.
        return Ok(VerifyLevel::Bool(true));
    };
    // The walker captures verify as raw token text (`true`, `false`,
    // `"test"`, `"neural"`, `"full"`, etc.). Strip surrounding quotes for
    // string-form values.
    let trimmed = raw.trim();
    let inner = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"'));
    Ok(match (trimmed, inner) {
        ("true", _) => VerifyLevel::Bool(true),
        ("false", _) => VerifyLevel::Bool(false),
        (_, Some("false")) => VerifyLevel::Bool(false),
        (_, Some("test")) => VerifyLevel::Method(VerifyMethod::Test),
        (_, Some("neural")) => VerifyLevel::Method(VerifyMethod::Neural),
        (_, Some("full")) => VerifyLevel::Method(VerifyMethod::Full),
        _ => {
            return Err(CliError::Other {
                message: format!(
                    "invalid verify value `{raw}` at {}:{} (expected true, false, \"false\", \"test\", \"neural\", or \"full\")",
                    d.file.display(),
                    d.annotation.line
                ),
                exit_code: 2,
            });
        }
    })
}

#[aristo::intent(
    "atomic_write writes via temp file + rename in the same directory, so \
     a crash leaves either the prior index or the new one — never a \
     half-written file. The temp suffix `.tmp` is fixed (no PID / random \
     component) so concurrent invocations of `aristo index` clash — that's \
     the right behavior; running two indexers against the same workspace \
     is a user error.",
    verify = "test",
    id = "atomic_write_via_tempfile_rename"
)]
fn atomic_write(target: &Path, content: &str) -> CliResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let tmp = target.with_extension("toml.tmp");
    fs::write(&tmp, content).map_err(CliError::Io)?;
    fs::rename(&tmp, target).map_err(CliError::Io)?;
    Ok(())
}

fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use aristo_core::walk::AnnotationForm;

    #[test]
    fn parse_verify_handles_all_documented_forms() {
        let dummy = DiscoveredAnnotation {
            file: std::path::PathBuf::from("x.rs"),
            annotation: aristo_core::walk::ExtractedAnnotation {
                kind: AnnotationKind::Intent,
                form: AnnotationForm::Attribute,
                text: "x".to_string(),
                verify: None,
                parent: None,
                id: None,
                site: "fn x".to_string(),
                line: 1,
                covered_region: aristo_core::index::CoveredRegion::Function,
                text_hash: aristo_core::hash::text_hash("x"),
                body_hash: aristo_core::hash::body_hash("x"),
            },
        };
        assert_eq!(
            parse_verify(&None, &dummy).unwrap(),
            VerifyLevel::Bool(true)
        );
        assert_eq!(
            parse_verify(&Some("true".into()), &dummy).unwrap(),
            VerifyLevel::Bool(true)
        );
        assert_eq!(
            parse_verify(&Some("false".into()), &dummy).unwrap(),
            VerifyLevel::Bool(false)
        );
        assert_eq!(
            parse_verify(&Some("\"test\"".into()), &dummy).unwrap(),
            VerifyLevel::Method(VerifyMethod::Test)
        );
        assert_eq!(
            parse_verify(&Some("\"neural\"".into()), &dummy).unwrap(),
            VerifyLevel::Method(VerifyMethod::Neural)
        );
        assert_eq!(
            parse_verify(&Some("\"full\"".into()), &dummy).unwrap(),
            VerifyLevel::Method(VerifyMethod::Full)
        );
        assert_eq!(
            parse_verify(&Some("\"false\"".into()), &dummy).unwrap(),
            VerifyLevel::Bool(false)
        );
        assert!(parse_verify(&Some("\"yolo\"".into()), &dummy).is_err());
    }
}
