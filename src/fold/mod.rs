//! Fold module — T₅, T₇.
//!
//! Responsibilities:
//! - Nullification detection (Def. 5.5)
//! - Consolidation via fixpoint iteration (Def. 5.8)
//! - Materialization to a new baseline (Def. 5.1, 5.11)
//! - Net-delta computation (Def. 5.12)
//! - Transition markers and transition graph (Def. 5.9, 5.10)

use crate::engine::Cascade;
use crate::graph::{EdgeData, GhostId, NodeData, Status, TypedGraph};
use crate::ops::{Op, OpError, OpTarget};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

// ══ Nullification ════════════════════════════════════════════════════════

/// Position of an Op in the cascade.
pub type OpPos = (usize, usize);

/// Collects all nullified Op positions via fixpoint iteration of the
/// three nullification clauses from Def. 5.5:
/// (i) rollup overlay, (ii) Add-Del pair, (iii) V₁₂-induced.
pub fn compute_nullifications(cascade: &Cascade) -> HashSet<OpPos> {
    let mut nullified: HashSet<OpPos> = HashSet::new();

    loop {
        let before = nullified.len();

        pass_rollup_and_cancellation(cascade, &mut nullified);
        pass_induces_propagation(cascade, &mut nullified);

        if nullified.len() == before {
            break;
        }
    }

    nullified
}

/// Unified rollup + cancellation pass.
///
/// Groups non-nullified Ops by their `OpTarget`. For each group:
/// - If it contains both Add and Del (Def. 5.5 (ii)): both are
///   nullified.
/// - Otherwise (pure rollup overlay, Def. 5.5 (i)): all except the
///   Op with the largest κ are nullified.
fn pass_rollup_and_cancellation(cascade: &Cascade, nullified: &mut HashSet<OpPos>) {
    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Kind {
        Add,
        Del,
        Set,
    }

    let mut by_target: HashMap<OpTarget, Vec<(OpPos, Kind)>> = HashMap::new();

    for (d_idx, entry) in cascade.entries.iter().enumerate() {
        for (o_idx, op) in entry.op_star.iter().enumerate() {
            if nullified.contains(&(d_idx, o_idx)) {
                continue;
            }
            let kind = match op {
                Op::AddNode { .. } | Op::AddEdge { .. } => Kind::Add,
                Op::DelNode { .. } | Op::DelEdge { .. } => Kind::Del,
                Op::SetAttr { .. } => Kind::Set,
            };
            by_target
                .entry(op.target())
                .or_default()
                .push(((d_idx, o_idx), kind));
        }
    }

    for (_target, positions) in by_target.into_iter() {
        let has_add = positions.iter().any(|(_, k)| *k == Kind::Add);
        let has_del = positions.iter().any(|(_, k)| *k == Kind::Del);

        if has_add && has_del {
            // Cancellation pair: all Add and Del ops on this target
            // are nullified (Def. 5.5 (ii)).
            for (pos, kind) in &positions {
                if matches!(kind, Kind::Add | Kind::Del) {
                    nullified.insert(*pos);
                }
            }
        } else if positions.len() > 1 {
            // Pure rollup overlay: all except max-κ (Def. 5.5 (i)).
            let max_pos = positions.iter().map(|(p, _)| *p).max().unwrap();
            for (pos, _) in &positions {
                if *pos != max_pos {
                    nullified.insert(*pos);
                }
            }
        }
    }
}

/// V₁₂-induced cancellation (Def. 5.5 (iii)): when an Op is nullified,
/// all follow-up Ops listed in its `induces` field are also
/// nullified.
fn pass_induces_propagation(cascade: &Cascade, nullified: &mut HashSet<OpPos>) {
    let to_propagate: Vec<OpPos> = nullified.iter().copied().collect();
    for (d_idx, o_idx) in to_propagate {
        if let Some(entry) = cascade.entries.get(d_idx) {
            if let Some(induced_list) = entry.induces.get(o_idx) {
                for &child in induced_list {
                    nullified.insert((d_idx, child));
                }
            }
        }
    }
}

// ══ Consolidation ════════════════════════════════════════════════════════

/// Result of consolidation.
#[derive(Debug)]
pub struct Consolidated {
    pub nullified: HashSet<OpPos>,
    pub new_baseline: TypedGraph,
    /// Statistic: how many Ops were eliminated?
    pub eliminated_count: usize,
    /// Empty delta entries (all Ops nullified — null-edge elimination).
    pub empty_deltas: Vec<usize>,
}

