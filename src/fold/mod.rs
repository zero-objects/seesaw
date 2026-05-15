//! Fold-Modul — T₅, T₇.
//!
//! Zuständigkeit:
//! - Nullifikations-Detektion (Def. 5.5)
//! - Konsolidierung via Fixpunkt-Iteration (Def. 5.8)
//! - Materialisierung zur neuen Baseline (Def. 5.1, 5.11)
//! - Netto-Delta-Berechnung (Def. 5.12)
//! - Transitions-Marker und -Graph (Def. 5.9, 5.10)

use crate::engine::Cascade;
use crate::graph::{EdgeData, GhostId, NodeData, Status, TypedGraph};
use crate::ops::{Op, OpError, OpTarget};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

// ══ Nullifikation ════════════════════════════════════════════════════════

/// Position einer Op in der Kaskade.
pub type OpPos = (usize, usize);

/// Sammelt alle nullifizierten Op-Positionen via Fixpunkt-Iteration der
/// drei Nullifikations-Klauseln aus Def. 5.5:
/// (i) Rollup-Überlagerung, (ii) Add-Del-Paar, (iii) V₁₂-induziert.
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

/// Vereinheitlichter Rollup + Kanzellations-Pass.
///
/// Gruppiert nicht-nullifizierte Ops nach ihrem `OpTarget`. Für jede
/// Gruppe:
/// - Enthält sie sowohl Add als auch Del (Def. 5.5 (ii)): beide
///   nullifiziert.
/// - Sonst (reine Rollup-Überlagerung, Def. 5.5 (i)): alle außer der
///   Op mit größtem κ werden nullifiziert.
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
            // Kanzellationspaar: alle Add- und Del-Ops auf diesem Target
            // werden nullifiziert (Def. 5.5 (ii)).
            for (pos, kind) in &positions {
                if matches!(kind, Kind::Add | Kind::Del) {
                    nullified.insert(*pos);
                }
            }
        } else if positions.len() > 1 {
            // Reine Rollup-Überlagerung: alle außer max-κ (Def. 5.5 (i)).
            let max_pos = positions.iter().map(|(p, _)| *p).max().unwrap();
            for (pos, _) in &positions {
                if *pos != max_pos {
                    nullified.insert(*pos);
                }
            }
        }
    }
}

/// V₁₂-induzierte Kanzellation (Def. 5.5 (iii)): wenn eine Op nullifiziert
/// ist, werden auch alle in ihrem `induces`-Feld gelisteten Folge-Ops
/// nullifiziert.
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

// ══ Konsolidierung ═══════════════════════════════════════════════════════

/// Ergebnis der Konsolidierung.
#[derive(Debug)]
pub struct Consolidated {
    pub nullified: HashSet<OpPos>,
    pub new_baseline: TypedGraph,
    /// Statistik: wie viele Ops wurden eliminiert?
    pub eliminated_count: usize,
    /// Leere Delta-Einträge (alle Ops nullifiziert — Null-Kanten-Elimination).
    pub empty_deltas: Vec<usize>,
}

/// Führt die vollständige Konsolidierung durch (Def. 5.8):
/// 1. Berechne Nullifikationen via Fixpunkt.
/// 2. Baue den neuen Baseline-Graph durch Anwenden aller nicht-nullifizierter
///    Ops auf `base` + Materialisierung.
/// 3. Identifiziere trägerlose Delta-Einträge (Def. 5.6).
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

// ══ Netto-Delta ══════════════════════════════════════════════════════════

/// Netto-Delta zwischen zwei Baselines (Def. 5.12, Observer-Ausgabe).
#[derive(Clone, Debug, Default)]
pub struct NetDelta {
    pub added_nodes: Vec<NodeData>,
    pub removed_node_ids: Vec<GhostId>,
    pub added_edges: Vec<(GhostId, GhostId, EdgeData)>,
    pub removed_edge_ids: Vec<GhostId>,
    /// Knoten mit geänderter Attributbelegung: (id, new attrs).
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

/// Berechnet das Netto-Delta zwischen `before` und `after`.
///
/// Basiert auf ID-Vergleich: Knoten/Kanten in `after` aber nicht
/// `before` → added; umgekehrt → removed; gleiche ID, verschiedene
/// Attribute → attr_change.
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

