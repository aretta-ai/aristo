//! `aristo rename <old_id> <new_id>` — atomic coordinated rename across
//! source files, `.aristo/index.toml`, and per-id artifact files
//! (`.aristo/critiques/<id>.critique`, `.aristo/proofs/<id>.proof`).
//!
//! Slice 32 scope (locked 2026-05-18, per `HANDOFF-SLICE-32.md`),
//! updated for §13 canon-and-matching (canon-strategy.md §CS13,
//! locked 2026-05-19):
//!
//! - **In:** bare → bare; `aret_*` → bare (with promotion note); target
//!   collision rejection; reserved-prefix (`aret_*`) target rejection
//!   (F1-b); canon-bound namespace (`aristos:` and `kanon:`) rejection
//!   in either direction (canon prefixes are applied / removed via the
//!   canon accept path + `aristo canon unbind`, never via rename).
//! - **Out (per §CS13: subsumed by canon accept path; no Phase 2
//!   `aristo sync`):** canon prefix application, canon binding
//!   removal, transactional rollback. Use
//!   `aristo critique --apply-findings` to apply a canon binding and
//!   `aristo canon unbind` to remove one.
//!
//! Commit 2 shipped the dispatch + validation skeleton. Commit 3 (THIS
//! commit) ships the dry-run plan + renderer. Commit 4 (next) applies
//! the plan to disk.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::index::{AnnotationId, IdNamespace, IndexEntry, IndexFile, ParentLink};
use aristo_core::walk::{scan_id_occurrences, IdOccurrence, IdOccurrenceKind};

use crate::commands::index::workspace_or_error;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult, Workspace};

/// Entry point invoked from `lib::dispatch`.
pub(crate) fn run(old_id: &str, new_id: &str, dry_run: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    let parsed = parse_and_validate(old_id, new_id, &index)?;
    let plan = compute_plan(&ws, &index, &parsed)?;

    if dry_run {
        print!("{plan}");
        return Ok(());
    }
    apply_plan(&ws, &index, plan)
}

// ─── Apply: source rewrite → artifact move → index write (last) ───────────

#[aristo::intent(
    "Apply order is load-bearing: source files first (in any order; each \
     is a single atomic temp+rename), THEN artifact moves, THEN the new \
     index.toml LAST (atomic). The reason: if source writes complete but \
     artifact-move or index-write fails, source has the new ids but the \
     index still references the old ones. `aristo stamp` detects this \
     and refuses with structural drift — the user reverts or completes \
     manually. The reverse order (index first, source last) would leave \
     the user with an index pointing at ids the source doesn't define, \
     making `aristo show` / `aristo list` lie. There is no transactional \
     rollback; 'best-effort recoverable' is the contract.",
    verify = "test",
    id = "rename_writes_index_last_for_recoverable_partial_failure"
)]
fn apply_plan(ws: &Workspace, index: &IndexFile, plan: RenamePlan) -> CliResult<()> {
    // 1. Source files. Group edits by file; apply highest-byte-first so
    // each substitution leaves earlier byte offsets stable.
    let mut by_file: std::collections::BTreeMap<&str, Vec<&SourceEdit>> =
        std::collections::BTreeMap::new();
    for edit in &plan.source_edits {
        by_file.entry(edit.file.as_str()).or_default().push(edit);
    }
    for (file_rel, edits) in &mut by_file {
        edits.sort_by_key(|e| std::cmp::Reverse(e.byte_start));
        let abs = ws.root.join(file_rel);
        let mut bytes = fs::read(&abs).map_err(CliError::Io)?;
        for edit in edits.iter() {
            bytes.splice(edit.byte_start..edit.byte_end, plan.new_id.as_str().bytes());
        }
        atomic_write_bytes(&abs, &bytes)?;
    }

    // 2. Artifact moves. Each is a single rename — no atomic temp dance
    // (the destination is a fresh id-safe filename; collisions were
    // ruled out at validation since the new id wasn't in the index).
    for mv in &plan.artifact_moves {
        if let Some(parent) = mv.to.parent() {
            fs::create_dir_all(parent).map_err(CliError::Io)?;
        }
        fs::rename(&mv.from, &mv.to).map_err(CliError::Io)?;
    }

    // 3. Index. Build the new IndexFile in memory, then atomic-write.
    let new_index = rewrite_index(index, &plan);
    let toml_text = toml::to_string_pretty(&new_index).map_err(|e| CliError::Other {
        message: format!("rename: serializing new index: {e}"),
        exit_code: 1,
    })?;
    let index_path = ws.index_path();
    atomic_write_bytes(&index_path, toml_text.as_bytes())?;

    print_apply_summary(&plan);
    Ok(())
}

/// Build the post-rename IndexFile: walk every entry, swap the self-key
/// for the renamed entry, and rewrite every child entry's `parent` link
/// that references the old id.
fn rewrite_index(index: &IndexFile, plan: &RenamePlan) -> IndexFile {
    use aristo_core::index::IndexFile;
    let mut new_entries = std::collections::BTreeMap::new();
    for (id, entry) in &index.entries {
        let new_id = if id == &plan.old_id {
            plan.new_id.clone()
        } else {
            id.clone()
        };
        let new_entry = rewrite_entry_parent(entry, &plan.old_id, &plan.new_id);
        new_entries.insert(new_id, new_entry);
    }
    IndexFile {
        meta: index.meta.clone(),
        entries: new_entries,
    }
}

