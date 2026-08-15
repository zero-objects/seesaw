//! Lowering a validated rule into two directed
//! creation plans.
//!
//! The algorithm is the same as in [`crate::plan`]
//! (`lower_directed`); its fix rounds were carried over too
//! (multiple establishes, reference corrs as context, identity-parent
//! precedence). Differences from the earlier positional form, all
//! the format:
//!
//! - Positions come from the `Resolved*` structures; a side's anchor
//!   is declared ([`crate::rules::validate::ResolvedSide`]) instead of
//!   "position 0".
//! - A corr's role is read ([`Role`]); the earlier bindings fallback
//!   no longer exists.
//! - Transformations are chains of primitives instead of enum
//!   variants. The backward direction is formed by `Chain::inverse()`,
//!   instead of picking an inverse variant from a closed set.
//! - The value predicate (matching) and the constant (creating) are
//!   separate fields in the format; the earlier form derived both from
//!   an equality constraint.
//!
//! `same_value_links` (within-side) and `joins` (cross-side) are
//! unchanged in effect and order, only declared via
//! names instead of positions.

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::matcher::{Link, Pattern, PatternNode};
use crate::graph::{Graph, PlanTransform};
use crate::plan::{CreateNode, DirectedRule, Ref, RuleDirection};
use crate::rules::format::Role;
use crate::rules::transform::{Chain, ChainTable};
use crate::rules::validate::{
    BindingSource, Resolved, ResolvedBinding, ResolvedCorr, ResolvedRule,
};

/// Lowering error.
pub use crate::plan::CompileError as LowerError;

/// Both directions of the rule at position `ix` of the validated set.
///
/// The rule is addressed via its set, not passed individually:
/// `ResolvedBinding.chain` is an id in `res.chains`, and lowering a
/// rule against another set's table gives at best a panic, at worst
/// silently the wrong chain.
pub fn lower_rule(
    res: &Resolved,
    ix: usize,
    g: &mut Graph,
) -> Result<[DirectedRule; 2], LowerError> {
    let rule = &res.rules[ix];
    Ok([
        lower_directed_v3(rule, g, &res.chains, true)?,
        lower_directed_v3(rule, g, &res.chains, false)?,
    ])
}

/// All rules of a validated set, forward and backward alternating in
/// declaration order.
pub fn lower_all(res: &Resolved, g: &mut Graph) -> Result<Vec<DirectedRule>, LowerError> {
    let mut out = Vec::with_capacity(res.rules.len() * 2);
    for ix in 0..res.rules.len() {
        let [f, b] = lower_rule(res, ix, g)?;
        out.push(f);
        out.push(b);
    }
    Ok(out)
}

/// Source and target side of a binding in the run direction.
fn binding_ends(b: &ResolvedBinding, forward: bool) -> (&BindingSource, &BindingSource) {
    if forward {
        (&b.left, &b.right)
    } else {
        (&b.right, &b.left)
    }
}

/// A binding's chain in the run direction. Bindings are declared
/// left→right; leaves created backward carry the INVERSE chain, so
/// value resolution always stays forward.
fn binding_chain(b: &ResolvedBinding, chains: &ChainTable, forward: bool) -> PlanTransform {
    let c = chains.chain(b.chain);
    PlanTransform::Chain(if forward { c.clone() } else { c.inverse() })
}

