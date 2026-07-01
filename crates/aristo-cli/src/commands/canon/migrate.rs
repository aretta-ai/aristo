//! `aristo canon migrate` — surface per-binding version drift between
//! the local cache and the canon API.
//!
//! Per canon-strategy.md §CS12, canon entries are immutable per-version
//! but the catalog can ship **patch bumps** (same `canon_id`, new
//! `version` — typically a wording polish or a minor correction) and
//! **minor bumps** (the `canon_id` is retired; users should re-bind
//! their annotations against the current catalog). The SDK needs a
//! way to detect both classes after a catalog publish.
//!
//! ## Phase 1 scope (this PR)
//!
//! Diagnostic-only. The command queries `POST /canon/match` for every
//! canon-bound annotation's text, then compares the response's
//! `(canon_id, version)` to the cached `accepted_matches[..]` entry
//! under the prefixed id. For each canon-bound annotation it reports
//! one of three statuses:
//!
//! - **`current`** — cache version matches API version.
//! - **`patch-bump`** — same `canon_id`, newer `version`. Recommended
//!   action: `aristo canon refresh` to update the cache, optionally
//!   re-run `aristo canon accept` if the canonical text has changed.
//! - **`minor-bump`** — the previously-bound `canon_id` is no longer
//!   matched by the canon API. Recommended action:
//!   `aristo canon unbind <prefixed_id>` then re-stamp.
//!
//! Phase 2 will auto-apply patch bumps in place (quiet cache update)
//! and surface minor bumps as critique findings for re-binding.

use std::fs;

use aristo_core::canon::cache::CanonMatchesFile;
use aristo_core::canon::{
    AnnotationMatchInput, CanonClient, CanonError, CanonMatchRequest, HttpCanonClient,
    MockCanonClient,
};
use aristo_core::index::{BindingState, IdNamespace, IndexEntry, IndexFile};

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult};

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;

    // Read index — gather canon-bound annotations.
    let index_path = ws.index_path();
    if !index_path.is_file() {
        return Err(CliError::Other {
            message: format!(
                "no .aristo/index.toml at {} — run `aristo stamp` first",
                index_path.display()
            ),
            exit_code: 2,
        });
    }
    let index_raw = fs::read_to_string(&index_path).map_err(CliError::Io)?;
    let index: IndexFile = toml::from_str(&index_raw).map_err(|e| CliError::Other {
        message: format!("parsing {}: {e}", index_path.display()),
        exit_code: 1,
    })?;

    let bound: Vec<&aristo_core::index::IntentEntry> = index
        .entries
        .iter()
        .filter_map(|(id, entry)| {
            let ns = id.namespace();
            if !matches!(ns, IdNamespace::Aristos | IdNamespace::Kanon) {
                return None;
            }
            if let IndexEntry::Intent(e) = entry {
                if matches!(e.binding, BindingState::Bound { .. }) {
                    return Some(e);
                }
            }
            None
        })
        .collect();
    let bound_ids: Vec<&aristo_core::index::AnnotationId> = index
        .entries
        .iter()
        .filter(|(id, entry)| {
            let ns = id.namespace();
            if !matches!(ns, IdNamespace::Aristos | IdNamespace::Kanon) {
                return false;
            }
            if let IndexEntry::Intent(e) = entry {
                matches!(e.binding, BindingState::Bound { .. })
            } else {
                false
            }
        })
        .map(|(id, _)| id)
        .collect();

    if bound.is_empty() {
        println!("ok: no canon-bound annotations in the index. Nothing to migrate.");
        return Ok(());
    }

    // Read the cache for the accepted_matches entries.
    let cache_path = ws.canon_matches_path();
    let cache = CanonMatchesFile::read(&cache_path).map_err(CliError::Io)?;

    // Build a client. Same precedence as runner.
    let client: Box<dyn CanonClient> = if let Some(mock) = MockCanonClient::from_env() {
        Box::new(mock)
    } else {
        match aristo_core::auth::resolve_full() {
            Ok(creds) => {
                let base_url = crate::data_plane::resolve_base(&creds.server);
                Box::new(HttpCanonClient::new(base_url, &creds.token))
            }
            Err(_) => {
                return Err(CliError::Other {
                    message: "canon API requires authentication.\n  \
                              Run `aristo auth login` first."
                        .into(),
                    exit_code: 1,
                });
            }
        }
    };

    // Build the batched match request from canon-bound annotations.
    let request = CanonMatchRequest {
        annotations: bound
            .iter()
            .map(|e| AnnotationMatchInput {
                annotation_text: e.text.clone(),
                applies_to: applies_to_from_site(&e.site),
            })
            .collect(),
        // Use the broader critique-threshold so a slightly drifted match
        // still surfaces during migration (better than missing a patch
        // bump because confidence dipped 0.01 below the stamp threshold).
        confidence_threshold: 0.65,
        include_suggestions: false,
    };
    let response = client
        .match_annotations(&request)
        .map_err(canon_error_to_cli)?;

    println!();
    println!("canon migration report:");
    println!();
    let mut current = 0usize;
    let mut patch_bump = 0usize;
    let mut minor_bump = 0usize;
    for (i, prefixed_id) in bound_ids.iter().enumerate() {
        let cached = cache
            .entries
            .get(prefixed_id)
            .and_then(|e| e.accepted_matches.first());
        let Some(cached_match) = cached else {
            println!(
                "  {prefixed_id}: ? (no cached accepted_match — re-stamp to refresh)",
                prefixed_id = prefixed_id.as_str()
            );
            continue;
        };
        let candidates = response.results.get(i).cloned().unwrap_or_default();
        let same_canon = candidates
            .iter()
            .find(|c| c.canon_id == cached_match.canon_id);
        match same_canon {
            Some(c) if c.version == cached_match.version => {
                current += 1;
                println!(
                    "  {prefixed_id}: current ({}@{})",
                    cached_match.canon_id, cached_match.version,
                );
            }
            Some(c) => {
                patch_bump += 1;
                println!(
                    "  {prefixed_id}: patch-bump ({} {} → {}). Run `aristo canon refresh` to update the cache.",
                    cached_match.canon_id, cached_match.version, c.version,
                );
            }
            None => {
                minor_bump += 1;
                println!(
                    "  {prefixed_id}: minor-bump ({} retired from catalog). Run `aristo canon unbind {prefixed_id}` then re-stamp.",
                    cached_match.canon_id,
                    prefixed_id = prefixed_id.as_str(),
                );
            }
        }
    }
    println!();
    println!("totals: {current} current, {patch_bump} patch-bump, {minor_bump} minor-bump");
    Ok(())
}

fn applies_to_from_site(site: &str) -> Vec<String> {
    // Same heuristic as the runner: pull the leading kind word out of
    // the site label (`"fn ..."` / `"struct ..."`).
    let head = site.split_whitespace().next().unwrap_or("");
    let kw = match head {
        "fn" => "fn",
        "struct" => "struct",
        "enum" => "enum",
        "trait" => "trait",
        "impl" => "method",
        "mod" => "mod",
        "type" => "type",
        _ => return vec![],
    };
    vec![kw.to_string()]
}

fn canon_error_to_cli(e: CanonError) -> CliError {
    CliError::Other {
        message: format!("canon API error: {e}"),
        exit_code: 1,
    }
}
