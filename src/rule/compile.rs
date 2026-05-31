//! Rule compiler — intermediate representation between `RuleSpec`
//! (declarative, bidirectional) and the concrete engine rule
//! instantiation.
//!
//! The compiler does two things that should NOT be mixed together via
//! closure magic:
//!
//! 1. **Static analysis**: which pattern nodes are shared anchors
//!    (present on L and R with the same ID and the same kind)? Which
//!    CorrespondenceLinks are context (all involved nodes are already
//!    bound) vs. to be created (at least one side is new)?
//! 2. **Production plan**: which nodes, edges, corrs must be created
//!    on match; which attribute propagations apply.
//!
//! The resulting [`CompiledRuleSpec`] is a pure data structure, free
//! of graph-topology decisions. The final mapping to `Box<dyn Rule>`
//! (with concrete corr-layout semantics) is the job of a follow-up
//! step — see the paper chapter "From abstract rule spec to engine
//! operationalization".

use std::collections::{HashMap, HashSet};
use thiserror::Error;

use super::spec::{
    AttrBindingSpec, AttrMatcherSpec, AttrTransform, CorrRole, CorrespondenceLinkSpec, PatternSpec,
    RuleSpec, UnknownTransform,
};

// ══════════════════════════════════════════════════════════════════════
// CompiledRuleSpec — the result of compilation
// ══════════════════════════════════════════════════════════════════════

/// Statically analyzed image of a [`RuleSpec`].
#[derive(Debug, Clone)]
pub struct CompiledRuleSpec {
    pub name: String,
    pub rank: i32,
    pub documentation: Option<String>,
    pub match_plan: MatchPlan,
    pub creation_plan: CreationPlan,
    pub propagation_plan: Vec<AttrPropagation>,
    /// Compiled NACs (M2). Checked by the matcher after the
    /// main match.
    pub nacs: Vec<CompiledNac>,
    /// "Trigger kinds" of this directed rule = the kinds of the L-side
    /// anchors of the correspondences this rule **establishes**. A rule
    /// is active for a delta when this set intersects the delta kinds —
    /// this derives the cascade direction from the delta (anti-ping-pong,
    /// see spec §5). Deriving it from the established-correspondence
    /// anchors (rather than an `l_pattern`-minus-`r_pattern` set
    /// difference) is what lets a `{JavaField}` delta activate only the
    /// attribute rule's backward direction, not the getter/setter rules.
    /// Sorted and deduplicated for deterministic comparisons.
    pub input_domain_kinds: Vec<String>,
}

/// Compiled Negative Application Condition.
#[derive(Debug, Clone)]
pub struct CompiledNac {
    pub name: String,
    /// All NodePatterns in the NAC — in canonical order.
    pub nodes: Vec<MatchNode>,
    /// Edge constraints in the NAC.
    pub edges: Vec<MatchEdge>,
    /// Attribute constraints (if any).
    pub constraints: Vec<MatchConstraint>,
    /// NodePattern IDs bound to the L match.
    /// Fixed via var name from the main match during the NAC
    /// check.
    pub shared_with_l: Vec<String>,
}

/// Match part: what the matcher must find for the rule to apply.
#[derive(Debug, Clone, Default)]
pub struct MatchPlan {
    /// Nodes the matcher finds — in canonical order
    /// (lPattern first, then rPattern, duplicates removed via
    /// shared anchor).
    pub nodes: Vec<MatchNode>,
    /// Edge constraints between matched nodes.
    pub edges: Vec<MatchEdge>,
    /// Literal attribute constraints.
    pub constraints: Vec<MatchConstraint>,
    /// Correspondence nodes required as context (via existing corr
    /// edges in the graph).
    pub context_correspondences: Vec<CorrespondenceLinkSpec>,
}

/// Creation part: what the rule produces.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreationPlan {
    /// R-pattern nodes not already bound in the match
    /// (shared anchors and context R nodes excluded).
    pub nodes_to_create: Vec<MatchNode>,
    /// R-pattern edges whose endpoints are new or context-
    /// bound.
    pub edges_to_create: Vec<MatchEdge>,
    /// Correspondence links this rule newly establishes — distinct
    /// from `context_correspondences` in the MatchPlan.
    pub correspondences_to_create: Vec<CorrespondenceLinkSpec>,
    /// R-pattern literal constraints on **context nodes** (shared
    /// anchor or R-only-with-context-corr): the attribute is to be
    /// changed on an already existing node. Materialized as
    /// `Op::SetAttr` on rule application.
    ///
    /// rc6 (B6): up to rc5, R literals landed on ALL R nodes in
    /// this list — including R-only creation nodes. That caused
    /// oscillation when two rules tried to produce structurally
    /// identical creation nodes (same corr parent, same
    /// attribute_bindings → same GhostId hash) but with different
    /// R-literal values for the same attribute — the SetAttrs
    /// overwrote each other on every iteration. Counterpart to the
    /// edge identity behavior (B4).
    pub attrs_to_set: Vec<AttrToSet>,
    /// R-pattern literal constraints on **R-only creation nodes**:
    /// the attribute is *part of the identity* of the newly created
    /// node and flows into the GhostId hash via `collect_r_attrs`
    /// (see module [`mod@crate::rule::instantiate`]). This way two
    /// rules with different literal values produce *two distinct*
    /// creation nodes instead of overwriting each other on the same
    /// node.
    pub creation_attrs: Vec<CreationAttr>,
}

/// SetAttr intent from a rule: sets an attribute on a (context) node
/// bound in the match to a literal value. Materialized to `Op::SetAttr`
/// in module [`mod@crate::rule::instantiate`].
#[derive(Debug, Clone, PartialEq)]
pub struct AttrToSet {
    /// NodePattern ID — must be bound in the MatchPlan.
    pub node_var: String,
    /// Attribute name.
    pub attr_name: String,
    /// Target value.
    pub value: String,
}

/// Identity attribute for an R-only creation node: derived from an
/// R-pattern literal constraint, flows into the ghost ID of the
/// newly created node. Prevents the rc5 oscillation between rules
/// that would produce structurally identical creation nodes with
/// different literal values.
#[derive(Debug, Clone, PartialEq)]
pub struct CreationAttr {
    /// NodePattern ID of the creation node — must be in
    /// `creation_plan.nodes_to_create`.
    pub node_var: String,
    /// Attribute name.
    pub attr_name: String,
    /// Literal value.
    pub value: String,
}

