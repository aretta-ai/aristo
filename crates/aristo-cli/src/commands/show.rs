//! `aristo show <selector>` — look up a single annotation (or set of
//! co-located annotations for `fn name` / `file:line`) by selector.
//!
//! Selectors:
//! - bare id: `returns_forty_two`, `aristos:foo`, `aret_a1b2c3d4`
//! - by item name: `fn name`, `mod name`, `struct name`, `enum name`,
//!   `trait name`
//! - by source location: `<file>:<line>`
//!
//! Output modes (per F3-a in mockup 06): plain text by default, `--json`
//! / `--toml` for piping. The structured shape mirrors `.aristo/index.toml`
//! plus an `id` field (which is the table key on disk).

use std::fs;

use aristo_core::index::{
    AnnotationId, AssumeEntry, BindingState, IndexEntry, IndexFile, IntentEntry, ParentLink,
    Status, VerifyLevel, VerifyMethod,
};
use serde::Serialize;

use crate::commands::index::workspace_or_error;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

/// How to render a found entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Text,
    Json,
    Toml,
}

pub(crate) fn run(selector: &str, mode: OutputMode) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;
    let parsed = parse_selector(selector);
    match parsed {
        Selector::Id(raw) => show_by_id(&index, &raw, mode),
        Selector::Item { kind, name } => show_by_item(&index, kind, &name, mode),
        Selector::FileLine { file, line } => show_by_file_line(&index, &file, line, mode),
    }
}

// ─── Selector parsing ──────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum Selector {
    Id(String),
    Item { kind: ItemKind, name: String },
    FileLine { file: String, line: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Fn,
    Mod,
    Struct,
    Enum,
    Trait,
    Impl,
    Type,
}

impl ItemKind {
    fn label(self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::Mod => "mod",
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Trait => "trait",
            ItemKind::Impl => "impl",
            ItemKind::Type => "type",
        }
    }
}

fn parse_selector(raw: &str) -> Selector {
    // `<keyword> <name>` form.
    if let Some((head, rest)) = raw.split_once(' ') {
        let kind = match head {
            "fn" => Some(ItemKind::Fn),
            "mod" => Some(ItemKind::Mod),
            "struct" => Some(ItemKind::Struct),
            "enum" => Some(ItemKind::Enum),
            "trait" => Some(ItemKind::Trait),
            "impl" => Some(ItemKind::Impl),
            "type" => Some(ItemKind::Type),
            _ => None,
        };
        if let Some(kind) = kind {
            return Selector::Item {
                kind,
                name: rest.trim().to_string(),
            };
        }
    }
    // `<file>:<line>` form. Distinguished from the `aristos:` id
    // namespace by requiring the right-hand side to parse as a u32 — id
    // suffixes never start with a digit (validated by AnnotationId::parse).
    if let Some((path, tail)) = raw.rsplit_once(':') {
        if let Ok(line) = tail.parse::<u32>() {
            return Selector::FileLine {
                file: path.to_string(),
                line,
            };
        }
    }
    Selector::Id(raw.to_string())
}

// ─── Index loading ─────────────────────────────────────────────────────────

