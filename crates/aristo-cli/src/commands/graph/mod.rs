//! `aristo graph` — render the annotation parent graph.
//!
//! Slice 29. Reads `.aristo/index.toml`, builds a node + edge model from
//! every entry's `parent` link, and emits the graph in one of three
//! formats:
//!
//! - **Mermaid** (default): `flowchart TD` with classDefs for verify-level
//!   coloring and critical-status border. Pastes into GitHub READMEs
//!   inline. No external dependency.
//! - **DOT** (slice 29 commit 3): Graphviz format, pipe into any
//!   Graphviz-compatible renderer.
//! - **SVG** (slice 29 commit 4): SVG produced by shelling out to `dot`.
//!
//! Visual encoding (from `aretta-sdk/docs/mockups/10-doc-and-graph/samples.md`):
//! - **Shape** = kind: rectangle for intent, hexagon for assume.
//! - **Color** = verify level: gray=false, yellow=neural, blue=test,
//!   green=full.
//! - **Border** = red for critical status (stale / orphan / forged) so
//!   the user notices what needs action.

use std::path::PathBuf;

use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Status, VerifyLevel, VerifyMethod};

use crate::commands::index::{atomic_write, workspace_or_error};
use crate::commands::show::read_index;
use crate::filter::Filter;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

pub(crate) mod dot;
pub(crate) mod mermaid;
pub(crate) mod model;
pub(crate) mod svg;

/// Output format selected by `--format`. Default is Mermaid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Mermaid,
    Dot,
    Svg,
}

impl Format {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "mermaid" => Ok(Self::Mermaid),
            "dot" => Ok(Self::Dot),
            "svg" => Ok(Self::Svg),
            other => Err(format!(
                "unknown --format `{other}`; expected `mermaid` (default), `dot`, or `svg`"
            )),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Mermaid => "Mermaid",
            Self::Dot => "DOT",
            Self::Svg => "SVG",
        }
    }
}

pub(crate) fn run(
    format: &str,
    out: Option<PathBuf>,
    filter_strings: &[String],
    exclude_assumes: bool,
    depth: Option<u32>,
    include_orphans: bool,
    include_status: bool,
) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    let format = Format::parse(format).map_err(|message| CliError::Other {
        message,
        exit_code: 2,
    })?;

    let filters = parse_filters(filter_strings)?;
    let mut scoped_index = if filters.is_empty() {
        index
    } else {
        let matched = filter_index(index.clone(), &filters);
        if let Some(n) = depth {
            expand_by_depth(&index, matched, n)
        } else {
            matched
        }
    };
    if exclude_assumes {
        scoped_index = drop_assumes(scoped_index);
    }
    if !include_orphans && filters.is_empty() {
        scoped_index = drop_orphan_intents(scoped_index);
    }
    let axis = if include_status {
        ColorAxis::Status
    } else {
        ColorAxis::Verify
    };
    let graph = model::build_with_axis(&scoped_index, axis);
    let rendered = match format {
        Format::Mermaid => mermaid::render(&graph),
        Format::Dot => dot::render(&graph),
        Format::Svg => svg::render(&graph)?,
    };

    match out {
        None => {
            print!("{rendered}");
            eprintln!(
                "ok: {} nodes, {} edges rendered. ({} to stdout)",
                graph.nodes.len(),
                graph.edges.len(),
                format.label()
            );
        }
        Some(path) => {
            atomic_write(&path, &rendered)?;
            eprintln!(
                "ok: wrote {} nodes, {} edges to {}",
                graph.nodes.len(),
                graph.edges.len(),
                path.display()
            );
            // DOT output is opaque without a renderer — surface the
            // standard Graphviz invocations so the user knows what to
            // run next without consulting docs.
            if format == Format::Dot {
                eprintln!();
                eprintln!("To render:");
                eprintln!("  dot -Tsvg {0} -o {0}.svg", path.display());
                eprintln!("  dot -Tpng {0} -o {0}.png", path.display());
            }
        }
    }
    Ok(())
}

/// Which axis drives node coloring. Default is `Verify` (matches the
/// mockup). `Status` is the `--include-status` mode that re-colors by
/// current B5b state instead — useful for "show me what's still
/// unverified" review meetings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorAxis {
    Verify,
    Status,
}

