//! Ops module — T₁, T₂, T₃.
//!
//! Responsibilities:
//! - Edit scripts (Def. 1.1, 1.2)
//! - Delta entries with cascade annotation (Def. 2.1, Def. 3.9)
//! - Overlay application onto TypedGraph (Def. 2.5)
//! - Rollup index κ (Def. 5.4)
//! - Nullification predicate (Def. 5.5)

use crate::graph::{GhostId, Status, TypedGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

// ── Rollup index κ ═══════════════════════════════════════════════════════

/// Rollup index κ = (delta_idx, op_idx), lexicographically ordered.
///
/// See Def. 5.4 in the paper. The larger κ-op wins on target overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Kappa {
    pub delta_idx: usize,
    pub op_idx: usize,
}

impl fmt::Display for Kappa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "κ({},{})", self.delta_idx, self.op_idx)
    }
}

// ── Atomic Operation ═════════════════════════════════════════════════════

/// Atomic operation on a graph.
///
/// See Def. 1.1 in the paper. Parent reference for AddNode per
/// Def. 5.3 (parent-rooted Ghost-ID).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    /// Adds a ghost node anchored at `parent`.
    AddNode {
        parent: GhostId,
        edge_type: String,
        type_id: String,
        attrs: BTreeMap<String, String>,
    },
    /// Adds a ghost edge.
    AddEdge {
        source: GhostId,
        target: GhostId,
        type_id: String,
        attrs: BTreeMap<String, String>,
    },
    /// Tombstones a node.
    DelNode { target: GhostId },
    /// Tombstones an edge.
    DelEdge { target: GhostId },
    /// Modifies an attribute (Phase 1: overwriting; Phase 3: shadow stack).
    SetAttr {
        target: GhostId,
        key: String,
        value: String,
    },
}

/// Target element of an Op (for rollup overlay).
///
/// Two Ops with the same target are overlaid under rollup
/// semantics (the later one wins).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpTarget {
    /// Node target (Add/Del on a node).
    Node(GhostId),
    /// Edge target (Add/Del on an edge).
    Edge(GhostId),
    /// Attribute target (SetAttr on (node, key)).
    Attr(GhostId, String),
}

impl Op {
    /// The target of this Op for rollup overlay (Def. 5.5 (i)).
    ///
    /// For AddNode the target is the new ghost, for AddEdge the new edge,
    /// for Del/SetAttr the referenced element.
    pub fn target(&self) -> OpTarget {
        match self {
            Op::AddNode {
                parent,
                edge_type,
                type_id,
                attrs,
            } => {
                let id = GhostId::from_parent(parent, edge_type, type_id, attrs);
                OpTarget::Node(id)
            }
            Op::AddEdge {
                source,
                target,
                type_id,
                attrs,
            } => {
                let id = GhostId::for_edge(source, target, type_id, attrs);
                OpTarget::Edge(id)
            }
            Op::DelNode { target } => OpTarget::Node(*target),
            Op::DelEdge { target } => OpTarget::Edge(*target),
            Op::SetAttr { target, key, .. } => OpTarget::Attr(*target, key.clone()),
        }
    }

    /// Checks whether this Op and `other` form a classic cancellation pair
    /// (Def. 5.5 (ii)): Add + Del on the same target.
    pub fn cancels_with(&self, other: &Op) -> bool {
        match (self, other) {
            (Op::AddNode { .. }, Op::DelNode { target: del_t })
            | (Op::DelNode { target: del_t }, Op::AddNode { .. }) => {
                matches!(self.target(), OpTarget::Node(id) if id == *del_t)
                    || matches!(other.target(), OpTarget::Node(id) if id == *del_t)
            }
            (Op::AddEdge { .. }, Op::DelEdge { target: del_t })
            | (Op::DelEdge { target: del_t }, Op::AddEdge { .. }) => {
                matches!(self.target(), OpTarget::Edge(id) if id == *del_t)
                    || matches!(other.target(), OpTarget::Edge(id) if id == *del_t)
            }
            _ => false,
        }
    }