fn rewrite_entry_parent(entry: &IndexEntry, old: &AnnotationId, new: &AnnotationId) -> IndexEntry {
    match entry {
        IndexEntry::Intent(e) => {
            let mut e = e.clone();
            e.parent = e.parent.map(|p| rewrite_parent_link(p, old, new));
            IndexEntry::Intent(e)
        }
        IndexEntry::Assume(e) => {
            let mut e = e.clone();
            e.parent = e.parent.map(|p| rewrite_parent_link(p, old, new));
            IndexEntry::Assume(e)
        }
    }
}

fn rewrite_parent_link(p: ParentLink, old: &AnnotationId, new: &AnnotationId) -> ParentLink {
    match p {
        ParentLink::Single(id) if &id == old => ParentLink::Single(new.clone()),
        ParentLink::Single(id) => ParentLink::Single(id),
        ParentLink::Multiple(ids) => ParentLink::Multiple(
            ids.into_iter()
                .map(|id| if &id == old { new.clone() } else { id })
                .collect(),
        ),
    }
}

fn print_apply_summary(plan: &RenamePlan) {
    let edit_count = plan.source_edits.len();
    let parent_updates = plan
        .index_updates
        .iter()
        .filter(|u| !matches!(u.kind, IndexUpdateKind::SelfKey))
        .count();
    let artifact_count = plan.artifact_moves.len();
    println!(
        "ok: renamed `{}` → `{}` ({} source edits, {} parent references, {} artifact files)",
        plan.old_id.as_str(),
        plan.new_id.as_str(),
        edit_count,
        parent_updates,
        artifact_count
    );
    if matches!(plan.shape, RenameShape::OpaquePromotion) {
        println!(
            "note: promoted opaque id → readable id. Future references to\n      \
             `{}` will fail. Update any external dashboards / links.",
            plan.old_id.as_str()
        );
    }
}

/// Atomic write for arbitrary file extensions: write `<path>.aristo-tmp`
/// then rename over `<path>`. Differs from `commands::index::atomic_write`,
/// which hardcodes a `.toml.tmp` suffix and only takes `&str` content.
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
                    "rename: cannot atomic-write `{}` — path has no file name",
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

#[derive(Debug, Clone)]
pub(crate) struct ParsedRename {
    pub old_id: AnnotationId,
    pub new_id: AnnotationId,
    pub shape: RenameShape,
}

// ─── Plan model ───────────────────────────────────────────────────────────

/// What `aristo rename` would change. Computed once before either
/// printing (dry-run) or applying; the apply path consumes the same
/// value.
#[derive(Debug, Clone)]
pub(crate) struct RenamePlan {
    pub old_id: AnnotationId,
    pub new_id: AnnotationId,
    pub shape: RenameShape,
    /// Byte-range edits across source files. Ordered: same file edits
    /// run highest-byte-first so earlier byte indexes stay stable.
    pub source_edits: Vec<SourceEdit>,
    /// Per-id artifact file renames (`.critique` + `.proof`). Empty if
    /// no artifacts exist for `old_id`.
    pub artifact_moves: Vec<ArtifactMove>,
    /// In-index updates: the self-key rename plus every parent-link
    /// rewrite. Computed deterministically from the index.
    pub index_updates: Vec<IndexUpdate>,
}

