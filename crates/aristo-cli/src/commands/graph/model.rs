//! Graph builder — walks the index and produces a `Graph` of nodes +
//! edges. Format-agnostic; each emitter (mermaid / dot / svg) renders
//! the same model into its own syntax.

use aristo_core::index::{AnnotationId, IndexEntry, IndexFile};

use super::{
    is_critical, parent_ids, status_of, verify_label, ColorAxis, StatusClass, VerifyClass,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub axis: ColorAxis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub id: AnnotationId,
    pub kind: Kind,
    /// Verify-level class. Always computed; renderer only uses this
    /// when `Graph.axis == ColorAxis::Verify`.
    pub verify_class: VerifyClass,
    /// Status class. Always computed; renderer only uses this when
    /// `Graph.axis == ColorAxis::Status`.
    pub status_class: StatusClass,
    /// Mockup pattern: `<id><br/>(<kind>, <verify=X>)`. The renderer
    /// composes the actual escaping per format.
    pub label_kind_suffix: String,
    pub is_critical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Intent,
    Assume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edge {
    /// Child annotation. Edges run child -> parent per the sample
    /// mockup ("Parent edges: child --> parent").
    pub from: AnnotationId,
    pub to: AnnotationId,
}

/// Default builder — Verify color axis. Convenience for unit tests
/// that don't care about the axis. Production code path goes through
/// `build_with_axis` so the `--include-status` flag wires through
/// cleanly.
#[cfg(test)]
pub(crate) fn build(index: &IndexFile) -> Graph {
    build_with_axis(index, ColorAxis::Verify)
}

pub(crate) fn build_with_axis(index: &IndexFile, axis: ColorAxis) -> Graph {
    let mut nodes = Vec::with_capacity(index.entries.len());
    let mut edges = Vec::new();
    for (id, entry) in index.entries.iter() {
        nodes.push(node_for(id.clone(), entry));
        for parent in parent_ids(entry) {
            // Skip dangling parent refs — cycle detection in slice 15
            // already rejects bad indexes at stamp time, so a dangling
            // ref here is unusual but possible if the index was
            // hand-edited. Don't render half an edge.
            if index.entries.contains_key(parent) {
                edges.push(Edge {
                    from: id.clone(),
                    to: parent.clone(),
                });
            }
        }
    }
    Graph { nodes, edges, axis }
}

fn node_for(id: AnnotationId, entry: &IndexEntry) -> Node {
    let kind = match entry {
        IndexEntry::Intent(_) => Kind::Intent,
        IndexEntry::Assume(_) => Kind::Assume,
    };
    let kind_word = match kind {
        Kind::Intent => "intent",
        Kind::Assume => "assume",
    };
    let label_kind_suffix = match verify_label(entry) {
        Some(v) => format!("({kind_word}, {v})"),
        None => format!("({kind_word})"),
    };
    let status = status_of(entry);
    Node {
        id,
        kind,
        verify_class: VerifyClass::from_entry(entry),
        status_class: StatusClass::from_status(status),
        label_kind_suffix,
        is_critical: is_critical(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AnnotationKind, AssumeEntry, BindingState, CoveredRegion, IntentEntry, Meta, ParentLink,
        Sha256, Status, VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(verify: VerifyLevel, status: Status, parent: Option<ParentLink>) -> IntentEntry {
        IntentEntry {
            text: "x".into(),
            verify,
            status,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/x.rs".into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        }
    }

    fn assume(parent: Option<ParentLink>) -> AssumeEntry {
        AssumeEntry {
            text: "y".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/x.rs".into(),
            site: "fn y (line 1)".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent,
        }
    }

    fn empty_meta() -> Meta {
        Meta {
            schema_version: 1,
            generated_by: None,
            generated_at: None,
            source_root: None,
        }
    }

    #[test]
    fn build_empty_index_is_empty_graph() {
        let index = IndexFile {
            meta: empty_meta(),
            entries: BTreeMap::new(),
        };
        let g = build(&index);
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn build_two_nodes_one_edge_child_to_parent() {
        let parent_id = AnnotationId::parse("parent").unwrap();
        let child_id = AnnotationId::parse("child").unwrap();
        let mut entries = BTreeMap::new();
        entries.insert(
            parent_id.clone(),
            IndexEntry::Intent(intent(
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                None,
            )),
        );
        entries.insert(
            child_id.clone(),
            IndexEntry::Intent(intent(
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                Some(ParentLink::Single(parent_id.clone())),
            )),
        );
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let g = build(&index);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].from, child_id);
        assert_eq!(g.edges[0].to, parent_id);
    }

    #[test]
    fn build_assume_node_has_kind_assume_and_verifyclass_false() {
        let id = AnnotationId::parse("storage_atomicity").unwrap();
        let mut entries = BTreeMap::new();
        entries.insert(id.clone(), IndexEntry::Assume(assume(None)));
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let g = build(&index);
        assert_eq!(g.nodes[0].kind, Kind::Assume);
        assert_eq!(g.nodes[0].verify_class, VerifyClass::False);
        assert_eq!(g.nodes[0].label_kind_suffix, "(assume)");
    }

    #[test]
    fn build_critical_flag_set_for_stale_orphan_forged() {
        for (i, (st, expect)) in [
            (Status::Verified, false),
            (Status::Stale, true),
            (Status::Orphan, true),
            (Status::Forged, true),
            (Status::Counterexample, false), // documented exclusion
            (Status::Unknown, false),
        ]
        .into_iter()
        .enumerate()
        {
            let id = AnnotationId::parse(&format!("ann_{i}")).unwrap();
            let mut entries = BTreeMap::new();
            entries.insert(
                id.clone(),
                IndexEntry::Intent(intent(VerifyLevel::Method(VerifyMethod::Full), st, None)),
            );
            let index = IndexFile {
                meta: empty_meta(),
                entries,
            };
            let g = build(&index);
            assert_eq!(g.nodes[0].is_critical, expect, "status {st:?}");
        }
    }

    #[test]
    fn build_dangling_parent_ref_emits_node_but_no_edge() {
        // Index hand-edit could leave a parent pointer to a removed id.
        // Don't render half an edge; cycle detection handles the real case.
        let child_id = AnnotationId::parse("child").unwrap();
        let dangling = AnnotationId::parse("ghost").unwrap();
        let mut entries = BTreeMap::new();
        entries.insert(
            child_id.clone(),
            IndexEntry::Intent(intent(
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Unknown,
                Some(ParentLink::Single(dangling)),
            )),
        );
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let g = build(&index);
        assert_eq!(g.nodes.len(), 1);
        assert!(g.edges.is_empty());
    }

    #[test]
    fn build_multi_parent_emits_one_edge_per_parent() {
        let p1 = AnnotationId::parse("p1").unwrap();
        let p2 = AnnotationId::parse("p2").unwrap();
        let c = AnnotationId::parse("c").unwrap();
        let mut entries = BTreeMap::new();
        entries.insert(
            p1.clone(),
            IndexEntry::Intent(intent(
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                None,
            )),
        );
        entries.insert(
            p2.clone(),
            IndexEntry::Intent(intent(
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                None,
            )),
        );
        entries.insert(
            c.clone(),
            IndexEntry::Intent(intent(
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                Some(ParentLink::Multiple(vec![p1.clone(), p2.clone()])),
            )),
        );
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let g = build(&index);
        let parents_of_c: Vec<_> = g
            .edges
            .iter()
            .filter(|e| e.from == c)
            .map(|e| &e.to)
            .collect();
        assert_eq!(parents_of_c.len(), 2);
        assert!(parents_of_c.contains(&&p1));
        assert!(parents_of_c.contains(&&p2));
    }

    // Suppress unused-import warning when assume() / AnnotationKind aren't
    // used by every test in this module.
    #[allow(dead_code)]
    fn _unused() {
        let _ = AnnotationKind::Intent;
    }
}
