//! The creation plan: what a lowered rule is, and how it is applied.
//!
//! A [`DirectedRule`] carries a match pattern and what to create when
//! it matches — nodes with their identity parent, links, the input
//! types for delta routing, and the correspondence recognition. There
//! are no dedicated corr constructs: a corr is a created node plus two
//! connections, and the provenance chain of a creation is
//! anchor → corr → created element (spec §1.3).
//!
//! [`crate::rules::load`] produces two of these per rule, forward and
//! backward. [`apply_creation`] executes one against a match.

use crate::ident::{GhostId, Status};

use crate::engine::matcher::{Bindings, Pattern};
use crate::graph::{Graph, PlanTransform};

// ── Spec (JSON) ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError(pub String);

// ── Directed operationalization ─────────────────────────────────────────────

/// Target of a creation: an existing match position or a node newly
/// created by this plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ref {
    Matched(usize),
    New(usize),
}

/// One node to be created.
#[derive(Debug, Clone)]
pub struct CreateNode {
    pub typ: String,
    /// Identity parent (spec §1.4: id = H(parent, typ [, source, transform])).
    pub parent: Ref,
    /// Ghost leaf: the source is a leaf of the INPUT side. The
    /// transformation is a chain in the plan (see [`PlanTransform`]).
    pub derived: Option<(usize, PlanTransform)>,
    /// Rule constant (§3b.4): the value lives in the rule; identity via
    /// (rule name, plan position), never via the value.
    pub konst: Option<String>,
    /// Dynamic binding (apply-if-present): (anchor pattern position,
    /// source leaf type, transform). The source is looked up at the
    /// anchor when applying; if it's missing, only THIS leaf is
    /// skipped.
    pub derived_dyn: Option<(usize, String, PlanTransform)>,
    /// Corr node: identity = anchor + type + match digest (§1.4).
    pub corr_full_match: bool,
}

/// Directed rule: pattern (input side + context corrs) and creation
/// plan (output side + corr + provenance connections).
#[derive(Debug, Clone)]
pub struct DirectedRule {
    pub name: String,
    pub rank: u64,
    pub pattern: Pattern,
    pub create_nodes: Vec<CreateNode>,
    pub create_links: Vec<(Ref, Ref)>,
    /// Type names of the input side (Δ routing: a rule is active when
    /// the delta touches one of these types).
    pub input_types: Vec<String>,
    /// rc8-#2 recognition: (corr type, input anchor position,
    /// endpoint type) of the establishes corr — if the anchor already
    /// has a corr of this type WITH a counterpart of the expected type
    /// (direction-free, provenance is data), the element is already
    /// translated ⇒ a duplicate, even across directions/variants. The
    /// endpoint type disambiguates rule sets with only ONE corr type
    /// (dry-cleaner: everything is tgg:refines). ALL establishes corrs
    /// must be present — otherwise a shared anchor (e.g. collection
    /// stitching) chokes off the follow-up matches.
    pub corr_recognition: Vec<(String, usize, String)>,
}

/// Applies a directed rule's creation plan to a match (idempotent via
/// the structural identity — reapplication produces exactly the same
/// ids; the engine's duplicate detection relies on this). Returns the
/// created node ids.
/// Creates the nodes and edges of the rule plan. Return value:
/// (created nodes, created edges) — kept separate because retraction
/// treats them differently (nodes have provenance children, edges
/// don't; different status setters).
pub fn apply_creation(
    g: &mut Graph,
    rule: &DirectedRule,
    bindings: &Bindings,
) -> (Vec<GhostId>, Vec<GhostId>) {
    // Plan-indexed; None = a dynamic leaf without a source (dropped).
    let mut slots: Vec<Option<GhostId>> = Vec::with_capacity(rule.create_nodes.len());
    for (plan_ix, cn) in rule.create_nodes.iter().enumerate() {
        let parent = match cn.parent {
            Ref::Matched(p) => bindings[p],
            Ref::New(ix) => match slots[ix] {
                Some(id) => id,
                None => {
                    slots.push(None);
                    continue;
                }
            },
        };
        let id = if cn.corr_full_match {
            let anchor = match cn.parent {
                Ref::Matched(p) => bindings[p],
                Ref::New(_) => unreachable!("a corr anchor is always matched"),
            };
            Some(g.add_corr(&anchor, &cn.typ, bindings))
        } else if let Some((anchor, ref attr, ref t)) = cn.derived_dyn {
            g.child_leaf_of_type(&bindings[anchor], attr)
                .map(|src| g.add_derived_leaf(&parent, &cn.typ, src, t))
        } else {
            Some(match (&cn.konst, &cn.derived) {
                (Some(v), _) => g.add_konst_leaf(&parent, &cn.typ, &rule.name, plan_ix as u32, v),
                (None, Some((in_leaf, t))) => {
                    g.add_derived_leaf(&parent, &cn.typ, bindings[*in_leaf], t)
                }
                (None, None) => g.add_ghost(&parent, &cn.typ),
            })
        };
        slots.push(id);
    }
    let mut edges = Vec::new();
    for &(a, b) in &rule.create_links {
        let s = match a {
            Ref::Matched(p) => Some(bindings[p]),
            Ref::New(ix) => slots[ix],
        };
        let t = match b {
            Ref::Matched(p) => Some(bindings[p]),
            Ref::New(ix) => slots[ix],
        };
        if let (Some(s), Some(t)) = (s, t) {
            // CAPTURE the edge id (don't discard it) — retraction must
            // retract created edges along with the node, otherwise an
            // id-stable node reclaim leaves a stale edge behind. ONLY
            // freshly created edges (`fresh`): a reused one already
            // belongs to its creator's `created_edges` (otherwise a
            // double TT). `connect_reporting` supplies the flag
            // without a second edge hash.
            if let Some((eid, fresh)) = g.connect_reporting(s, t, Status::Ghost) {
                if fresh {
                    edges.push(eid);
                }
            }
        }
    }
    (slots.into_iter().flatten().collect(), edges)
}