pub(crate) fn read_index(path: &std::path::Path) -> CliResult<IndexFile> {
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

// ─── Lookup paths ──────────────────────────────────────────────────────────

fn show_by_id(index: &IndexFile, raw: &str, mode: OutputMode) -> CliResult<()> {
    let parsed = AnnotationId::parse(raw).ok();
    let entry = parsed.as_ref().and_then(|id| index.entries.get(id));
    match (parsed, entry) {
        (Some(id), Some(entry)) => emit_one(&id, entry, index, mode),
        _ => Err(no_id_error(index, raw)),
    }
}

fn show_by_item(index: &IndexFile, kind: ItemKind, name: &str, mode: OutputMode) -> CliResult<()> {
    let matches: Vec<(&AnnotationId, &IndexEntry)> = index
        .entries
        .iter()
        .filter(|(_, e)| site_matches(entry_site(e), kind, name))
        .collect();

    match matches.len() {
        0 => Err(CliError::Other {
            message: format!(
                "No items matching `{} {name}` found in indexed source files.\n\
                 \n\
                 Try:\n  \
                   * `aristo list --filter file=<path>` to see annotations in a file\n  \
                   * `aristo stamp` to refresh if you've added new sources",
                kind.label()
            ),
            exit_code: 1,
        }),
        1 => {
            let (id, entry) = matches[0];
            emit_one(id, entry, index, mode)
        }
        n => emit_disambiguation(kind, name, n, &matches, mode),
    }
}

fn show_by_file_line(index: &IndexFile, file: &str, line: u32, mode: OutputMode) -> CliResult<()> {
    let matches: Vec<(&AnnotationId, &IndexEntry)> = index
        .entries
        .iter()
        .filter(|(_, e)| entry_file(e) == file && entry_line(e) == Some(line))
        .collect();

    if matches.is_empty() {
        return Err(CliError::Other {
            message: format!(
                "No annotation at {file}:{line} in the index.\n\
                 hint: line numbers must match an annotation's source line exactly; \
                 use `aristo list --filter file={file}` to see what's indexed."
            ),
            exit_code: 1,
        });
    }
    if matches.len() == 1 {
        let (id, entry) = matches[0];
        return emit_one(id, entry, index, mode);
    }
    // Multiple annotations sharing one source line — list all.
    emit_disambiguation(
        ItemKind::Fn,
        &format!("{file}:{line}"),
        matches.len(),
        &matches,
        mode,
    )
}

// ─── Selector matching ─────────────────────────────────────────────────────

fn site_matches(site: &str, kind: ItemKind, name: &str) -> bool {
    // Index entry's `site` is e.g. `"fn balance_non_root (line 3010)"` (per
    // build_entries in commands/index.rs). Match the leading kind + name
    // tokens regardless of trailing line annotation.
    let head = site.split(" (line ").next().unwrap_or(site);
    let expected_prefix = format!("{} {name}", kind.label());
    head == expected_prefix || head.starts_with(&format!("{expected_prefix}::"))
}

fn entry_site(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.site,
        IndexEntry::Assume(e) => &e.site,
    }
}

fn entry_file(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

fn entry_line(entry: &IndexEntry) -> Option<u32> {
    let site = entry_site(entry);
    let after = site.split(" (line ").nth(1)?;
    let num = after.trim_end_matches(')');
    num.parse().ok()
}

// ─── Did-you-mean ──────────────────────────────────────────────────────────

fn no_id_error(index: &IndexFile, raw: &str) -> CliError {
    let mut message = format!("no annotation with id `{raw}`.");
    if let Some(suggestions) = did_you_mean(index, raw) {
        message.push_str("\n\nDid you mean:");
        for (id, entry) in suggestions {
            message.push_str(&format!("\n  * {id}  ({})", entry_file(entry)));
        }
    }
    CliError::Other {
        message,
        exit_code: 1,
    }
}

#[aristo::intent(
    "The Levenshtein threshold is tuned to suppress unrelated ids: too \
     loose floods the user with noise, too tight hides real typos. Both \
     regressions silently erode trust in the \"did you mean\" signal \
     until it gets ignored.",
    verify = "neural",
    id = "did_you_mean_threshold_filters_noise"
)]
/// Pick up to 3 close ids by Levenshtein distance, capped at one third of
/// the query length so unrelated ids don't bubble up.
fn did_you_mean<'a>(
    index: &'a IndexFile,
    raw: &str,
) -> Option<Vec<(&'a AnnotationId, &'a IndexEntry)>> {
    let max_dist = (raw.len() / 3).max(1);
    let mut scored: Vec<(usize, &AnnotationId, &IndexEntry)> = index
        .entries
        .iter()
        .map(|(id, e)| (levenshtein(raw, id.as_str()), id, e))
        .filter(|(d, _, _)| *d <= max_dist)
        .collect();
    if scored.is_empty() {
        return None;
    }
    scored.sort_by_key(|(d, _, _)| *d);
    Some(
        scored
            .into_iter()
            .take(3)
            .map(|(_, id, e)| (id, e))
            .collect(),
    )
}