    /// Applies this Op to `graph` (overlay operation ⊕, Def. 2.5).
    ///
    /// Returns: for Add-Ops the Ghost-ID of the new element; for
    /// Del/SetAttr `None`.
    pub fn apply(&self, graph: &mut TypedGraph) -> Result<Option<GhostId>, OpError> {
        match self {
            Op::AddNode {
                parent,
                edge_type,
                type_id,
                attrs,
            } => {
                if graph.get_node(parent).is_none() {
                    return Err(OpError::ParentNotFound(*parent));
                }
                let new_id = graph.add_ghost_node(*parent, edge_type, type_id, attrs.clone());
                // Additionally an edge from the parent to the new ghost:
                graph
                    .add_edge(*parent, new_id, edge_type, BTreeMap::new(), Status::Ghost)
                    .ok_or(OpError::EdgeCreationFailed)?;
                Ok(Some(new_id))
            }
            Op::AddEdge {
                source,
                target,
                type_id,
                attrs,
            } => {
                if graph.get_node(source).is_none() {
                    return Err(OpError::NodeNotFound(*source));
                }
                if graph.get_node(target).is_none() {
                    return Err(OpError::NodeNotFound(*target));
                }
                let id = graph
                    .add_edge(*source, *target, type_id, attrs.clone(), Status::Ghost)
                    .ok_or(OpError::EdgeCreationFailed)?;
                Ok(Some(id))
            }
            Op::DelNode { target } => {
                if !graph.set_node_status(target, Status::Tombstone) {
                    return Err(OpError::NodeNotFound(*target));
                }
                Ok(None)
            }
            Op::DelEdge { target } => {
                if !graph.set_edge_status(target, Status::Tombstone) {
                    return Err(OpError::EdgeNotFound(*target));
                }
                Ok(None)
            }
            Op::SetAttr { target, key, value } => {
                if !graph.set_node_attr(target, key, value) {
                    return Err(OpError::NodeNotFound(*target));
                }
                Ok(None)
            }
        }
    }
}

/// Error during Op application.
#[derive(Debug, Error)]
pub enum OpError {
    #[error("Parent node {0:?} not found")]
    ParentNotFound(GhostId),
    #[error("Node {0:?} not found")]
    NodeNotFound(GhostId),
    #[error("Edge {0:?} not found")]
    EdgeNotFound(GhostId),
    #[error("Edge creation failed")]
    EdgeCreationFailed,
    #[error("Not yet implemented: {0}")]
    NotYetImplemented(&'static str),
}

// ── Delta Entry ══════════════════════════════════════════════════════════

/// Origin of a delta entry.
///
/// See Def. 2.1 in the paper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    User,
    Rule { rule_id: String },
}

/// Delta entry with cascade annotation.
///
/// See Def. 2.1 (base) and Def. 3.9 (cascade annotation) in the paper.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeltaEntry {
    pub origin: Origin,
    pub rank: u64,
    /// Ordered Op list, |op_star| ≤ k.
    pub op_star: Vec<Op>,
    /// Anchor nodes (referenced elements).
    pub anchor: Vec<GhostId>,
    /// Induces relation from V₁₂.
    /// `induces[i]` = list of indices of Ops induced by `op_star[i]`.
    pub induces: Vec<Vec<usize>>,
    /// Match bindings of this rule application (M5).
    /// Empty for `Origin::User`. For `Origin::Rule` it contains the
    /// pattern-variable → GhostId bindings produced during the match.
    /// Used by the watch hook (M5.2) to find affected rule applications.
    #[serde(default)]
    pub bindings: std::collections::BTreeMap<String, GhostId>,
}

impl DeltaEntry {
    pub fn new_user(op_star: Vec<Op>, anchor: Vec<GhostId>) -> Self {
        let len = op_star.len();
        Self {
            origin: Origin::User,
            rank: 0,
            op_star,
            anchor,
            induces: vec![Vec::new(); len],
            bindings: std::collections::BTreeMap::new(),
        }
    }

    /// Applies all Ops in `op_star` sequentially to `graph`.
    pub fn apply(&self, graph: &mut TypedGraph) -> Result<Vec<Option<GhostId>>, OpError> {
        self.op_star.iter().map(|op| op.apply(graph)).collect()
    }
}

