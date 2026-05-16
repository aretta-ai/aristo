//! Detect cycles in the annotation parent graph.
//!
//! `aristo stamp` calls this after building the id → parents map from
//! source. Cycles are forbidden: an annotation's `parent` is a strict
//! sub-obligation pointer, so `a -> b -> a` would mean each is a
//! sub-obligation of the other, which is incoherent.
//!
//! Diamonds — `a -> b`, `a -> c`, `b -> d`, `c -> d` — are allowed and
//! common. The detector visits each node once, not once per path, so
//! diamond traversal is cheap.
//!
//! Self-cycles (`a -> a`) get the same error type as multi-node cycles;
//! the path simply has length 1.
//!
//! References to ids that aren't in the map (orphan parent references)
//! are NOT cycle errors — they're caught by `aristo stamp`'s separate
//! id-resolution pass and surfaced with a different diagnostic.

use std::collections::{HashMap, HashSet};

use crate::index::AnnotationId;

/// A detected cycle. `path` lists the nodes in traversal order, ending
/// with the node that closed the cycle (which equals `path[0]`). For a
/// self-cycle, `path == [id]`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("annotation parent graph contains a cycle: {}", format_path(path))]
pub struct CycleError {
    pub path: Vec<AnnotationId>,
}

fn format_path(path: &[AnnotationId]) -> String {
    let mut s = String::new();
    for (i, id) in path.iter().enumerate() {
        if i > 0 {
            s.push_str(" -> ");
        }
        s.push_str(id.as_str());
    }
    if path.len() > 1 {
        // Append the closing edge so the user sees the loop visually.
        s.push_str(" -> ");
        s.push_str(path[0].as_str());
    } else if path.len() == 1 {
        // Self-cycle: render as `id -> id` for clarity.
        s.push_str(" -> ");
        s.push_str(path[0].as_str());
    }
    s
}

#[aristo::intent(
    "detect_cycles returns the FIRST cycle it finds and stops; it does \
     not enumerate all cycles in the graph. The diagnostic-friendly path \
     is enough for the user to break the cycle and re-run; chasing every \
     cycle on the same pass would multiply diagnostic noise without \
     helping the fix.",
    verify = "test",
    id = "detect_cycles_returns_first_cycle_only"
)]
pub fn detect_cycles(parents: &HashMap<AnnotationId, Vec<AnnotationId>>) -> Result<(), CycleError> {
    let mut visited: HashSet<AnnotationId> = HashSet::new();
    let mut on_stack: HashSet<AnnotationId> = HashSet::new();
    let mut path: Vec<AnnotationId> = Vec::new();

    for start in parents.keys() {
        if visited.contains(start) {
            continue;
        }
        if let Some(cycle) = dfs(start, parents, &mut visited, &mut on_stack, &mut path) {
            return Err(CycleError { path: cycle });
        }
    }
    Ok(())
}

