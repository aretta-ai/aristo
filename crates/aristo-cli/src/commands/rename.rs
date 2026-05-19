//! `aristo rename <old_id> <new_id>` — atomic coordinated rename across
//! source files, `.aristo/index.toml`, and per-id artifact files
//! (`.aristo/critiques/<id>.critique`, `.aristo/proofs/<id>.proof`).
//!
//! Slice 32 scope (locked 2026-05-18, per `HANDOFF-SLICE-32.md`):
//!
//! - **In:** bare → bare; `aret_*` → bare (with promotion note); target
//!   collision rejection; reserved-prefix (`aret_*`) target rejection
//!   (F1-b); `aristos:` namespace rejection (deferred-to-Phase-2 message);
//!   cross-namespace rejection (`aristos:foo` → `bar`).
//! - **Out (deferred to Phase 2 sync):** `aristos:` ↔ `aristos:` renames,
//!   server-binding warning, `aristo unbind`, transactional rollback.
//!
//! This commit ships the dispatch + validation skeleton; the dry-run plan
//! (commit 3) and the actual edits (commit 4) land in following commits.

use std::fs;
use std::path::Path;

use aristo_core::index::{AnnotationId, IdNamespace, IndexFile};

use crate::commands::index::workspace_or_error;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

/// Entry point invoked from `lib::dispatch`.
pub(crate) fn run(old_id: &str, new_id: &str, dry_run: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    let parsed = parse_and_validate(old_id, new_id, &index)?;
    let _ = (&parsed, &ws);

    if dry_run {
        // commit 3 lands the plan rendering. Until then, fail loudly so
        // CI catches any premature wiring.
        return Err(CliError::NotImplemented {
            what: "aristo rename --dry-run",
            slice: "slice 32 commit 3",
        });
    }
    Err(CliError::NotImplemented {
        what: "aristo rename (apply)",
        slice: "slice 32 commit 4",
    })
}

/// Tagged outcome of validation. Each variant is a legal rename shape;
/// the rejection paths return `Err` before we reach here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenameShape {
    /// Bare local id → bare local id.
    LocalToLocal,
    /// Opaque stamp-assigned id → bare local id. F1-c "promotion" path —
    /// the caller renders a "promoted opaque → readable" note alongside
    /// the success line.
    OpaquePromotion,
}

// Fields are read by tests + by the plan computation in commit 3;
// gate the skeleton-only warning here so the struct stays public-typed.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct ParsedRename {
    pub old_id: AnnotationId,
    pub new_id: AnnotationId,
    pub shape: RenameShape,
}

#[aristo::intent(
    "Rename validation rejects three classes BEFORE any plan computation: \
     (1) `aristos:` in either old or new id — server-bound renames are \
     deferred to Phase 2 alongside `aristo sync`, so the surface lies; \
     (2) cross-namespace renames (`aristos:foo` → bare) — that's an \
     unbind, not a rename, and ships with sync; (3) reserved `aret_*` \
     prefix in the target — opaque ids are stamp-assigned only (F1-b); \
     a readable id renaming TO an opaque slot would let a user manually \
     mint identities the stamp pipeline reserves. The fourth check, \
     target collision against the live index, is the only one that \
     depends on the workspace; the others can be tested in isolation.",
    verify = "test",
    id = "rename_validation_rejects_aristos_reserved_prefix_and_collision"
)]
pub(crate) fn parse_and_validate(
    old_raw: &str,
    new_raw: &str,
    index: &IndexFile,
) -> CliResult<ParsedRename> {
    // Step 1 — old id must parse and currently exist in the index.
    let old_id = match AnnotationId::parse(old_raw) {
        Ok(id) => id,
        Err(e) => {
            return Err(reject(format!(
                "source id `{old_raw}` is not a valid annotation id ({e}).\n\
                 Run `aristo list` to see indexed ids."
            )));
        }
    };
    // Reject aristos: ids in EITHER direction (scope trim).
    if old_id.namespace() == IdNamespace::Aristos {
        return Err(reject_aristos_deferred(old_raw));
    }
    if !index.entries.contains_key(&old_id) {
        return Err(reject(format!(
            "source id `{old_raw}` not found in .aristo/index.toml.\n\
             Run `aristo list` to see indexed ids, or `aristo stamp` if \
             you have just edited source."
        )));
    }

    // Step 2 — new id must parse.
    let new_id = match AnnotationId::parse(new_raw) {
        Ok(id) => id,
        Err(e) => {
            return Err(reject(format!(
                "target id `{new_raw}` is not a valid annotation id ({e}).\n\
                 Pick a snake_case id (letters / digits / underscores; \
                 first char letter or underscore)."
            )));
        }
    };

    // Step 3 — namespace rules.
    let new_ns = new_id.namespace();
    match new_ns {
        // bare → aret_X (F1-b: reject readable → opaque).
        IdNamespace::Opaque => {
            return Err(reject(format!(
                "id `{new_raw}` uses the reserved `aret_` prefix (stamp-assigned only).\n       \
                 Renaming a readable id to an opaque one is not supported.\n       \
                 Note: `aristos:` is also reserved; it may only appear via\n       \
                 `aristo sync` binding, never via `aristo rename`.\n       \
                 If you intended to make this annotation unaliased, delete the `id` arg\n       \
                 in source and re-run `aristo stamp` — stamp will assign an opaque id."
            )));
        }
        IdNamespace::Aristos => {
            // anything → aristos:* — reject. Distinguish two sub-cases for
            // a cleaner message: if old is local, it's a cross-namespace
            // bind attempt; if old is opaque, same. Either way the user
            // wants `aristo sync` (deferred).
            return Err(reject_aristos_deferred(new_raw));
        }
        IdNamespace::Local => {}
    }

    // Step 4 — target collision.
    if index.entries.contains_key(&new_id) {
        let site = site_for_collision(index, &new_id);
        return Err(reject(format!(
            "id `{new_raw}` is already in use at {site}.\n       \
             Pick a different id or delete the conflicting annotation first."
        )));
    }

    // Step 5 — shape classification (drives the per-shape success note).
    let shape = match old_id.namespace() {
        IdNamespace::Local => RenameShape::LocalToLocal,
        IdNamespace::Opaque => RenameShape::OpaquePromotion,
        // Aristos was rejected above.
        IdNamespace::Aristos => unreachable!(),
    };

    Ok(ParsedRename {
        old_id,
        new_id,
        shape,
    })
}