/// Verify-level color class. One of the four `vFalse`/`vNeural`/`vTest`/
/// `vFull` Mermaid + DOT classes from the mockup palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyClass {
    /// `verify = false` — documentation-only intent. Also used for
    /// assumes (which have no verify level — the gray styling matches
    /// "background fact").
    False,
    /// `verify = true` (project default) OR `verify = "neural"`.
    Neural,
    Test,
    Full,
}

/// Status-axis color class. Drives palette when `--include-status` is
/// passed. Counterexample / Inconclusive share the Forged "red border"
/// treatment because they're all terminal "needs action" states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusClass {
    Verified,
    Tested,
    Neural,
    Stale,
    Orphan,
    Forged,
    Unknown,
    PendingDeepen,
    Counterexample,
    Inconclusive,
}

impl VerifyClass {
    pub(crate) fn from_entry(entry: &IndexEntry) -> Self {
        match entry {
            IndexEntry::Assume(_) => Self::False,
            IndexEntry::Intent(e) => match e.verify {
                VerifyLevel::Bool(false) => Self::False,
                VerifyLevel::Bool(true) => Self::Neural,
                VerifyLevel::Method(VerifyMethod::Neural) => Self::Neural,
                VerifyLevel::Method(VerifyMethod::Test) => Self::Test,
                VerifyLevel::Method(VerifyMethod::Full) => Self::Full,
            },
        }
    }
}

impl StatusClass {
    pub(crate) fn from_status(s: Status) -> Self {
        match s {
            Status::Verified => Self::Verified,
            Status::Tested => Self::Tested,
            Status::Neural => Self::Neural,
            Status::Stale => Self::Stale,
            Status::Orphan => Self::Orphan,
            Status::Forged => Self::Forged,
            Status::Unknown => Self::Unknown,
            Status::PendingDeepen => Self::PendingDeepen,
            Status::Counterexample => Self::Counterexample,
            Status::Inconclusive => Self::Inconclusive,
        }
    }
}

/// Per the sample mockup: "border = red for critical status
/// (stale / orphan / forged)". Counterexample is a strictly worse
/// state but wasn't in the original spec; left as default-bordered
/// until a follow-up extends the rule with a recorded decision.
pub(crate) fn is_critical(status: Status) -> bool {
    matches!(status, Status::Stale | Status::Orphan | Status::Forged)
}

pub(crate) fn status_of(entry: &IndexEntry) -> Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

pub(crate) fn parent_ids(entry: &IndexEntry) -> Vec<&AnnotationId> {
    match entry {
        IndexEntry::Intent(e) => e.parent.iter().flat_map(|p| p.iter()).collect(),
        IndexEntry::Assume(e) => e.parent.iter().flat_map(|p| p.iter()).collect(),
    }
}

fn parse_filters(filter_strings: &[String]) -> CliResult<Vec<Filter>> {
    let mut out = Vec::with_capacity(filter_strings.len());
    for raw in filter_strings {
        let f: Filter = raw.parse().map_err(|e| CliError::Other {
            message: format!("{e}"),
            exit_code: 2,
        })?;
        out.push(f);
    }
    Ok(out)
}

fn filter_index(index: IndexFile, filters: &[Filter]) -> IndexFile {
    let entries = index
        .entries
        .into_iter()
        .filter(|(id, entry)| filters.iter().all(|f| matches_filter(id, entry, f)))
        .collect();
    IndexFile {
        meta: index.meta,
        entries,
    }
}

fn drop_assumes(index: IndexFile) -> IndexFile {
    let entries = index
        .entries
        .into_iter()
        .filter(|(_, entry)| matches!(entry, IndexEntry::Intent(_)))
        .collect();
    IndexFile {
        meta: index.meta,
        entries,
    }
}

/// Drop intent entries that have no parent AND no children (graph
/// orphans). Assumes are always preserved — they're contextual
/// background facts; `--exclude-assumes` is the separate opt-out
/// for that population.
fn drop_orphan_intents(index: IndexFile) -> IndexFile {
    // Precompute which annotations are referenced as a parent by any
    // other entry. `has_children[id] = true` means at least one other
    // entry has `id` in its `parent` field.
    let mut has_children: std::collections::HashSet<&AnnotationId> =
        std::collections::HashSet::new();
    for entry in index.entries.values() {
        for p in parent_ids(entry) {
            has_children.insert(p);
        }
    }
    let has_children_owned: std::collections::HashSet<AnnotationId> =
        has_children.into_iter().cloned().collect();

    let entries = index
        .entries
        .into_iter()
        .filter(|(id, entry)| match entry {
            IndexEntry::Assume(_) => true, // Assumes always kept.
            IndexEntry::Intent(_) => {
                let no_parent = parent_ids(entry).is_empty();
                let no_children = !has_children_owned.contains(id);
                !(no_parent && no_children)
            }
        })
        .collect();
    IndexFile {
        meta: index.meta,
        entries,
    }
}

