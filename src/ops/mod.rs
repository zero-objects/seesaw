//! Ops-Modul — T₁, T₂, T₃.
//!
//! Zuständigkeit:
//! - Edit-Skripte (Def. 1.1, 1.2)
//! - Delta-Einträge mit Kaskaden-Annotation (Def. 2.1, Def. 3.9)
//! - Overlay-Anwendung auf TypedGraph (Def. 2.5)
//! - Rollup-Index κ (Def. 5.4)
//! - Nullifikations-Prädikat (Def. 5.5)

use crate::graph::{GhostId, Status, TypedGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

// ── Rollup-Index κ ═══════════════════════════════════════════════════════

/// Rollup-Index κ = (delta_idx, op_idx), lexikographisch geordnet.
///
/// Siehe Def. 5.4 im PDF. Die größere κ-Op gewinnt bei Target-Überlagerung.
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

/// Atomare Operation auf einem Graphen.
///
/// Siehe Def. 1.1 im PDF. Parent-Referenz für AddNode gemäß
/// Def. 5.3 (Parent-rooted Ghost-ID).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    /// Fügt einen Ghost-Knoten anchored an `parent` hinzu.
    AddNode {
        parent: GhostId,
        edge_type: String,
        type_id: String,
        attrs: BTreeMap<String, String>,
    },
    /// Fügt eine Ghost-Kante hinzu.
    AddEdge {
        source: GhostId,
        target: GhostId,
        type_id: String,
        attrs: BTreeMap<String, String>,
    },
    /// Tombstoned einen Knoten.
    DelNode { target: GhostId },
    /// Tombstoned eine Kante.
    DelEdge { target: GhostId },
    /// Modifiziert ein Attribut (Phase 1: überschreibend; Phase 3: Shadow-Stack).
    SetAttr {
        target: GhostId,
        key: String,
        value: String,
    },
}

/// Zielelement einer Op (für Rollup-Überlagerung).
///
/// Zwei Ops mit gleichem Target werden unter Rollup-Semantik
/// überlagert (der spätere gewinnt).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpTarget {
    /// Knoten-Ziel (Add/Del auf einem Knoten).
    Node(GhostId),
    /// Kanten-Ziel (Add/Del auf einer Kante).
    Edge(GhostId),
    /// Attribut-Ziel (SetAttr auf (node, key)).
    Attr(GhostId, String),
}

impl Op {
    /// Das Target dieser Op für Rollup-Überlagerung (Def. 5.5 (i)).
    ///
    /// Für AddNode ist das Target der neue Ghost, für AddEdge die neue Kante,
    /// für Del/SetAttr das referenzierte Element.
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

    /// Prüft, ob diese Op und `other` ein klassisches Kanzellationspaar bilden
    /// (Def. 5.5 (ii)): Add + Del auf demselben Target.
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

    /// Wendet diese Op auf `graph` an (Overlay-Operation ⊕, Def. 2.5).
    ///
    /// Rückgabe: bei Add-Ops die Ghost-ID des neuen Elements; bei
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
                // Zusätzlich eine Kante vom Parent zum neuen Ghost:
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

/// Fehler bei Op-Anwendung.
#[derive(Debug, Error)]
pub enum OpError {
    #[error("Parent-Knoten {0:?} nicht gefunden")]
    ParentNotFound(GhostId),
    #[error("Knoten {0:?} nicht gefunden")]
    NodeNotFound(GhostId),
    #[error("Kante {0:?} nicht gefunden")]
    EdgeNotFound(GhostId),
    #[error("Kanten-Erzeugung fehlgeschlagen")]
    EdgeCreationFailed,
    #[error("Noch nicht implementiert: {0}")]
    NotYetImplemented(&'static str),
}

// ── Delta Entry ══════════════════════════════════════════════════════════

/// Herkunft eines Delta-Eintrags.
///
/// Siehe Def. 2.1 im PDF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    User,
    Rule { rule_id: String },
}

/// Delta-Eintrag mit Kaskaden-Annotation.
///
/// Siehe Def. 2.1 (Basis) und Def. 3.9 (Kaskaden-Annotation) im PDF.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeltaEntry {
    pub origin: Origin,
    pub rank: u64,
    /// Geordnete Op-Liste, |op_star| ≤ k.
    pub op_star: Vec<Op>,
    /// Anchor-Knoten (referenzierte Elemente).
    pub anchor: Vec<GhostId>,
    /// Induziert-Relation aus V₁₂.
    /// `induces[i]` = Liste der Indizes der durch `op_star[i]` induzierten Ops.
    pub induces: Vec<Vec<usize>>,
    /// Match-Bindings dieser Rule-Anwendung (M5).
    /// Leer für `Origin::User`. Für `Origin::Rule` enthält es die
    /// Pattern-Variable → GhostId-Bindings, die beim Match
    /// produziert wurden. Wird vom Watch-Hook (M5.2) verwendet,
    /// um affected Rule-Anwendungen zu finden.
    #[serde(default)]
    pub bindings: std::collections::HashMap<String, GhostId>,
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
            bindings: std::collections::HashMap::new(),
        }
    }

    /// Wendet alle Ops in `op_star` nacheinander auf `graph` an.
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
            "AddNode erzeugt implizit Kante zum Parent"
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
        assert_eq!(g.node_count(), 1, "TOMB bleibt physisch");
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
        assert_eq!(
            op1.target(),
            op2.target(),
            "gleiche Eingabe → gleiches Target"
        );
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
        // Die erzeugte Ghost-ID:
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

        assert!(a < b, "delta_idx dominiert op_idx");
        assert!(a < c, "gleicher delta_idx → op_idx entscheidet");
        assert!(c < b, "lexikographisch");
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
        assert_eq!(g.node_count(), 3, "Person + 2 Attribute");
        assert_eq!(g.edge_count(), 2, "2 hasAttribute-Kanten");
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

        // Die durch AddNode erzeugte Parent-Kante hat eine berechnete ID.
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

    /// Reconciliation-Szenario: Attribut wird hinzugefügt und wieder entfernt.
    /// Nach der Kaskade ist das Attribut tombstoned; die Rollup-/Kanzellations-
    /// Detektion in Phase 1.3 wird dann das Paar erkennen.
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

        // Das Kanzellationspaar lässt sich erkennen:
        assert!(add_op.cancels_with(&del_op));
    }
}