/// Standard iterative Levenshtein. O(|a|·|b|) time, O(min(|a|,|b|)) space.
fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ac) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let cost = if ac == bc { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

// ─── Output formatting ─────────────────────────────────────────────────────

fn emit_one(
    id: &AnnotationId,
    entry: &IndexEntry,
    index: &IndexFile,
    mode: OutputMode,
) -> CliResult<()> {
    match mode {
        OutputMode::Text => {
            print!("{}", format_text(id, entry, index));
            Ok(())
        }
        OutputMode::Json => {
            let record = ShowRecord::from_entry(id, entry, index);
            let s = serde_json::to_string_pretty(&record).map_err(|e| CliError::Other {
                message: format!("serializing show record as JSON: {e}"),
                exit_code: 1,
            })?;
            println!("{s}");
            Ok(())
        }
        OutputMode::Toml => {
            // Mirror the on-disk schema: a single quoted-key table with the
            // entry's fields. Re-using the IndexFile serializer with one
            // entry gives us byte-for-byte index-shape output.
            let mut single = std::collections::BTreeMap::new();
            single.insert(id.clone(), entry.clone());
            let synthetic = aristo_core::index::IndexFile {
                meta: aristo_core::index::Meta {
                    schema_version: 1,
                    generated_by: None,
                    generated_at: None,
                    source_root: None,
                },
                entries: single,
            };
            let s = toml::to_string_pretty(&synthetic).map_err(|e| CliError::Other {
                message: format!("serializing as TOML: {e}"),
                exit_code: 1,
            })?;
            print!("{s}");
            Ok(())
        }
    }
}

fn emit_disambiguation(
    kind: ItemKind,
    name: &str,
    n: usize,
    matches: &[(&AnnotationId, &IndexEntry)],
    mode: OutputMode,
) -> CliResult<()> {
    match mode {
        OutputMode::Text => {
            println!(
                "Found {n} sites matching `{} {name}`. Disambiguate by id, file:line, or item path:",
                kind.label()
            );
            println!();
            for (id, entry) in matches {
                let kind_label = match entry {
                    IndexEntry::Intent(_) => "intent",
                    IndexEntry::Assume(_) => "assume",
                };
                println!(
                    "  {id}  ({kind_label}, status={})\n    {}  - {}",
                    status_label(entry_status(entry)),
                    entry_file(entry),
                    entry_site(entry),
                );
            }
            println!();
            println!("To show one, run: `aristo show <id>` or `aristo show <file>:<line>`");
            Ok(())
        }
        OutputMode::Json => {
            let arr: Vec<ShowRecord> = matches
                .iter()
                .map(|(id, e)| ShowRecord::from_entry(id, e, &empty_index()))
                .collect();
            let s = serde_json::to_string_pretty(&arr).map_err(|e| CliError::Other {
                message: format!("serializing disambiguation as JSON: {e}"),
                exit_code: 1,
            })?;
            println!("{s}");
            Ok(())
        }
        OutputMode::Toml => {
            // Build a synthetic IndexFile with all matches; same shape as
            // single-match TOML emission.
            let mut entries = std::collections::BTreeMap::new();
            for (id, e) in matches {
                entries.insert((*id).clone(), (*e).clone());
            }
            let synthetic = aristo_core::index::IndexFile {
                meta: aristo_core::index::Meta {
                    schema_version: 1,
                    generated_by: None,
                    generated_at: None,
                    source_root: None,
                },
                entries,
            };
            let s = toml::to_string_pretty(&synthetic).map_err(|e| CliError::Other {
                message: format!("serializing as TOML: {e}"),
                exit_code: 1,
            })?;
            print!("{s}");
            Ok(())
        }
    }
}

fn empty_index() -> IndexFile {
    IndexFile {
        meta: aristo_core::index::Meta {
            schema_version: 1,
            generated_by: None,
            generated_at: None,
            source_root: None,
        },
        entries: std::collections::BTreeMap::new(),
    }
}

fn format_text(id: &AnnotationId, entry: &IndexEntry, index: &IndexFile) -> String {
    let mut out = String::new();
    let kind_word = match entry {
        IndexEntry::Intent(_) => "intent",
        IndexEntry::Assume(_) => "assume",
    };
    out.push_str(&format!("{id} ({kind_word})\n"));
    let status = entry_status(entry);
    out.push_str(&format!("  status:    {}\n", status_label(status)));

    if let IndexEntry::Intent(e) = entry {
        out.push_str(&format!("  verify:    {}\n", verify_label(e.verify)));
    }

    out.push_str(&format!("  file:      {}\n", entry_file(entry)));
    out.push_str(&format!("  site:      {}\n", entry_site(entry)));
    out.push_str(&format!(
        "  covered_region: {}\n",
        covered_region_label(entry)
    ));
    out.push_str(&format!("  text_hash: {}\n", entry_text_hash(entry)));
    out.push_str(&format!("  body_hash: {}\n", entry_body_hash(entry)));

    // Server binding (intent only — assume's binding is just `linked`).
    if let IndexEntry::Intent(e) = entry {
        match &e.binding {
            BindingState::Local => {}
            BindingState::Bound { linked } => {
                out.push_str(&format!("  linked:    {linked}\n"));
            }
            BindingState::Certified {
                linked,
                verified_outcome,
                last_verified_at_commit,
            } => {
                out.push_str(&format!("  linked:    {linked}\n"));
                out.push_str(&format!("  verified_outcome: {verified_outcome}\n"));
                out.push_str(&format!(
                    "  last_verified_at_commit: {last_verified_at_commit}\n"
                ));
            }
        }
    } else if let IndexEntry::Assume(a) = entry {
        if let Some(linked) = &a.linked {
            out.push_str(&format!("  linked:    {linked}\n"));
        }
    }

    if let Some(parent) = entry_parent(entry) {
        out.push_str("  parent:    ");
        let parents: Vec<String> = parent.iter().map(|p| p.to_string()).collect();
        out.push_str(&parents.join(", "));
        out.push('\n');
    }

    out.push('\n');
    out.push_str("  Text:\n");
    for line in entry_text(entry).lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }

    let children = collect_children(index, id);
    if !children.is_empty() {
        out.push('\n');
        out.push_str(&format!(
            "  Children (annotations with parent = \"{id}\"):\n"
        ));
        for (cid, ce) in children {
            let kind_label = match ce {
                IndexEntry::Intent(_) => "intent",
                IndexEntry::Assume(_) => "assume",
            };
            let verify_str = match ce {
                IndexEntry::Intent(e) => format!(", verify={}", verify_label(e.verify)),
                IndexEntry::Assume(_) => String::new(),
            };
            out.push_str(&format!(
                "    {cid}  ({kind_label}{verify_str}, status={})\n      {}  - {}\n",
                status_label(entry_status(ce)),
                entry_file(ce),
                entry_site(ce),
            ));
        }
    }

    out
}