    // Kanten
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

// ══ Transitions-Marker und -Graph ════════════════════════════════════════

/// Eindeutige Baseline-Kennung im Transitions-Graphen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BaselineId(pub u64);

/// Transitions-Marker zwischen zwei Baselines (Def. 5.9).
#[derive(Clone, Debug)]
pub struct TransitionMarker {
    pub from: BaselineId,
    pub to: BaselineId,
    pub rule_count: usize,
    pub eliminated_count: usize,
    pub net_delta_summary: String,
}

/// Transitions-Graph T (Def. 5.10), minimale In-Memory-Struktur.
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

    /// Registriert eine neue Baseline und liefert ihre BaselineId.
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

    // ── Kanzellation via Add-Del-Paar ────────────────────────────────

    #[test]
    fn add_del_cancellation_nullifies_both() {
        let (g, person, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Synthetische Kaskade: d_0 addet ein Attribut, d_1 tombstoned es.
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
        assert_eq!(nullified.len(), 2, "beide Ops nullifiziert");
        assert!(nullified.contains(&(0, 0)));
        assert!(nullified.contains(&(1, 0)));
        let _ = g;
    }

    // ── Rollup: letzter gewinnt bei SetAttr ──────────────────────────

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
            rank: 5, // niedrigerer Rang, aber spätere Emission.
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
        // Der frühere Set wird nullifiziert, der spätere überlebt.
        assert!(nullified.contains(&(0, 0)));
        assert!(!nullified.contains(&(1, 0)));
    }

    // ── V₁₂-Induzierte Kanzellation ──────────────────────────────────

    #[test]
    fn induces_propagation() {
        let (_, person, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // d_0 hat zwei Ops; op[0] induziert op[1].
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

        // Zweiter Delta-Eintrag tombstoned den Primär-Ghost → Primär wird
        // via Cancellation nullifiziert, sein induziertes Folge-Op per V₁₂.
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
        assert!(nullified.contains(&(0, 0)), "Primär-Op nullifiziert");
        assert!(
            nullified.contains(&(0, 1)),
            "V₁₂-induzierte Op nullifiziert"
        );
        assert!(nullified.contains(&(1, 0)), "Del-Op ebenfalls nullifiziert");
    }

    // ── Konsolidierung + Materialisierung ────────────────────────────

    #[test]
    fn consolidate_produces_baseline() {
        let (mut g, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Eine Regel, die pro Klasse ein Attribut "derived" erzeugt.
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

        // base-Snapshot vor Kaskade:
        let base = g.clone();
        let state = run_cascade(&mut c, &mut g, &rules, 100).unwrap();
        assert!(matches!(
            state,
            TerminationState::Duplication | TerminationState::Convergence
        ));

        let result = consolidate(&base, &c).unwrap();
        // Kein Add-Del-Paar → keine Nullifikationen erwartet.
        assert_eq!(result.nullified.len(), 0);
        // Die neue Baseline enthält mindestens die zwei ursprünglichen
        // Klassen plus zwei "derived"-Attribute (4 Knoten).
        assert!(result.new_baseline.node_count() >= 4);
        // Alle Knoten in der Baseline sind SOLID.
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

        // d_1: del (tombstoned im Graph via apply)
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
        assert_eq!(result.nullified.len(), 2, "beide Ops gehen weg");
        assert_eq!(result.eliminated_count, 2);
        assert_eq!(result.empty_deltas.len(), 2, "beide Deltas trägerlos");

        // Baseline = base (zwei Klassen, keine extra Attribute).
        assert_eq!(result.new_baseline.node_count(), 2);
    }

    // ── Netto-Delta ──────────────────────────────────────────────────

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

    // ── Transitions-Graph ────────────────────────────────────────────

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

    // ── End-zu-End ───────────────────────────────────────────────────

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
        assert_eq!(c.len(), 2, "zwei Methoden erzeugt, eine pro Klasse");

        let result = consolidate(&base, &c).unwrap();
        let net = diff(&base, &result.new_baseline);

        assert_eq!(net.added_nodes.len(), 2, "2 Method-Knoten hinzugefügt");
        assert_eq!(net.added_edges.len(), 2, "2 hasMethod-Kanten hinzugefügt");
        assert_eq!(net.removed_node_ids.len(), 0);

        // Transitions-Graph minimal.
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