/// Runs the full consolidation (Def. 5.8):
/// 1. Compute nullifications via fixpoint.
/// 2. Build the new baseline graph by applying all non-nullified
///    Ops to `base` + materialization.
/// 3. Identify support-less delta entries (Def. 5.6).
pub fn consolidate(base: &TypedGraph, cascade: &Cascade) -> Result<Consolidated, OpError> {
    let nullified = compute_nullifications(cascade);

    let mut working = base.clone();
    let mut eliminated_count = 0;
    let mut empty_deltas = Vec::new();

    for (d_idx, entry) in cascade.entries.iter().enumerate() {
        let mut op_applied = 0;
        for (o_idx, op) in entry.op_star.iter().enumerate() {
            if nullified.contains(&(d_idx, o_idx)) {
                eliminated_count += 1;
                continue;
            }
            op.apply(&mut working)?;
            op_applied += 1;
        }
        if op_applied == 0 && !entry.op_star.is_empty() {
            empty_deltas.push(d_idx);
        }
    }

    let new_baseline = working.materialize();

    Ok(Consolidated {
        nullified,
        new_baseline,
        eliminated_count,
        empty_deltas,
    })
}

// ══ Net delta ════════════════════════════════════════════════════════════

/// Net delta between two baselines (Def. 5.12, observer output).
#[derive(Clone, Debug, Default)]
pub struct NetDelta {
    pub added_nodes: Vec<NodeData>,
    pub removed_node_ids: Vec<GhostId>,
    pub added_edges: Vec<(GhostId, GhostId, EdgeData)>,
    pub removed_edge_ids: Vec<GhostId>,
    /// Nodes with changed attribute valuation: (id, new attrs).
    pub attr_changes: Vec<(GhostId, BTreeMap<String, String>)>,
}

impl NetDelta {
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_node_ids.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edge_ids.is_empty()
            && self.attr_changes.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "+N:{} -N:{} +E:{} -E:{} Δattr:{}",
            self.added_nodes.len(),
            self.removed_node_ids.len(),
            self.added_edges.len(),
            self.removed_edge_ids.len(),
            self.attr_changes.len(),
        )
    }
}

/// Computes the net delta between `before` and `after`.
///
/// Based on ID comparison: nodes/edges in `after` but not in
/// `before` → added; vice versa → removed; same ID, different
/// attributes → attr_change.
pub fn diff(before: &TypedGraph, after: &TypedGraph) -> NetDelta {
    let before_node_ids: HashMap<GhostId, &NodeData> = before
        .iter_nodes()
        .filter(|n| n.status != Status::Tombstone)
        .map(|n| (n.id, n))
        .collect();
    let after_node_ids: HashMap<GhostId, &NodeData> = after
        .iter_nodes()
        .filter(|n| n.status != Status::Tombstone)
        .map(|n| (n.id, n))
        .collect();

    let mut delta = NetDelta::default();

    for (id, node) in &after_node_ids {
        if !before_node_ids.contains_key(id) {
            delta.added_nodes.push((*node).clone());
        } else {
            let b = before_node_ids[id];
            if b.attrs != node.attrs {
                delta.attr_changes.push((*id, node.attrs.clone()));
            }
        }
    }
    for id in before_node_ids.keys() {
        if !after_node_ids.contains_key(id) {
            delta.removed_node_ids.push(*id);
        }
    }

    // Edges
    let before_edges: HashMap<GhostId, (GhostId, GhostId, &EdgeData)> = before
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.status != Status::Tombstone)
        .map(|(s, t, e)| (e.id, (s, t, e)))
        .collect();
    let after_edges: HashMap<GhostId, (GhostId, GhostId, &EdgeData)> = after
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.status != Status::Tombstone)
        .map(|(s, t, e)| (e.id, (s, t, e)))
        .collect();

    for (id, (s, t, e)) in &after_edges {
        if !before_edges.contains_key(id) {
            delta.added_edges.push((*s, *t, (*e).clone()));
        }
    }
    for id in before_edges.keys() {
        if !after_edges.contains_key(id) {
            delta.removed_edge_ids.push(*id);
        }
    }

    delta
}

// ══ Transition markers and transition graph ═════════════════════════════

/// Unique baseline identifier in the transition graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BaselineId(pub u64);

/// Transition marker between two baselines (Def. 5.9).
#[derive(Clone, Debug)]
pub struct TransitionMarker {
    pub from: BaselineId,
    pub to: BaselineId,
    pub rule_count: usize,
    pub eliminated_count: usize,
    pub net_delta_summary: String,
}

/// Transition graph T (Def. 5.10), minimal in-memory structure.
#[derive(Debug, Default)]
pub struct TransitionGraph {
    next_id: u64,
    pub baselines: HashMap<BaselineId, TypedGraph>,
    pub markers: Vec<TransitionMarker>,
}

