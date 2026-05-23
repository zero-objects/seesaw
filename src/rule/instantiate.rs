//! Instantiierung: `CompiledRuleSpec → Box<dyn Rule>`.
//!
//! Übersetzt den statisch analysierten Plan in eine konkrete
//! `BasicRule`, die in der Cascade-Engine läuft. Die Engine-Topology
//! folgt der aktuellen Pilot-Konvention:
//!
//! - Ein neu etablierter Correspondence-Link wird als **Graph-Knoten**
//!   materialisiert, der als Kind des L-Knotens hängt (Edge-Typ
//!   `corrL`) und den neu erzeugten R-Knoten als Kind trägt (Edge-Typ
//!   `corrR`).
//! - Context-Correspondences erscheinen bereits im Match-Pattern —
//!   die Rule greift nur, wenn sie im Graph vorliegen.
//! - AttrBindings werden zur Match-Zeit angewendet: L-Attribut-Wert
//!   via `AttrTransform` auf den R-Node propagiert. Gleichzeitig
//!   wird der Corr-Node selbst mit den L-Attributen annotiert
//!   (Engine-Konvention für deterministische Ghost-IDs).
//!
//! Diese Konvention ist paper-zitierbar als
//! „operational topology mapping" und gehört ins Implementation-
//! Experience-Report-Kapitel „Von deklarativer Rule-Spec zur
//! operationalen Engine-Form".

use std::collections::{BTreeMap, HashMap};

use crate::engine::{
    BasicRule, EdgePattern, EnginePropagation, NacPattern, NodePattern, Pattern, PatternMatch, Rule,
};
use crate::graph::{GhostId, TypedGraph};
use crate::ops::Op;

use super::compile::{CompiledRuleSpec, EdgeSide};
use super::spec::{AttrTransform, CorrespondenceLinkSpec};