/// Propagation plan: which attributes go where with which transformation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrPropagation {
    pub source_node_var: String,
    pub source_attr: String,
    pub target_node_var: String,
    pub target_attr: String,
    pub transform: AttrTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchNode {
    pub var: String,
    pub kind: String,
    /// Which side brought this node into the pattern.
    pub origin: NodeOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeOrigin {
    /// Only in the L pattern.
    LOnly,
    /// Only in the R pattern.
    ROnly,
    /// In both with identical ID and kind — shared anchor.
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEdge {
    pub source_var: String,
    pub target_var: String,
    pub kind: String,
    pub side: EdgeSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSide {
    L,
    R,
}

#[derive(Debug, Clone)]
pub struct MatchConstraint {
    pub node_var: String,
    pub attr_name: String,
    pub predicate: crate::engine::AttrPredicate,
}

/// Compiles an `AttrMatcherSpec` to an engine-native
/// `AttrPredicate`. Regex syntax errors are surfaced as
/// `CompileError::InvalidRegex`.
pub fn compile_matcher(
    spec: &super::spec::AttrMatcherSpec,
) -> Result<crate::engine::AttrPredicate, CompileError> {
    use super::spec::AttrMatcherSpec;
    Ok(match spec {
        AttrMatcherSpec::Literal { value } => crate::engine::AttrPredicate::Equals(value.clone()),
        AttrMatcherSpec::Regex { pattern } => {
            let re = regex::Regex::new(pattern).map_err(|e| CompileError::InvalidRegex {
                pattern: pattern.clone(),
                reason: e.to_string(),
            })?;
            crate::engine::AttrPredicate::Regex(re)
        }
        AttrMatcherSpec::Prefix { prefix } => crate::engine::AttrPredicate::Prefix(prefix.clone()),
        AttrMatcherSpec::Suffix { suffix } => crate::engine::AttrPredicate::Suffix(suffix.clone()),
        AttrMatcherSpec::NumericRange { min, max } => crate::engine::AttrPredicate::NumericRange {
            min: *min,
            max: *max,
        },
    })
}

// ══════════════════════════════════════════════════════════════════════
// Errors
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Rule '{rule}' hat weder l_pattern noch r_pattern")]
    EmptyRule { rule: String },

    #[error(
        "Rule '{rule}': Shared-Anchor '{var}' hat inkonsistenten Kind \
             ('{l_kind}' auf L, '{r_kind}' auf R)"
    )]
    SharedAnchorKindMismatch {
        rule: String,
        var: String,
        l_kind: String,
        r_kind: String,
    },

    #[error("Rule '{rule}': Kante referenziert unbekannten NodePattern '{var}'")]
    DanglingEdge { rule: String, var: String },

    #[error(
        "Rule '{rule}': CorrespondenceLink referenziert unbekannten \
             NodePattern (l_node_id='{l}', r_node_id='{r}')"
    )]
    DanglingCorrespondence { rule: String, l: String, r: String },

    #[error(
        "Rule '{rule}': created node '{var}' (kind '{kind}') has NO \
         correspondence. In seesaw every created node carries a \
         correspondence (otherwise `instantiate` never materializes it and \
         it is unreachable via corrL/corrR on deletion). Give the node an \
         Establishes correspondence (several target nodes may correspond to \
         the same source element) instead of a correspondence-less \
         'virtual' node."
    )]
    CreatedNodeWithoutCorrespondence {
        rule: String,
        var: String,
        kind: String,
    },

    #[error("Rule '{rule}': unbekannte Transformation '{tag}'")]
    InvalidTransform { rule: String, tag: String },

    #[error("Ungültiger Regex '{pattern}': {reason}")]
    InvalidRegex { pattern: String, reason: String },

    #[error("Rule '{rule}' NAC '{nac}': shared_with_l-Node '{var}' existiert nicht im l_pattern")]
    NacSharedAnchorUnknown {
        rule: String,
        nac: String,
        var: String,
    },

    #[error(
        "Rule '{rule}': R-Pattern-Knoten '{node}' Attribut '{attr}' hat \
         einen nicht-Literal-Matcher (Regex/Prefix/Suffix/NumericRange). \
         R-Constraints werden in attrs_to_set uebersetzt; das geht nur \
         mit Literal-Werten, weil 'Wert auf Regex setzen' semantisch \
         undefiniert ist."
    )]
    NonLiteralRAttrUnsupported {
        rule: String,
        node: String,
        attr: String,
    },
}