fn collect_children<'a>(
    index: &'a IndexFile,
    parent_id: &AnnotationId,
) -> Vec<(&'a AnnotationId, &'a IndexEntry)> {
    let mut out: Vec<(&'a AnnotationId, &'a IndexEntry)> = index
        .entries
        .iter()
        .filter(|(_, e)| match entry_parent(e) {
            Some(link) => link.iter().any(|p| p == parent_id),
            None => false,
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(b.0));
    out
}

fn entry_status(entry: &IndexEntry) -> Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

fn entry_parent(entry: &IndexEntry) -> Option<&ParentLink> {
    match entry {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }
}

fn entry_text(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.text,
        IndexEntry::Assume(e) => &e.text,
    }
}

fn entry_text_hash(entry: &IndexEntry) -> String {
    match entry {
        IndexEntry::Intent(e) => e.text_hash.to_string(),
        IndexEntry::Assume(e) => e.text_hash.to_string(),
    }
}

fn entry_body_hash(entry: &IndexEntry) -> String {
    match entry {
        IndexEntry::Intent(e) => e.body_hash.to_string(),
        IndexEntry::Assume(e) => e.body_hash.to_string(),
    }
}

fn covered_region_label(entry: &IndexEntry) -> &'static str {
    use aristo_core::index::CoveredRegion::*;
    let region = match entry {
        IndexEntry::Intent(e) => e.covered_region,
        IndexEntry::Assume(e) => e.covered_region,
    };
    match region {
        Function => "function",
        Statement => "statement",
        ImplMethods => "impl_methods",
        ModuleInlineBody => "module_inline_body",
        Type => "type",
        Field => "field",
        Crate => "crate",
        Trait => "trait",
    }
}

pub(crate) fn status_label(status: Status) -> &'static str {
    match status {
        Status::Verified => "verified",
        Status::Tested => "tested",
        Status::Neural => "neural",
        Status::Stale => "stale",
        Status::Orphan => "orphan",
        Status::Forged => "forged",
        Status::Unknown => "unknown",
        Status::PendingDeepen => "pending-deepen",
        Status::Counterexample => "counterexample",
    }
}

pub(crate) fn verify_label(level: VerifyLevel) -> String {
    match level {
        VerifyLevel::Bool(true) => "true".to_string(),
        VerifyLevel::Bool(false) => "false".to_string(),
        VerifyLevel::Method(VerifyMethod::Test) => "test".to_string(),
        VerifyLevel::Method(VerifyMethod::Neural) => "neural".to_string(),
        VerifyLevel::Method(VerifyMethod::Full) => "full".to_string(),
    }
}

// ─── Structured (JSON) projection ──────────────────────────────────────────
//
// Public shape pinned here so future schema additions to IndexEntry don't
// silently leak into `--json` output without a deliberate update.

