//! Graphviz DOT emitter.
//!
//! Universal interop format. Pipe into any Graphviz tool for SVG / PNG /
//! PDF rendering (`dot -Tsvg`, `dot -Tpng`, `neato`, `fdp`, etc.).
//! Aristo does NOT shell out for this format — we just emit text.

use std::fmt::Write;

use super::model::{Edge, Graph, Kind, Node};
use super::VerifyClass;

pub(crate) fn render(g: &Graph) -> String {
    let mut out = String::new();
    out.push_str("digraph aristo {\n");
    out.push_str("    rankdir=TB;\n");
    out.push_str("    node [fontname=\"Helvetica\", fontsize=10];\n\n");

    let intents: Vec<&Node> = g.nodes.iter().filter(|n| n.kind == Kind::Intent).collect();
    let assumes: Vec<&Node> = g.nodes.iter().filter(|n| n.kind == Kind::Assume).collect();

    if !intents.is_empty() {
        out.push_str("    // Intent nodes — rectangles, colored by verify level\n");
        for n in &intents {
            push_node(&mut out, n);
        }
    }
    if !assumes.is_empty() {
        if !intents.is_empty() {
            out.push('\n');
        }
        out.push_str("    // Assume nodes — hexagons, gray\n");
        for n in &assumes {
            push_node(&mut out, n);
        }
    }

    if !g.edges.is_empty() {
        out.push('\n');
        out.push_str("    // Parent edges: child -> parent\n");
        for e in &g.edges {
            push_edge(&mut out, e);
        }
    }

    out.push_str("}\n");
    out
}

fn push_node(out: &mut String, n: &Node) {
    let id = escape_dot_id(n.id.as_str());
    let label = format!("{}\\n{}", n.id.as_str(), n.label_kind_suffix);
    let label = escape_dot_label(&label);

    let (shape, fillcolor, stroke) = node_style(n);
    let style_parts = if n.is_critical {
        // Critical: red 3px border, fillcolor preserved.
        format!("shape={shape}, style=\"filled,bold\", fillcolor=\"{fillcolor}\", color=\"#dc2626\", penwidth=3")
    } else {
        format!("shape={shape}, style=filled, fillcolor=\"{fillcolor}\", color=\"{stroke}\"")
    };

    writeln!(out, "    \"{id}\" [{style_parts}, label=\"{label}\"];")
        .expect("string write never fails");
}

fn push_edge(out: &mut String, e: &Edge) {
    let from = escape_dot_id(e.from.as_str());
    let to = escape_dot_id(e.to.as_str());
    writeln!(out, "    \"{from}\" -> \"{to}\";").expect("string write never fails");
}

/// (shape, fillcolor, stroke) for a node. Matches the sample mockup's
/// color palette so users get visual parity across formats.
fn node_style(n: &Node) -> (&'static str, &'static str, &'static str) {
    let shape = match n.kind {
        Kind::Intent => "box",
        Kind::Assume => "hexagon",
    };
    // Assumes always use the neutral gray palette regardless of class.
    if n.kind == Kind::Assume {
        return ("hexagon", "#f3f4f6", "#6b7280");
    }
    let (fill, stroke) = match n.verify_class {
        VerifyClass::False => ("#e5e5e5", "#999999"),
        VerifyClass::Neural => ("#fef3c7", "#b45309"),
        VerifyClass::Test => ("#dbeafe", "#1d4ed8"),
        VerifyClass::Full => ("#bbf7d0", "#15803d"),
    };
    (shape, fill, stroke)
}

/// DOT identifiers MUST be either a simple id (`[A-Za-z_][A-Za-z0-9_]*`)
/// or a quoted string. We always emit quoted strings, so escape any
/// embedded `"` or `\` in the raw id.
fn escape_dot_id(id: &str) -> String {
    id.replace('\\', "\\\\").replace('"', "\\\"")
}

