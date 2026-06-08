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
use aristo_core::walk::{walk_directory_with, DiscoveredAnnotation, ParentRaw, WalkOptions};

use crate::{CliError, CliResult, Workspace};

/// `(id-keyed entries, id → parent ids)` — the two parallel maps
/// `aristo index` builds in one walk: the first becomes the
/// `IndexFile.entries`, the second feeds [`detect_cycles`].
pub(crate) type BuiltEntries = (
    BTreeMap<AnnotationId, IndexEntry>,
    HashMap<AnnotationId, Vec<AnnotationId>>,
);

pub(crate) fn run(_all: bool) -> CliResult<()> {
    // _all is a slice-17+ flag (mtime cache); accepted as no-op for now.
    let ws = workspace_or_error()?;
    crate::session::guard::ensure_no_active_session(&ws, "aristo index")?;

    println!("→ Walking source from {} …", ws.root.display());
    let walk_opts = walk_options_from_workspace(&ws)?;
    let discovered = walk_directory_with(&ws.root, &walk_opts).map_err(|e| CliError::Other {
        message: format!("walk failed: {e}"),
        exit_code: 1,
    })?;
    println!("→ Found {} annotations", discovered.len());

    let (entries, parents_map) = build_entries(&discovered, &ws.root)?;

    println!("→ Checking for parent-link cycles");
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
    let noun = if entry_count == 1 {
        "annotation"
    } else {
        "annotations"
    };
    println!("ok: index regenerated ({entry_count} {noun}).");
    Ok(())
}

pub(crate) fn workspace_or_error() -> CliResult<Workspace> {
    Workspace::find(None).map_err(|e| match e {
        crate::WorkspaceError::NotFound { searched_from } => {
            CliError::NotInWorkspace { searched_from }
        }
    })
}

/// Read the workspace's `[index]` config and turn it into a
/// [`WalkOptions`]. Bad glob patterns surface as a hard error — the user
/// authored them and needs to know they don't compile.
pub(crate) fn walk_options_from_workspace(ws: &Workspace) -> CliResult<WalkOptions> {
    let cfg = ws.load_config();
    WalkOptions::from_index_config(&cfg.index).map_err(|e| CliError::Other {
        message: format!("aristo.toml [index].exclude: {e}"),
        exit_code: 2,
    })
}

#[aristo::intent(
    "Every discovered annotation gets an id: the user-written `id =` if \
     present, otherwise a deterministic content-addressed `aret_…` id \
     derived from the annotation's kind, text, and site. The build never \
     returns an entry without an id; there is no `unindexed` half-state. \
     Because the generated id is a pure function of identity, re-stamping \
     unchanged source mints the same ids, so the index keeps each entry's \
     prior status and proof instead of treating it as removed-then-new.",
    verify = "test",
    id = "build_entries_assigns_deterministic_ids_when_missing"
)]
pub(crate) fn build_entries(
    discovered: &[DiscoveredAnnotation],
    _root: &Path,
) -> CliResult<BuiltEntries> {
    let mut entries: BTreeMap<AnnotationId, IndexEntry> = BTreeMap::new();
    let mut parents_map: HashMap<AnnotationId, Vec<AnnotationId>> = HashMap::new();
    let mut skipped = 0usize;
    // Counts idless annotations per identity bucket so duplicates that would
    // otherwise mint the same deterministic id get distinct source-order
    // ordinals. Keyed exactly the way `id::deterministic_id` hashes (ID-D2).
    let mut ordinal_counter: HashMap<(AnnotationKind, String, String), usize> = HashMap::new();

    for d in discovered {
        let Some(ann_id) = resolve_id(d, &mut skipped, &mut ordinal_counter) else {
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

fn resolve_id(
    d: &DiscoveredAnnotation,
    skipped: &mut usize,
    ordinal_counter: &mut HashMap<(AnnotationKind, String, String), usize>,
) -> Option<AnnotationId> {
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
        None => {
            // No user-written id → derive a deterministic content-addressed
            // one from (kind, text, site). Uses the LINE-FREE site
            // (`ExtractedAnnotation.site`, not the index entry's
            // `"… (line N)"`) so unrelated line shifts don't re-churn the id.
            let key = id::id_bucket_key(d.annotation.kind, &d.annotation.text, &d.annotation.site);
            let ordinal = ordinal_counter.entry(key).or_insert(0);
            let resolved = id::deterministic_id(
                d.annotation.kind,
                &d.annotation.text,
                &d.annotation.site,
                *ordinal,
            );
            *ordinal += 1;
            Some(resolved)
        }
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
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
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
    "A crash mid-write leaves either the prior file or the new file at \
     the target — never a partial one. The temp file's suffix is fixed, \
     not randomized, so two concurrent invocations clash on the temp \
     file — intentional, since running two indexers against one \
     workspace is a user error we surface loudly.",
    verify = "neural",
    id = "atomic_write_via_tempfile_rename"
)]
pub(crate) fn atomic_write(target: &Path, content: &str) -> CliResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let tmp = target.with_extension("toml.tmp");
    fs::write(&tmp, content).map_err(CliError::Io)?;
    fs::rename(&tmp, target).map_err(CliError::Io)?;
    Ok(())
}

pub(crate) fn now_rfc3339() -> String {
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