#[derive(Debug, Clone)]
pub(crate) struct SourceEdit {
    /// Workspace-relative path (matches the `file` field stored in the
    /// index entries — and what the spec rendering shows the user).
    pub file: String,
    /// 1-indexed line of the opening quote.
    pub line: usize,
    pub kind: SourceEditKind,
    /// Byte range of the literal CONTENTS (no surrounding quotes) in
    /// the file's bytes. Used by commit 4's substituter.
    pub byte_start: usize,
    pub byte_end: usize,
    /// For `ParentArrayElement`: the BEFORE-line shown to the user
    /// (e.g., `parent = ["foo", "bar"]`). Computed from the surrounding
    /// source so the display matches the user's actual text.
    pub before_array_text: Option<String>,
    /// For `ParentArrayElement`: the AFTER-line (same array with this
    /// one element substituted to `new_id`).
    pub after_array_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceEditKind {
    Id,
    ParentSingle,
    ParentArrayElement,
}

#[derive(Debug, Clone)]
pub(crate) struct ArtifactMove {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Display label for the dry-run output, e.g. ".aristo/critiques/<id>.critique".
    pub from_label: String,
    pub to_label: String,
}

#[derive(Debug, Clone)]
pub(crate) struct IndexUpdate {
    pub kind: IndexUpdateKind,
}

#[derive(Debug, Clone)]
pub(crate) enum IndexUpdateKind {
    /// The renamed entry's own table key.
    SelfKey,
    /// A child entry's singular `parent = "<old>"`.
    ChildParentSingle { child_id: AnnotationId },
    /// A child entry's array `parent = [.., "<old>", ..]`.
    ChildParentArray {
        child_id: AnnotationId,
        all_elements: Vec<AnnotationId>,
    },
}

// ─── Plan computation ─────────────────────────────────────────────────────

#[aristo::intent(
    "Plan computation reads each candidate source file ONCE, then \
     defers all writes to commit 4. The candidate file set is the union \
     of (the renamed entry's own file) + (every file containing an \
     entry whose parent references the renamed id) — the index alone \
     determines this set, no broad source walk. If the scan finds zero \
     `id = \"old\"` occurrences in the entry's own file, the rename \
     refuses with a stale-index diagnostic rather than producing a \
     misleading partial plan; the index says one occurrence MUST exist \
     and its absence is structural drift the user needs to know about.",
    verify = "test",
    id = "rename_plan_reads_only_index_referenced_files_once"
)]
fn compute_plan(ws: &Workspace, index: &IndexFile, parsed: &ParsedRename) -> CliResult<RenamePlan> {
    let mut candidate_files: BTreeSet<String> = BTreeSet::new();
    // 1a. The renamed entry's own file (carries the `id = "..."` site).
    let owner_entry = index
        .entries
        .get(&parsed.old_id)
        .expect("validation guarantees old_id is in the index");
    candidate_files.insert(entry_file(owner_entry).to_string());
    // 1b. Files containing any child entry whose parent references old_id.
    for entry in index.entries.values() {
        if parent_references(entry, &parsed.old_id) {
            candidate_files.insert(entry_file(entry).to_string());
        }
    }

    // 2. Scan each candidate file and collect SourceEdits.
    let mut source_edits = Vec::new();
    for file_rel in &candidate_files {
        let abs = ws.root.join(file_rel);
        let source = fs::read_to_string(&abs).map_err(|e| CliError::Other {
            message: format!(
                "rename: cannot read `{}`: {e}\n\
                 hint: the index references this file; run `aristo stamp` if you have moved or removed it.",
                abs.display()
            ),
            exit_code: 1,
        })?;
        let occurrences = scan_id_occurrences(&source).map_err(|e| CliError::Other {
            message: format!("rename: failed to parse `{}` as Rust: {e}", abs.display()),
            exit_code: 1,
        })?;
        for occ in occurrences {
            if occ.value != parsed.old_id.as_str() {
                continue;
            }
            let kind = match occ.kind {
                IdOccurrenceKind::Id => SourceEditKind::Id,
                IdOccurrenceKind::ParentSingle => SourceEditKind::ParentSingle,
                IdOccurrenceKind::ParentArrayElement => SourceEditKind::ParentArrayElement,
            };
            let (before_array_text, after_array_text) =
                if matches!(occ.kind, IdOccurrenceKind::ParentArrayElement) {
                    render_array_context(&source, &occ, parsed.new_id.as_str())
                } else {
                    (None, None)
                };
            source_edits.push(SourceEdit {
                file: file_rel.clone(),
                line: occ.line,
                kind,
                byte_start: occ.byte_start,
                byte_end: occ.byte_end,
                before_array_text,
                after_array_text,
            });
        }
    }
    // Refuse on stale-index drift: the owner file must contain at least
    // one `id = "<old>"` occurrence for the rename to make sense.
    let owner_file = entry_file(owner_entry);
    let has_owner_id_edit = source_edits
        .iter()
        .any(|e| e.file == owner_file && e.kind == SourceEditKind::Id);
    if !has_owner_id_edit {
        return Err(CliError::Other {
            message: format!(
                "rename: index says `{}` lives in `{}` but no `id = \"{}\"` \
                 occurrence was found in that file.\n\
                 hint: source has drifted from the index. Run `aristo stamp` and retry.",
                parsed.old_id.as_str(),
                owner_file,
                parsed.old_id.as_str()
            ),
            exit_code: 1,
        });
    }

    // 3. Artifact moves (`.critique` + `.proof`).
    let mut artifact_moves = Vec::new();
    for (subdir, ext) in [("critiques", "critique"), ("proofs", "proof")] {
        let old_name = format!("{}.{ext}", id_safe(parsed.old_id.as_str()));
        let new_name = format!("{}.{ext}", id_safe(parsed.new_id.as_str()));
        let from_path = ws.aristo_dir().join(subdir).join(&old_name);
        if from_path.is_file() {
            let to_path = ws.aristo_dir().join(subdir).join(&new_name);
            artifact_moves.push(ArtifactMove {
                from: from_path,
                to: to_path,
                from_label: format!(".aristo/{subdir}/{old_name}"),
                to_label: format!(".aristo/{subdir}/{new_name}"),
            });
        }
    }

    // 4. Index updates: self-key + parent rewrites on children.
    let mut index_updates = vec![IndexUpdate {
        kind: IndexUpdateKind::SelfKey,
    }];
    for (child_id, entry) in &index.entries {
        match entry_parent(entry) {
            Some(ParentLink::Single(p)) if p == &parsed.old_id => {
                index_updates.push(IndexUpdate {
                    kind: IndexUpdateKind::ChildParentSingle {
                        child_id: child_id.clone(),
                    },
                });
            }
            Some(ParentLink::Multiple(ids)) if ids.contains(&parsed.old_id) => {
                index_updates.push(IndexUpdate {
                    kind: IndexUpdateKind::ChildParentArray {
                        child_id: child_id.clone(),
                        all_elements: ids.clone(),
                    },
                });
            }
            _ => {}
        }
    }

    Ok(RenamePlan {
        old_id: parsed.old_id.clone(),
        new_id: parsed.new_id.clone(),
        shape: parsed.shape.clone(),
        source_edits,
        artifact_moves,
        index_updates,
    })
}

