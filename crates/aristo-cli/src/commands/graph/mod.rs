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
        filter_index(index, &filters)
    };
    if exclude_assumes {
        scoped_index = drop_assumes(scoped_index);
    }
    let graph = model::build(&scoped_index);
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

/// What color class a node belongs to. Drives the Mermaid / DOT
/// fill+stroke pair. Verify-level coloring (the default mode); the
/// status-axis mode (slice 29 commit 9, `--include-status`) will live
/// in a sibling enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyClass {
    /// `verify = false` — documentation-only intent. Also used for
    /// assumes (which have no verify level — the gray styling matches
    /// "background fact").
    False,
    /// `verify = true` (project default) OR `verify = "neural"`.
    /// Project default usually resolves to neural for free-tier
    /// projects; rendering as Neural here keeps the color stable for
    /// the common case. A future commit could resolve to project
    /// default before mapping.
    Neural,
    Test,
    Full,
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