fn dfs(
    node: &AnnotationId,
    parents: &HashMap<AnnotationId, Vec<AnnotationId>>,
    visited: &mut HashSet<AnnotationId>,
    on_stack: &mut HashSet<AnnotationId>,
    path: &mut Vec<AnnotationId>,
) -> Option<Vec<AnnotationId>> {
    visited.insert(node.clone());
    on_stack.insert(node.clone());
    path.push(node.clone());

    if let Some(children) = parents.get(node) {
        for child in children {
            if on_stack.contains(child) {
                // Found a cycle. Slice path from the first occurrence of
                // child onward — that's the loop.
                let start = path
                    .iter()
                    .position(|p| p == child)
                    .expect("on_stack ⇒ in path");
                return Some(path[start..].to_vec());
            }
            if !visited.contains(child) {
                if let Some(cycle) = dfs(child, parents, visited, on_stack, path) {
                    return Some(cycle);
                }
            }
        }
    }

    on_stack.remove(node);
    path.pop();
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> AnnotationId {
        AnnotationId::parse(s).expect("test id must parse")
    }

    fn graph(edges: &[(&str, &[&str])]) -> HashMap<AnnotationId, Vec<AnnotationId>> {
        edges
            .iter()
            .map(|(node, parents)| (id(node), parents.iter().map(|p| id(p)).collect::<Vec<_>>()))
            .collect()
    }

    #[test]
    fn empty_graph_is_acyclic() {
        let g = HashMap::new();
        detect_cycles(&g).unwrap();
    }

    #[test]
    fn linear_chain_is_acyclic() {
        // a -> b -> c -> d
        let g = graph(&[("a", &["b"]), ("b", &["c"]), ("c", &["d"]), ("d", &[])]);
        detect_cycles(&g).unwrap();
    }

    #[test]
    fn self_cycle_detected() {
        let g = graph(&[("a", &["a"])]);
        let err = detect_cycles(&g).unwrap_err();
        assert_eq!(err.path, vec![id("a")]);
        assert!(err.to_string().contains("a -> a"));
    }

    #[test]
    fn two_cycle_detected() {
        // a -> b -> a
        let g = graph(&[("a", &["b"]), ("b", &["a"])]);
        let err = detect_cycles(&g).unwrap_err();
        // Cycle path includes both nodes.
        assert_eq!(err.path.len(), 2);
        assert!(err.path.contains(&id("a")));
        assert!(err.path.contains(&id("b")));
    }

    #[test]
    fn three_cycle_detected() {
        // a -> b -> c -> a
        let g = graph(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]);
        let err = detect_cycles(&g).unwrap_err();
        assert_eq!(err.path.len(), 3);
    }

    #[test]
    fn diamond_is_acyclic() {
        // a -> b, a -> c, b -> d, c -> d
        let g = graph(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"]), ("d", &[])]);
        detect_cycles(&g).unwrap();
    }

    #[test]
    fn multiple_disconnected_components_acyclic() {
        // Component 1: a -> b
        // Component 2: c -> d -> e
        let g = graph(&[
            ("a", &["b"]),
            ("b", &[]),
            ("c", &["d"]),
            ("d", &["e"]),
            ("e", &[]),
        ]);
        detect_cycles(&g).unwrap();
    }

    #[test]
    fn cycle_in_one_component_detected_when_other_is_clean() {
        // Component 1 (clean): a -> b
        // Component 2 (cycle): c -> d -> c
        let g = graph(&[("a", &["b"]), ("b", &[]), ("c", &["d"]), ("d", &["c"])]);
        let err = detect_cycles(&g).unwrap_err();
        assert!(err.path.contains(&id("c")));
        assert!(err.path.contains(&id("d")));
    }

    #[test]
    fn orphan_parent_reference_is_not_a_cycle_error() {
        // a -> nonexistent_parent. We don't have nonexistent_parent in
        // the map; that's an id-resolution problem, not a cycle. Cycle
        // detection passes; stamp's id-resolution pass would flag it.
        let g = graph(&[("a", &["nonexistent_parent"])]);
        detect_cycles(&g).unwrap();
    }

    #[test]
    fn multi_parent_node_with_one_cycle_branch_detected() {
        // a -> b, a -> c, b -> a (b's parent is a — cycle through a-b)
        let g = graph(&[("a", &["b", "c"]), ("b", &["a"]), ("c", &[])]);
        let err = detect_cycles(&g).unwrap_err();
        assert_eq!(err.path.len(), 2);
    }

    #[test]
    fn long_cycle_detected() {
        // Ten-node cycle: n0 -> n1 -> ... -> n9 -> n0
        let edges: Vec<(String, Vec<String>)> = (0..10)
            .map(|i| (format!("n{i}"), vec![format!("n{}", (i + 1) % 10)]))
            .collect();
        let g: HashMap<AnnotationId, Vec<AnnotationId>> = edges
            .iter()
            .map(|(n, ps)| (id(n), ps.iter().map(|p| id(p)).collect()))
            .collect();
        let err = detect_cycles(&g).unwrap_err();
        assert_eq!(err.path.len(), 10);
    }

    #[test]
    fn cycle_error_message_renders_path_with_arrows() {
        let g = graph(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]);
        let err = detect_cycles(&g).unwrap_err();
        let msg = err.to_string();
        // Three nodes + closing edge = 3 arrow separators.
        assert_eq!(msg.matches(" -> ").count(), 3, "got: {msg}");
    }

    #[test]
    fn aristos_namespace_ids_work() {
        // Server-bound ids with `aristos:` prefix flow through cycle
        // detection like any other id.
        let g = graph(&[
            ("aristos:foo", &["aristos:bar"]),
            ("aristos:bar", &["aristos:foo"]),
        ]);
        let err = detect_cycles(&g).unwrap_err();
        assert_eq!(err.path.len(), 2);
    }
}