fn lower_directed_v3(
    rule: &ResolvedRule,
    g: &mut Graph,
    chains: &ChainTable,
    forward: bool,
) -> Result<DirectedRule, LowerError> {
    let (inn, out) = if forward {
        (&rule.left, &rule.right)
    } else {
        (&rule.right, &rule.left)
    };

    // Pattern: input side …
    let mut pattern = Pattern::default();
    for ns in &inn.nodes {
        pattern.nodes.push(PatternNode {
            typ: g.types.intern(&ns.typ),
            value: ns.predicate.clone(),
        });
    }
    for &(a, b) in &inn.links {
        pattern.links.push(Link::directed(a, b));
    }
    // … plus context corrs (references) as direction-free links at the
    // input anchor. The OUTPUT-side node of a reference corr is
    // context: it is NOT created, but matched alongside behind the
    // corr — creation links may anchor at it.
    let mut out_ctx_pattern_pos: BTreeMap<usize, usize> = BTreeMap::new();
    // Same-domain context (same_as): the right node IS a left one.
    // Forward: right output position j = input position k.
    // Backward: left output position k = input position j.
    for (j, ns) in rule.right.nodes.iter().enumerate() {
        if let Some(k) = ns.same_as {
            if forward {
                out_ctx_pattern_pos.insert(j, k);
            } else {
                out_ctx_pattern_pos.insert(k, j);
            }
        }
    }
    // Context-without-corr (context: true) of the OUTPUT side: matched
    // as a pattern node (type + predicate + links), never created.
    // Context nodes of the INPUT side are ordinary pattern nodes
    // anyway.
    for (j, ns) in out.nodes.iter().enumerate() {
        if ns.context && !out_ctx_pattern_pos.contains_key(&j) {
            let pos = pattern.nodes.len();
            pattern.nodes.push(PatternNode {
                typ: g.types.intern(&ns.typ),
                value: ns.predicate.clone(),
            });
            out_ctx_pattern_pos.insert(j, pos);
        }
    }
    // Register reference corrs in the context table BEFORE the output
    // link loop -- otherwise output links hanging off a reference
    // endpoint would silently drop out as a pattern constraint
    // (case-24 finding). Only adds NODES at the end of the pattern, no
    // position shift of the input nodes (μ stable).
    let mut ref_endpoints: BTreeSet<usize> = BTreeSet::new();
    for c in &rule.corrs {
        if c.role == Role::References {
            let (anchor, out_pos) = corr_ends(c, forward);
            let corr_pat = pattern.nodes.len();
            pattern.nodes.push(PatternNode {
                typ: g.types.intern(&c.typ),
                value: None,
            });
            pattern.links.push(Link::context(corr_pat, anchor));
            let ctx_pat = pattern.nodes.len();
            pattern.nodes.push(PatternNode {
                typ: g.types.intern(&out.nodes[out_pos].typ),
                value: None,
            });
            pattern.links.push(Link::context(corr_pat, ctx_pat));
            out_ctx_pattern_pos.insert(out_pos, ctx_pat);
            ref_endpoints.insert(out_pos);
        }
    }
    // Output links BETWEEN context nodes → pattern constraints.
    //
    // EXCEPTION (semantic fork, dry-cleaner evidence): an edge between
    // TWO references endpoints is NOT a precondition, but something to
    // ESTABLISH — two independently referenced existences say nothing
    // about their adjacency, creating the edge is the purpose of the
    // rule. Every edge that touches a context/same_as node remains a
    // precondition: its attachment IS the author's existence
    // statement.
    for &(a, b) in &out.links {
        if ref_endpoints.contains(&a) && ref_endpoints.contains(&b) {
            continue; // Ensure edge: created by the plan (create_links).
        }
        if let (Some(&pa), Some(&pb)) = (out_ctx_pattern_pos.get(&a), out_ctx_pattern_pos.get(&b)) {
            pattern.links.push(Link::directed(pa, pb));
        }
    }
    // Value joins: within-side
    // (input directly; output only if both are context) + cross-side
    // (input ↔ output context).
    //
    // The positions of the INPUT side are at the same time the
    // pattern positions 0..inn.nodes.len() -- so they enter the link
    // unadjusted. Output positions first have to go through
    // `out_ctx_pattern_pos`.
    for &(a, b) in &inn.same_value_links {
        pattern.links.push(Link::same_value(a, b));
    }
    for &(a, b) in &out.same_value_links {
        if let (Some(&pa), Some(&pb)) = (out_ctx_pattern_pos.get(&a), out_ctx_pattern_pos.get(&b)) {
            pattern.links.push(Link::same_value(pa, pb));
        }
    }
    for &(l_pos, r_pos) in &rule.joins {
        // `joins` is declared left→right; in the backward direction
        // the right node is the input and the left one the output.
        let (in_pos, out_pos) = if forward {
            (l_pos, r_pos)
        } else {
            (r_pos, l_pos)
        };
        if let Some(&pb) = out_ctx_pattern_pos.get(&out_pos) {
            pattern.links.push(Link::same_value(in_pos, pb));
        }
        // Join on a CREATED output node: no constraint — the value
        // arises via binding/constant.
    }

    // Creation plan: output side (parent chain via the output side's
    // links: an edge's target hangs off its source node; the root
    // hangs off the anchor's corr or the input anchor).
    // Context positions (reference corrs) are NOT created.
    let mut create_nodes = Vec::new();
    let mut create_links = Vec::new();

    // Establishes corrs: EACH establishes one corr node
    // anchor → corr → endpoint. A rule may have several (stitching),
    // endpoints may be context.
    let establishes: Vec<&ResolvedCorr> = rule
        .corrs
        .iter()
        .filter(|c| c.role == Role::Establishes)
        .collect();
    // Empty chain for the corr's pair identity — it carries no value,
    // only the counterpart's ref.
    let ident_chain = PlanTransform::Chain(Chain::default());
    // (in_anchor, est_out, corr_plan_ix) per establishes corr.
    let mut est_corrs: Vec<(usize, usize, usize)> = Vec::new();
    // Output position → plan index of the corr that establishes it.
    let mut corr_of_out: BTreeMap<usize, usize> = BTreeMap::new();
    for c in &establishes {
        let (in_anchor, est_out) = corr_ends(c, forward);
        // Pair identity (siblings via source refs): if the
        // ESTABLISHED counterpart is itself matched (context/same-
        // domain), its ref enters the corr identity — otherwise
        // multiple matches at the same anchor would collapse.
        let est_matched_pos = out_ctx_pattern_pos.get(&est_out).copied();
        let ix = create_nodes.len();
        create_nodes.push(CreateNode {
            typ: c.typ.clone(),
            parent: Ref::Matched(in_anchor),
            derived: est_matched_pos.map(|p| (p, ident_chain.clone())),
            konst: None,
            derived_dyn: None,
            corr_full_match: false,
            is_corr: true,
            attr_corr: None,
        });
        create_links.push((Ref::Matched(in_anchor), Ref::New(ix)));
        est_corrs.push((in_anchor, est_out, ix));
        corr_of_out.entry(est_out).or_insert(ix);
    }
    let corr_new_ix = est_corrs.first().map(|&(_, _, ix)| ix);

    // Derive the parent per output position from the output links.
    let mut out_parent: Vec<Option<usize>> = vec![None; out.nodes.len()];
    for &(a, b) in &out.links {
        if out_parent[b].is_none() {
            out_parent[b] = Some(a);
        }
    }
    let mut out_new_ix: Vec<Option<usize>> = vec![None; out.nodes.len()];
    let mut first_created: Option<usize> = None;
    for i in 0..out.nodes.len() {
        if out_ctx_pattern_pos.contains_key(&i) {
            continue; // Context — already exists, gets matched.
        }
        // Leaf bindings: created as a derived leaf from the input leaf
        // (invertible chain) — searched over ALL establishes corrs.
        // Welche Korrespondenz stellt dieses Ausgabe-Blatt einem
        // Eingabe-Blatt gegenueber? Der Index wird gebraucht, weil das
        // Paar seine EIGENE Korrespondenz bekommt (siehe unten).
        let hit = establishes.iter().enumerate().find_map(|(ci, c)| {
            c.bindings
                .iter()
                .find(|b| {
                    let (_, dst) = binding_ends(b, forward);
                    matches!(dst, BindingSource::Node(p) if *p == i)
                })
                .map(|b| (ci, b))
        });
        let attr_corr_of = hit.map(|(ci, _)| establishes[ci].typ.clone());
        let hit = hit.map(|(_, b)| b);
        let derived = match hit {
            None => None,
            Some(b) => {
                let (src, _) = binding_ends(b, forward);
                let BindingSource::Node(in_leaf) = src else {
                    return Err(LowerError(format!(
                        "{}: binding on output node {i} mixes a node and a type source",
                        rule.name
                    )));
                };
                let in_leaf = *in_leaf;
                Some((in_leaf, binding_chain(b, chains, forward)))
            }
        };
        // Creation constant: fixed value from the rule.
        let ns = &out.nodes[i];
        let konst = match (&ns.constant, &derived) {
            (None, _) => None,
            (Some(_), Some(_)) => {
                return Err(LowerError(format!(
                    "{}: output node {i} has both a constant and a binding",
                    rule.name
                )))
            }
            (Some(v), None) => Some(v.clone()),
        };
        let ix = create_nodes.len();
        out_new_ix[i] = Some(ix);
        if first_created.is_none() {
            first_created = Some(ix);
        }
        // Identity parent: the corr that establishes THIS position
        // (match-unique), otherwise the created structural parent,
        // otherwise the first corr, otherwise the input anchor.
        // Context parents do NOT qualify as an identity parent
        // (sibling collision).
        let parent = match corr_of_out.get(&i).copied() {
            Some(cix) => Ref::New(cix),
            None => match out_parent[i].and_then(|p| out_new_ix.get(p).copied().flatten()) {
                Some(p) => Ref::New(p),
                None => match corr_new_ix {
                    Some(cix) => Ref::New(cix),
                    None => Ref::Matched(inn.anchor),
                },
            },
        };
        let leaf_ix = create_nodes.len();
        let src_leaf = derived.as_ref().map(|(in_leaf, _)| *in_leaf);
        create_nodes.push(CreateNode {
            typ: ns.typ.clone(),
            parent,
            derived,
            konst,
            derived_dyn: None,
            corr_full_match: false,
            is_corr: false,
            attr_corr: None,
        });
        // Attribut-Korrespondenz: das Blattpaar bekommt eine EIGENE
        // Korrespondenz, nicht eine Kante an die Knoten-Korrespondenz.
        //
        // Was das Regelformat `bindings` nennt, ist im TGG-Sinn ein
        // Attribut-Constraint zwischen zwei Blaettern -- also selbst
        // eine Korrespondenz, nur auf Blatt-Ebene (Sandra 2026-08-12).
        // Sie traegt dieselbe Identitaetsableitung wie jede andere
        // Korrespondenz und faellt fuer sich. Deshalb reisst ein Blatt,
        // das in mehreren Attribut-Korrespondenzen steht, keine davon
        // mit: es gibt keine geteilte Traegerschaft.
        if let (Some(ct), Some(src)) = (attr_corr_of, src_leaf) {
            let cix = create_nodes.len();
            create_nodes.push(CreateNode {
                typ: format!("{ct}_{}", ns.typ),
                attr_corr: None,
                parent: Ref::Matched(src),
                derived: None,
                konst: None,
                derived_dyn: None,
                corr_full_match: false,
                is_corr: true,
            });
            create_links.push((Ref::New(cix), Ref::Matched(src)));
            create_links.push((Ref::New(cix), Ref::New(leaf_ix)));
        }
    }
    // DYNAMIC bindings (leaf type instead of leaf position): one leaf
    // per binding at the ESTABLISHED endpoint of ITS corr, the source
    // is looked up at that corr's input anchor when applying
    // (apply-if-present) — over ALL establishes corrs.
    for (ci, c) in establishes.iter().enumerate() {
        let (in_anchor, est_out, _) = est_corrs[ci];
        // Target parent: created node OR context (attributes are also
        // set on existing ones).
        let est_ref = match out_new_ix.get(est_out).copied().flatten() {
            Some(ix) => Some(Ref::New(ix)),
            None => out_ctx_pattern_pos.get(&est_out).map(|&p| Ref::Matched(p)),
        };
        for b in &c.bindings {
            let (src, dst) = binding_ends(b, forward);
            let (in_attr, out_attr) = match (src, dst) {
                (BindingSource::LeafType(a), BindingSource::LeafType(z)) => (a, z),
                // Static binding — handled above in the creation plan.
                (BindingSource::Node(_), BindingSource::Node(_)) => continue,
                // Mixed: this case has no meaning, and the format
                // assigns it no meaning. Report instead of guessing.
                _ => {
                    return Err(LowerError(format!(
                        "{}: corr {} mixes a node and a type source in one binding",
                        rule.name, c.typ
                    )))
                }
            };
            let Some(est) = est_ref else {
                return Err(LowerError(format!(
                    "{}: dynamic binding — established node neither created nor context",
                    rule.name
                )));
            };
            let t = binding_chain(b, chains, forward);
            let ix = create_nodes.len();
            create_nodes.push(CreateNode {
                typ: out_attr.clone(),
                parent: est,
                derived: None,
                konst: None,
                derived_dyn: Some((in_anchor, in_attr.clone(), t)),
                corr_full_match: false,
                is_corr: false,
                // Der dynamische Fall bekommt seine Blatt-Korrespondenz
                // beim Anwenden, weil die Quelle erst dort feststeht.
                attr_corr: Some(c.typ.clone()),
            });
            create_links.push((est, Ref::New(ix)));
        }
    }
    for &(a, b) in &out.links {
        let r = |i: usize| -> Result<Ref, LowerError> {
            if let Some(ix) = out_new_ix[i] {
                Ok(Ref::New(ix))
            } else if let Some(&pat) = out_ctx_pattern_pos.get(&i) {
                Ok(Ref::Matched(pat))
            } else {
                Err(LowerError(format!(
                    "{}: output link endpoint {i} neither created nor context",
                    rule.name
                )))
            }
        };
        // Even (Matched, Matched) is legitimate: pure edge creation
        // between two already-translated contexts (next chains);
        // duplicate detection runs via creation_exists/connected.
        create_links.push((r(a)?, r(b)?));
    }
    // Corr → ESTABLISHED endpoint (provenance chain), per corr. The
    // corr belongs at the node its establishes names — only there does
    // backward-direction recognition look for it.
    for &(_, est_out, cix) in &est_corrs {
        let target = match out_new_ix.get(est_out).copied().flatten() {
            Some(ix) => Some(Ref::New(ix)),
            None => out_ctx_pattern_pos
                .get(&est_out)
                .map(|&p| Ref::Matched(p))
                .or(first_created.map(Ref::New)),
        };
        if let Some(t) = target {
            create_links.push((Ref::New(cix), t));
        }
    }

    let corr_recognition = establishes
        .iter()
        .map(|c| {
            let (anchor, est_out) = corr_ends(c, forward);
            (c.typ.clone(), anchor, out.nodes[est_out].typ.clone())
        })
        .collect();
    Ok(DirectedRule {
        name: format!("{}{}", rule.name, if forward { "→" } else { "←" }),
        rank: rule.rank,
        direction: if establishes.is_empty() {
            RuleDirection::Undirected
        } else if forward {
            RuleDirection::Forward
        } else {
            RuleDirection::Backward
        },
        pattern,
        create_nodes,
        create_links,
        // Delta routing is derived only from the input anchors of
        // establishing correspondences. References are context and do
        // not select a direction. Once selected, however, the complete
        // directed family stays active so newly created context can
        // enable follow-up rules in the same direction.
        input_types: {
            let mut t: Vec<String> = establishes
                .iter()
                .map(|c| {
                    let (anchor, _) = corr_ends(c, forward);
                    inn.nodes[anchor].typ.clone()
                })
                .collect();
            t.sort();
            t.dedup();
            t
        },
        corr_recognition,
    })
}