/// Given a `ParentArrayElement` occurrence, scan the surrounding bytes
/// for the enclosing `parent = [ ... ]` literal so the dry-run can
/// render BEFORE / AFTER with the full array contents. Returns
/// `(None, None)` if the surrounding `[ ... ]` extent can't be
/// determined — the renderer falls back to a single-line representation
/// in that case.
fn render_array_context(
    source: &str,
    occ: &IdOccurrence,
    new_id: &str,
) -> (Option<String>, Option<String>) {
    let bytes = source.as_bytes();
    // Walk backward from the element's opening quote to find the `[`.
    // Stop at `parent` boundary or fail.
    let mut i = occ.byte_start.saturating_sub(1);
    while i > 0 && bytes[i] != b'[' {
        i -= 1;
    }
    if bytes[i] != b'[' {
        return (None, None);
    }
    let bracket_open = i;
    // Walk back further to find `parent` keyword start.
    let mut p = bracket_open;
    while p > 0 && bytes[p] != b'p' {
        p -= 1;
    }
    // Be defensive about matching "parent" exactly to avoid false hits.
    if bytes.get(p..p + 6) != Some(b"parent") {
        return (None, None);
    }
    let parent_kw_start = p;
    // Walk forward from the element's closing quote to find `]`.
    let mut j = occ.byte_end;
    while j < bytes.len() && bytes[j] != b']' {
        j += 1;
    }
    if j >= bytes.len() {
        return (None, None);
    }
    let bracket_close = j;
    let before = match std::str::from_utf8(&bytes[parent_kw_start..=bracket_close]) {
        Ok(s) => s.to_string(),
        Err(_) => return (None, None),
    };
    // AFTER: replace just this element's value-bytes with new_id.
    let mut after_bytes: Vec<u8> = bytes[parent_kw_start..=bracket_close].to_vec();
    let rel_start = occ.byte_start - parent_kw_start;
    let rel_end = occ.byte_end - parent_kw_start;
    after_bytes.splice(rel_start..rel_end, new_id.bytes());
    let after = match String::from_utf8(after_bytes) {
        Ok(s) => s,
        Err(_) => return (None, None),
    };
    (Some(before), Some(after))
}

// ─── Plan rendering ───────────────────────────────────────────────────────

impl fmt::Display for RenamePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(
            f,
            "Plan: rename `{}` → `{}`",
            self.old_id.as_str(),
            self.new_id.as_str()
        )?;
        writeln!(f)?;
        writeln!(f, "Source edits:")?;
        let old = self.old_id.as_str();
        let new = self.new_id.as_str();
        for edit in &self.source_edits {
            let location = format!("  {}:{}", edit.file, edit.line);
            match edit.kind {
                SourceEditKind::Id => {
                    writeln!(f, "{location}    id = \"{old}\"   →   id = \"{new}\"")?;
                }
                SourceEditKind::ParentSingle => {
                    writeln!(
                        f,
                        "{location}    parent = \"{old}\"   →   parent = \"{new}\""
                    )?;
                }
                SourceEditKind::ParentArrayElement => {
                    match (&edit.before_array_text, &edit.after_array_text) {
                        (Some(b), Some(a)) => {
                            writeln!(f, "{location}    {b}")?;
                            writeln!(f, "                       →   {a}")?;
                        }
                        _ => {
                            // Fallback when the surrounding `[ ... ]` could
                            // not be parsed from source — give a minimal
                            // single-line representation rather than
                            // suppressing the edit entirely.
                            writeln!(
                                f,
                                "{location}    parent[..] = \"{old}\"   →   parent[..] = \"{new}\""
                            )?;
                        }
                    }
                }
            }
        }
        if !self.artifact_moves.is_empty() {
            writeln!(f)?;
            writeln!(f, "Artifact files:")?;
            for mv in &self.artifact_moves {
                writeln!(f, "  {} → {}", mv.from_label, mv.to_label)?;
            }
        }
        writeln!(f)?;
        writeln!(f, "Index updates:")?;
        for upd in &self.index_updates {
            match &upd.kind {
                IndexUpdateKind::SelfKey => {
                    writeln!(f, "  [\"{old}\"]  →  [\"{new}\"]")?;
                }
                IndexUpdateKind::ChildParentSingle { child_id } => {
                    writeln!(f, "  {}.parent: \"{old}\" → \"{new}\"", child_id.as_str())?;
                }
                IndexUpdateKind::ChildParentArray {
                    child_id,
                    all_elements,
                } => {
                    let before = render_array(all_elements.iter().map(|id| id.as_str()));
                    let after = render_array(all_elements.iter().map(|id| {
                        if id == &self.old_id {
                            new
                        } else {
                            id.as_str()
                        }
                    }));
                    writeln!(f, "  {}.parent: {before} → {after}", child_id.as_str())?;
                }
            }
        }
        writeln!(f)?;
        writeln!(f, "(no changes written — dry-run)")?;
        Ok(())
    }
}

fn render_array<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let inner: Vec<String> = items.map(|s| format!("\"{s}\"")).collect();
    format!("[{}]", inner.join(", "))
}

// ─── helpers (id-safe, parent/file readout) ───────────────────────────────

