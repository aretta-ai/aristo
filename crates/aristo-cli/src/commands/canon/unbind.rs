//! `aristo canon unbind <prefixed_id>` — reverse of accept. Strips
//! the `aristos:` / `kanon:` prefix from the annotation id, reverts
//! the index entry's binding state to `Local`, and removes the
//! accepted_matches entry from the cache so the next stamp can
//! re-match the annotation freely.
//!
//! ## Source rewrite
//!
//! Replaces `id = "aristos:foo"` (or `kanon:foo`) with `id = "foo"`
//! at the same byte span — reusing the existing rename-style span
//! substitution primitive. The canonical text is **preserved** —
//! unbinding doesn't restore the user's original prose (we don't
//! remember it). If the user wants the original text back, they
//! edit source directly.
//!
//! ## Collision handling
//!
//! If the post-unbind bare id (`<canon_id>`) collides with an
//! existing index entry, the command refuses with a clear diagnostic
//! pointing at `aristo rename` for manual conflict resolution.
//!
//! ## Cache effect
//!
//! - The `accepted_matches[..]` entry under the prefixed id is
//!   removed (the whole CacheEntry is dropped if no other buckets
//!   are populated).
//! - The bare-id cache entry, if any, is left alone — the next
//!   `aristo stamp` will re-pull pending matches against the
//!   current annotation text.

use std::fs;
use std::path::Path;

use aristo_core::canon::cache::CanonMatchesFile;
use aristo_core::index::{AnnotationId, BindingState, IdNamespace, IndexEntry, IndexFile};
use aristo_core::walk::{scan_id_occurrences, IdOccurrenceKind};

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult, Workspace};

pub(crate) fn run(prefixed_id_raw: &str) -> CliResult<()> {
    let ws = workspace_or_error()?;
    apply_unbind(&ws, prefixed_id_raw)
}

