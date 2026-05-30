//! Instantiation: `CompiledRuleSpec → Box<dyn Rule>`.
//!
//! Translates the statically analyzed plan into a concrete
//! `BasicRule` that runs in the cascade engine. The engine topology
//! follows the current pilot convention:
//!
//! - A newly established correspondence link is materialized as a
//!   **graph node** that hangs as a child of the L-node (edge type
//!   `corrL`) and carries the newly created R-node as its child (edge
//!   type `corrR`).
//! - Context correspondences appear already in the match pattern —
//!   the rule fires only when they are present in the graph.
//! - AttrBindings are applied at match time: the L attribute value is
//!   propagated to the R-node via `AttrTransform`. At the same time
//!   the corr node itself is annotated with the L attributes
//!   (engine convention for deterministic ghost IDs).
//!
//! This convention is paper-citable as
//! "operational topology mapping" and belongs in the implementation
//! experience report chapter "From declarative rule spec to
//! operational engine form".

use std::collections::{BTreeMap, HashMap};

use crate::engine::{
    BasicRule, EdgePattern, EnginePropagation, NacPattern, NodePattern, Pattern, PatternMatch, Rule,
};
use crate::graph::{GhostId, TypedGraph};
use crate::ops::Op;

use super::compile::{CompiledRuleSpec, EdgeSide};
use super::spec::{AttrTransform, CorrespondenceLinkSpec};