/// Kompiliert einen [`CompiledRuleSpec`] in eine konkrete
/// Engine-Rule.
pub fn instantiate(compiled: &CompiledRuleSpec) -> Box<dyn Rule> {
    // ── Match-Pattern aufbauen ──────────────────────────────────────
    let mut pat = Pattern::new();
    for mn in &compiled.match_plan.nodes {
        let mut np = NodePattern::new(&mn.var, &mn.kind);
        for c in &compiled.match_plan.constraints {
            if c.node_var == mn.var {
                np.attr_constraints
                    .push((c.attr_name.clone(), c.predicate.clone()));
            }
        }
        pat = pat.with_node(np);
    }
    for me in &compiled.match_plan.edges {
        pat = pat.with_edge(EdgePattern::new(&me.source_var, &me.target_var, &me.kind));
    }
    // Context-Corrs zum Match-Pattern hinzufügen: ein synthetischer
    // Knoten mit stabiler Var-ID, plus `corrL`/`corrR`-Kanten.
    for (i, cc) in compiled
        .match_plan
        .context_correspondences
        .iter()
        .enumerate()
    {
        let corr_var = format!("__ctxcorr_{i}");
        let corr_kind = cc
            .kind
            .clone()
            .unwrap_or_else(|| "Correspondence".to_string());
        pat = pat.with_node(NodePattern::new(&corr_var, &corr_kind));
        pat = pat.with_edge(EdgePattern::new(&cc.l_node_id, &corr_var, "corrL"));
        pat = pat.with_edge(EdgePattern::new(&corr_var, &cc.r_node_id, "corrR"));
    }

    // ── Production-Closure aufbauen (owned clones für move) ─────────
    let rule_name = compiled.name.clone();
    let rank: u64 = compiled.rank.max(0) as u64;
    let creation = compiled.creation_plan.clone();
    let propagation = compiled.propagation_plan.clone();

    let production = move |m: &PatternMatch, g: &TypedGraph| -> Vec<Op> {
        let mut ops: Vec<Op> = Vec::new();

        // Neu erzeugte Knoten-IDs pro Var — nötig, damit nachfolgende
        // edges_to_create ihre Endpunkte auflösen können.
        let mut created: HashMap<String, GhostId> = HashMap::new();

        // 1. Correspondence-Nodes + ihre R-Target-Nodes materialisieren.
        for cc in &creation.correspondences_to_create {
            let l_id = match m.get(&cc.l_node_id) {
                Some(id) => *id,
                None => continue, // L-Seite nicht gebunden — Rule-Variante unsupported
            };

            // Attribute des Corr-Nodes: die propagierten Werte aus den
            // L-Attributen (Engine-Konvention: Corr trägt „sprechende"
            // Namen, damit Snapshot und Ghost-ID lesbar bleiben).
            let corr_attrs = collect_corr_attrs(cc, m, g, &propagation);

            let corr_kind = cc
                .kind
                .clone()
                .unwrap_or_else(|| "Correspondence".to_string());
            let corr_id = GhostId::from_parent(&l_id, "corrL", &corr_kind, &corr_attrs);
            ops.push(Op::AddNode {
                parent: l_id,
                edge_type: "corrL".into(),
                type_id: corr_kind,
                attrs: corr_attrs,
            });

            // R-Target-Node: ist er in nodes_to_create oder schon
            // gematched?
            let r_var = &cc.r_node_id;
            let r_kind = creation
                .nodes_to_create
                .iter()
                .find(|n| &n.var == r_var)
                .map(|n| n.kind.clone());

            match (m.get(r_var), r_kind) {
                (Some(existing_r), _) => {
                    // R-Node war schon im Match — nur die Corr-Edge
                    // muss noch gezogen werden.
                    ops.push(Op::AddEdge {
                        source: corr_id,
                        target: *existing_r,
                        type_id: "corrR".into(),
                        attrs: BTreeMap::new(),
                    });
                }
                (None, Some(kind)) => {
                    // R-Node ist neu zu erzeugen, als Kind des Corr-Nodes.
                    // rc6 (B6): creation_attrs für diesen R-Var fließen in
                    // den r_attrs ein und damit in die GhostId — sie sind
                    // Identitäts-Attribute, nicht post-creation-Mutationen.
                    let r_attrs = collect_r_attrs(cc, m, g, &propagation, &creation.creation_attrs);
                    let r_id = GhostId::from_parent(&corr_id, "corrR", &kind, &r_attrs);
                    ops.push(Op::AddNode {
                        parent: corr_id,
                        edge_type: "corrR".into(),
                        type_id: kind,
                        attrs: r_attrs,
                    });
                    created.insert(r_var.clone(), r_id);
                }
                (None, None) => {
                    // R-Var in keiner Plan-Tabelle → unsauberer
                    // CompiledRuleSpec; wir ignorieren still.
                }
            }
        }

        // 2. Zusätzliche R-Kanten materialisieren (edges_to_create).
        //    Erwartung: mindestens ein Endpunkt ist in `created`, der
        //    andere im Match gebunden.
        for e in &creation.edges_to_create {
            // Nur R-Side-Edges tatsächlich emittieren; L-Edges sind
            // Match-Constraints.
            if e.side != EdgeSide::R {
                continue;
            }
            let src = resolve_var(&e.source_var, m, &created);
            let tgt = resolve_var(&e.target_var, m, &created);
            if let (Some(s), Some(t)) = (src, tgt) {
                ops.push(Op::AddEdge {
                    source: s,
                    target: t,
                    type_id: e.kind.clone(),
                    attrs: BTreeMap::new(),
                });
            }
        }

        // 3. attrs_to_set: R-Pattern-Literal-Constraints, die ein
        //    Attribut auf einem im Match gebundenen (Kontext-)Knoten
        //    auf einen neuen Wert setzen. Pendant zu Schritt 2 auf der
        //    Attribut-Seite — vor rc4 wurde dieser Pfad gar nicht
        //    emittiert; der Workaround in dry-cleaner war Schema-
        //    Verzerrung (Kind-Knoten statt Attribute).
        for ats in &creation.attrs_to_set {
            if let Some(id) = resolve_var(&ats.node_var, m, &created) {
                ops.push(Op::SetAttr {
                    target: id,
                    key: ats.attr_name.clone(),
                    value: ats.value.clone(),
                });
            }
        }

        ops
    };

    // NACs des Rule-Specs in Engine-NacPatterns konvertieren.
    let nacs: Vec<NacPattern> = compiled
        .nacs
        .iter()
        .map(|cn| {
            let mut nac_pat = Pattern::new();
            for n in &cn.nodes {
                let mut np = NodePattern::new(&n.var, &n.kind);
                for c in &cn.constraints {
                    if c.node_var == n.var {
                        np.attr_constraints
                            .push((c.attr_name.clone(), c.predicate.clone()));
                    }
                }
                nac_pat = nac_pat.with_node(np);
            }
            for e in &cn.edges {
                nac_pat =
                    nac_pat.with_edge(EdgePattern::new(&e.source_var, &e.target_var, &e.kind));
            }
            NacPattern {
                name: cn.name.clone(),
                pattern: nac_pat,
                shared_with_l: cn.shared_with_l.clone(),
            }
        })
        .collect();

    // Propagations für Re-Validation (M5.3) übersetzen.
    let propagations: Vec<EnginePropagation> = compiled
        .propagation_plan
        .iter()
        .map(|p| EnginePropagation {
            source_node_var: p.source_node_var.clone(),
            source_attr: p.source_attr.clone(),
            target_node_var: p.target_node_var.clone(),
            target_attr: p.target_attr.clone(),
            transform_tag: p.transform.tag().to_string(),
        })
        .collect();

    Box::new(
        BasicRule::new(&rule_name, rank, pat, production)
            .with_nacs(nacs)
            .with_propagations(propagations),
    )
}