impl CompileError {
    fn from_transform(rule: &str, err: UnknownTransform) -> Self {
        CompileError::InvalidTransform {
            rule: rule.to_string(),
            tag: err.0,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Compiler
// ══════════════════════════════════════════════════════════════════════

/// Decides whether a correspondence link references an **existing**
/// correspondence (context) or establishes a **new** one (creation).
/// rc7: role-aware with rc6 fallback.
///
/// - `Some(References)` → context (both endpoints matched, nothing
///   created; span bindings serve only the GhostId identity).
/// - `Some(Establishes)` → creation (output-side endpoint created).
/// - `None` → rc6 behavior: empty `attribute_bindings` ⟹ References.
pub(crate) fn corr_is_reference(cl: &CorrespondenceLinkSpec) -> bool {
    match cl.role {
        Some(CorrRole::References) => true,
        Some(CorrRole::Establishes) => false,
        None => cl.attribute_bindings.is_empty(),
    }
}

/// Transformation direction for the bidirectional lowering (rc7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Lhs→Rhs (unchanged).
    Fwd,
    /// Rhs→Lhs: l_pattern↔r_pattern swapped, corr endpoints swapped,
    /// bindings mirrored.
    Bwd,
}

/// Returns the rule view directed by `dir`.
///
/// `Fwd` = unchanged. `Bwd` = l_pattern↔r_pattern + corr endpoints
/// swapped + bindings mirrored (l_attr↔r_attr). `role` and
/// `transformation` stay unchanged (direction-neutral; the engine
/// picks the transform inverse at propagation time).
pub(crate) fn directed_spec(spec: &RuleSpec, dir: Direction) -> RuleSpec {
    if dir == Direction::Fwd {
        return spec.clone();
    }
    let swap_corr = |cl: &CorrespondenceLinkSpec| CorrespondenceLinkSpec {
        l_node_id: cl.r_node_id.clone(),
        r_node_id: cl.l_node_id.clone(),
        kind: cl.kind.clone(),
        role: cl.role,
        attribute_bindings: cl
            .attribute_bindings
            .iter()
            .map(|b| AttrBindingSpec {
                l_attr_name: b.r_attr_name.clone(),
                r_attr_name: b.l_attr_name.clone(),
                transformation: b.transformation.clone(),
            })
            .collect(),
    };
    RuleSpec {
        name: spec.name.clone(),
        rank: spec.rank,
        documentation: spec.documentation.clone(),
        l_pattern: spec.r_pattern.clone(),
        r_pattern: spec.l_pattern.clone(),
        correspondence_links: spec.correspondence_links.iter().map(swap_corr).collect(),
        nacs: spec.nacs.clone(),
    }
}

/// Lowers a declarative rule into **both** directed
/// [`CompiledRuleSpec`] (rc7). IDs: `"<name>→"` (Fwd), `"<name>←"`
/// (Bwd) — unique for the cascade-origin lookup. The
/// context-vs-creation role and the span anchor are derived per
/// direction from `role` + bindings (see `directed_spec` +
/// `corr_is_reference`, both crate-internal).
pub fn compile_bidirectional(spec: &RuleSpec) -> Result<Vec<CompiledRuleSpec>, CompileError> {
    let mut out = Vec::with_capacity(2);
    for (dir, suffix) in [(Direction::Fwd, "\u{2192}"), (Direction::Bwd, "\u{2190}")] {
        let mut compiled = compile(&directed_spec(spec, dir))?;
        compiled.name = format!("{}{}", spec.name, suffix);
        out.push(compiled);
    }
    Ok(out)
}

/// Compiles a single [`RuleSpec`] into a [`CompiledRuleSpec`].
pub fn compile(spec: &RuleSpec) -> Result<CompiledRuleSpec, CompileError> {
    // ── 1. Fetch patterns (empty ok, but not both empty) ─────────────
    let empty = PatternSpec::default();
    let l_pat = spec.l_pattern.as_ref().unwrap_or(&empty);
    let r_pat = spec.r_pattern.as_ref().unwrap_or(&empty);
    if l_pat.nodes.is_empty() && r_pat.nodes.is_empty() {
        return Err(CompileError::EmptyRule {
            rule: spec.name.clone(),
        });
    }

    // ── 2. Index NodePatterns, determine shared anchors ──────────────
    let mut l_by_id: HashMap<&str, &str> = HashMap::new();
    for n in &l_pat.nodes {
        l_by_id.insert(&n.id, &n.kind);
    }
    let mut r_by_id: HashMap<&str, &str> = HashMap::new();
    for n in &r_pat.nodes {
        r_by_id.insert(&n.id, &n.kind);
    }

    let mut match_nodes: Vec<MatchNode> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // L-pattern nodes: LOnly or Shared (if also in R)
    for n in &l_pat.nodes {
        let origin = match r_by_id.get(n.id.as_str()) {
            None => NodeOrigin::LOnly,
            Some(r_kind) => {
                if *r_kind != n.kind.as_str() {
                    return Err(CompileError::SharedAnchorKindMismatch {
                        rule: spec.name.clone(),
                        var: n.id.clone(),
                        l_kind: n.kind.clone(),
                        r_kind: r_kind.to_string(),
                    });
                }
                NodeOrigin::Shared
            }
        };
        if seen.insert(n.id.clone()) {
            match_nodes.push(MatchNode {
                var: n.id.clone(),
                kind: n.kind.clone(),
                origin,
            });
        }
    }
    // R-pattern nodes not already in L → either match part (if bound
    // via a context corr to an existing graph node) or creation part
    // (if they only come into being via a new corr with this rule).
    //
    // Context corr = CorrespondenceLink without attribute_bindings.
    let r_only_in_context: HashSet<&str> = spec
        .correspondence_links
        .iter()
        .filter(|cl| corr_is_reference(cl))
        .map(|cl| cl.r_node_id.as_str())
        .collect();

    let mut nodes_to_create: Vec<MatchNode> = Vec::new();
    for n in &r_pat.nodes {
        if l_by_id.contains_key(n.id.as_str()) {
            continue; // already in the match as Shared
        }
        let is_context_r = r_only_in_context.contains(n.id.as_str());
        let mn = MatchNode {
            var: n.id.clone(),
            kind: n.kind.clone(),
            origin: NodeOrigin::ROnly,
        };
        if is_context_r {
            // Context R: must exist in the graph — goes into the match.
            if seen.insert(n.id.clone()) {
                match_nodes.push(mn);
            }
        } else {
            // Creation R: does not belong in the match pattern, otherwise
            // the rule would never fire (the matcher cannot find the
            // node before rule application).
            nodes_to_create.push(mn);
        }
    }

    // ── 3. Edges: check referential integrity ────────────────────────
    // All known vars = match vars + to-be-created vars.
    let mut all_known: HashSet<String> = match_nodes.iter().map(|n| n.var.clone()).collect();
    for n in &nodes_to_create {
        all_known.insert(n.var.clone());
    }

    let _match_var: HashSet<String> = match_nodes.iter().map(|n| n.var.clone()).collect();

    let mut match_edges: Vec<MatchEdge> = Vec::new();
    let mut edges_to_create: Vec<MatchEdge> = Vec::new();
    for e in &l_pat.edges {
        check_node_ref(spec, &all_known, &e.source_node_id)?;
        check_node_ref(spec, &all_known, &e.target_node_id)?;
        match_edges.push(MatchEdge {
            source_var: e.source_node_id.clone(),
            target_var: e.target_node_id.clone(),
            kind: e.kind.clone(),
            side: EdgeSide::L,
        });
    }
    for e in &r_pat.edges {
        check_node_ref(spec, &all_known, &e.source_node_id)?;
        check_node_ref(spec, &all_known, &e.target_node_id)?;
        let edge = MatchEdge {
            source_var: e.source_node_id.clone(),
            target_var: e.target_node_id.clone(),
            kind: e.kind.clone(),
            side: EdgeSide::R,
        };
        // R-edge classification:
        // If the edge appears identically in the L pattern, it is
        // already counted as an L-match edge → skip. Every other R
        // edge is created (edges_to_create) — including one between
        // two context nodes. The op-granular duplicate check
        // (is_duplicate with .all()) makes a second, purely repeating
        // application cleanly idempotent instead of blocking the rule
        // — so the earlier R-context-edge special case (cf1c6c4) is
        // obsolete.
        let also_in_l = l_pat.edges.iter().any(|le| {
            le.source_node_id == e.source_node_id
                && le.target_node_id == e.target_node_id
                && le.kind == e.kind
        });
        if also_in_l {
            continue;
        }
        edges_to_create.push(edge);
    }

    // ── 4. Correspondence links: context vs. new ─────────────────────
    let mut context_corrs: Vec<CorrespondenceLinkSpec> = Vec::new();
    let mut corrs_to_create: Vec<CorrespondenceLinkSpec> = Vec::new();
    for cl in &spec.correspondence_links {
        let known_l = all_known.contains(&cl.l_node_id);
        let known_r = all_known.contains(&cl.r_node_id);
        if !known_l || !known_r {
            return Err(CompileError::DanglingCorrespondence {
                rule: spec.name.clone(),
                l: cl.l_node_id.clone(),
                r: cl.r_node_id.clone(),
            });
        }
        // rc7: role decides context-vs-creation (role-aware with rc6
        // fallback via corr_is_reference). References = references an
        // existing correspondence (context), Establishes = establishes
        // a new bijective synchronization (materialized). For References,
        // span bindings serve only the GhostId identity, not the
        // classification.
        if corr_is_reference(cl) {
            context_corrs.push(cl.clone());
        } else {
            corrs_to_create.push(cl.clone());
        }
    }

    // ── 4b. Contract: every created node carries a correspondence ────
    // `instantiate` only materializes a `nodes_to_create` entry when it is
    // the `r_node_id` target of an Establishes correspondence (corr-rooted).
    // A correspondence-less R-only node would be silently dropped
    // (plan-vs-production mismatch) and would be unreachable by `corrL`/
    // `corrR` on deletion. Rather than emit such a "virtual" node, reject
    // here: the consumer must give every created node a correspondence (a
    // fan-out correspondence onto the same source element is allowed).
    let established_targets: HashSet<&str> = corrs_to_create
        .iter()
        .map(|cl| cl.r_node_id.as_str())
        .collect();
    for n in &nodes_to_create {
        if !established_targets.contains(n.var.as_str()) {
            return Err(CompileError::CreatedNodeWithoutCorrespondence {
                rule: spec.name.clone(),
                var: n.var.clone(),
                kind: n.kind.clone(),
            });
        }
    }

    // ── 5. Derive propagations from AttrBindings ─────────────────────
    // For every newly established corr link, a propagation L→R (and
    // the reverse direction) is planned per AttrBinding. The engine
    // decides the concrete direction (which one when) at match time.
    let mut propagations: Vec<AttrPropagation> = Vec::new();
    for cl in &corrs_to_create {
        for b in &cl.attribute_bindings {
            let transform = AttrTransform::parse(b.transformation.as_deref())
                .map_err(|e| CompileError::from_transform(&spec.name, e))?;
            propagations.push(AttrPropagation {
                source_node_var: cl.l_node_id.clone(),
                source_attr: b.l_attr_name.clone(),
                target_node_var: cl.r_node_id.clone(),
                target_attr: b.r_attr_name.clone(),
                transform,
            });
        }
    }

    // ── 6. Classify attribute constraints ────────────────────────────
    // L constraints are always match constraints (we match the
    // initial state).
    let mut constraints: Vec<MatchConstraint> = Vec::new();
    for n in &l_pat.nodes {
        for c in &n.constraints {
            let predicate = compile_matcher(&c.matcher)?;
            constraints.push(MatchConstraint {
                node_var: n.id.clone(),
                attr_name: c.name.clone(),
                predicate,
            });
        }
    }
    // R constraints are classified:
    //   1. Identical to an L constraint on the same node/attribute
    //      → redundant, drop (the L constraint covers the match).
    //   2. On an **R-only creation node** (listed in
    //      `nodes_to_create`) → `creation_attrs`. The literal is an
    //      identity attribute and flows into the ghost hash, NOT as
    //      a SetAttr op. Prevents the rc5 oscillation between rules
    //      that would produce structurally identical creation nodes
    //      with different literal values.
    //   3. On a **context node** (shared anchor or R-only-with-
    //      context-corr) → `attrs_to_set` (B5/rc4 semantics:
    //      `Op::SetAttr` mutates the attribute on the existing
    //      node).
    //   4. Non-literal matcher (Regex/Prefix/Suffix/NumericRange)
    //      → CompileError, because "setting a value to a regex" is
    //      semantically undefined.
    let creation_var_set: HashSet<&str> =
        nodes_to_create.iter().map(|mn| mn.var.as_str()).collect();
    let mut attrs_to_set: Vec<AttrToSet> = Vec::new();
    let mut creation_attrs: Vec<CreationAttr> = Vec::new();
    for n in &r_pat.nodes {
        let l_node = l_pat.nodes.iter().find(|ln| ln.id == n.id);
        let is_creation = creation_var_set.contains(n.id.as_str());
        for c in &n.constraints {
            let l_same_attr =
                l_node.and_then(|ln| ln.constraints.iter().find(|lc| lc.name == c.name));
            if let Some(lc) = l_same_attr {
                if lc.matcher == c.matcher {
                    continue; // identical → L covers it
                }
            }
            match &c.matcher {
                AttrMatcherSpec::Literal { value } => {
                    if is_creation {
                        creation_attrs.push(CreationAttr {
                            node_var: n.id.clone(),
                            attr_name: c.name.clone(),
                            value: value.clone(),
                        });
                    } else {
                        attrs_to_set.push(AttrToSet {
                            node_var: n.id.clone(),
                            attr_name: c.name.clone(),
                            value: value.clone(),
                        });
                    }
                }
                _ => {
                    return Err(CompileError::NonLiteralRAttrUnsupported {
                        rule: spec.name.clone(),
                        node: n.id.clone(),
                        attr: c.name.clone(),
                    });
                }
            }
        }
    }

    // Trigger kinds (computed before `corrs_to_create` is moved into the
    // struct below): the kinds of the L-side anchors of the correspondences
    // this rule establishes. A delta of one of these kinds is what
    // activates this (directed) rule — see the `input_domain_kinds` field.
    // Deriving it from the established-correspondence anchors (rather than
    // an L-minus-R set difference) is what lets a `{JavaField}` delta
    // activate only the attribute rule's backward direction, not the
    // getter/setter rules.
    let input_domain_kinds: Vec<String> = {
        let l_kind_of: std::collections::HashMap<&str, &str> = l_pat
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.kind.as_str()))
            .collect();
        let mut v: Vec<String> = corrs_to_create
            .iter()
            .filter_map(|cc| l_kind_of.get(cc.l_node_id.as_str()).map(|k| k.to_string()))
            .collect();
        v.sort();
        v.dedup();
        v
    };

