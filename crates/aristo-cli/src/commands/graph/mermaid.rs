//! Mermaid `flowchart TD` emitter.
//!
//! Renders a [`Graph`] as a fenced ```mermaid block (the literal triple
//! backticks are emitted) suitable for pasting into a GitHub README,
//! rustdoc, or any markdown viewer that supports Mermaid.

use std::fmt::Write;

use super::model::{Edge, Graph, Kind, Node};
use super::VerifyClass;

pub(crate) fn render(g: &Graph) -> String {
    let mut out = String::new();
    out.push_str("```mermaid\n");
    out.push_str("flowchart TD\n");
    push_class_defs(&mut out);
    out.push('\n');

    let id_for_node = node_ids(g);

    // Group nodes by kind for readability in the emitted source (the
    // mockup splits "Intent nodes" from "Assume nodes" with comments).
    let intents: Vec<&Node> = g.nodes.iter().filter(|n| n.kind == Kind::Intent).collect();
    let assumes: Vec<&Node> = g.nodes.iter().filter(|n| n.kind == Kind::Assume).collect();

    if !intents.is_empty() {
        out.push_str("    %% Intent nodes (rectangles)\n");
        for n in &intents {
            push_node_line(&mut out, n, &id_for_node[n.id.as_str()]);
        }
        if !assumes.is_empty() {
            out.push('\n');
        }
    }
    if !assumes.is_empty() {
        out.push_str("    %% Assume nodes (hexagons)\n");
        for n in &assumes {
            push_node_line(&mut out, n, &id_for_node[n.id.as_str()]);
        }
    }

    if !g.edges.is_empty() {
        out.push('\n');
        out.push_str("    %% Parent edges: child --> parent\n");
        for e in &g.edges {
            push_edge_line(&mut out, e, &id_for_node);
        }
    }

    let critical: Vec<&Node> = g.nodes.iter().filter(|n| n.is_critical).collect();
    if !critical.is_empty() {
        out.push('\n');
        out.push_str("    %% Critical-status border\n");
        for n in &critical {
            writeln!(out, "    class {} critical", id_for_node[n.id.as_str()])
                .expect("string write never fails");
        }
    }

    out.push_str("```\n");
    out
}

fn push_class_defs(out: &mut String) {
    // Color palette is copied verbatim from the sample mockup so the
    // rendered output matches the spec byte-for-byte.
    out.push_str("    classDef vFalse  fill:#e5e5e5,stroke:#999\n");
    out.push_str("    classDef vNeural fill:#fef3c7,stroke:#b45309\n");
    out.push_str("    classDef vTest   fill:#dbeafe,stroke:#1d4ed8\n");
    out.push_str("    classDef vFull   fill:#bbf7d0,stroke:#15803d\n");
    out.push_str("    classDef critical stroke:#dc2626,stroke-width:3px\n");
}

fn push_node_line(out: &mut String, n: &Node, node_id: &str) {
    let (open, close) = match n.kind {
        Kind::Intent => ("[\"", "\"]"),
        Kind::Assume => ("{{\"", "\"}}"),
    };
    let class = match n.verify_class {
        VerifyClass::False => "vFalse",
        VerifyClass::Neural => "vNeural",
        VerifyClass::Test => "vTest",
        VerifyClass::Full => "vFull",
    };
    let label_id = escape_label(n.id.as_str());
    let label_suffix = escape_label(&n.label_kind_suffix);
    writeln!(
        out,
        "    {node_id}{open}{label_id}<br/>{label_suffix}{close}:::{class}"
    )
    .expect("string write never fails");
}

fn push_edge_line(
    out: &mut String,
    e: &Edge,
    id_for_node: &std::collections::HashMap<String, String>,
) {
    let from = &id_for_node[e.from.as_str()];
    let to = &id_for_node[e.to.as_str()];
    writeln!(out, "    {from} --> {to}").expect("string write never fails");
}