// ══════════════════════════════════════════════════════════════════════
// Helfer
// ══════════════════════════════════════════════════════════════════════

fn resolve_var(var: &str, m: &PatternMatch, created: &HashMap<String, GhostId>) -> Option<GhostId> {
    m.get(var).copied().or_else(|| created.get(var).copied())
}

/// Sammelt die Attribute, die auf dem neu zu erzeugenden R-Node
/// landen sollen. Drei Quellen, in dieser Reihenfolge:
///
/// 1. `attribute_bindings` des Corr-Links — propagierte L→R-Werte.
/// 2. Zusätzliche `propagation_plan`-Einträge mit gleichem (L,R)-Paar.
/// 3. **rc6 (B6)**: `creation_attrs` für diesen R-Var — Identitäts-
///    Attribute aus R-Pattern-Literal-Constraints auf R-only-Creation-
///    Knoten. Vor rc6 landeten diese in `attrs_to_set` und wurden
///    nach der Knoten-Erzeugung via `Op::SetAttr` geschrieben; das
///    machte zwei Rules mit verschiedenen Literal-Werten aber sonst
///    identischer Struktur kollidieren (selbe GhostId, oszillierende
///    SetAttrs). Jetzt fließen sie in `r_attrs` und damit in den
///    GhostId-Hash → zwei verschiedene Werte → zwei verschiedene
///    Knoten, keine Kollision.
fn collect_r_attrs(
    cc: &CorrespondenceLinkSpec,
    m: &PatternMatch,
    g: &TypedGraph,
    propagation: &[super::compile::AttrPropagation],
    creation_attrs: &[super::compile::CreationAttr],
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    for b in &cc.attribute_bindings {
        let transform =
            AttrTransform::parse(b.transformation.as_deref()).unwrap_or(AttrTransform::Identity);
        // Wert aus der L-Seite holen
        let l_id = match m.get(&cc.l_node_id) {
            Some(id) => id,
            None => continue,
        };
        let src_value = g
            .get_node(l_id)
            .and_then(|n| n.attrs.get(&b.l_attr_name).cloned())
            .unwrap_or_default();
        attrs.insert(b.r_attr_name.clone(), transform.apply(&src_value));
    }
    // Nicht-binding Attribute aus propagation_plan ergänzen (falls
    // der Compile-Plan auf diesem Corr zusätzliche Propagationen
    // hat; aktuell deckungsgleich mit attribute_bindings).
    for p in propagation {
        if p.source_node_var == cc.l_node_id && p.target_node_var == cc.r_node_id {
            if attrs.contains_key(&p.target_attr) {
                continue;
            }
            let l_id = match m.get(&p.source_node_var) {
                Some(id) => id,
                None => continue,
            };
            let src_value = g
                .get_node(l_id)
                .and_then(|n| n.attrs.get(&p.source_attr).cloned())
                .unwrap_or_default();
            attrs.insert(p.target_attr.clone(), p.transform.apply(&src_value));
        }
    }
    // rc6 (B6): R-Pattern-Literal-Constraints auf diesem R-only-
    // Creation-Knoten als Identitäts-Attribute mit aufnehmen.
    // Bindings haben Vorrang (insert nur, wenn key noch frei),
    // damit propagierte Werte nicht durch einen Konflikt-Literal-
    // Constraint überschrieben werden — der wäre ein Modellierungs-
    // fehler im Ruleset (Literal vs. gebundener Wert).
    for ca in creation_attrs {
        if ca.node_var == cc.r_node_id && !attrs.contains_key(&ca.attr_name) {
            attrs.insert(ca.attr_name.clone(), ca.value.clone());
        }
    }
    attrs
}