// ══ Tests (stage 3) ═══════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::matcher::find_matches;
    use crate::engine::{Engine, Termination};
    use crate::graph::{Graph, ValueStore};
    use crate::ident::Status;
    use crate::rules::format::RuleFile;

    /// Regeln ueber den einen Ladeweg: Vorwaerts- und Rueckwaertsplan
    /// je Regel, in Deklarationsreihenfolge.
    fn load(rules: serde_json::Value, g: &mut Graph) -> Vec<DirectedRule> {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3, "name": "plan_tests", "rules": rules,
        }))
        .expect("Regeldatei parst");
        crate::rules::load_file(&file, g).expect("Regeldatei laedt")
    }

    fn father_rule() -> serde_json::Value {
        serde_json::json!({
            "name": "Father_2_Male", "rank": 850,
            "left": {
                "anchor": "fam",
                "nodes": [
                    {"name": "fam", "type": "Family"},
                    {"name": "father", "type": "Father"},
                    {"name": "member", "type": "Member"},
                    {"name": "first", "type": "firstName"}
                ],
                "links": [["fam", "father"], ["father", "member"], ["member", "first"]]
            },
            "right": {
                "anchor": "male",
                "nodes": [
                    {"name": "male", "type": "Male"},
                    {"name": "name", "type": "name"}
                ],
                "links": [["male", "name"]]
            },
            "corrs": [
                {"type": "PersonCorr", "left": "member", "right": "male",
                 "role": "establishes",
                 "bindings": [{"left": "first", "right": "name"}]}
            ]
        })
    }

    fn seed(n: usize) -> (Graph, ValueStore) {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        for i in 0..n {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            let r = g.add_baseline(&format!("f{i}/father"), "Father");
            let m = g.add_baseline(&format!("f{i}/father/m"), "Member");
            let leaf = g.add_baseline(&format!("f{i}/father/m/fn"), "firstName");
            g.connect(f, r, Status::Solid);
            g.connect(r, m, Status::Solid);
            g.connect(m, leaf, Status::Solid);
            vs.insert(leaf, format!("John{i}"));
        }
        (g, vs)
    }

    #[test]
    fn forward_application_builds_the_provenance_chain() {
        let (mut g, vs) = seed(3);
        let fwd = load(serde_json::json!([father_rule()]), &mut g).remove(0);
        assert_eq!(fwd.pattern.nodes.len(), 4, "linke Seite, kein Kontext-Corr");
        let matches = find_matches(&g, &vs, &fwd.pattern);
        assert_eq!(matches.len(), 3);

        let (created, _edges) = apply_creation(&mut g, &fwd, &matches[0]);
        // Corr + Male + Namensblatt.
        assert_eq!(created.len(), 3);
        let corr = created[0];
        let male = created[1];
        let name_leaf = created[2];
        // Provenienzkette Member -> Corr -> Male.
        assert!(g.connected(&matches[0][2], &corr));
        assert!(g.connected(&corr, &male));
        // Abgeleitetes Blatt: Wert nirgends gespeichert, ueber den
        // Resolver aufgeloest.
        assert_eq!(
            g.resolve_value(&name_leaf, &vs),
            g.resolve_value(&matches[0][3], &vs)
        );
    }

    #[test]
    fn reapplication_is_idempotent() {
        let (mut g, vs) = seed(1);
        let fwd = load(serde_json::json!([father_rule()]), &mut g).remove(0);
        let m = find_matches(&g, &vs, &fwd.pattern);
        let (c1, e1) = apply_creation(&mut g, &fwd, &m[0]);
        let n_after_first = g.node_count();
        let (c2, e2) = apply_creation(&mut g, &fwd, &m[0]);
        assert_eq!(
            c1, c2,
            "strukturelle Identitaet ergibt dieselben Knoten-Ids"
        );
        // Kanten entstehen nur beim ersten Mal (frisch-Semantik von
        // `created_edges`, kein Doppeleintrag).
        assert!(
            !e1.is_empty() && e2.is_empty(),
            "Kanten nur bei der ersten Erzeugung"
        );
        assert_eq!(g.node_count(), n_after_first, "keine Duplikate");
    }

    /// Regel-Konstante: zwei Regeln erzeugen am selben Anker
    /// gleichgetypte Knoten, die sich NUR in der Konstanten
    /// unterscheiden. Beide muessen feuern (Identitaet ueber die
    /// erzeugende Regel), der Wert muss aufloesen, und im ValueStore
    /// darf nichts landen.
    fn variant_rule(name: &str, rank: u64, strategy: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name, "rank": rank,
            "left": {
                "anchor": "sel",
                "nodes": [{"name": "sel", "type": "SelectStatement"}]
            },
            "right": {
                "anchor": "ann",
                "nodes": [
                    {"name": "ann", "type": "Annotation"},
                    {"name": "strat", "type": "strategy",
                     "predicate": {"kind": "equals", "value": strategy},
                     "constant": strategy}
                ],
                "links": [["ann", "strat"]]
            },
            "corrs": [{"type": format!("NumCorr_{strategy}"), "left": "sel",
                       "right": "ann", "role": "establishes"}]
        })
    }

    #[test]
    fn creation_constant_variants_do_not_collide() {
        let mut g = Graph::new();
        let vs = ValueStore::default();
        g.add_baseline("sel1", "SelectStatement");
        let rules = load(
            serde_json::json!([
                variant_rule("Num_Sequential", 900, "sequential"),
                variant_rule("Num_Hierarchical", 890, "hierarchical"),
            ]),
            &mut g,
        );
        let mut e = Engine::new(&rules);
        assert_eq!(e.run(&mut g, &vs, 100), Termination::Duplication);
        assert_eq!(e.cascade.len(), 2, "beide Varianten muessen feuern");
        // Der Wert loest aus der Regel auf, der ValueStore bleibt leer.
        let st = g.types.lookup("strategy").unwrap();
        let mut values: Vec<String> = g
            .nodes_of_type(st)
            .filter_map(|n| g.resolve_value(&n.id, &vs))
            .collect();
        values.sort();
        assert_eq!(values, ["hierarchical", "sequential"]);
        // Idempotenz: ein frischer Lauf erzeugt nichts Neues.
        let mut e2 = Engine::new(&rules);
        assert_eq!(e2.run(&mut g, &vs, 100), Termination::Duplication);
        assert_eq!(e2.cascade.len(), 0);
    }

    #[test]
    fn constant_matches_against_the_constant_leaf() {
        // Eine nachgelagerte Regel matcht per Gleichheit gegen das
        // Konstanten-Blatt.
        let mut g = Graph::new();
        let vs = ValueStore::default();
        g.add_baseline("sel1", "SelectStatement");
        let follow = serde_json::json!({
            "name": "Seq_Follow", "rank": 800,
            "left": {
                "anchor": "ann",
                "nodes": [
                    {"name": "ann", "type": "Annotation"},
                    {"name": "strat", "type": "strategy",
                     "predicate": {"kind": "equals", "value": "sequential"},
                     "constant": "sequential"}
                ],
                "links": [["ann", "strat"]]
            },
            "right": {
                "anchor": "marker",
                "nodes": [{"name": "marker", "type": "SeqMarker"}]
            },
            "corrs": [{"type": "SeqCorr", "left": "ann", "right": "marker",
                       "role": "establishes"}]
        });
        let rules = load(
            serde_json::json!([variant_rule("Num_Seq", 900, "sequential"), follow]),
            &mut g,
        );
        let mut e = Engine::new(&rules);
        e.run(&mut g, &vs, 100);
        let marker = g.types.lookup("SeqMarker").unwrap();
        assert_eq!(
            g.nodes_of_type(marker).count(),
            1,
            "Gleichheit gegen das Konstanten-Blatt greift"
        );
    }
}