    Ok(CompiledRuleSpec {
        name: spec.name.clone(),
        rank: spec.rank,
        documentation: spec.documentation.clone(),
        match_plan: MatchPlan {
            nodes: match_nodes,
            edges: match_edges,
            constraints,
            context_correspondences: context_corrs,
        },
        creation_plan: CreationPlan {
            nodes_to_create,
            edges_to_create,
            correspondences_to_create: corrs_to_create,
            attrs_to_set,
            creation_attrs,
        },
        propagation_plan: propagations,
        nacs: compile_nacs(spec, l_pat)?,
        input_domain_kinds,
    })
}

/// Compiles all NACs of a rule.
fn compile_nacs(spec: &RuleSpec, l_pat: &PatternSpec) -> Result<Vec<CompiledNac>, CompileError> {
    let l_vars: HashSet<&str> = l_pat.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut compiled = Vec::with_capacity(spec.nacs.len());
    for nac in &spec.nacs {
        // validate shared_with_l
        for var in &nac.shared_with_l {
            if !l_vars.contains(var.as_str()) {
                return Err(CompileError::NacSharedAnchorUnknown {
                    rule: spec.name.clone(),
                    nac: nac.name.clone(),
                    var: var.clone(),
                });
            }
        }
        // Nodes
        let mut nodes = Vec::with_capacity(nac.nodes.len());
        let mut constraints = Vec::new();
        for n in &nac.nodes {
            nodes.push(MatchNode {
                var: n.id.clone(),
                kind: n.kind.clone(),
                origin: NodeOrigin::LOnly,
            });
            for c in &n.constraints {
                constraints.push(MatchConstraint {
                    node_var: n.id.clone(),
                    attr_name: c.name.clone(),
                    predicate: compile_matcher(&c.matcher)?,
                });
            }
        }
        // Edges
        let edges = nac
            .edges
            .iter()
            .map(|e| MatchEdge {
                source_var: e.source_node_id.clone(),
                target_var: e.target_node_id.clone(),
                kind: e.kind.clone(),
                side: EdgeSide::L,
            })
            .collect();
        compiled.push(CompiledNac {
            name: nac.name.clone(),
            nodes,
            edges,
            constraints,
            shared_with_l: nac.shared_with_l.clone(),
        });
    }
    Ok(compiled)
}