/// Walk `depth` hops in both directions (ancestors via `parent` links,
/// descendants via reverse-edge traversal) from each annotation in the
/// `matched` subset of `full`. Returns a new IndexFile whose entries
/// are `matched` ∪ everything reachable within `depth` hops.
///
/// Used by `--depth` to add context around filter-matched nodes. A
/// `depth` of 0 means "matched only" (effectively the same as no
/// `--depth` flag); higher values expand the context.
fn expand_by_depth(full: &IndexFile, matched: IndexFile, depth: u32) -> IndexFile {
    use std::collections::{BTreeSet, VecDeque};

    if depth == 0 {
        return matched;
    }

    // Build a reverse index: parent_id → [child_ids] so descendant
    // expansion is a constant-time lookup per node.
    let mut children_of: std::collections::HashMap<&AnnotationId, Vec<&AnnotationId>> =
        std::collections::HashMap::new();
    for (id, entry) in full.entries.iter() {
        for p in parent_ids(entry) {
            children_of.entry(p).or_default().push(id);
        }
    }

    let mut included: BTreeSet<AnnotationId> = matched.entries.keys().cloned().collect();
    // BFS frontier with remaining-hops counter per node.
    let mut frontier: VecDeque<(AnnotationId, u32)> = matched
        .entries
        .keys()
        .cloned()
        .map(|id| (id, depth))
        .collect();

    while let Some((id, hops_remaining)) = frontier.pop_front() {
        if hops_remaining == 0 {
            continue;
        }
        // Ancestors (follow parent links).
        if let Some(entry) = full.entries.get(&id) {
            for parent in parent_ids(entry) {
                if included.insert(parent.clone()) {
                    frontier.push_back((parent.clone(), hops_remaining - 1));
                }
            }
        }
        // Descendants (follow reverse edges).
        if let Some(kids) = children_of.get(&id) {
            for child in kids {
                if included.insert((*child).clone()) {
                    frontier.push_back(((*child).clone(), hops_remaining - 1));
                }
            }
        }
    }

    let entries = full
        .entries
        .iter()
        .filter(|(id, _)| included.contains(id))
        .map(|(id, entry)| (id.clone(), entry.clone()))
        .collect();
    IndexFile {
        meta: full.meta.clone(),
        entries,
    }
}

fn matches_filter(id: &AnnotationId, entry: &IndexEntry, f: &Filter) -> bool {
    match f {
        Filter::Id(want) => id.as_str() == want,
        Filter::File { path, line_range } => {
            if file_of(entry) != path {
                return false;
            }
            match line_range {
                None => true,
                Some((lo, hi)) => match site_line(entry) {
                    Some(line) => line >= *lo && line <= *hi,
                    None => false,
                },
            }
        }
        Filter::Parent(want) => parent_ids(entry).iter().any(|p| p.as_str() == want),
        Filter::Status(want) => crate::commands::show::status_label(status_of(entry)) == want,
    }
}