/// Construct the canonical "aristos: is deferred to Phase 2" diagnostic.
/// Same message for either-side `aristos:` rejection — the user's next
/// action is the same (wait for sync, or use bare ids).
fn reject_aristos_deferred(raw: &str) -> CliError {
    if raw.starts_with("aristos:") {
        reject(
            "the `aristos:` namespace is reserved for server-bound ids\n       \
             (Phase 2). `aristo rename` is local-only in this release; the\n       \
             rebind / unbind surface ships with `aristo sync`.\n       \
             For bare → bare or `aret_*` → bare renames, use this command.\n       \
             For aristos: ids, wait for Phase 2 sync."
                .to_string(),
        )
    } else {
        reject(format!(
            "id `{raw}` looks like a misformed `aristos:` reference.\n       \
             The `aristos:` namespace is reserved for server-bound ids\n       \
             (Phase 2) and may not appear as a rename source or target."
        ))
    }
}

fn reject(message: String) -> CliError {
    CliError::Other {
        message,
        exit_code: 1,
    }
}

/// Best-effort site string for the collision-error diagnostic. Returns
/// `<unknown>` if the entry is not an intent / assume with file+site
/// fields (defensive — should be unreachable, but the message renders
/// regardless).
fn site_for_collision(index: &IndexFile, id: &AnnotationId) -> String {
    use aristo_core::index::IndexEntry;
    match index.entries.get(id) {
        Some(IndexEntry::Intent(e)) => format!("{}:{}", e.file, e.site),
        Some(IndexEntry::Assume(e)) => format!("{}:{}", e.file, e.site),
        None => "<unknown>".to_string(),
    }
}

// ─── workspace IO (shared with other commands) ────────────────────────────