/// DOT label escapes: `\n` becomes a line break in the rendered node
/// (so we use literal `\n` sequences in the source). Quote and
/// backslash both need escaping.
fn escape_dot_label(s: &str) -> String {
    // Order matters: replace `\` first so we don't double-escape the
    // backslashes we add for `"`.
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        // Preserve our intentional `\n` line breaks (they were inserted
        // as the literal two-char sequence by the caller; the global
        // backslash replace above turned them into `\\n`, undo that).
        .replace("\\\\n", "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::graph::model::{Edge, Graph, Kind, Node};
    use crate::commands::graph::VerifyClass;
    use aristo_core::index::AnnotationId;

    fn id(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    fn intent_node(s: &str, vc: VerifyClass, critical: bool) -> Node {
        Node {
            id: id(s),
            kind: Kind::Intent,
            verify_class: vc,
            label_kind_suffix: format!(
                "(intent, verify={})",
                match vc {
                    VerifyClass::False => "false",
                    VerifyClass::Neural => "neural",
                    VerifyClass::Test => "test",
                    VerifyClass::Full => "full",
                }
            ),
            is_critical: critical,
        }
    }

    fn assume_node(s: &str) -> Node {
        Node {
            id: id(s),
            kind: Kind::Assume,
            verify_class: VerifyClass::False,
            label_kind_suffix: "(assume)".to_string(),
            is_critical: false,
        }
    }

    #[test]
    fn render_empty_graph_emits_skeleton_only() {
        let g = Graph {
            nodes: vec![],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.starts_with("digraph aristo {\n"));
        assert!(out.contains("rankdir=TB"));
        assert!(out.contains("node [fontname=\"Helvetica\""));
        assert!(out.ends_with("}\n"));
        assert!(!out.contains("// Intent nodes"));
        assert!(!out.contains("// Assume nodes"));
    }

    #[test]
    fn render_intent_uses_box_shape_and_verify_class_palette() {
        let g = Graph {
            nodes: vec![intent_node("foo", VerifyClass::Full, false)],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("shape=box"));
        assert!(out.contains("fillcolor=\"#bbf7d0\""));
        assert!(out.contains("color=\"#15803d\""));
        assert!(out.contains("foo\\n(intent, verify=full)"));
    }

    #[test]
    fn render_assume_uses_hexagon_shape_and_gray_palette() {
        let g = Graph {
            nodes: vec![assume_node("bar")],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("shape=hexagon"));
        assert!(out.contains("fillcolor=\"#f3f4f6\""));
        assert!(out.contains("color=\"#6b7280\""));
    }

    #[test]
    fn render_critical_node_uses_red_border_with_penwidth_3() {
        let g = Graph {
            nodes: vec![intent_node("foo", VerifyClass::Full, true)],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("color=\"#dc2626\""));
        assert!(out.contains("penwidth=3"));
        // Fillcolor preserved (verify-level still shows under the red border).
        assert!(out.contains("fillcolor=\"#bbf7d0\""));
    }

    #[test]
    fn render_edge_uses_quoted_arrow() {
        let g = Graph {
            nodes: vec![
                intent_node("parent", VerifyClass::Full, false),
                intent_node("child", VerifyClass::Neural, false),
            ],
            edges: vec![Edge {
                from: id("child"),
                to: id("parent"),
            }],
        };
        let out = render(&g);
        assert!(out.contains("\"child\" -> \"parent\";"));
    }

    #[test]
    fn render_aristos_id_emits_quoted_form_unescaped() {
        let g = Graph {
            nodes: vec![intent_node("aristos:foo", VerifyClass::Full, false)],
            edges: vec![],
        };
        let out = render(&g);
        // DOT quoted strings tolerate `:` directly; no transformation needed.
        assert!(out.contains("\"aristos:foo\""));
        // Label preserves the raw id with a \n separator before the kind suffix.
        assert!(out.contains("aristos:foo\\n(intent, verify=full)"));
    }

    #[test]
    fn escape_dot_id_handles_quote_and_backslash() {
        assert_eq!(escape_dot_id(r#"a"b\c"#), r#"a\"b\\c"#);
    }
}