fn check_node_ref(spec: &RuleSpec, known: &HashSet<String>, var: &str) -> Result<(), CompileError> {
    if known.contains(var) {
        Ok(())
    } else {
        Err(CompileError::DanglingEdge {
            rule: spec.name.clone(),
            var: var.to_string(),
        })
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::spec::parse_ruleset;

    const DEMO_FIXTURE: &str = include_str!("../../tests/fixtures/demo-ruleset.json");

    #[test]
    fn corr_is_reference_role_aware_with_rc6_fallback() {
        use crate::rule::spec::{AttrBindingSpec, CorrRole, CorrespondenceLinkSpec};
        let mk = |role: Option<CorrRole>, has_binding: bool| CorrespondenceLinkSpec {
            l_node_id: "c".into(),
            r_node_id: "jc".into(),
            kind: None,
            attribute_bindings: if has_binding {
                vec![AttrBindingSpec {
                    l_attr_name: "name".into(),
                    r_attr_name: "name".into(),
                    transformation: None,
                }]
            } else {
                vec![]
            },
            role,
        };
        // explicit
        assert!(corr_is_reference(&mk(Some(CorrRole::References), true)));
        assert!(!corr_is_reference(&mk(Some(CorrRole::Establishes), false)));
        // rc6 fallback (role = None)
        assert!(corr_is_reference(&mk(None, false))); // empty bindings → context
        assert!(!corr_is_reference(&mk(None, true))); // bindings → creation
    }

    fn demo_rule(name: &str) -> RuleSpec {
        let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
        rs.rules.into_iter().find(|r| r.name == name).unwrap()
    }

    /// A creation block whose R-side node (`skel`) has no correspondence is
    /// never materialized by `instantiate` (plan-vs-production mismatch) →
    /// `compile` rejects it.
    #[test]
    fn compile_rejects_created_node_without_correspondence() {
        let json = r#"{"rules":[{
            "name":"R_Skel","rank":10,
            "l_pattern":{"nodes":[{"id":"c","kind":"Class","constraints":[]}],"edges":[]},
            "r_pattern":{"nodes":[{"id":"jc","kind":"JavaClass","constraints":[]},
                                  {"id":"skel","kind":"Sequence","constraints":[]}],
                         "edges":[{"kind":"hasSeq","source_node_id":"jc","target_node_id":"skel"}]},
            "correspondence_links":[
                {"l_node_id":"c","r_node_id":"jc","role":"Establishes",
                 "attribute_bindings":[{"l_attr_name":"name","r_attr_name":"name"}]}
            ]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        assert!(
            matches!(&err,
                CompileError::CreatedNodeWithoutCorrespondence { var, .. } if var == "skel"),
            "expected rejection of the correspondence-less node 'skel', was: {err:?}"
        );
    }

    /// Counter-check: the demo rules are conformant (every created node
    /// carries a correspondence, incl. getter/setter via a fan-out corr on
    /// 'a') → bidirectional compilation still succeeds.
    #[test]
    fn compile_accepts_demo_rules_every_created_node_has_correspondence() {
        for name in ["R_Class", "R_Attr", "R_Getter", "R_Setter"] {
            let rule = demo_rule(name);
            compile_bidirectional(&rule)
                .unwrap_or_else(|e| panic!("demo rule {name} must compile: {e:?}"));
        }
    }

    #[test]
    fn input_domain_kinds_discriminate_direction() {
        let json = r#"{"rules":[{
            "name":"R_Class","rank":40,
            "l_pattern":{"nodes":[{"id":"m","kind":"Model","constraints":[]},
                                  {"id":"c","kind":"Class","constraints":[]}],
                         "edges":[{"kind":"classes","source_node_id":"m","target_node_id":"c"}]},
            "r_pattern":{"nodes":[{"id":"m","kind":"Model","constraints":[]},
                                  {"id":"jc","kind":"JavaClass","constraints":[]}],
                         "edges":[{"kind":"javaClasses","source_node_id":"m","target_node_id":"jc"}]},
            "correspondence_links":[{"l_node_id":"c","r_node_id":"jc","kind":"CorrClass","role":"Establishes",
                "attribute_bindings":[{"l_attr_name":"name","r_attr_name":"name","transformation":"identity"}]}]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let rules = compile_bidirectional(&rs.rules[0]).unwrap();
        let fwd = rules.iter().find(|r| r.name.ends_with('\u{2192}')).unwrap();
        let bwd = rules.iter().find(|r| r.name.ends_with('\u{2190}')).unwrap();
        // Shared anchor "Model" is dropped; only the discriminating
        // input kind remains.
        assert_eq!(fwd.input_domain_kinds, vec!["Class".to_string()]);
        assert_eq!(bwd.input_domain_kinds, vec!["JavaClass".to_string()]);
    }

    #[test]
    fn compile_bidirectional_yields_two_directed_rules_with_span_anchor() {
        let json = r#"{"rules":[{
            "name":"R_Attr","rank":30,
            "l_pattern":{"nodes":[{"id":"c","kind":"Class","constraints":[]},
                                  {"id":"a","kind":"Attribute","constraints":[]}],
                         "edges":[{"kind":"attributes","source_node_id":"c","target_node_id":"a"}]},
            "r_pattern":{"nodes":[{"id":"jc","kind":"JavaClass","constraints":[]},
                                  {"id":"jf","kind":"JavaField","constraints":[]}],
                         "edges":[{"kind":"hasField","source_node_id":"jc","target_node_id":"jf"}]},
            "correspondence_links":[
                {"l_node_id":"c","r_node_id":"jc","role":"References",
                 "attribute_bindings":[{"l_attr_name":"name","r_attr_name":"name"}]},
                {"l_node_id":"a","r_node_id":"jf","role":"Establishes",
                 "attribute_bindings":[{"l_attr_name":"name","r_attr_name":"name"}]}
            ]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let rules = compile_bidirectional(&rs.rules[0]).unwrap();
        assert_eq!(rules.len(), 2, "eine Regel → Fwd + Bwd");

        let fwd = rules.iter().find(|r| r.name.ends_with('\u{2192}')).unwrap();
        let bwd = rules.iter().find(|r| r.name.ends_with('\u{2190}')).unwrap();

        // Fwd: jf (Establishes) created, jc (References) context
        assert!(fwd
            .creation_plan
            .nodes_to_create
            .iter()
            .any(|n| n.var == "jf"));
        assert!(!fwd
            .creation_plan
            .nodes_to_create
            .iter()
            .any(|n| n.var == "jc"));
        // Bwd: a (Establishes endpoint, now output) created, c (References) context
        assert!(bwd
            .creation_plan
            .nodes_to_create
            .iter()
            .any(|n| n.var == "a"));
        assert!(!bwd
            .creation_plan
            .nodes_to_create
            .iter()
            .any(|n| n.var == "c"));

        // References corr is context in BOTH directions WITH a span binding
        // (no empty corr_attrs → no GhostId collapse).
        assert!(
            fwd.match_plan
                .context_correspondences
                .iter()
                .any(|c| !c.attribute_bindings.is_empty()),
            "Fwd: CorrClass context mit span-binding"
        );
        assert!(
            bwd.match_plan
                .context_correspondences
                .iter()
                .any(|c| !c.attribute_bindings.is_empty()),
            "Bwd: CorrClass context mit span-binding"
        );
    }

    #[test]
    fn directed_spec_bwd_swaps_patterns_and_corr_endpoints() {
        use crate::rule::spec::CorrRole;
        let json = r#"{"rules":[{
            "name":"T","rank":40,
            "l_pattern":{"nodes":[{"id":"c","kind":"Class","constraints":[]}],"edges":[]},
            "r_pattern":{"nodes":[{"id":"jc","kind":"JavaClass","constraints":[]}],"edges":[]},
            "correspondence_links":[
                {"l_node_id":"c","r_node_id":"jc","role":"Establishes",
                 "attribute_bindings":[{"l_attr_name":"uName","r_attr_name":"jName"}]}
            ]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let spec = &rs.rules[0];

        let bwd = directed_spec(spec, Direction::Bwd);
        assert_eq!(bwd.l_pattern.as_ref().unwrap().nodes[0].kind, "JavaClass");
        assert_eq!(bwd.r_pattern.as_ref().unwrap().nodes[0].kind, "Class");
        let cl = &bwd.correspondence_links[0];
        assert_eq!(cl.l_node_id, "jc");
        assert_eq!(cl.r_node_id, "c");
        assert_eq!(cl.role, Some(CorrRole::Establishes)); // direction-neutral
                                                          // Binding mirrored: l_attr↔r_attr
        assert_eq!(cl.attribute_bindings[0].l_attr_name, "jName");
        assert_eq!(cl.attribute_bindings[0].r_attr_name, "uName");

        // Fwd = unchanged
        let fwd = directed_spec(spec, Direction::Fwd);
        assert_eq!(fwd.l_pattern.as_ref().unwrap().nodes[0].kind, "Class");
        assert_eq!(fwd.correspondence_links[0].l_node_id, "c");
    }

    #[test]
    fn reference_corr_with_bindings_is_context_not_creation() {
        // R_Attr-like: CorrClass=References (WITH span binding),
        // CorrAttr=Establishes. rc7 acceptance: a References corr with
        // bindings makes its R endpoint context, not creation.
        let json = r#"{"rules":[{
            "name":"T","rank":30,
            "l_pattern":{"nodes":[{"id":"c","kind":"Class","constraints":[]},
                                  {"id":"a","kind":"Attribute","constraints":[]}],
                         "edges":[{"kind":"attributes","source_node_id":"c","target_node_id":"a"}]},
            "r_pattern":{"nodes":[{"id":"jc","kind":"JavaClass","constraints":[]},
                                  {"id":"jf","kind":"JavaField","constraints":[]}],
                         "edges":[{"kind":"hasField","source_node_id":"jc","target_node_id":"jf"}]},
            "correspondence_links":[
                {"l_node_id":"c","r_node_id":"jc","role":"References",
                 "attribute_bindings":[{"l_attr_name":"name","r_attr_name":"name"}]},
                {"l_node_id":"a","r_node_id":"jf","role":"Establishes",
                 "attribute_bindings":[{"l_attr_name":"name","r_attr_name":"name"}]}
            ]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let c = compile(&rs.rules[0]).unwrap();
        assert!(
            !c.creation_plan
                .nodes_to_create
                .iter()
                .any(|n| n.var == "jc"),
            "jc (References-corr) darf nicht erzeugt werden"
        );
        assert!(
            c.creation_plan
                .nodes_to_create
                .iter()
                .any(|n| n.var == "jf"),
            "jf (Establishes-corr) muss erzeugt werden"
        );
    }

    // ── Error cases ──────────────────────────────────────────────────

    #[test]
    fn leere_rule_ist_fehler() {
        let r = RuleSpec {
            name: "empty".into(),
            rank: 0,
            documentation: None,
            l_pattern: None,
            r_pattern: None,
            correspondence_links: vec![],
            nacs: vec![],
        };
        let err = compile(&r).unwrap_err();
        matches!(err, CompileError::EmptyRule { .. });
    }

    #[test]
    fn dangling_edge_schlaegt_fehl() {
        let json = r#"{"rules":[{
            "name":"bad","rank":1,
            "l_pattern":{"nodes":[{"id":"a","kind":"Foo","constraints":[]}],
                         "edges":[{"kind":"x","source_node_id":"a","target_node_id":"unknown"}]},
            "r_pattern":{"nodes":[],"edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        match err {
            CompileError::DanglingEdge { var, .. } => assert_eq!(var, "unknown"),
            _ => panic!("falsche Fehlerart"),
        }
    }

    #[test]
    fn shared_anchor_kind_mismatch_schlaegt_fehl() {
        let json = r#"{"rules":[{
            "name":"bad","rank":1,
            "l_pattern":{"nodes":[{"id":"m","kind":"Model","constraints":[]}],"edges":[]},
            "r_pattern":{"nodes":[{"id":"m","kind":"SomethingElse","constraints":[]}],"edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        match err {
            CompileError::SharedAnchorKindMismatch {
                var,
                l_kind,
                r_kind,
                ..
            } => {
                assert_eq!(var, "m");
                assert_eq!(l_kind, "Model");
                assert_eq!(r_kind, "SomethingElse");
            }
            _ => panic!("falsche Fehlerart"),
        }
    }

    #[test]
    fn dangling_correspondence_schlaegt_fehl() {
        let json = r#"{"rules":[{
            "name":"bad","rank":1,
            "l_pattern":{"nodes":[{"id":"a","kind":"Foo","constraints":[]}],"edges":[]},
            "r_pattern":{"nodes":[],"edges":[]},
            "correspondence_links":[{"l_node_id":"a","r_node_id":"nope",
                "attribute_bindings":[]}]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        matches!(err, CompileError::DanglingCorrespondence { .. });
    }

    // ── Success cases on the demo fixture ────────────────────────────

    #[test]
    fn rclass_wird_kompiliert() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        assert_eq!(cr.name, "R_Class");
        assert_eq!(cr.rank, 40);
    }

    #[test]
    fn rclass_erkennt_model_als_shared_anchor() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        // Shared anchor (Model): is in the match pattern
        let m = cr.match_plan.nodes.iter().find(|n| n.var == "m").unwrap();
        assert_eq!(m.kind, "Model");
        assert_eq!(m.origin, NodeOrigin::Shared);

        // LOnly (Class): is in the match pattern
        let c = cr.match_plan.nodes.iter().find(|n| n.var == "c").unwrap();
        assert_eq!(c.origin, NodeOrigin::LOnly);

        // R-only without a context corr: NOT in the match pattern, but
        // in creation_plan.nodes_to_create. Otherwise the rule would
        // never fire — jc only exists after the rule has fired.
        assert!(
            cr.match_plan.nodes.iter().all(|n| n.var != "jc"),
            "jc darf nicht im Match-Pattern sein (R-only, keine Context-Corr)"
        );
        let jc_create = cr
            .creation_plan
            .nodes_to_create
            .iter()
            .find(|n| n.var == "jc")
            .expect("jc muss in creation_plan.nodes_to_create sein");
        assert_eq!(jc_create.kind, "JavaClass");
        assert_eq!(jc_create.origin, NodeOrigin::ROnly);
    }

    #[test]
    fn rclass_plant_jc_als_zu_erzeugen_mit_namen_propagation() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        // jc must be in nodes_to_create
        let jc = cr
            .creation_plan
            .nodes_to_create
            .iter()
            .find(|n| n.var == "jc");
        assert!(jc.is_some(), "jc fehlt in nodes_to_create");
        // Propagation: c.name → jc.name identity
        let p = cr
            .propagation_plan
            .iter()
            .find(|p| p.source_attr == "name" && p.target_attr == "name")
            .expect("name-Propagation fehlt");
        assert_eq!(p.source_node_var, "c");
        assert_eq!(p.target_node_var, "jc");
        assert_eq!(p.transform, AttrTransform::Identity);
    }

    #[test]
    fn rclass_corr_mit_bindings_wird_als_zu_erzeugen_klassifiziert() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        assert_eq!(cr.match_plan.context_correspondences.len(), 0);
        assert_eq!(cr.creation_plan.correspondences_to_create.len(), 1);
        assert_eq!(
            cr.creation_plan.correspondences_to_create[0]
                .kind
                .as_deref(),
            Some("CorrClass")
        );
    }

    #[test]
    fn rattr_trennt_kontext_von_neuer_corr() {
        let cr = compile(&demo_rule("R_Attr")).unwrap();
        // The first corr (CorrClass without bindings) is context
        assert_eq!(cr.match_plan.context_correspondences.len(), 1);
        assert_eq!(
            cr.match_plan.context_correspondences[0].kind.as_deref(),
            Some("CorrClass")
        );
        // The second (CorrAttr with 2 bindings) is new
        assert_eq!(cr.creation_plan.correspondences_to_create.len(), 1);
        assert_eq!(
            cr.creation_plan.correspondences_to_create[0]
                .kind
                .as_deref(),
            Some("CorrAttr")
        );
        assert_eq!(cr.propagation_plan.len(), 2); // name + type
    }

    #[test]
    fn rgetter_propagation_mit_getter_name() {
        let cr = compile(&demo_rule("R_Getter")).unwrap();
        let gn = cr
            .propagation_plan
            .iter()
            .find(|p| p.transform == AttrTransform::GetterName)
            .expect("GetterName-Propagation fehlt");
        assert_eq!(gn.source_attr, "name");
        assert_eq!(gn.target_attr, "name");
    }

    #[test]
    fn edges_werden_korrekt_auf_l_oder_r_seite_zugeordnet() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        // The classes edge is L-side, match pattern.
        // The javaClasses edge is R-side; its endpoint jc is ROnly,
        // so it must be in edges_to_create.
        assert!(cr
            .match_plan
            .edges
            .iter()
            .any(|e| e.kind == "classes" && e.side == EdgeSide::L));
        assert!(cr
            .creation_plan
            .edges_to_create
            .iter()
            .any(|e| e.kind == "javaClasses" && e.side == EdgeSide::R));
    }

    #[test]
    fn alle_vier_demo_rules_kompilieren_fehlerfrei() {
        let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
        for r in &rs.rules {
            let cr = compile(r).unwrap_or_else(|_| panic!("rule {} muss kompilieren", r.name));
            assert!(
                !cr.match_plan.nodes.is_empty(),
                "Rule {} hat leeren MatchPlan",
                r.name
            );
        }
    }

    #[test]
    fn literal_constraints_landen_im_match_plan() {
        let json = r#"{"rules":[{
            "name":"WithConstraints","rank":1,
            "l_pattern":{"nodes":[{"id":"c","kind":"Class",
                "constraints":[{"name":"isInterface","matcher":{"type":"literal","value":"false"}}]}],
                "edges":[]},
            "r_pattern":{"nodes":[],"edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();
        assert_eq!(cr.match_plan.constraints.len(), 1);
        assert_eq!(cr.match_plan.constraints[0].attr_name, "isInterface");
        assert!(matches!(
            &cr.match_plan.constraints[0].predicate,
            crate::engine::AttrPredicate::Equals(v) if v == "false"
        ));
    }

    #[test]
    fn r_kante_zwischen_kontext_knoten_wird_erzeugt() {
        // L matches two nodes a, b without an edge; R adds an edge a→b
        // between the same (context) nodes. This edge must be created
        // — not silently reclassified as a match condition.
        let json = r#"{"rules":[{
            "name":"CtxEdge","rank":1,
            "l_pattern":{"nodes":[
                {"id":"a","kind":"Class"},
                {"id":"b","kind":"Class"}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"a","kind":"Class"},
                {"id":"b","kind":"Class"}],
                "edges":[{"kind":"link","source_node_id":"a","target_node_id":"b"}]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();
        assert!(
            cr.creation_plan
                .edges_to_create
                .iter()
                .any(|e| e.kind == "link" && e.side == EdgeSide::R),
            "Context-Context-R-Kante muss in edges_to_create stehen, \
             nicht als Match-Constraint"
        );
    }

    #[test]
    fn r_literal_unterschiedlich_zu_l_klassifiziert_als_attrs_to_set() {
        // Counterpart to the edge bug (B4) on the attribute side: L
        // matches Job with image="old"; R says image="new" on the same
        // (context) node. The R literal must NOT land as a match
        // constraint — otherwise the matcher searches for "old" and
        // "new" simultaneously and never finds anything. It belongs in
        // creation_plan.attrs_to_set as a SetAttr intent.
        let json = r#"{"rules":[{
            "name":"CtxAttr","rank":1,
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
        let cr = compile(&rs.rules[0]).unwrap();

        // (1) R literal "new" must not appear as a match constraint
        let r_literal_in_match = cr.match_plan.constraints.iter().any(|c| {
            c.node_var == "j"
                && c.attr_name == "image"
                && matches!(
                    &c.predicate,
                    crate::engine::AttrPredicate::Equals(v) if v == "new"
                )
        });
        assert!(
            !r_literal_in_match,
            "R-Literal 'new' darf nicht als Match-Constraint behandelt \
             werden — die Regel würde sonst nie matchen (sucht \
             gleichzeitig nach 'old' und 'new')"
        );

        // (2) R literal must be in attrs_to_set in the creation_plan
        assert_eq!(
            cr.creation_plan.attrs_to_set.len(),
            1,
            "R-Literal auf Kontext-Knoten muss als attrs_to_set \
             materialisiert werden"
        );
        let ats = &cr.creation_plan.attrs_to_set[0];
        assert_eq!(ats.node_var, "j");
        assert_eq!(ats.attr_name, "image");
        assert_eq!(ats.value, "new");

        // (3) L literal "old" remains as a match constraint — the rule
        //     should only match nodes with image="old" and then set it
        //     to "new".
        let l_literal_in_match = cr.match_plan.constraints.iter().any(|c| {
            c.node_var == "j"
                && c.attr_name == "image"
                && matches!(
                    &c.predicate,
                    crate::engine::AttrPredicate::Equals(v) if v == "old"
                )
        });
        assert!(
            l_literal_in_match,
            "L-Literal 'old' muss als Match-Constraint erhalten bleiben"
        );
    }

    #[test]
    fn r_literal_auf_creation_knoten_landet_in_creation_attrs_nicht_attrs_to_set() {
        // rc6/B6 classification regression: L has only Source, R has
        // Target (R-only creation) with literal `label=x`. Before rc6,
        // `label=x` landed in `attrs_to_set` and, when two rules with
        // the same structural R output but different literal values
        // collided, caused an oscillation loop (see
        // tests/repro_rc5_bug2_attrs_to_set_collision.rs). Now it
        // lands in `creation_attrs` and flows into the GhostId of the
        // newly created node.
        let json = r#"{"rules":[{
            "name":"CreationLit","rank":1,
            "l_pattern":{"nodes":[
                {"id":"L","kind":"Source","constraints":[]}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"R","kind":"Target",
                 "constraints":[{"name":"label","matcher":{"type":"literal","value":"x"}}]}],
                "edges":[]},
            "correspondence_links":[{
                "l_node_id":"L","r_node_id":"R","kind":"tgg:refines",
                "attribute_bindings":[]
            }]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();

        // R node is R-only creation (not in r_only_in_context,
        // because the corr has `attribute_bindings: []` — a context
        // corr — wait: context corrs promote R vars into the match.
        // If bindings are empty, R is context, not creation. So the
        // corr must have bindings for R to be classified as creation.
        // We verify this via a separate spec with non-empty bindings.
        // → see next test: here R is *context* (corr without
        // bindings).
        eprintln!(
            "(context-corr variant) attrs_to_set={:?} creation_attrs={:?}",
            cr.creation_plan.attrs_to_set, cr.creation_plan.creation_attrs
        );
        assert_eq!(
            cr.creation_plan.nodes_to_create.len(),
            0,
            "R via context-corr (no bindings) gehört in match, nicht in nodes_to_create"
        );
        assert_eq!(
            cr.creation_plan.attrs_to_set.len(),
            1,
            "R-Knoten ist hier Kontext (context-corr) → Literal in attrs_to_set"
        );
        assert_eq!(cr.creation_plan.creation_attrs.len(), 0);

        // Second variant: corr WITH bindings → R is R-only creation
        // → literal lands in creation_attrs.
        let json2 = r#"{"rules":[{
            "name":"CreationLit2","rank":1,
            "l_pattern":{"nodes":[
                {"id":"L","kind":"Source",
                 "constraints":[{"name":"tag","matcher":{"type":"literal","value":"foo"}}]}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"R","kind":"Target",
                 "constraints":[{"name":"label","matcher":{"type":"literal","value":"x"}}]}],
                "edges":[]},
            "correspondence_links":[{
                "l_node_id":"L","r_node_id":"R","kind":"tgg:refines",
                "attribute_bindings":[{
                    "l_attr_name":"tag","r_attr_name":"tag","transformation":"identity"
                }]
            }]
        }]}"#;
        let rs2 = parse_ruleset(json2).unwrap();
        let cr2 = compile(&rs2.rules[0]).unwrap();
        assert_eq!(
            cr2.creation_plan.nodes_to_create.len(),
            1,
            "R muss Creation sein"
        );
        assert_eq!(cr2.creation_plan.nodes_to_create[0].var, "R");
        assert_eq!(
            cr2.creation_plan.attrs_to_set.len(),
            0,
            "Creation-Knoten-Literal darf NICHT in attrs_to_set landen — \
             das war der rc5-Oszillations-Bug",
        );
        assert_eq!(
            cr2.creation_plan.creation_attrs.len(),
            1,
            "Creation-Knoten-Literal muss in creation_attrs landen",
        );
        let ca = &cr2.creation_plan.creation_attrs[0];
        assert_eq!(ca.node_var, "R");
        assert_eq!(ca.attr_name, "label");
        assert_eq!(ca.value, "x");
    }
}