fn entry_file(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

fn entry_parent(entry: &IndexEntry) -> Option<&ParentLink> {
    match entry {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }
}

fn parent_references(entry: &IndexEntry, id: &AnnotationId) -> bool {
    match entry_parent(entry) {
        Some(ParentLink::Single(p)) => p == id,
        Some(ParentLink::Multiple(ids)) => ids.contains(id),
        None => false,
    }
}

fn id_safe(id: &str) -> String {
    id.replace(':', "__")
}

// ─── validation (shipped in commit 2; unchanged here) ─────────────────────

#[aristo::intent(
    "Rename validation rejects three classes BEFORE any plan computation: \
     (1) canon-bound prefix (`aristos:` or `kanon:`) in either old or \
     new id — canon prefixes are applied exclusively by the canon \
     accept path and removed by `aristo canon unbind` per CS13, so the \
     surface lies; \
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
    let old_id = match AnnotationId::parse(old_raw) {
        Ok(id) => id,
        Err(e) => {
            return Err(reject(format!(
                "source id `{old_raw}` is not a valid annotation id ({e}).\n\
                 Run `aristo list` to see indexed ids."
            )));
        }
    };
    if matches!(
        old_id.namespace(),
        IdNamespace::Aristos | IdNamespace::Kanon
    ) {
        return Err(reject_canon_bound_id(old_raw));
    }
    if !index.entries.contains_key(&old_id) {
        return Err(reject(format!(
            "source id `{old_raw}` not found in .aristo/index.toml.\n\
             Run `aristo list` to see indexed ids, or `aristo stamp` if \
             you have just edited source."
        )));
    }
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
    let new_ns = new_id.namespace();
    match new_ns {
        IdNamespace::Opaque => {
            return Err(reject(format!(
                "id `{new_raw}` uses the reserved `aret_` prefix (stamp-assigned only).\n       \
                 Renaming a readable id to an opaque one is not supported.\n       \
                 Note: `aristos:` and `kanon:` are also reserved; they may only appear\n       \
                 via the canon accept path (`aristo critique --apply-findings`),\n       \
                 never via `aristo rename`.\n       \
                 If you intended to make this annotation unaliased, delete the `id` arg\n       \
                 in source and re-run `aristo stamp` — stamp will assign an opaque id."
            )));
        }
        IdNamespace::Aristos | IdNamespace::Kanon => {
            return Err(reject_canon_bound_id(new_raw));
        }
        IdNamespace::Local => {}
    }
    if index.entries.contains_key(&new_id) {
        let site = site_for_collision(index, &new_id);
        return Err(reject(format!(
            "id `{new_raw}` is already in use at {site}.\n       \
             Pick a different id or delete the conflicting annotation first."
        )));
    }
    let shape = match old_id.namespace() {
        IdNamespace::Local => RenameShape::LocalToLocal,
        IdNamespace::Opaque => RenameShape::OpaquePromotion,
        // Canon-bound ids (aristos: and kanon:) are rejected upstream
        // — they're removed via `aristo canon unbind`, not renamed.
        IdNamespace::Aristos | IdNamespace::Kanon => unreachable!(),
    };
    Ok(ParsedRename {
        old_id,
        new_id,
        shape,
    })
}

fn reject_canon_bound_id(raw: &str) -> CliError {
    let prefix = if raw.starts_with("aristos:") {
        "aristos:"
    } else if raw.starts_with("kanon:") {
        "kanon:"
    } else {
        // Defensive: caller checked the namespace; fall back to a
        // generic message if somehow the raw doesn't match.
        return reject(format!(
            "id `{raw}` looks like a misformed canon-bound reference.\n       \
             The `aristos:` and `kanon:` namespaces are reserved for canon-bound\n       \
             ids and may not appear as a rename source or target."
        ));
    };
    reject(format!(
        "the `{prefix}` namespace is reserved for canon-bound ids and may not\n       \
         appear as a rename source or target. `aristo rename` is for bare → bare\n       \
         or `aret_*` → bare renames only.\n       \
         To remove a canon binding, use `aristo canon unbind <{prefix}<id>>`.\n       \
         Canon prefixes are applied exclusively by the canon accept path\n       \
         (`aristo critique --apply-findings`)."
    ))
}

fn reject(message: String) -> CliError {
    CliError::Other {
        message,
        exit_code: 1,
    }
}

fn site_for_collision(index: &IndexFile, id: &AnnotationId) -> String {
    match index.entries.get(id) {
        Some(IndexEntry::Intent(e)) => format!("{}:{}", e.file, e.site),
        Some(IndexEntry::Assume(e)) => format!("{}:{}", e.file, e.site),
        None => "<unknown>".to_string(),
    }
}

