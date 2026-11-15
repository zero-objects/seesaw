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
use crate::graph::{Graph, PlanTransform, TypeId};

// The names a rule's plan refers to and what they resolve to in a
// graph: the types `Names` (one per engine), `RuleNames` (one rule of
// it, for a step) and `OwnNames` (one rule outside an engine). Two
// implementations of one interface: without `opt_g` every use resolves
// the name, with it the id is kept after the first use.
// Included, not modules of their own, like the stores of the graph.
#[cfg(not(feature = "opt_g"))]
include!("plan/names_plain.rs");
#[cfg(feature = "opt_g")]
include!("plan/names_kept.rs");

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
    /// Is this created node the correspondence of its rule?
    ///
    /// Set by lowering from the rule file's `corrs`. The engine follows
    /// it when deleting: a fallen correspondence takes the elements it
    /// connects with it.
    pub is_corr: bool,
    /// Typ der Attribut-Korrespondenz dieses Blattes, wenn es aus einem
    /// DYNAMISCHEN Constraint stammt (`left_type`/`right_type`).
    ///
    /// Im statischen Fall legt das Lowering die Blatt-Korrespondenz
    /// selbst an, weil dort beide Blaetter als Musterposition
    /// feststehen. Im dynamischen Fall wird die Quelle erst beim
    /// Anwenden ueber `child_leaf_of_type` gefunden, also muss die
    /// Korrespondenz dort entstehen -- anderer Ort, gleiche Sache.
    pub attr_corr: Option<String>,
}

/// Operational direction produced by lowering a declarative rule.
///
/// This is not a pass switch. It is immutable plan metadata; an
/// incoming delta selects one or both directed families for its
/// realisation wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDirection {
    Forward,
    Backward,
    /// A rule without an establishing correspondence has no side from
    /// which a direction could be derived. Such rules remain available
    /// in every routed wave, preserving the legacy un-routed meaning.
    Undirected,
}

impl RuleDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
            Self::Undirected => "undirected",
        }
    }
}