fn file_of(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

/// Parse the trailing `(line N)` suffix the index stamper writes onto
/// every site string. Returns `None` if the suffix isn't present;
/// the caller treats absent line as "filter doesn't match" (strict).
fn site_line(entry: &IndexEntry) -> Option<u32> {
    let site = match entry {
        IndexEntry::Intent(e) => &e.site,
        IndexEntry::Assume(e) => &e.site,
    };
    let open = site.rfind("(line ")?;
    let after = &site[open + "(line ".len()..];
    let close = after.rfind(')')?;
    after[..close].trim().parse().ok()
}

/// Verify-level → human label appended to the node label (in
/// parentheses after the kind). Mirrors the mockup wording.
pub(crate) fn verify_label(entry: &IndexEntry) -> Option<String> {
    match entry {
        IndexEntry::Assume(_) => None,
        IndexEntry::Intent(e) => Some(match e.verify {
            VerifyLevel::Bool(false) => "verify=false".to_string(),
            VerifyLevel::Bool(true) => "verify=true".to_string(),
            VerifyLevel::Method(VerifyMethod::Neural) => "verify=neural".to_string(),
            VerifyLevel::Method(VerifyMethod::Test) => "verify=test".to_string(),
            VerifyLevel::Method(VerifyMethod::Full) => "verify=full".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AssumeEntry, BindingState, CoveredRegion, IntentEntry, Meta, ParentLink, Sha256,
        VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(
        file: &str,
        verify: VerifyLevel,
        status: Status,
        parent: Option<ParentLink>,
    ) -> IntentEntry {
        IntentEntry {
            text: "x".into(),
            verify,
            status,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: file.into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        }
    }

    fn make_index() -> IndexFile {
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("a").unwrap(),
            IndexEntry::Intent(intent(
                "src/a.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Verified,
                None,
            )),
        );
        entries.insert(
            AnnotationId::parse("b").unwrap(),
            IndexEntry::Intent(intent(
                "src/b.rs",
                VerifyLevel::Method(VerifyMethod::Test),
                Status::Stale,
                Some(ParentLink::Single(AnnotationId::parse("a").unwrap())),
            )),
        );
        entries.insert(
            AnnotationId::parse("c").unwrap(),
            IndexEntry::Intent(intent(
                "src/a.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Unknown,
                None,
            )),
        );
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries,
        }
    }

    #[test]
    fn filter_index_id_keeps_only_matching() {
        let idx = make_index();
        let filtered = filter_index(idx, &[Filter::Id("b".into())]);
        assert_eq!(filtered.entries.len(), 1);
        assert!(filtered
            .entries
            .contains_key(&AnnotationId::parse("b").unwrap()));
    }

    #[test]
    fn filter_index_file_keeps_all_in_file() {
        let idx = make_index();
        let filtered = filter_index(
            idx,
            &[Filter::File {
                path: "src/a.rs".into(),
                line_range: None,
            }],
        );
        // a + c both live in src/a.rs.
        assert_eq!(filtered.entries.len(), 2);
        assert!(filtered
            .entries
            .contains_key(&AnnotationId::parse("a").unwrap()));
        assert!(filtered
            .entries
            .contains_key(&AnnotationId::parse("c").unwrap()));
    }

    #[test]
    fn filter_index_parent_finds_children() {
        let idx = make_index();
        let filtered = filter_index(idx, &[Filter::Parent("a".into())]);
        // b is the only child of a.
        assert_eq!(filtered.entries.len(), 1);
        assert!(filtered
            .entries
            .contains_key(&AnnotationId::parse("b").unwrap()));
    }

    #[test]
    fn filter_index_status_keeps_matching_state() {
        let idx = make_index();
        let filtered = filter_index(idx, &[Filter::Status("stale".into())]);
        assert_eq!(filtered.entries.len(), 1);
        assert!(filtered
            .entries
            .contains_key(&AnnotationId::parse("b").unwrap()));
    }

    #[test]
    fn filter_index_multiple_ands_together() {
        let idx = make_index();
        // src/a.rs AND status=verified → just `a`.
        let filtered = filter_index(
            idx,
            &[
                Filter::File {
                    path: "src/a.rs".into(),
                    line_range: None,
                },
                Filter::Status("verified".into()),
            ],
        );
        assert_eq!(filtered.entries.len(), 1);
        assert!(filtered
            .entries
            .contains_key(&AnnotationId::parse("a").unwrap()));
    }

    #[test]
    fn filter_index_no_matches_returns_empty_keeps_meta() {
        let idx = make_index();
        let filtered = filter_index(idx, &[Filter::Id("does_not_exist".into())]);
        assert!(filtered.entries.is_empty());
        assert_eq!(filtered.meta.schema_version, 1);
    }

    #[test]
    fn site_line_parses_trailing_line_suffix() {
        let entry = IndexEntry::Assume(AssumeEntry {
            text: "x".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/x.rs".into(),
            site: "fn foo (line 42)".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        });
        assert_eq!(site_line(&entry), Some(42));
    }

    #[test]
    fn expand_by_depth_zero_returns_matched_unchanged() {
        let idx = make_index();
        let matched = filter_index(idx.clone(), &[Filter::Id("a".into())]);
        let expanded = expand_by_depth(&idx, matched.clone(), 0);
        assert_eq!(
            expanded.entries.keys().collect::<Vec<_>>(),
            matched.entries.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn expand_by_depth_one_picks_up_immediate_descendants() {
        let idx = make_index();
        // a is the parent of b. Filter to {a}; depth=1 should pull in b.
        let matched = filter_index(idx.clone(), &[Filter::Id("a".into())]);
        let expanded = expand_by_depth(&idx, matched, 1);
        let keys: Vec<&str> = expanded.entries.keys().map(|k| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
        assert!(
            !keys.contains(&"c"),
            "c is a separate root, should not be reached"
        );
    }

    #[test]
    fn expand_by_depth_one_picks_up_immediate_ancestors() {
        let idx = make_index();
        // Filter to {b}; depth=1 should pull in b's parent a (but not c).
        let matched = filter_index(idx.clone(), &[Filter::Id("b".into())]);
        let expanded = expand_by_depth(&idx, matched, 1);
        let keys: Vec<&str> = expanded.entries.keys().map(|k| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
        assert!(!keys.contains(&"c"));
    }

    #[test]
    fn expand_by_depth_bounded_by_n() {
        // Create a linear chain a → b → c (b parent=a, c parent=b).
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("a").unwrap(),
            IndexEntry::Intent(intent(
                "src/a.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Unknown,
                None,
            )),
        );
        entries.insert(
            AnnotationId::parse("b").unwrap(),
            IndexEntry::Intent(intent(
                "src/a.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Unknown,
                Some(ParentLink::Single(AnnotationId::parse("a").unwrap())),
            )),
        );
        entries.insert(
            AnnotationId::parse("c").unwrap(),
            IndexEntry::Intent(intent(
                "src/a.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Unknown,
                Some(ParentLink::Single(AnnotationId::parse("b").unwrap())),
            )),
        );
        let idx = IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries,
        };
        // Start at {a}; depth=1 should reach b but not c.
        let matched = filter_index(idx.clone(), &[Filter::Id("a".into())]);
        let exp1 = expand_by_depth(&idx, matched.clone(), 1);
        assert!(exp1
            .entries
            .contains_key(&AnnotationId::parse("b").unwrap()));
        assert!(!exp1
            .entries
            .contains_key(&AnnotationId::parse("c").unwrap()));
        // depth=2 should reach c.
        let exp2 = expand_by_depth(&idx, matched, 2);
        assert!(exp2
            .entries
            .contains_key(&AnnotationId::parse("c").unwrap()));
    }

    #[test]
    fn drop_orphan_intents_removes_standalone_intent_keeps_connected() {
        // make_index has a (no parent, has child b), b (has parent a),
        // c (no parent, no children → orphan intent).
        let idx = make_index();
        let pruned = drop_orphan_intents(idx);
        let keys: Vec<&str> = pruned.entries.keys().map(|k| k.as_str()).collect();
        assert!(keys.contains(&"a"), "a has child b, should stay");
        assert!(keys.contains(&"b"), "b has parent a, should stay");
        assert!(!keys.contains(&"c"), "c is an orphan intent, should drop");
    }

    #[test]
    fn drop_orphan_intents_keeps_assumes_even_when_orphan() {
        let id_assume = AnnotationId::parse("storage_atom").unwrap();
        let mut entries = BTreeMap::new();
        entries.insert(
            id_assume.clone(),
            IndexEntry::Assume(AssumeEntry {
                text: "x".into(),
                status: Status::Unknown,
                text_hash: sha('a'),
                body_hash: sha('b'),
                file: "src/x.rs".into(),
                site: "mod storage".into(),
                covered_region: CoveredRegion::ModuleInlineBody,
                linked: None,
                parent: None,
            }),
        );
        let idx = IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries,
        };
        let pruned = drop_orphan_intents(idx);
        assert!(
            pruned.entries.contains_key(&id_assume),
            "orphan assume must stay"
        );
    }

    #[test]
    fn site_line_returns_none_when_suffix_missing() {
        let entry = IndexEntry::Assume(AssumeEntry {
            text: "x".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/x.rs".into(),
            site: "mod storage".into(),
            covered_region: CoveredRegion::ModuleInlineBody,
            linked: None,
            parent: None,
        });
        assert_eq!(site_line(&entry), None);
    }
}