pub(crate) fn apply_unbind(ws: &Workspace, prefixed_id_raw: &str) -> CliResult<()> {
    let prefixed_id = AnnotationId::parse(prefixed_id_raw).map_err(|e| CliError::Other {
        message: format!("annotation id `{prefixed_id_raw}` is not valid ({e})."),
        exit_code: 2,
    })?;
    let bare_str: String = match prefixed_id.namespace() {
        IdNamespace::Aristos => prefixed_id_raw
            .strip_prefix("aristos:")
            .expect("namespace says aristos:")
            .to_string(),
        IdNamespace::Kanon => prefixed_id_raw
            .strip_prefix("kanon:")
            .expect("namespace says kanon:")
            .to_string(),
        _ => {
            return Err(CliError::Other {
                message: format!(
                    "id `{prefixed_id_raw}` is not canon-bound (must use \
                     `aristos:` or `kanon:` prefix). Use `aristo rename` for \
                     bare-id renames."
                ),
                exit_code: 1,
            });
        }
    };
    let bare_id = AnnotationId::parse(&bare_str).map_err(|e| CliError::Other {
        message: format!("post-unbind bare id `{bare_str}` is not valid: {e}"),
        exit_code: 1,
    })?;

    // Load index, locate the entry under the prefixed id.
    let index_path = ws.index_path();
    let mut index = read_index(&index_path)?;
    let entry = index
        .entries
        .get(&prefixed_id)
        .ok_or_else(|| CliError::Other {
            message: format!(
                "no entry `{prefixed_id_raw}` in .aristo/index.toml.\n\
                 Run `aristo canon list` to see canon-bound annotations."
            ),
            exit_code: 1,
        })?
        .clone();
    let intent = match entry {
        IndexEntry::Intent(e) => e,
        IndexEntry::Assume(_) => {
            return Err(CliError::Other {
                message: format!(
                    "entry `{prefixed_id_raw}` is an `assume` — canon bindings \
                     only apply to intents."
                ),
                exit_code: 1,
            });
        }
    };
    if !matches!(intent.binding, BindingState::Bound { .. }) {
        return Err(CliError::Other {
            message: format!(
                "entry `{prefixed_id_raw}` is not in `Bound` state. Unbind only \
                 reverses a previous accept."
            ),
            exit_code: 1,
        });
    }
    if index.entries.contains_key(&bare_id) {
        return Err(CliError::Other {
            message: format!(
                "post-unbind id `{bare_str}` already exists in the index — would \
                 collide. Rename the existing entry first, or unbind manually \
                 by editing source + running `aristo stamp`."
            ),
            exit_code: 1,
        });
    }

    // Source rewrite: replace `id = "<prefixed>"` with `id = "<bare>"` by
    // span substitution. Find the occurrence via scan_id_occurrences.
    let src_path = ws.root.join(&intent.file);
    let source = fs::read_to_string(&src_path).map_err(|e| CliError::Other {
        message: format!("cannot read {}: {e}", src_path.display()),
        exit_code: 1,
    })?;
    let occurrences = scan_id_occurrences(&source).map_err(|e| CliError::Other {
        message: format!("parsing {}: {e}", src_path.display()),
        exit_code: 1,
    })?;
    let target_occ = occurrences
        .iter()
        .find(|o| o.kind == IdOccurrenceKind::Id && o.value == prefixed_id_raw)
        .ok_or_else(|| CliError::Other {
            message: format!(
                "could not locate `id = \"{prefixed_id_raw}\"` in {}.\n\
                 Source may have drifted from the index — run `aristo stamp`.",
                src_path.display()
            ),
            exit_code: 1,
        })?;
    let mut bytes = source.as_bytes().to_vec();
    bytes.splice(target_occ.byte_start..target_occ.byte_end, bare_str.bytes());
    let new_source = String::from_utf8(bytes).expect("utf-8 preserved");
    atomic_write_bytes(&src_path, new_source.as_bytes())?;

    // Index rewrite: re-key + clear binding.
    let mut new_intent = intent.clone();
    new_intent.binding = BindingState::Local;
    new_intent.last_critiqued_at_text_hash = None;
    new_intent.last_critique_finding_count = None;
    index.entries.remove(&prefixed_id);
    index
        .entries
        .insert(bare_id.clone(), IndexEntry::Intent(new_intent));
    let toml_text = toml::to_string_pretty(&index).map_err(|e| CliError::Other {
        message: format!("serializing index: {e}"),
        exit_code: 1,
    })?;
    atomic_write_bytes(&index_path, toml_text.as_bytes())?;

    // Cache: drop the accepted_matches entry under the prefixed id.
    let cache_path = ws.canon_matches_path();
    let mut cache = CanonMatchesFile::read(&cache_path).map_err(CliError::Io)?;
    cache.entries.remove(&prefixed_id);
    cache.write_atomic(&cache_path).map_err(CliError::Io)?;

    println!(
        "ok: unbound `{prefixed_id_raw}` → `{bare_str}`. Binding cleared; \
         run `aristo stamp` to re-evaluate canon matches against the \
         current text."
    );
    Ok(())
}

fn atomic_write_bytes(target: &Path, content: &[u8]) -> CliResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let tmp_name = match target.file_name() {
        Some(name) => {
            let mut s = name.to_os_string();
            s.push(".aristo-tmp");
            s
        }
        None => {
            return Err(CliError::Other {
                message: format!(
                    "canon unbind: cannot atomic-write `{}` — path has no file name",
                    target.display()
                ),
                exit_code: 1,
            });
        }
    };
    let tmp = target.with_file_name(tmp_name);
    fs::write(&tmp, content).map_err(CliError::Io)?;
    fs::rename(&tmp, target).map_err(CliError::Io)?;
    Ok(())
}

fn read_index(path: &Path) -> CliResult<IndexFile> {
    if !path.is_file() {
        return Err(CliError::Other {
            message: format!(
                "no .aristo/index.toml at {} — run `aristo stamp` first",
                path.display()
            ),
            exit_code: 2,
        });
    }
    let text = fs::read_to_string(path).map_err(CliError::Io)?;
    toml::from_str(&text).map_err(|e| CliError::Other {
        message: format!("parsing {}: {e}", path.display()),
        exit_code: 1,
    })
}