#[derive(Debug, Serialize)]
pub(crate) struct ShowRecord {
    pub id: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
    pub status: String,
    pub file: String,
    pub site: String,
    pub covered_region: &'static str,
    pub text: String,
    pub text_hash: String,
    pub body_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_verified_at_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ShowChildRecord>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ShowChildRecord {
    pub id: String,
    pub kind: &'static str,
    pub status: String,
    pub file: String,
    pub site: String,
}

impl ShowRecord {
    pub(crate) fn from_entry(id: &AnnotationId, entry: &IndexEntry, index: &IndexFile) -> Self {
        let (kind, verify, linked, outcome, commit) = match entry {
            IndexEntry::Intent(e) => intent_facets(e),
            IndexEntry::Assume(e) => assume_facets(e),
        };
        let parent: Option<Vec<String>> =
            entry_parent(entry).map(|p| p.iter().map(|x| x.to_string()).collect());
        let children: Vec<ShowChildRecord> = collect_children(index, id)
            .into_iter()
            .map(|(cid, ce)| ShowChildRecord {
                id: cid.to_string(),
                kind: match ce {
                    IndexEntry::Intent(_) => "intent",
                    IndexEntry::Assume(_) => "assume",
                },
                status: status_label(entry_status(ce)).to_string(),
                file: entry_file(ce).to_string(),
                site: entry_site(ce).to_string(),
            })
            .collect();
        Self {
            id: id.to_string(),
            kind,
            verify,
            status: status_label(entry_status(entry)).to_string(),
            file: entry_file(entry).to_string(),
            site: entry_site(entry).to_string(),
            covered_region: covered_region_label(entry),
            text: entry_text(entry).to_string(),
            text_hash: entry_text_hash(entry),
            body_hash: entry_body_hash(entry),
            linked,
            verified_outcome: outcome,
            last_verified_at_commit: commit,
            parent,
            children,
        }
    }
}

type EntryFacets = (
    &'static str,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn intent_facets(e: &IntentEntry) -> EntryFacets {
    let (linked, outcome, commit) = match &e.binding {
        BindingState::Local => (None, None, None),
        BindingState::Bound { linked } => (Some(linked.to_string()), None, None),
        BindingState::Certified {
            linked,
            verified_outcome,
            last_verified_at_commit,
        } => (
            Some(linked.to_string()),
            Some(verified_outcome.to_string()),
            Some(last_verified_at_commit.to_string()),
        ),
    };
    (
        "intent",
        Some(verify_label(e.verify)),
        linked,
        outcome,
        commit,
    )
}

fn assume_facets(e: &AssumeEntry) -> EntryFacets {
    let linked = e.linked.as_ref().map(|l| l.to_string());
    ("assume", None, linked, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_id_selector() {
        assert_eq!(parse_selector("foo_bar"), Selector::Id("foo_bar".into()));
        assert_eq!(
            parse_selector("aristos:thing"),
            Selector::Id("aristos:thing".into()),
        );
        assert_eq!(
            parse_selector("aret_a1b2c3d4"),
            Selector::Id("aret_a1b2c3d4".into()),
        );
    }

    #[test]
    fn parses_fn_keyword_selector() {
        assert_eq!(
            parse_selector("fn balance"),
            Selector::Item {
                kind: ItemKind::Fn,
                name: "balance".into(),
            }
        );
    }

    #[test]
    fn parses_struct_keyword_selector() {
        assert_eq!(
            parse_selector("struct BTreeCursor"),
            Selector::Item {
                kind: ItemKind::Struct,
                name: "BTreeCursor".into(),
            }
        );
    }

    #[test]
    fn parses_file_line_selector() {
        assert_eq!(
            parse_selector("src/lib.rs:42"),
            Selector::FileLine {
                file: "src/lib.rs".into(),
                line: 42,
            }
        );
    }

    #[test]
    fn aristos_id_does_not_get_misparsed_as_file_line() {
        // The right side of `:` must parse as u32; `thing` doesn't, so we
        // fall back to the id path.
        assert_eq!(
            parse_selector("aristos:thing"),
            Selector::Id("aristos:thing".into()),
        );
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("same", "same"), 0);
    }

    #[test]
    fn site_matches_strips_line_suffix() {
        assert!(site_matches("fn answer (line 1)", ItemKind::Fn, "answer"));
        assert!(!site_matches(
            "fn answer (line 1)",
            ItemKind::Fn,
            "answer_other"
        ));
        assert!(!site_matches(
            "fn answer (line 1)",
            ItemKind::Struct,
            "answer"
        ));
    }

    #[test]
    fn site_matches_module_qualified_paths() {
        assert!(site_matches(
            "fn outer::inner (line 5)",
            ItemKind::Fn,
            "outer"
        ));
    }
}