/// Directed rule: pattern (input side + context corrs) and creation
/// plan (output side + corr + provenance connections).
#[derive(Debug, Clone)]
pub struct DirectedRule {
    pub name: String,
    pub rank: u64,
    pub direction: RuleDirection,
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
// `&Bindings` is the signature 2.0.2 published.
#[allow(clippy::ptr_arg)]
pub fn apply_creation(
    g: &mut Graph,
    rule: &DirectedRule,
    bindings: &Bindings,
) -> (Vec<GhostId>, Vec<GhostId>) {
    apply_creation_at(g, rule, bindings, None, &mut OwnNames::of(rule).names())
}

/// [`apply_creation`], optionally with the identities of the plan's
/// nodes already derived (`ids`, plan-indexed, as the engine derives
/// them for its checks before it applies). An entry for a dynamic leaf
/// without a source is not read. `names`: the names of `rule`
/// ([`RuleNames`]).
pub(crate) fn apply_creation_at(
    g: &mut Graph,
    rule: &DirectedRule,
    bindings: &[GhostId],
    ids: Option<&[GhostId]>,
    names: &mut RuleNames<'_>,
) -> (Vec<GhostId>, Vec<GhostId>) {
    // Plan-indexed; None = a dynamic leaf without a source (dropped).
    let mut slots: Vec<Option<GhostId>> = Vec::with_capacity(rule.create_nodes.len());
    let mut dyn_corrs: Vec<(GhostId, GhostId, GhostId)> = Vec::new();
    let mut dyn_edges: Vec<GhostId> = Vec::new();
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
            Some(match ids {
                Some(ids) => {
                    debug_assert_eq!(
                        ids[plan_ix],
                        crate::graph::preview_corr_id(&anchor, &cn.typ, bindings)
                    );
                    names.add_corr(g, plan_ix, ids[plan_ix], &cn.typ)
                }
                None => g.add_corr(&anchor, &cn.typ, bindings),
            })
        } else if let Some((anchor, ref attr, ref t)) = cn.derived_dyn {
            let source = names.child_leaf(g, plan_ix, &bindings[anchor], attr);
            source.map(|src| {
                let leaf = match ids {
                    Some(ids) => names.add_derived_leaf(g, plan_ix, ids[plan_ix], &cn.typ, src, t),
                    None => g.add_derived_leaf(&parent, &cn.typ, src, t),
                };
                // Die Attribut-Korrespondenz des dynamischen Falls.
                // Fehlt die Quelle, entsteht kein Blatt und folglich
                // auch keine Korrespondenz -- apply-if-present gilt
                // fuer beide.
                if let Some(ct) = &cn.attr_corr {
                    let ac = names.add_attr_corr(g, plan_ix, &src, ct, &cn.typ, bindings);
                    g.mark_corr(&ac);
                    dyn_corrs.push((ac, src, leaf));
                }
                leaf
            })
        } else {
            Some(match (ids, &cn.konst, &cn.derived) {
                (Some(ids), Some(v), _) => {
                    names.add_konst_leaf(g, plan_ix, ids[plan_ix], &cn.typ, v)
                }
                (Some(ids), None, Some((in_leaf, t))) => {
                    names.add_derived_leaf(g, plan_ix, ids[plan_ix], &cn.typ, bindings[*in_leaf], t)
                }
                (Some(ids), None, None) => names.add_ghost(g, plan_ix, ids[plan_ix], &cn.typ),
                (None, Some(v), _) => {
                    g.add_konst_leaf(&parent, &cn.typ, &rule.name, plan_ix as u32, v)
                }
                (None, None, Some((in_leaf, t))) => {
                    g.add_derived_leaf(&parent, &cn.typ, bindings[*in_leaf], t)
                }
                (None, None, None) => g.add_ghost(&parent, &cn.typ),
            })
        };
        if cn.is_corr {
            if let Some(id) = id {
                g.mark_corr(&id);
            }
        }
        slots.push(id);
    }
    // Attribut-Korrespondenzen des dynamischen Falls: erst hier
    // bekannt, weil ihre Quelle beim Anwenden gefunden wurde.
    let mut dyn_nodes: Vec<GhostId> = Vec::new();
    for (ac, src, leaf) in dyn_corrs {
        dyn_nodes.push(ac);
        for (a, b) in [(ac, src), (ac, leaf)] {
            if let Some(e) = g.connect(a, b, crate::ident::Status::Ghost) {
                dyn_edges.push(e);
            }
        }
    }
    let mut edges = Vec::with_capacity(rule.create_links.len());
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
    let mut nodes: Vec<GhostId> = slots.into_iter().flatten().collect();
    nodes.extend(dyn_nodes);
    edges.extend(dyn_edges);
    (nodes, edges)
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
        // Corr + Male + Namensblatt + die Attribut-Korrespondenz des
        // Namensblatts. Was das Regelformat `bindings` nennt, ist eine
        // Korrespondenz auf Blatt-Ebene und traegt seit 2026-08-12
        // einen eigenen Knoten.
        assert_eq!(created.len(), 4);
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

    /// An engine that goes on with another graph names the types of
    /// its plans in that graph's tables. The copy is taken before the
    /// first step, where the correspondence type is not interned yet,
    /// and gets another type first: the number the correspondence type
    /// has in the original belongs to that other type in the copy.
    #[test]
    fn a_plan_names_its_types_in_the_graph_at_hand() {
        let (mut a, vs) = seed(2);
        let rules = load(serde_json::json!([father_rule()]), &mut a);
        assert!(
            a.types.lookup("PersonCorr").is_none(),
            "the correspondence type is interned by the first step"
        );
        let mut b = a.clone();
        let zebra = b.types.intern("Zebra");

        let mut e = Engine::new(&rules);
        e.seed(&a, &vs);
        assert_eq!(e.step(&mut a, &vs), Some(true));
        assert_eq!(a.types.lookup("PersonCorr"), Some(zebra), "same number");

        // The rest of the cascade, in the copy.
        while e.step(&mut b, &vs).is_some() {}
        let corr = b.types.lookup("PersonCorr").expect("created in the copy");
        assert_ne!(corr, zebra);
        assert!(b.type_count(corr) >= 1);
        assert_eq!(b.type_count(zebra), 0, "no node is a Zebra");
    }

    /// The ids kept for the names of a rule belong to one set of
    /// tables and one rule slice; for others they are dropped.
    #[cfg(feature = "opt_g")]
    #[test]
    fn kept_ids_are_dropped_for_other_tables_and_other_rules() {
        let (mut a, _vs) = seed(1);
        let rules = load(serde_json::json!([father_rule()]), &mut a);
        let id = |n: u8| GhostId::from_raw([n; 32]);
        let mut names = Names::default();

        // A copy before the type is interned, with another type at
        // the number it gets in the original.
        let mut b = a.clone();
        assert_ne!(
            a.types.uid.get(),
            b.types.uid.get(),
            "a copy is another table"
        );
        let other = b.types.intern("Other");

        // Rule 1 (the backward plan), plan position 1.
        let place = |names: &Names| names.create[names.starts[1].0 as usize + 1].typ;
        names
            .for_step(&a, &rules, 1)
            .add_ghost(&mut a, 1, id(1), "Fresh");
        let in_a = a.types.lookup("Fresh").unwrap();
        assert_eq!(in_a, other, "same number");
        assert_eq!(a.node(&id(1)).unwrap().typ, in_a);
        // Kept: the second node of the same plan position.
        names
            .for_step(&a, &rules, 1)
            .add_ghost(&mut a, 1, id(2), "Fresh");
        assert_eq!(a.node(&id(2)).unwrap().typ, in_a);
        assert_eq!(place(&names), Some(in_a));
        // The places of the other rule are its own.
        assert_eq!(names.create[names.starts[0].0 as usize + 1].typ, None);

        // The copy: resolved again, in its tables.
        names
            .for_step(&b, &rules, 1)
            .add_ghost(&mut b, 1, id(1), "Fresh");
        let in_b = b.types.lookup("Fresh").unwrap();
        assert_ne!(in_b, in_a);
        assert_eq!(b.node(&id(1)).unwrap().typ, in_b);
        assert_eq!(b.types.name(b.node(&id(1)).unwrap().typ), "Fresh");

        // And the original again.
        names
            .for_step(&a, &rules, 1)
            .add_ghost(&mut a, 1, id(3), "Fresh");
        assert_eq!(a.node(&id(3)).unwrap().typ, in_a);

        // Another rule slice: nothing of the first is kept.
        let copy = rules.clone();
        names.for_step(&a, &copy, 1);
        assert_eq!(names.rules, (copy.as_ptr() as usize, copy.len()));
        assert_eq!(place(&names), None);
    }
}