// ══ Tests ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn setup_person_graph() -> (TypedGraph, GhostId) {
        let mut g = TypedGraph::new();
        let person = g.add_baseline_node("Class", "Person", attrs(&[("name", "Person")]));
        (g, person)
    }

    #[test]
    fn add_node_creates_ghost() {
        let (mut g, person) = setup_person_graph();
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "age"), ("type", "Integer")]),
        };
        let result = op.apply(&mut g).unwrap();
        let new_id = result.unwrap();

        let new_node = g.get_node(&new_id).unwrap();
        assert_eq!(new_node.type_id, "Attribute");
        assert_eq!(new_node.status, Status::Ghost);
        assert_eq!(g.node_count(), 2);
        assert_eq!(
            g.edge_count(),
            1,
            "AddNode implicitly creates an edge to the parent"
        );
    }

    #[test]
    fn add_node_fails_if_parent_missing() {
        let mut g = TypedGraph::new();
        let phantom = GhostId::from_baseline("Phantom");
        let op = Op::AddNode {
            parent: phantom,
            edge_type: "any".into(),
            type_id: "Any".into(),
            attrs: BTreeMap::new(),
        };
        let result = op.apply(&mut g);
        assert!(matches!(result, Err(OpError::ParentNotFound(_))));
    }

    #[test]
    fn del_node_tombstones() {
        let (mut g, person) = setup_person_graph();
        let op = Op::DelNode { target: person };
        op.apply(&mut g).unwrap();

        let node = g.get_node(&person).unwrap();
        assert_eq!(node.status, Status::Tombstone);
        assert_eq!(g.matchable_nodes().count(), 0);
        assert_eq!(g.node_count(), 1, "TOMB remains physical");
    }

    // ── Add/Remove edge — Op layer ───────────────────────────────────

    fn setup_two_classes() -> (TypedGraph, GhostId, GhostId) {
        let mut g = TypedGraph::new();
        let a = g.add_baseline_node("Class", "A", attrs(&[("name", "A")]));
        let b = g.add_baseline_node("Class", "B", attrs(&[("name", "B")]));
        (g, a, b)
    }

    fn add_edge_op(source: GhostId, target: GhostId) -> Op {
        Op::AddEdge {
            source,
            target,
            type_id: "link".into(),
            attrs: BTreeMap::new(),
        }
    }

    #[test]
    fn add_edge_connects_two_existing_nodes() {
        let (mut g, a, b) = setup_two_classes();
        let edge_id = add_edge_op(a, b).apply(&mut g).unwrap().unwrap();

        let edge = g.get_edge(&edge_id).unwrap();
        assert_eq!(edge.type_id, "link");
        assert_eq!(edge.status, Status::Ghost);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn add_edge_is_idempotent_when_edge_exists() {
        let (mut g, a, b) = setup_two_classes();
        let first = add_edge_op(a, b).apply(&mut g).unwrap().unwrap();
        let second = add_edge_op(a, b).apply(&mut g).unwrap().unwrap();

        assert_eq!(first, second, "same edge → same Ghost-ID");
        assert_eq!(g.edge_count(), 1, "re-apply creates no edge duplicate");
    }

    #[test]
    fn add_edge_fails_when_endpoint_missing() {
        let (mut g, a, _) = setup_two_classes();
        let phantom = GhostId::from_baseline("Phantom");
        let result = add_edge_op(a, phantom).apply(&mut g);
        assert!(matches!(result, Err(OpError::NodeNotFound(_))));
    }

    #[test]
    fn del_edge_tombstones_the_edge() {
        let (mut g, a, b) = setup_two_classes();
        let edge_id = add_edge_op(a, b).apply(&mut g).unwrap().unwrap();

        Op::DelEdge { target: edge_id }.apply(&mut g).unwrap();

        assert_eq!(g.get_edge(&edge_id).unwrap().status, Status::Tombstone);
        assert_eq!(g.edge_count(), 1, "TOMB remains physical");
    }

    #[test]
    fn del_edge_fails_when_edge_missing() {
        let (mut g, ..) = setup_two_classes();
        let phantom = GhostId::from_baseline("PhantomEdge");
        let result = Op::DelEdge { target: phantom }.apply(&mut g);
        assert!(matches!(result, Err(OpError::EdgeNotFound(_))));
    }

    #[test]
    fn target_equality_identifies_same_element() {
        let parent = GhostId::from_baseline("X");
        let op1 = Op::AddNode {
            parent,
            edge_type: "e".into(),
            type_id: "T".into(),
            attrs: attrs(&[("k", "v")]),
        };
        let op2 = Op::AddNode {
            parent,
            edge_type: "e".into(),
            type_id: "T".into(),
            attrs: attrs(&[("k", "v")]),
        };
        assert_eq!(op1.target(), op2.target(), "same input → same target");
    }

    #[test]
    fn add_del_cancellation_pair() {
        let parent = GhostId::from_baseline("X");
        let add = Op::AddNode {
            parent,
            edge_type: "e".into(),
            type_id: "T".into(),
            attrs: BTreeMap::new(),
        };
        // The generated Ghost-ID:
        let OpTarget::Node(id) = add.target() else {
            panic!("Node target expected");
        };
        let del = Op::DelNode { target: id };

        assert!(add.cancels_with(&del));
        assert!(del.cancels_with(&add));
    }

    #[test]
    fn kappa_ordering() {
        let a = Kappa {
            delta_idx: 1,
            op_idx: 5,
        };
        let b = Kappa {
            delta_idx: 2,
            op_idx: 0,
        };
        let c = Kappa {
            delta_idx: 1,
            op_idx: 7,
        };

        assert!(a < b, "delta_idx dominates op_idx");
        assert!(a < c, "same delta_idx → op_idx decides");
        assert!(c < b, "lexicographic");
    }

    #[test]
    fn delta_entry_applies_op_sequence() {
        let (mut g, person) = setup_person_graph();
        let delta = DeltaEntry::new_user(
            vec![
                Op::AddNode {
                    parent: person,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "age")]),
                },
                Op::AddNode {
                    parent: person,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "email")]),
                },
            ],
            vec![person],
        );

        let results = delta.apply(&mut g).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(g.node_count(), 3, "Person + 2 attributes");
        assert_eq!(g.edge_count(), 2, "2 hasAttribute edges");
    }

    #[test]
    fn del_edge_tombstones() {
        let (mut g, person) = setup_person_graph();
        let add_attr = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "age")]),
        };
        let attr_id = add_attr.apply(&mut g).unwrap().unwrap();

        // The parent edge created by AddNode has a computed ID.
        let edge_id = GhostId::for_edge(&person, &attr_id, "hasAttribute", &BTreeMap::new());

        let del = Op::DelEdge { target: edge_id };
        del.apply(&mut g).unwrap();

        let edge = g.get_edge(&edge_id).unwrap();
        assert_eq!(edge.status, Status::Tombstone);
    }

    #[test]
    fn set_attr_updates_value() {
        let (mut g, person) = setup_person_graph();
        let set = Op::SetAttr {
            target: person,
            key: "package".into(),
            value: "com.example".into(),
        };
        set.apply(&mut g).unwrap();

        let node = g.get_node(&person).unwrap();
        assert_eq!(node.attrs.get("package"), Some(&"com.example".to_string()));
    }

    #[test]
    fn set_attr_fails_on_missing_node() {
        let mut g = TypedGraph::new();
        let phantom = GhostId::from_baseline("Phantom");
        let set = Op::SetAttr {
            target: phantom,
            key: "x".into(),
            value: "y".into(),
        };
        assert!(matches!(set.apply(&mut g), Err(OpError::NodeNotFound(_))));
    }

    /// Reconciliation scenario: an attribute is added and then removed again.
    /// After the cascade the attribute is tombstoned; the rollup/cancellation
    /// detection in phase 1.3 will then identify the pair.
    #[test]
    fn add_then_del_results_in_tombstone() {
        let (mut g, person) = setup_person_graph();
        let add_op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "transient")]),
        };
        let new_id = add_op.apply(&mut g).unwrap().unwrap();
        assert_eq!(g.get_node(&new_id).unwrap().status, Status::Ghost);

        let del_op = Op::DelNode { target: new_id };
        del_op.apply(&mut g).unwrap();
        assert_eq!(g.get_node(&new_id).unwrap().status, Status::Tombstone);

        // The cancellation pair can be detected:
        assert!(add_op.cancels_with(&del_op));
    }
}