/// Mermaid label text rules:
/// - quotes must be escaped (we use `["..."]`)
/// - `<` / `>` are HTML in Mermaid labels — escape to avoid corruption
fn escape_label(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Assign a Mermaid-safe node identifier to each graph node. Mermaid
/// identifiers can only contain `[A-Za-z0-9_]`, so any id with `:` (the
/// `aristos:` namespace) or `-` gets transformed. Sanitization is
/// deterministic and reversible: `:` → `__`, anything else outside the
/// safe charset → `_`. Iteration order matches the BTreeMap-driven
/// build (stable across runs).
fn node_ids(g: &Graph) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::with_capacity(g.nodes.len());
    for n in &g.nodes {
        let mermaid_id = sanitize_for_mermaid(n.id.as_str());
        out.insert(n.id.as_str().to_string(), mermaid_id);
    }
    out
}

fn sanitize_for_mermaid(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 2);
    for c in id.chars() {
        if c == ':' {
            out.push_str("__");
        } else if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    // Mermaid IDs can't start with a digit; if they do, prefix.
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'n');
    }
    out
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
        assert!(out.starts_with("```mermaid\nflowchart TD\n"));
        assert!(out.contains("classDef vFalse"));
        assert!(out.contains("classDef critical"));
        assert!(out.ends_with("```\n"));
        // No node / edge / critical comment sections.
        assert!(!out.contains("%% Intent nodes"));
        assert!(!out.contains("%% Assume nodes"));
        assert!(!out.contains("%% Parent edges"));
    }

    #[test]
    fn render_intent_uses_rectangle_syntax() {
        let g = Graph {
            nodes: vec![intent_node("foo", VerifyClass::Neural, false)],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("foo[\"foo<br/>(intent, verify=neural)\"]:::vNeural"));
    }

    #[test]
    fn render_assume_uses_hexagon_syntax() {
        let g = Graph {
            nodes: vec![assume_node("bar")],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("bar{{\"bar<br/>(assume)\"}}:::vFalse"));
    }

    #[test]
    fn render_critical_node_gets_class_critical_line() {
        let g = Graph {
            nodes: vec![intent_node("foo", VerifyClass::Full, true)],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("class foo critical"));
        assert!(out.contains("%% Critical-status border"));
    }

    #[test]
    fn render_edge_uses_arrow_with_sanitized_ids() {
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
        assert!(out.contains("child --> parent"));
    }

    #[test]
    fn render_aristos_prefixed_id_gets_sanitized() {
        // The `aristos:` namespace contains a colon, which Mermaid IDs
        // disallow. The display label still shows the raw id; the
        // Mermaid identifier becomes `aristos__foo`.
        let g = Graph {
            nodes: vec![intent_node("aristos:foo", VerifyClass::Full, false)],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("aristos__foo[\"aristos:foo<br/>"));
    }

    #[test]
    fn sanitize_replaces_colon_with_double_underscore() {
        assert_eq!(sanitize_for_mermaid("aristos:foo"), "aristos__foo");
    }

    #[test]
    fn sanitize_prepends_n_when_id_starts_with_digit() {
        assert_eq!(sanitize_for_mermaid("9lives"), "n9lives");
    }

    #[test]
    fn sanitize_preserves_alphanumeric_and_underscore() {
        assert_eq!(sanitize_for_mermaid("foo_bar_42"), "foo_bar_42");
    }

    #[test]
    fn escape_label_handles_quote_lt_gt() {
        assert_eq!(escape_label("a\"b<c>d"), "a&quot;b&lt;c&gt;d");
    }

    #[test]
    fn intent_and_assume_groups_render_with_comments() {
        let g = Graph {
            nodes: vec![
                intent_node("foo", VerifyClass::Full, false),
                assume_node("bar"),
            ],
            edges: vec![],
        };
        let out = render(&g);
        assert!(out.contains("%% Intent nodes (rectangles)"));
        assert!(out.contains("%% Assume nodes (hexagons)"));
        // Intents render before assumes in the source order.
        let intent_pos = out.find("%% Intent nodes").unwrap();
        let assume_pos = out.find("%% Assume nodes").unwrap();
        assert!(intent_pos < assume_pos);
    }
}