impl TransitionGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new baseline and returns its BaselineId.
    pub fn register_baseline(&mut self, baseline: TypedGraph) -> BaselineId {
        let id = BaselineId(self.next_id);
        self.next_id += 1;
        self.baselines.insert(id, baseline);
        id
    }

    pub fn add_marker(&mut self, marker: TransitionMarker) {
        self.markers.push(marker);
    }

    pub fn baseline_count(&self) -> usize {
        self.baselines.len()
    }

    pub fn marker_count(&self) -> usize {
        self.markers.len()
    }
}

// ══ Tests ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{run_cascade, BasicRule, NodePattern, Pattern, Rule, TerminationState};
    use crate::ops::{DeltaEntry, Origin};
    use std::collections::BTreeMap;

    fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn setup_uml_graph() -> (TypedGraph, GhostId, GhostId) {
        let mut g = TypedGraph::new();
        let person = g.add_baseline_node("Class", "Person", attrs(&[("name", "Person")]));
        let car = g.add_baseline_node("Class", "Car", attrs(&[("name", "Car")]));
        (g, person, car)
    }

    // ── Cancellation via Add-Del pair ────────────────────────────────

    #[test]
    fn add_del_cancellation_nullifies_both() {
        let (g, person, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Synthetic cascade: d_0 adds an attribute, d_1 tombstones it.
        let add_op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "transient")]),
        };
        let ghost_id = match add_op.target() {
            OpTarget::Node(id) => id,
            _ => panic!(),
        };

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r_add".into(),
            },
            rank: 10,
            op_star: vec![add_op],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r_del".into(),
            },
            rank: 20,
            op_star: vec![Op::DelNode { target: ghost_id }],
            anchor: vec![ghost_id],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let nullified = compute_nullifications(&c);
        assert_eq!(nullified.len(), 2, "both Ops nullified");
        assert!(nullified.contains(&(0, 0)));
        assert!(nullified.contains(&(1, 0)));
        let _ = g;
    }

    // ── Rollup: last wins for SetAttr ────────────────────────────────

    #[test]
    fn rollup_setattr_keeps_only_last() {
        let (_, person, _) = setup_uml_graph();
        let mut c = Cascade::new();

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r1".into(),
            },
            rank: 10,
            op_star: vec![Op::SetAttr {
                target: person,
                key: "pkg".into(),
                value: "old".into(),
            }],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r2".into(),
            },
            rank: 5, // lower rank, but later emission.
            op_star: vec![Op::SetAttr {
                target: person,
                key: "pkg".into(),
                value: "new".into(),
            }],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let nullified = compute_nullifications(&c);
        // The earlier Set is nullified, the later one survives.
        assert!(nullified.contains(&(0, 0)));
        assert!(!nullified.contains(&(1, 0)));
    }

    // ── V₁₂-induced cancellation ─────────────────────────────────────

    #[test]
    fn induces_propagation() {
        let (_, person, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // d_0 has two Ops; op[0] induces op[1].
        let primary = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "x")]),
        };
        let primary_id = match primary.target() {
            OpTarget::Node(id) => id,
            _ => panic!(),
        };
        let induced = Op::AddEdge {
            source: person,
            target: primary_id,
            type_id: "extra".into(),
            attrs: BTreeMap::new(),
        };

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r".into(),
            },
            rank: 10,
            op_star: vec![primary, induced],
            anchor: vec![person],
            induces: vec![vec![1], Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        // Second delta entry tombstones the primary ghost → primary is
        // nullified via cancellation, its induced follow-up Op via V₁₂.
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "del".into(),
            },
            rank: 20,
            op_star: vec![Op::DelNode { target: primary_id }],
            anchor: vec![primary_id],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let nullified = compute_nullifications(&c);
        assert!(nullified.contains(&(0, 0)), "primary Op nullified");
        assert!(nullified.contains(&(0, 1)), "V₁₂-induced Op nullified");
        assert!(nullified.contains(&(1, 0)), "Del-Op also nullified");
    }

    // ── Consolidation + materialization ──────────────────────────────

    #[test]
    fn consolidate_produces_baseline() {
        let (mut g, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // A rule that produces a "derived" attribute per class.
        let rule = BasicRule::new(
            "AddDerived",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "derived")]),
                }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        // base snapshot before cascade:
        let base = g.clone();
        let state = run_cascade(&mut c, &mut g, &rules, 100).unwrap();
        assert!(matches!(
            state,
            TerminationState::Duplication | TerminationState::Convergence
        ));

        let result = consolidate(&base, &c).unwrap();
        // No Add-Del pair → no nullifications expected.
        assert_eq!(result.nullified.len(), 0);
        // The new baseline contains at least the two original
        // classes plus two "derived" attributes (4 nodes).
        assert!(result.new_baseline.node_count() >= 4);
        // All nodes in the baseline are SOLID.
        assert!(result
            .new_baseline
            .iter_nodes()
            .all(|n| n.status == Status::Solid));
    }

    #[test]
    fn consolidate_eliminates_add_del_pair() {
        let (mut g, person, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        // d_0: add
        let add_op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "transient")]),
        };
        let ghost_id = match add_op.target() {
            OpTarget::Node(id) => id,
            _ => panic!(),
        };
        add_op.apply(&mut g).unwrap();

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "add".into(),
            },
            rank: 10,
            op_star: vec![add_op],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        // d_1: del (tombstoned in the graph via apply)
        let del_op = Op::DelNode { target: ghost_id };
        del_op.apply(&mut g).unwrap();

        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "del".into(),
            },
            rank: 20,
            op_star: vec![del_op],
            anchor: vec![ghost_id],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let result = consolidate(&base, &c).unwrap();
        assert_eq!(result.nullified.len(), 2, "both Ops are removed");
        assert_eq!(result.eliminated_count, 2);
        assert_eq!(result.empty_deltas.len(), 2, "both deltas support-less");

        // Baseline = base (two classes, no extra attributes).
        assert_eq!(result.new_baseline.node_count(), 2);
    }

    // ── Net delta ────────────────────────────────────────────────────

    #[test]
    fn diff_captures_added_nodes() {
        let (mut g, _, _) = setup_uml_graph();
        let base = g.clone();
        g.add_baseline_node("Class", "Dog", attrs(&[("name", "Dog")]));
        let nd = diff(&base, &g);
        assert_eq!(nd.added_nodes.len(), 1);
        assert_eq!(nd.removed_node_ids.len(), 0);
    }

    #[test]
    fn diff_empty_when_unchanged() {
        let (g, _, _) = setup_uml_graph();
        let nd = diff(&g, &g);
        assert!(nd.is_empty());
    }

    #[test]
    fn diff_summary_format() {
        let nd = NetDelta {
            added_nodes: vec![],
            removed_node_ids: vec![],
            added_edges: vec![],
            removed_edge_ids: vec![],
            attr_changes: vec![],
        };
        assert_eq!(nd.summary(), "+N:0 -N:0 +E:0 -E:0 Δattr:0");
    }

    // ── Transition graph ─────────────────────────────────────────────

    #[test]
    fn transition_graph_tracks_baselines() {
        let (g, _, _) = setup_uml_graph();
        let mut t = TransitionGraph::new();
        let b0 = t.register_baseline(g.clone());
        let b1 = t.register_baseline(g.clone());
        assert_ne!(b0, b1);
        assert_eq!(t.baseline_count(), 2);
        assert_eq!(t.marker_count(), 0);

        t.add_marker(TransitionMarker {
            from: b0,
            to: b1,
            rule_count: 3,
            eliminated_count: 1,
            net_delta_summary: "+N:2 -N:0 +E:2 -E:0 Δattr:0".into(),
        });
        assert_eq!(t.marker_count(), 1);
    }

    // ── End-to-end ───────────────────────────────────────────────────

    #[test]
    fn e2e_cascade_consolidate_baseline() {
        let (mut g, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let rule = BasicRule::new(
            "AddGetter",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasMethod".into(),
                    type_id: "Method".into(),
                    attrs: attrs(&[("name", "getName"), ("returns", "String")]),
                }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let state = run_cascade(&mut c, &mut g, &rules, 100).unwrap();
        assert!(matches!(
            state,
            TerminationState::Duplication | TerminationState::Convergence
        ));
        assert_eq!(c.len(), 2, "two methods produced, one per class");

        let result = consolidate(&base, &c).unwrap();
        let net = diff(&base, &result.new_baseline);

        assert_eq!(net.added_nodes.len(), 2, "2 Method nodes added");
        assert_eq!(net.added_edges.len(), 2, "2 hasMethod edges added");
        assert_eq!(net.removed_node_ids.len(), 0);

        // Minimal transition graph.
        let mut t = TransitionGraph::new();
        let b0 = t.register_baseline(base);
        let b1 = t.register_baseline(result.new_baseline);
        t.add_marker(TransitionMarker {
            from: b0,
            to: b1,
            rule_count: c.len(),
            eliminated_count: result.eliminated_count,
            net_delta_summary: net.summary(),
        });
        assert_eq!(t.baseline_count(), 2);
        assert_eq!(t.marker_count(), 1);
    }
}