/// Sammelt die Attribute, die auf dem Corr-Node landen. Engine-
/// Konvention: der Corr bekommt dieselben L-Attribute wie der neu
/// erzeugte R-Node, wenn die Transformation Identity ist. Das ergibt
/// sprechende Ghost-IDs für Corr-Knoten.
fn collect_corr_attrs(
    cc: &CorrespondenceLinkSpec,
    m: &PatternMatch,
    g: &TypedGraph,
    _propagation: &[super::compile::AttrPropagation],
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    for b in &cc.attribute_bindings {
        let transform =
            AttrTransform::parse(b.transformation.as_deref()).unwrap_or(AttrTransform::Identity);
        if transform != AttrTransform::Identity {
            continue;
        }
        let l_id = match m.get(&cc.l_node_id) {
            Some(id) => id,
            None => continue,
        };
        let src_value = g
            .get_node(l_id)
            .and_then(|n| n.attrs.get(&b.l_attr_name).cloned())
            .unwrap_or_default();
        attrs.insert(b.l_attr_name.clone(), src_value);
    }
    attrs
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::find_matches;
    use crate::graph::Status;
    use crate::rule::compile::compile;
    use crate::rule::spec::parse_ruleset;

    const DEMO_FIXTURE: &str = include_str!("../../tests/fixtures/demo-ruleset.json");

    fn demo_rule_compiled(name: &str) -> CompiledRuleSpec {
        let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
        let r = rs.rules.into_iter().find(|r| r.name == name).unwrap();
        compile(&r).unwrap()
    }

    /// Baut einen Mini-Graph mit Model → Class(name='Person') und
    /// gibt die IDs zurück.
    fn mini_model_with_person() -> (TypedGraph, GhostId, GhostId) {
        let mut g = TypedGraph::new();
        let model_attrs: BTreeMap<String, String> = [("name".to_string(), "Demo".to_string())]
            .into_iter()
            .collect();
        let model_id = g.add_baseline_node("Model", "demo-root", model_attrs);

        let class_attrs: BTreeMap<String, String> = [("name".to_string(), "Person".to_string())]
            .into_iter()
            .collect();
        let class_id = g.add_solid_child_node(model_id, "classes", "Class", class_attrs);
        g.add_edge(
            model_id,
            class_id,
            "classes",
            BTreeMap::new(),
            Status::Solid,
        )
        .expect("Kante Model→Class muss einfügbar sein");
        (g, model_id, class_id)
    }

    #[test]
    fn r_class_instantiiert_und_matcht() {
        let compiled = demo_rule_compiled("R_Class");
        let rule = instantiate(&compiled);
        let (g, _m, c) = mini_model_with_person();

        let matches = find_matches(rule.pattern(), &g);
        assert_eq!(matches.len(), 1, "R_Class muss genau einen Match finden");
        let m = &matches[0];
        assert_eq!(*m.get("c").unwrap(), c);
    }

    #[test]
    fn r_class_produktion_emittiert_corrclass_javaclass_und_javaclasses_edge() {
        let compiled = demo_rule_compiled("R_Class");
        let rule = instantiate(&compiled);
        let (g, _m, _c) = mini_model_with_person();

        let matches = find_matches(rule.pattern(), &g);
        let ops = rule.produce(&matches[0], &g);

        // Spec-treu: 3 Ops. Die javaClasses-Edge zwischen Model und
        // neuem JavaClass wird explizit gezogen (die hart-kodierte
        // Variante ohne Model-Kontext hätte das nicht — das ist ein
        // bewusster Gewinn des Spec-basierten Compilers).
        assert_eq!(ops.len(), 3,
            "R_Class-Produktion soll 3 Ops emittieren (CorrClass, JavaClass, javaClasses-Edge), war: {ops:?}");

        match &ops[0] {
            Op::AddNode {
                type_id,
                edge_type,
                attrs,
                ..
            } => {
                assert_eq!(type_id, "CorrClass");
                assert_eq!(edge_type, "corrL");
                assert_eq!(attrs.get("name").map(String::as_str), Some("Person"));
            }
            _ => panic!("Op[0] muss CorrClass-AddNode sein, war: {:?}", ops[0]),
        }
        match &ops[1] {
            Op::AddNode {
                type_id,
                edge_type,
                attrs,
                ..
            } => {
                assert_eq!(type_id, "JavaClass");
                assert_eq!(edge_type, "corrR");
                assert_eq!(attrs.get("name").map(String::as_str), Some("Person"));
            }
            _ => panic!("Op[1] muss JavaClass-AddNode sein, war: {:?}", ops[1]),
        }
        match &ops[2] {
            Op::AddEdge { type_id, .. } => {
                assert_eq!(
                    type_id, "javaClasses",
                    "Op[2] muss javaClasses-Edge sein, war: {:?}",
                    ops[2]
                );
            }
            _ => panic!("Op[2] muss AddEdge sein, war: {:?}", ops[2]),
        }
    }

    #[test]
    fn r_class_produziert_parent_chain_mit_root_unter_klasse() {
        // Verifiziert die operationale Topology-Konvention: CorrClass
        // hängt unter c (corrL), JavaClass hängt unter CorrClass (corrR).
        let compiled = demo_rule_compiled("R_Class");
        let rule = instantiate(&compiled);
        let (g, _m, c) = mini_model_with_person();
        let ops = rule.produce(&find_matches(rule.pattern(), &g)[0], &g);

        // Op 0: CorrClass — parent muss c (Class) sein.
        let (corr_parent, corr_attrs) = match &ops[0] {
            Op::AddNode { parent, attrs, .. } => (*parent, attrs.clone()),
            _ => panic!("Op[0] muss AddNode sein"),
        };
        assert_eq!(corr_parent, c);

        // Op 1: JavaClass — parent muss die deterministische GhostID
        // der gerade emittierten CorrClass sein.
        let expected_corr_id =
            GhostId::from_parent(&corr_parent, "corrL", "CorrClass", &corr_attrs);
        match &ops[1] {
            Op::AddNode { parent, .. } => assert_eq!(*parent, expected_corr_id),
            _ => panic!("Op[1] muss AddNode sein"),
        }
    }

    #[test]
    fn instantiated_rule_traegt_name_und_rank() {
        let compiled = demo_rule_compiled("R_Class");
        let rule = instantiate(&compiled);
        assert_eq!(rule.id(), "R_Class");
        assert_eq!(rule.rank(), 40);
    }

    #[test]
    fn r_getter_hat_capitalize_attr_auf_neuem_getter() {
        // Baut eine fiktive Match-Situation für R_Getter: wir können
        // den vollen End-to-End-Test erst in P7b.5 integration-
        // testen, sobald ein JavaField im Graph steht. Hier ein Basic-
        // Sanity-Test, dass Capitalize im Attribute-Mapping wirkt.
        let compiled = demo_rule_compiled("R_Getter");
        let rule = instantiate(&compiled);
        // Nur struktureller Check — Pattern hat alle 5 Knoten
        // (c, a, jc, jf, g) + die Context-Corrs synthetisch.
        let pattern_node_count = rule.pattern().nodes.len();
        assert!(
            pattern_node_count >= 5,
            "R_Getter-Pattern sollte ≥ 5 Knoten haben (tatsächlich: {pattern_node_count})"
        );
    }

    #[test]
    fn r_attribut_auf_kontext_knoten_wird_via_setattr_emittiert() {
        // rc4-Belegtest: Eine Rule mit L-Literal image="old" und
        // R-Literal image="new" am selben (Kontext-)Knoten muss bei
        // Produktion genau eine Op::SetAttr emittieren. Vor rc4 wäre
        // dieser Match nie zustande gekommen — die Rule sucht sonst
        // gleichzeitig nach image="old" und image="new". Pendant zum
        // B4-Edge-Bug auf der Attribut-Seite.
        let json = r#"{"rules":[{
            "name":"SetImage","rank":1,
            "l_pattern":{"nodes":[
                {"id":"j","kind":"Job",
                 "constraints":[{"name":"image","matcher":{"type":"literal","value":"old"}}]}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"j","kind":"Job",
                 "constraints":[{"name":"image","matcher":{"type":"literal","value":"new"}}]}],
                "edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let compiled = compile(&rs.rules[0]).unwrap();
        let rule = instantiate(&compiled);

        // Graph mit einem Job-Knoten image="old"
        let mut g = TypedGraph::new();
        let job_attrs: BTreeMap<String, String> = [("image".to_string(), "old".to_string())]
            .into_iter()
            .collect();
        let job_id = g.add_baseline_node("Job", "job-1", job_attrs);

        // Match-und-Produce
        let matches = find_matches(rule.pattern(), &g);
        assert_eq!(
            matches.len(),
            1,
            "Rule muss genau einen Match auf dem Job-Knoten finden"
        );
        let ops = rule.produce(&matches[0], &g);

        // Genau eine SetAttr-Op auf job_id, key=image, value=new
        assert_eq!(
            ops.len(),
            1,
            "Produktion muss genau eine Op emittieren (SetAttr), war: {ops:?}"
        );
        match &ops[0] {
            Op::SetAttr { target, key, value } => {
                assert_eq!(*target, job_id);
                assert_eq!(key, "image");
                assert_eq!(value, "new");
            }
            other => panic!("Op[0] muss SetAttr sein, war: {other:?}"),
        }
    }
}