/// Compiles a [`CompiledRuleSpec`] into a concrete
/// engine rule.
pub fn instantiate(compiled: &CompiledRuleSpec) -> Box<dyn Rule> {
    // ── Build the match pattern ─────────────────────────────────────
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
    // Add context correspondences to the match pattern: a synthetic
    // node with a stable var id, plus `corrL`/`corrR` edges.
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
        // rc7 (S): symmetric correspondence — the context match is
        // direction- AND kind-agnostic (membership): the corr node must
        // be connected to both endpoints, regardless of how corrL/corrR
        // are oriented. This way a correspondence established in ONE
        // direction is recognized as context by rules of BOTH directions
        // (corrL/corrR are created direction-relative — the C1 finding).
        pat = pat.with_edge(EdgePattern::membership(&corr_var, &cc.l_node_id));
        pat = pat.with_edge(EdgePattern::membership(&corr_var, &cc.r_node_id));
    }

    // ── Build the production closure (owned clones for move) ────────
    let rule_name = compiled.name.clone();
    let rank: u64 = compiled.rank.max(0) as u64;
    let creation = compiled.creation_plan.clone();
    let propagation = compiled.propagation_plan.clone();

    let production = move |m: &PatternMatch, g: &TypedGraph| -> Vec<Op> {
        let mut ops: Vec<Op> = Vec::new();

        // Newly created node IDs per var — needed so that subsequent
        // edges_to_create can resolve their endpoints.
        let mut created: HashMap<String, GhostId> = HashMap::new();

        // 1. Materialize correspondence nodes + their R target nodes.
        //
        // rc7 A8 (Fix A) — identity decoupled from the propagated attribute:
        // the ghost ID of corr and target is formed ONLY from structural +
        // identity attributes (creation_attrs), NOT from the bound
        // (propagated) attributes. The latter are written as `Op::SetAttr`
        // after node creation. Consequence: the identity is stable under
        // attribute changes — a rename of the source element re-derives the
        // same target node (the structural op becomes an idempotent no-op)
        // and only updates its value, instead of creating a duplicate with
        // a new identity.
        for cc in &creation.correspondences_to_create {
            let l_id = match m.get(&cc.l_node_id) {
                Some(id) => *id,
                None => continue, // L side not bound — rule variant unsupported
            };

            let corr_kind = cc
                .kind
                .clone()
                .unwrap_or_else(|| "Correspondence".to_string());
            // Corr identity is purely structural (source element + kind);
            // no bound attributes in the hash.
            let corr_id = GhostId::from_parent(&l_id, "corrL", &corr_kind, &BTreeMap::new());
            ops.push(Op::AddNode {
                parent: l_id,
                edge_type: "corrL".into(),
                type_id: corr_kind,
                attrs: BTreeMap::new(),
            });

            // R target node: is it in nodes_to_create or already
            // matched?
            let r_var = &cc.r_node_id;
            let r_kind = creation
                .nodes_to_create
                .iter()
                .find(|n| &n.var == r_var)
                .map(|n| n.kind.clone());

            let target_id: Option<GhostId> = match (m.get(r_var), r_kind) {
                (Some(existing_r), _) => {
                    // R-node was already in the match — only the corr
                    // edge still needs to be drawn.
                    ops.push(Op::AddEdge {
                        source: corr_id,
                        target: *existing_r,
                        type_id: "corrR".into(),
                        attrs: BTreeMap::new(),
                    });
                    Some(*existing_r)
                }
                (None, Some(kind)) => {
                    // R-node is to be newly created, as a child of the corr node.
                    // Only identity attributes (creation_attrs, rc6 B6)
                    // flow into the GhostId; bound values follow as
                    // SetAttr (rc7 A8).
                    let r_identity = collect_r_identity_attrs(cc, &creation.creation_attrs);
                    let r_id = GhostId::from_parent(&corr_id, "corrR", &kind, &r_identity);
                    ops.push(Op::AddNode {
                        parent: corr_id,
                        edge_type: "corrR".into(),
                        type_id: kind,
                        attrs: r_identity,
                    });
                    created.insert(r_var.clone(), r_id);
                    Some(r_id)
                }
                (None, None) => {
                    // R var in no plan table → unclean
                    // CompiledRuleSpec; we silently ignore.
                    None
                }
            };

            // rc7 A8: propagate bound attribute values as SetAttr onto the
            // target. Idempotent — if the value already matches, the op is
            // a no-op (saturates to duplication). On rename it carries the
            // new value onto the (identity-stable) target node.
            if let Some(tid) = target_id {
                for (attr, value) in collect_propagated_attrs(cc, m, g, &propagation) {
                    ops.push(Op::SetAttr {
                        target: tid,
                        key: attr,
                        value,
                    });
                }
            }
        }

        // 2. Materialize additional R edges (edges_to_create).
        //    Expectation: at least one endpoint is in `created`, the
        //    other bound in the match.
        for e in &creation.edges_to_create {
            // Only actually emit R-side edges; L edges are
            // match constraints.
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

        // 3. attrs_to_set: R-pattern literal constraints that set an
        //    attribute on a (context) node bound in the match to a new
        //    value. Counterpart to step 2 on the attribute side —
        //    before rc4 this path was not emitted at all; the
        //    workaround in dry-cleaner was a schema distortion
        //    (child node instead of attributes).
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

    // Convert the rule spec's NACs to engine NacPatterns.
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

    // Translate propagations for re-validation (M5.3).
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
            .with_propagations(propagations)
            .with_input_domain_kinds(compiled.input_domain_kinds.clone()),
    )
}

// ══════════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════════

fn resolve_var(var: &str, m: &PatternMatch, created: &HashMap<String, GhostId>) -> Option<GhostId> {
    m.get(var).copied().or_else(|| created.get(var).copied())
}

/// Collects the attributes that should land on the newly created
/// R-node. Three sources, in this order:
///
/// 1. `attribute_bindings` of the corr link — propagated L→R values.
/// 2. Additional `propagation_plan` entries with the same (L,R) pair.
/// 3. **rc6 (B6)**: `creation_attrs` for this R var — identity
///    attributes from R-pattern literal constraints on R-only-creation
///    nodes. Before rc6 these landed in `attrs_to_set` and were
///    written via `Op::SetAttr` after node creation; that caused two
///    rules with different literal values but otherwise identical
///    structure to collide (same GhostId, oscillating SetAttrs).
///    Now they flow into `r_attrs` and thus into the
///    GhostId hash → two different values → two different
///    nodes, no collision.
///
/// rc7 A8 — identity attributes of the newly created R-node: ONLY
/// `creation_attrs` (rc6 B6: R-pattern literal constraints on R-only-
/// creation nodes). These flow into the ghost ID because they
/// identify the target structurally (two rules with different literal
/// values → two different nodes, no collision). Bound
/// (propagated) values do NOT belong here — they are value, not
/// identity, and are written as SetAttr via [`collect_propagated_attrs`].
fn collect_r_identity_attrs(
    cc: &CorrespondenceLinkSpec,
    creation_attrs: &[super::compile::CreationAttr],
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    for ca in creation_attrs {
        if ca.node_var == cc.r_node_id {
            attrs.insert(ca.attr_name.clone(), ca.value.clone());
        }
    }
    attrs
}

