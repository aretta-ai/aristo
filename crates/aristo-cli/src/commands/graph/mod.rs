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

use aristo_core::index::{AnnotationId, IndexEntry, Status, VerifyLevel, VerifyMethod};

use crate::commands::index::{atomic_write, workspace_or_error};
use crate::commands::show::read_index;
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

pub(crate) fn run(format: &str, out: Option<PathBuf>) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    let format = Format::parse(format).map_err(|message| CliError::Other {
        message,
        exit_code: 2,
    })?;

    let graph = model::build(&index);
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