/// (Input anchor, output endpoint) of a corr in the run direction.
fn corr_ends(c: &ResolvedCorr, forward: bool) -> (usize, usize) {
    if forward {
        (c.left, c.right)
    } else {
        (c.right, c.left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::matcher::LinkKind;
    use crate::rules::format::RuleFile;
    use crate::rules::validate::validate;

    const MIN: &str = include_str!("../../tests/fixtures/rules/uml_java_min.json");

    /// Both directions of a single-rule file.
    fn both(json: &str) -> [DirectedRule; 2] {
        let f = RuleFile::from_json(json).expect("json");
        let res = validate(&f).expect("validation");
        let mut g = Graph::default();
        lower_rule(&res, 0, &mut g).expect("lowering")
    }

    /// A pattern's value-equality constraints as position pairs, in
    /// creation order.
    fn same_value_pairs(r: &DirectedRule) -> Vec<(usize, usize)> {
        r.pattern
            .links
            .iter()
            .filter(|l| l.kind == LinkKind::SameValue)
            .map(|l| (l.from, l.to))
            .collect()
    }

    /// A within-side value link on the INPUT side: the input side's
    /// positions are at the same time the pattern positions, the link
    /// goes into the pattern unadjusted. In the backward direction the
    /// same side is the output side and both its nodes are created --
    /// there's nothing to check there, the link drops out.
    #[test]
    fn same_value_link_on_the_input_side_becomes_a_constraint() {
        let [fwd, bwd] = both(
            r#"{"format":3,"name":"sv","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},
                {"name":"n1","type":"name"},
                {"name":"n2","type":"name"}],
              "links":[["a","n1"],["a","n2"]],"same_value_links":[["n1","n2"]]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes"}]}]}"#,
        );
        assert_eq!(same_value_pairs(&fwd), vec![(1, 2)]);
        assert_eq!(same_value_pairs(&bwd), Vec::new());
    }

    /// A within-side value link on the OUTPUT side: only if BOTH ends
    /// are context, otherwise the counterpart doesn't exist yet while
    /// matching. The positions run through the context table.
    #[test]
    fn same_value_link_on_the_output_side_only_between_context_nodes() {
        let with_context = r#"{"format":3,"name":"sv","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},{"name":"x","type":"X"}],
              "links":[["a","x"]]},
            "right":{"anchor":"b","nodes":[
                {"name":"b","type":"B"},
                {"name":"c1","type":"name","context":true},
                {"name":"c2","type":"name","context":KTX}],
              "links":[["b","c1"],["b","c2"]],"same_value_links":[["c1","c2"]]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes"}]}]}"#;

        // c2 is context: pattern = 2 input nodes, then c1 at 2, c2 at 3.
        let [fwd, bwd] = both(&with_context.replace("KTX", "true"));
        assert_eq!(same_value_pairs(&fwd), vec![(2, 3)]);
        // Backward: the same side is now the input, positions raw.
        assert_eq!(same_value_pairs(&bwd), vec![(1, 2)]);

        // c2 gets created: forward has no constraint.
        let [fwd, bwd] = both(&with_context.replace("KTX", "false"));
        assert_eq!(same_value_pairs(&fwd), Vec::new());
        assert_eq!(same_value_pairs(&bwd), vec![(1, 2)]);
    }

    /// A cross-side join. Both ends are context, so each direction
    /// carries the link -- with swapped roles: forward the left node
    /// is the input, backward the right one. The positions are
    /// therefore different.
    #[test]
    fn join_applies_in_both_directions_with_swapped_roles() {
        let [fwd, bwd] = both(
            r#"{"format":3,"name":"j","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},
                {"name":"extra","type":"X"},
                {"name":"lname","type":"name","context":true}],
              "links":[["a","extra"],["a","lname"]]},
            "right":{"anchor":"b","nodes":[
                {"name":"b","type":"B"},
                {"name":"rname","type":"name","context":true}],
              "links":[["b","rname"]]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes"}],
            "joins":[["lname","rname"]]}]}"#,
        );
        // Forward: input is lname (position 2 of the left side),
        // output is rname as a context node at the end of the pattern (3).
        assert_eq!(same_value_pairs(&fwd), vec![(2, 3)]);
        // Backward: input is rname (position 1 of the right side),
        // output is lname as a context node at the end of the pattern (2).
        assert_eq!(same_value_pairs(&bwd), vec![(1, 2)]);
    }

    /// Shows the special case unchanged: if the join hangs off a
    /// node that the run direction CREATES, there's no constraint --
    /// the value only arises there.
    #[test]
    fn join_on_created_output_node_is_not_a_constraint() {
        let [fwd, bwd] = both(
            r#"{"format":3,"name":"j","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},
                {"name":"lname","type":"name","context":true}],
              "links":[["a","lname"]]},
            "right":{"anchor":"b","nodes":[
                {"name":"b","type":"B"},
                {"name":"rname","type":"name"}],
              "links":[["b","rname"]]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes"}],
            "joins":[["lname","rname"]]}]}"#,
        );
        // Forward creates rname: no constraint.
        assert_eq!(same_value_pairs(&fwd), Vec::new());
        // Backward, rname is the input (1) and lname is context (2).
        assert_eq!(same_value_pairs(&bwd), vec![(1, 2)]);
    }

    #[test]
    fn lowers_in_both_directions() {
        let f = RuleFile::from_json(MIN).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let [fwd, bwd] = lower_rule(&res, 0, &mut g).unwrap();

        assert_eq!(fwd.name, "R_Class→");
        assert_eq!(bwd.name, "R_Class←");
        assert_eq!(fwd.rank, 40);
        assert_eq!(fwd.direction, RuleDirection::Forward);
        assert_eq!(bwd.direction, RuleDirection::Backward);
        assert_eq!(fwd.input_types, vec!["Class"]);
        assert_eq!(bwd.input_types, vec!["JavaClass"]);
    }

    #[test]
    fn establishes_corr_lands_in_recognition() {
        let f = RuleFile::from_json(MIN).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let [fwd, _] = lower_rule(&res, 0, &mut g).unwrap();
        assert_eq!(fwd.corr_recognition.len(), 1);
        assert_eq!(fwd.corr_recognition[0].0, "CorrClass");
        assert_eq!(fwd.corr_recognition[0].2, "JavaClass");
    }

    #[test]
    fn creation_plan_hangs_off_the_anchor() {
        let f = RuleFile::from_json(MIN).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let [fwd, _] = lower_rule(&res, 0, &mut g).unwrap();
        assert!(
            !fwd.create_nodes.is_empty(),
            "forward must create JavaClass and the name leaf"
        );
    }
}