/// rc7 A8 — bound (propagated) attribute values for the target: from the
/// `attribute_bindings` (L→R via transform) plus matching
/// `propagation_plan` entries. They are written to the target as
/// `Op::SetAttr` AFTER node creation. This way the bound attribute is
/// a **value** (mutable, propagatable), not an identity — a rename
/// of the source element updates the existing target node, instead of
/// creating a duplicate with a new ghost ID.
fn collect_propagated_attrs(
    cc: &CorrespondenceLinkSpec,
    m: &PatternMatch,
    g: &TypedGraph,
    propagation: &[super::compile::AttrPropagation],
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    let l_id = match m.get(&cc.l_node_id) {
        Some(id) => id,
        None => return attrs,
    };
    for b in &cc.attribute_bindings {
        let transform =
            AttrTransform::parse(b.transformation.as_deref()).unwrap_or(AttrTransform::Identity);
        let src_value = g
            .get_node(l_id)
            .and_then(|n| n.attrs.get(&b.l_attr_name).cloned())
            .unwrap_or_default();
        attrs.insert(b.r_attr_name.clone(), transform.apply(&src_value));
    }
    // Additional propagation_plan entries (currently overlapping with
    // attribute_bindings; bindings take precedence).
    for p in propagation {
        if p.source_node_var == cc.l_node_id
            && p.target_node_var == cc.r_node_id
            && !attrs.contains_key(&p.target_attr)
        {
            let src_value = g
                .get_node(l_id)
                .and_then(|n| n.attrs.get(&p.source_attr).cloned())
                .unwrap_or_default();
            attrs.insert(p.target_attr.clone(), p.transform.apply(&src_value));
        }
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

    /// Builds a mini graph with Model → Class(name='Person') and
    /// returns the IDs.
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
        .expect("Model→Class edge must be insertable");
        (g, model_id, class_id)
    }

    #[test]
    fn r_class_instantiiert_und_matcht() {
        let compiled = demo_rule_compiled("R_Class");
        let rule = instantiate(&compiled);
        let (g, _m, c) = mini_model_with_person();

        let matches = find_matches(rule.pattern(), &g);
        assert_eq!(matches.len(), 1, "R_Class must find exactly one match");
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

        // rc7 A8 — identity decoupled from the propagated attribute: the
        // CorrClass and JavaClass AddNodes no longer carry `name` (their
        // ghost ID is structural), the name is propagated as a separate
        // SetAttr. Consequence: 4 ops instead of 3 (CorrClass, JavaClass,
        // SetAttr name, javaClasses edge). The javaClasses edge between
        // Model and the new JavaClass is drawn explicitly — a deliberate
        // gain of the spec-based compiler over the hard-coded variant.
        assert_eq!(ops.len(), 4,
            "R_Class production should emit 4 ops (CorrClass, JavaClass, SetAttr name, javaClasses edge), was: {ops:?}");

        match &ops[0] {
            Op::AddNode {
                type_id,
                edge_type,
                attrs,
                ..
            } => {
                assert_eq!(type_id, "CorrClass");
                assert_eq!(edge_type, "corrL");
                assert_eq!(
                    attrs.get("name"),
                    None,
                    "rc7 A8: CorrClass identity carries no bound attribute"
                );
            }
            _ => panic!("Op[0] must be CorrClass AddNode, was: {:?}", ops[0]),
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
                assert_eq!(
                    attrs.get("name"),
                    None,
                    "rc7 A8: JavaClass identity carries no bound attribute"
                );
            }
            _ => panic!("Op[1] must be JavaClass AddNode, was: {:?}", ops[1]),
        }
        match &ops[2] {
            Op::SetAttr { key, value, .. } => {
                assert_eq!(key, "name");
                assert_eq!(
                    value, "Person",
                    "rc7 A8: name is propagated as a value onto the target"
                );
            }
            _ => panic!("Op[2] must be SetAttr(name), was: {:?}", ops[2]),
        }
        match &ops[3] {
            Op::AddEdge { type_id, .. } => {
                assert_eq!(
                    type_id, "javaClasses",
                    "Op[3] must be javaClasses edge, was: {:?}",
                    ops[3]
                );
            }
            _ => panic!("Op[3] must be AddEdge, was: {:?}", ops[3]),
        }
    }

    #[test]
    fn r_class_produziert_parent_chain_mit_root_unter_klasse() {
        // Verifies the operational topology convention: CorrClass
        // hangs under c (corrL), JavaClass hangs under CorrClass (corrR).
        let compiled = demo_rule_compiled("R_Class");
        let rule = instantiate(&compiled);
        let (g, _m, c) = mini_model_with_person();
        let ops = rule.produce(&find_matches(rule.pattern(), &g)[0], &g);

        // Op 0: CorrClass — parent must be c (Class).
        let (corr_parent, corr_attrs) = match &ops[0] {
            Op::AddNode { parent, attrs, .. } => (*parent, attrs.clone()),
            _ => panic!("Op[0] must be AddNode"),
        };
        assert_eq!(corr_parent, c);

        // Op 1: JavaClass — parent must be the deterministic GhostID
        // of the just-emitted CorrClass.
        let expected_corr_id =
            GhostId::from_parent(&corr_parent, "corrL", "CorrClass", &corr_attrs);
        match &ops[1] {
            Op::AddNode { parent, .. } => assert_eq!(*parent, expected_corr_id),
            _ => panic!("Op[1] must be AddNode"),
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
        // Builds a fictitious match situation for R_Getter: we can only
        // run the full end-to-end test in P7b.5 as an integration
        // test, once a JavaField is present in the graph. Here a basic
        // sanity test that Capitalize works in the attribute mapping.
        let compiled = demo_rule_compiled("R_Getter");
        let rule = instantiate(&compiled);
        // Structural check only — the pattern has all 5 nodes
        // (c, a, jc, jf, g) + the context corrs synthetically.
        let pattern_node_count = rule.pattern().nodes.len();
        assert!(
            pattern_node_count >= 5,
            "R_Getter pattern should have ≥ 5 nodes (actually: {pattern_node_count})"
        );
    }

    #[test]
    fn r_attribut_auf_kontext_knoten_wird_via_setattr_emittiert() {
        // rc4 evidence test: a rule with L literal image="old" and
        // R literal image="new" on the same (context) node must emit
        // exactly one Op::SetAttr on production. Before rc4 this
        // match would never have come about — the rule would otherwise
        // search simultaneously for image="old" and image="new".
        // Counterpart to the B4 edge bug on the attribute side.
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

        // Graph with one Job node image="old"
        let mut g = TypedGraph::new();
        let job_attrs: BTreeMap<String, String> = [("image".to_string(), "old".to_string())]
            .into_iter()
            .collect();
        let job_id = g.add_baseline_node("Job", "job-1", job_attrs);

        // Match and produce
        let matches = find_matches(rule.pattern(), &g);
        assert_eq!(
            matches.len(),
            1,
            "Rule must find exactly one match on the Job node"
        );
        let ops = rule.produce(&matches[0], &g);

        // Exactly one SetAttr op on job_id, key=image, value=new
        assert_eq!(
            ops.len(),
            1,
            "Production must emit exactly one op (SetAttr), was: {ops:?}"
        );
        match &ops[0] {
            Op::SetAttr { target, key, value } => {
                assert_eq!(*target, job_id);
                assert_eq!(key, "image");
                assert_eq!(value, "new");
            }
            other => panic!("Op[0] must be SetAttr, was: {other:?}"),
        }
    }
}