// ─── workspace IO ─────────────────────────────────────────────────────────

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

    fn intent_with_parent(file: &str, site: &str, parent: ParentLink) -> IndexEntry {
        match intent(file, site) {
            IndexEntry::Intent(mut e) => {
                e.parent = Some(parent);
                IndexEntry::Intent(e)
            }
            _ => unreachable!(),
        }
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

    // ─── canon-bound rejection (aristos: + kanon:, per §CS13) ──────────

    #[test]
    fn source_aristos_id_rejected_as_canon_bound() {
        let index = build_index(&[(
            "aristos:foo",
            intent("src/x.rs", "fn aristos_site (line 1)"),
        )]);
        let err = parse_and_validate("aristos:foo", "bar", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristos:"), "msg: {msg}");
        assert!(msg.contains("reserved"), "msg: {msg}");
        assert!(msg.contains("aristo canon unbind"), "msg: {msg}");
    }

    #[test]
    fn target_aristos_id_rejected_as_canon_bound() {
        let index = build_index(&[("foo", intent("src/x.rs", "fn foo_site (line 1)"))]);
        let err = parse_and_validate("foo", "aristos:foo", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristos:"), "msg: {msg}");
        assert!(msg.contains("aristo canon unbind"), "msg: {msg}");
    }

    #[test]
    fn cross_namespace_aristos_to_bare_rejected_as_canon_bound() {
        let index = build_index(&[(
            "aristos:foo",
            intent("src/x.rs", "fn aristos_site (line 1)"),
        )]);
        let err = parse_and_validate("aristos:foo", "bar_local", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristos:"), "msg: {msg}");
        assert!(msg.contains("aristo canon unbind"), "msg: {msg}");
    }

    #[test]
    fn source_kanon_id_rejected_as_canon_bound() {
        let index = build_index(&[("kanon:foo", intent("src/x.rs", "fn kanon_site (line 1)"))]);
        let err = parse_and_validate("kanon:foo", "bar", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("kanon:"), "msg: {msg}");
        assert!(msg.contains("aristo canon unbind"), "msg: {msg}");
    }

    #[test]
    fn target_kanon_id_rejected_as_canon_bound() {
        let index = build_index(&[("foo", intent("src/x.rs", "fn foo_site (line 1)"))]);
        let err = parse_and_validate("foo", "kanon:foo", &index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("kanon:"), "msg: {msg}");
        assert!(msg.contains("aristo canon unbind"), "msg: {msg}");
    }

    // ─── Plan computation + rendering ────────────────────────────────────

    fn ws_with_source(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("aristo.toml"), "").unwrap();
        fs::create_dir_all(dir.path().join(".aristo")).unwrap();
        for (rel, contents) in files {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, contents).unwrap();
        }
        let ws = Workspace {
            root: dir.path().to_path_buf(),
        };
        (dir, ws)
    }

    #[test]
    fn plan_collects_id_edit_for_renamed_entry_own_file() {
        let src = r#"
            #[aristo::intent("the claim", verify = "test", id = "old_id")]
            fn f() {}
        "#;
        let (_dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("old_id", intent("src/lib.rs", "fn f (line 2)"))]);
        let parsed = parse_and_validate("old_id", "new_id", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        let id_edits: Vec<_> = plan
            .source_edits
            .iter()
            .filter(|e| e.kind == SourceEditKind::Id)
            .collect();
        assert_eq!(id_edits.len(), 1);
        assert_eq!(id_edits[0].file, "src/lib.rs");
    }

    #[test]
    fn plan_collects_parent_single_edit_from_child_file() {
        let owner_src = r#"
            #[aristo::intent("p", id = "parent_id")]
            fn p() {}
        "#;
        let child_src = r#"
            #[aristo::intent("c", parent = "parent_id", id = "child_id")]
            fn c() {}
        "#;
        let (_dir, ws) = ws_with_source(&[("src/p.rs", owner_src), ("src/c.rs", child_src)]);
        let mut entries = vec![
            ("parent_id", intent("src/p.rs", "fn p (line 2)")),
            (
                "child_id",
                intent_with_parent(
                    "src/c.rs",
                    "fn c (line 2)",
                    ParentLink::Single(AnnotationId::parse("parent_id").unwrap()),
                ),
            ),
        ];
        let index = build_index(&entries);
        let _ = &mut entries;
        let parsed = parse_and_validate("parent_id", "new_parent", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        let parent_edits: Vec<_> = plan
            .source_edits
            .iter()
            .filter(|e| e.kind == SourceEditKind::ParentSingle)
            .collect();
        assert_eq!(parent_edits.len(), 1);
        assert_eq!(parent_edits[0].file, "src/c.rs");
    }

    #[test]
    fn plan_collects_each_parent_array_element_with_array_context() {
        let owner_src = r#"
            #[aristo::intent("p", id = "parent_id")]
            fn p() {}
        "#;
        let child_src = r#"
            #[aristo::intent("c", parent = ["parent_id", "other"], id = "child_id")]
            fn c() {}
        "#;
        let (_dir, ws) = ws_with_source(&[("src/p.rs", owner_src), ("src/c.rs", child_src)]);
        let index = build_index(&[
            ("parent_id", intent("src/p.rs", "fn p (line 2)")),
            ("other", intent("src/p.rs", "fn other (line 5)")),
            (
                "child_id",
                intent_with_parent(
                    "src/c.rs",
                    "fn c (line 2)",
                    ParentLink::Multiple(vec![
                        AnnotationId::parse("parent_id").unwrap(),
                        AnnotationId::parse("other").unwrap(),
                    ]),
                ),
            ),
        ]);
        let parsed = parse_and_validate("parent_id", "new_parent", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        let array_edits: Vec<_> = plan
            .source_edits
            .iter()
            .filter(|e| e.kind == SourceEditKind::ParentArrayElement)
            .collect();
        assert_eq!(array_edits.len(), 1);
        let before = array_edits[0].before_array_text.as_deref().unwrap();
        let after = array_edits[0].after_array_text.as_deref().unwrap();
        assert!(before.contains("\"parent_id\""), "before: {before}");
        assert!(before.contains("\"other\""), "before: {before}");
        assert!(after.contains("\"new_parent\""), "after: {after}");
        assert!(after.contains("\"other\""), "after: {after}");
        assert!(!after.contains("\"parent_id\""), "after: {after}");
    }

    #[test]
    fn plan_includes_self_key_index_update() {
        let src = r#"
            #[aristo::intent("a", id = "alpha")]
            fn a() {}
        "#;
        let (_dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("alpha", intent("src/lib.rs", "fn a (line 2)"))]);
        let parsed = parse_and_validate("alpha", "beta", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        let n_self = plan
            .index_updates
            .iter()
            .filter(|u| matches!(u.kind, IndexUpdateKind::SelfKey))
            .count();
        assert_eq!(n_self, 1);
    }

    #[test]
    fn plan_index_updates_include_each_child_parent_link() {
        let owner_src = r#"
            #[aristo::intent("p", id = "parent_id")]
            fn p() {}
        "#;
        let child_a_src = r#"
            #[aristo::intent("a", parent = "parent_id", id = "child_a")]
            fn a() {}
        "#;
        let child_b_src = r#"
            #[aristo::intent("b", parent = ["parent_id", "x"], id = "child_b")]
            fn b() {}
            #[aristo::intent("x", id = "x")]
            fn xx() {}
        "#;
        let (_dir, ws) = ws_with_source(&[
            ("src/p.rs", owner_src),
            ("src/ca.rs", child_a_src),
            ("src/cb.rs", child_b_src),
        ]);
        let index = build_index(&[
            ("parent_id", intent("src/p.rs", "fn p (line 2)")),
            ("x", intent("src/cb.rs", "fn xx (line 4)")),
            (
                "child_a",
                intent_with_parent(
                    "src/ca.rs",
                    "fn a (line 2)",
                    ParentLink::Single(AnnotationId::parse("parent_id").unwrap()),
                ),
            ),
            (
                "child_b",
                intent_with_parent(
                    "src/cb.rs",
                    "fn b (line 2)",
                    ParentLink::Multiple(vec![
                        AnnotationId::parse("parent_id").unwrap(),
                        AnnotationId::parse("x").unwrap(),
                    ]),
                ),
            ),
        ]);
        let parsed = parse_and_validate("parent_id", "new_parent", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        let single_updates: Vec<_> = plan
            .index_updates
            .iter()
            .filter(|u| matches!(u.kind, IndexUpdateKind::ChildParentSingle { .. }))
            .collect();
        let array_updates: Vec<_> = plan
            .index_updates
            .iter()
            .filter(|u| matches!(u.kind, IndexUpdateKind::ChildParentArray { .. }))
            .collect();
        assert_eq!(single_updates.len(), 1);
        assert_eq!(array_updates.len(), 1);
    }

    #[test]
    fn plan_artifact_moves_appear_only_when_files_exist() {
        let src = r#"
            #[aristo::intent("a", id = "alpha")]
            fn a() {}
        "#;
        let (dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("alpha", intent("src/lib.rs", "fn a (line 2)"))]);
        // No artifact files yet: no moves expected.
        let parsed = parse_and_validate("alpha", "beta", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        assert!(plan.artifact_moves.is_empty(), "no artifacts → no moves");

        // Now drop a .critique file and re-plan.
        let crit_dir = dir.path().join(".aristo/critiques");
        fs::create_dir_all(&crit_dir).unwrap();
        fs::write(crit_dir.join("alpha.critique"), "").unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        assert_eq!(plan.artifact_moves.len(), 1);
        assert!(plan.artifact_moves[0]
            .from_label
            .ends_with("alpha.critique"));
        assert!(plan.artifact_moves[0].to_label.ends_with("beta.critique"));
    }

    #[test]
    fn plan_refuses_when_source_drift_hides_owner_id_occurrence() {
        // Index says alpha is in src/lib.rs but source has no `id = "alpha"`.
        let src = r#"
            // intentionally missing the annotation
            fn f() {}
        "#;
        let (_dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("alpha", intent("src/lib.rs", "fn f (line 3)"))]);
        let parsed = parse_and_validate("alpha", "beta", &index).unwrap();
        let err = compute_plan(&ws, &index, &parsed).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("source has drifted"), "msg: {msg}");
        assert!(msg.contains("aristo stamp"), "msg: {msg}");
    }

    // ─── Apply path (commit 4) ───────────────────────────────────────────

    #[test]
    fn apply_substitutes_id_byte_range_in_source_file() {
        let src = "#[aristo::intent(\"x\", id = \"old_id\")] fn f() {}\n";
        let (dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("old_id", intent("src/lib.rs", "fn f (line 1)"))]);
        let parsed = parse_and_validate("old_id", "new_id", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan");
        apply_plan(&ws, &index, plan).expect("apply ok");
        let post = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
        assert!(post.contains("id = \"new_id\""), "post: {post}");
        assert!(!post.contains("\"old_id\""), "post: {post}");
    }

    #[test]
    fn apply_substitutes_parent_array_element_without_disturbing_siblings() {
        let owner_src = "#[aristo::intent(\"p\", id = \"parent_id\")] fn p() {}\n";
        let child_src =
            "#[aristo::intent(\"c\", parent = [\"parent_id\", \"keep_me\"], id = \"child_id\")] fn c() {}\n";
        let (dir, ws) = ws_with_source(&[("src/p.rs", owner_src), ("src/c.rs", child_src)]);
        let index = build_index(&[
            ("parent_id", intent("src/p.rs", "fn p (line 1)")),
            ("keep_me", intent("src/p.rs", "fn k (line 1)")),
            (
                "child_id",
                intent_with_parent(
                    "src/c.rs",
                    "fn c (line 1)",
                    ParentLink::Multiple(vec![
                        AnnotationId::parse("parent_id").unwrap(),
                        AnnotationId::parse("keep_me").unwrap(),
                    ]),
                ),
            ),
        ]);
        let parsed = parse_and_validate("parent_id", "new_parent", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan");
        apply_plan(&ws, &index, plan).expect("apply ok");
        let post = fs::read_to_string(dir.path().join("src/c.rs")).unwrap();
        assert!(post.contains("\"new_parent\""), "post: {post}");
        assert!(post.contains("\"keep_me\""), "sibling intact: {post}");
        assert!(!post.contains("\"parent_id\""), "post: {post}");
    }

    #[test]
    fn apply_preserves_whitespace_and_comments_around_id() {
        // Span-based rewriting must leave the file otherwise byte-identical.
        let src = "// leading comment\n\
                   #[aristo::intent(\n    \"prose\",\n    verify = \"test\",\n    id = \"alpha\"\n)]\n\
                   fn f() { /* body */ }\n";
        let (dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("alpha", intent("src/lib.rs", "fn f (line 6)"))]);
        let parsed = parse_and_validate("alpha", "beta", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan");
        apply_plan(&ws, &index, plan).expect("apply ok");
        let post = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
        let expected = src.replace("\"alpha\"", "\"beta\"");
        assert_eq!(
            post, expected,
            "exactly-one-substitution; rest byte-identical"
        );
    }

    #[test]
    fn apply_writes_new_index_with_swapped_self_key_and_rewritten_parents() {
        let owner_src = "#[aristo::intent(\"p\", id = \"parent_id\")] fn p() {}\n";
        let child_src =
            "#[aristo::intent(\"c\", parent = \"parent_id\", id = \"child_id\")] fn c() {}\n";
        let (dir, ws) = ws_with_source(&[("src/p.rs", owner_src), ("src/c.rs", child_src)]);
        let index = build_index(&[
            ("parent_id", intent("src/p.rs", "fn p (line 1)")),
            (
                "child_id",
                intent_with_parent(
                    "src/c.rs",
                    "fn c (line 1)",
                    ParentLink::Single(AnnotationId::parse("parent_id").unwrap()),
                ),
            ),
        ]);
        let parsed = parse_and_validate("parent_id", "new_parent", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan");
        apply_plan(&ws, &index, plan).expect("apply ok");
        let raw = fs::read_to_string(dir.path().join(".aristo/index.toml")).unwrap();
        let post: IndexFile = toml::from_str(&raw).expect("parse rewritten index");
        assert!(post
            .entries
            .contains_key(&AnnotationId::parse("new_parent").unwrap()));
        assert!(!post
            .entries
            .contains_key(&AnnotationId::parse("parent_id").unwrap()));
        // Child's parent link was rewritten.
        match post
            .entries
            .get(&AnnotationId::parse("child_id").unwrap())
            .unwrap()
        {
            IndexEntry::Intent(e) => match e.parent.as_ref().unwrap() {
                ParentLink::Single(id) => assert_eq!(id.as_str(), "new_parent"),
                _ => panic!("expected single-parent form"),
            },
            _ => panic!("expected intent"),
        }
    }

    #[test]
    fn apply_renames_artifact_files_when_present() {
        let src = "#[aristo::intent(\"x\", id = \"alpha\")] fn f() {}\n";
        let (dir, ws) = ws_with_source(&[("src/lib.rs", src)]);
        let index = build_index(&[("alpha", intent("src/lib.rs", "fn f (line 1)"))]);
        let crit_dir = dir.path().join(".aristo/critiques");
        fs::create_dir_all(&crit_dir).unwrap();
        fs::write(crit_dir.join("alpha.critique"), "critique-body").unwrap();
        let parsed = parse_and_validate("alpha", "beta", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan");
        apply_plan(&ws, &index, plan).expect("apply ok");
        assert!(crit_dir.join("beta.critique").is_file());
        assert!(!crit_dir.join("alpha.critique").is_file());
        let body = fs::read_to_string(crit_dir.join("beta.critique")).unwrap();
        assert_eq!(body, "critique-body");
    }

    #[test]
    fn rewrite_parent_link_leaves_unrelated_arrays_intact() {
        let old = AnnotationId::parse("foo").unwrap();
        let new = AnnotationId::parse("baz").unwrap();
        let parent = ParentLink::Multiple(vec![
            AnnotationId::parse("a").unwrap(),
            AnnotationId::parse("b").unwrap(),
        ]);
        match rewrite_parent_link(parent, &old, &new) {
            ParentLink::Multiple(v) => {
                let ids: Vec<&str> = v.iter().map(|id| id.as_str()).collect();
                assert_eq!(ids, vec!["a", "b"]);
            }
            _ => panic!("expected multiple"),
        }
    }

    #[test]
    fn rendered_plan_matches_spec_format_for_local_to_local() {
        let owner_src = r#"
#[aristo::intent("p", id = "parent_id")]
fn p() {}
"#;
        let child_src = r#"
#[aristo::intent("c", parent = "parent_id", id = "child_id")]
fn c() {}
"#;
        let (_dir, ws) = ws_with_source(&[("src/p.rs", owner_src), ("src/c.rs", child_src)]);
        let index = build_index(&[
            ("parent_id", intent("src/p.rs", "fn p (line 2)")),
            (
                "child_id",
                intent_with_parent(
                    "src/c.rs",
                    "fn c (line 2)",
                    ParentLink::Single(AnnotationId::parse("parent_id").unwrap()),
                ),
            ),
        ]);
        let parsed = parse_and_validate("parent_id", "new_parent", &index).unwrap();
        let plan = compute_plan(&ws, &index, &parsed).expect("plan ok");
        let rendered = format!("{plan}");
        assert!(
            rendered.contains("Plan: rename `parent_id` → `new_parent`"),
            "rendered:\n{rendered}"
        );
        assert!(rendered.contains("Source edits:"), "rendered:\n{rendered}");
        assert!(
            rendered.contains("id = \"parent_id\"   →   id = \"new_parent\""),
            "rendered:\n{rendered}"
        );
        assert!(
            rendered.contains("parent = \"parent_id\"   →   parent = \"new_parent\""),
            "rendered:\n{rendered}"
        );
        assert!(rendered.contains("Index updates:"), "rendered:\n{rendered}");
        assert!(
            rendered.contains("[\"parent_id\"]  →  [\"new_parent\"]"),
            "rendered:\n{rendered}"
        );
        assert!(
            rendered.contains("child_id.parent: \"parent_id\" → \"new_parent\""),
            "rendered:\n{rendered}"
        );
        assert!(
            rendered.contains("(no changes written — dry-run)"),
            "rendered:\n{rendered}"
        );
    }
}