fn read_index(path: &Path) -> CliResult<IndexFile> {
    if !path.is_file() {
        return Err(CliError::Other {
            message: format!(
                "no .aristo/index.toml at {}\n\
                 hint: run `aristo stamp` (or `aristo index`) to build one",
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

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AnnotationId, AssumeEntry, BindingState, CoveredRegion, IndexEntry, IndexFile, IntentEntry,
        Meta, Sha256, Status, VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(file: &str, site: &str) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify: VerifyLevel::Method(VerifyMethod::Test),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: file.into(),
            site: site.into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn assume(file: &str, site: &str) -> IndexEntry {
        IndexEntry::Assume(AssumeEntry {
            text: "x".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: file.into(),
            site: site.into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn build_index(entries: &[(&str, IndexEntry)]) -> IndexFile {
        let mut map = BTreeMap::new();
        for (id, e) in entries {
            map.insert(AnnotationId::parse(id).unwrap(), e.clone());
        }
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: map,
        }
    }

    // ─── happy paths ─────────────────────────────────────────────────────

    #[test]
    fn local_to_local_rename_succeeds_with_local_shape() {
        let index = build_index(&[("foo", intent("src/x.rs", "fn foo (line 1)"))]);
        let parsed = parse_and_validate("foo", "bar", &index).expect("legal rename");
        assert_eq!(parsed.old_id.as_str(), "foo");
        assert_eq!(parsed.new_id.as_str(), "bar");
        assert_eq!(parsed.shape, RenameShape::LocalToLocal);
    }

    #[test]
    fn opaque_to_local_rename_succeeds_with_promotion_shape() {
        let index = build_index(&[(
            "aret_a1b2c3d4",
            intent("src/x.rs", "fn opaque_site (line 1)"),
        )]);
        let parsed = parse_and_validate("aret_a1b2c3d4", "post_balance_validator", &index)
            .expect("opaque → readable promotion is legal (F1-c)");
        assert_eq!(parsed.shape, RenameShape::OpaquePromotion);
    }

    // ─── source-id rejections ────────────────────────────────────────────

    #[test]
    fn unknown_source_id_rejected_with_list_hint() {
        let index = build_index(&[]);
        let err = parse_and_validate("ghost", "phantom", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ghost"), "msg: {msg}");
        assert!(msg.contains("not found"), "msg: {msg}");
        assert!(msg.contains("aristo list"), "msg: {msg}");
    }

    #[test]
    fn invalid_source_id_rejected_with_parse_diagnostic() {
        let index = build_index(&[]);
        let err = parse_and_validate("Bad-Name", "ok", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Bad-Name"), "msg: {msg}");
        assert!(msg.contains("not a valid annotation id"), "msg: {msg}");
    }

    // ─── target-id rejections ────────────────────────────────────────────

    #[test]
    fn target_collision_rejected_with_site_hint() {
        let index = build_index(&[
            ("source_id", intent("src/x.rs", "fn source (line 1)")),
            ("taken", intent("src/y.rs", "fn taken (line 42)")),
        ]);
        let err = parse_and_validate("source_id", "taken", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("`taken`"), "msg: {msg}");
        assert!(msg.contains("already in use"), "msg: {msg}");
        assert!(msg.contains("src/y.rs"), "msg: {msg}");
        assert!(msg.contains("fn taken"), "msg: {msg}");
    }

    #[test]
    fn target_collision_with_assume_entry_renders_site() {
        let index = build_index(&[
            ("from_id", intent("src/x.rs", "fn s (line 1)")),
            ("taken_by_assume", assume("src/z.rs", "fn z (line 7)")),
        ]);
        let err = parse_and_validate("from_id", "taken_by_assume", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("src/z.rs"), "msg: {msg}");
        assert!(msg.contains("already in use"), "msg: {msg}");
    }

    #[test]
    fn target_with_reserved_aret_prefix_rejected_per_f1b() {
        let index = build_index(&[("foo", intent("src/x.rs", "fn s (line 1)"))]);
        let err = parse_and_validate("foo", "aret_xyz1234", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("reserved `aret_` prefix"), "msg: {msg}");
        assert!(msg.contains("stamp-assigned only"), "msg: {msg}");
        assert!(msg.contains("aristo stamp"), "msg: {msg}");
    }

    // ─── aristos: rejection (scope trim) ─────────────────────────────────

    #[test]
    fn source_aristos_id_rejected_as_deferred_to_phase_2() {
        let index = build_index(&[(
            "aristos:foo",
            intent("src/x.rs", "fn aristos_site (line 1)"),
        )]);
        let err = parse_and_validate("aristos:foo", "bar", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristos:"), "msg: {msg}");
        assert!(msg.contains("reserved"), "msg: {msg}");
        assert!(msg.contains("Phase 2"), "msg: {msg}");
    }

    #[test]
    fn target_aristos_id_rejected_as_deferred_to_phase_2() {
        let index = build_index(&[("foo", intent("src/x.rs", "fn foo_site (line 1)"))]);
        let err = parse_and_validate("foo", "aristos:foo", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristos:"), "msg: {msg}");
        assert!(msg.contains("Phase 2"), "msg: {msg}");
    }

    #[test]
    fn cross_namespace_aristos_to_bare_rejected_with_phase_2_message() {
        // Per the scope trim, cross-namespace renames are also rejected
        // via the same "aristos: deferred" path — sync (Phase 2) carries
        // the unbind surface.
        let index = build_index(&[(
            "aristos:foo",
            intent("src/x.rs", "fn aristos_site (line 1)"),
        )]);
        let err = parse_and_validate("aristos:foo", "bar_local", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristos:"), "msg: {msg}");
        assert!(msg.contains("Phase 2"), "msg: {msg}");
    }
}
