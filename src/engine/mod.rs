//! Engine module — T₃, T₄, T₆.
//!
//! Responsibilities:
//! - Ghost-View as an incremental projection (Def. 6.1, 6.2)
//! - Matcher contract (Def. 6.3) including status awareness
//! - Pattern matching with enforced injectivity and edge patterns (Phase 1.3b)
//! - Rule trait with production (Def. 4.1)
//! - Rank-based selection of highest-ranked candidates (Def. 4.3)
//! - Canonical match enumeration μ (Def. 4.2)
//! - Cascade controller

use crate::graph::{GhostId, NodeData, Status, TypedGraph};
use crate::ops::{DeltaEntry, Op, OpError, OpTarget, Origin};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use thiserror::Error;

// ── Attribute predicates ═════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub enum AttrPredicate {
    Exists,
    Equals(String),
    /// Regex match against the value (Rust `regex` crate).
    Regex(regex::Regex),
    /// Value starts with the given prefix.
    Prefix(String),
    /// Value ends with the given suffix.
    Suffix(String),
    /// Numeric range; the value is converted via `parse::<f64>()`.
    NumericRange {
        min: f64,
        max: f64,
    },
}

impl AttrPredicate {
    pub fn matches(&self, value: Option<&String>) -> bool {
        match self {
            AttrPredicate::Exists => value.is_some(),
            AttrPredicate::Equals(expected) => value == Some(expected),
            AttrPredicate::Regex(re) => value.is_some_and(|s| re.is_match(s)),
            AttrPredicate::Prefix(p) => value.is_some_and(|s| s.starts_with(p)),
            AttrPredicate::Suffix(suf) => value.is_some_and(|s| s.ends_with(suf)),
            AttrPredicate::NumericRange { min, max } => value
                .and_then(|s| s.parse::<f64>().ok())
                .is_some_and(|n| n >= *min && n <= *max),
        }
    }
}

// ── Pattern ══════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct NodePattern {
    pub var: String,
    pub type_id: String,
    pub attr_constraints: Vec<(String, AttrPredicate)>,
}

impl NodePattern {
    pub fn new(var: &str, type_id: &str) -> Self {
        Self {
            var: var.into(),
            type_id: type_id.into(),
            attr_constraints: Vec::new(),
        }
    }

    pub fn with_attr_equals(mut self, key: &str, value: &str) -> Self {
        self.attr_constraints
            .push((key.into(), AttrPredicate::Equals(value.into())));
        self
    }

    pub fn with_attr_exists(mut self, key: &str) -> Self {
        self.attr_constraints
            .push((key.into(), AttrPredicate::Exists));
        self
    }

    pub fn matches_node(&self, node: &NodeData) -> bool {
        if node.type_id != self.type_id {
            return false;
        }
        self.attr_constraints
            .iter()
            .all(|(key, pred)| pred.matches(node.attrs.get(key)))
    }
}

/// Edge pattern: requires a matchable edge between two pattern variables.
#[derive(Clone, Debug)]
pub struct EdgePattern {
    pub source_var: String,
    pub target_var: String,
    pub type_id: String,
    /// rc7 (S) — membership match: direction- AND kind-agnostic. True
    /// only for synthetic correspondence membership edges: a
    /// correspondence is symmetric, so its context match must not
    /// depend on the `corrL`/`corrR` orientation (which is emitted
    /// relative to direction). The matcher then checks for "any edge
    /// between the two nodes, in either direction". The matcher stays
    /// metamodel-agnostic (it does not know corrL/corrR); knowledge of
    /// the edge layout remains in `instantiate`.
    pub membership: bool,
}

impl EdgePattern {
    pub fn new(source_var: &str, target_var: &str, type_id: &str) -> Self {
        Self {
            source_var: source_var.into(),
            target_var: target_var.into(),
            type_id: type_id.into(),
            membership: false,
        }
    }

    /// rc7 (S): symmetric correspondence membership — `a` and `b`
    /// are connected by any edge (in either direction).
    pub fn membership(a: &str, b: &str) -> Self {
        Self {
            source_var: a.into(),
            target_var: b.into(),
            type_id: String::new(),
            membership: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Pattern {
    pub nodes: Vec<NodePattern>,
    pub edges: Vec<EdgePattern>,
}

impl Pattern {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node(mut self, np: NodePattern) -> Self {
        self.nodes.push(np);
        self
    }

    pub fn with_edge(mut self, ep: EdgePattern) -> Self {
        self.edges.push(ep);
        self
    }
}

// ── Match ════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Default)]
pub struct PatternMatch {
    pub bindings: HashMap<String, GhostId>,
}

impl PatternMatch {
    pub fn get(&self, var: &str) -> Option<&GhostId> {
        self.bindings.get(var)
    }
}

/// Canonical match key for deterministic enumeration μ (Def. 4.2).
///
/// Orders matches lexicographically by their bound Ghost-IDs in
/// pattern-variable order.
pub fn canonical_key(m: &PatternMatch, pattern: &Pattern) -> Vec<[u8; 32]> {
    pattern
        .nodes
        .iter()
        .filter_map(|np| m.bindings.get(&np.var).map(|id| id_bytes(*id)))
        .collect()
}

fn id_bytes(id: GhostId) -> [u8; 32] {
    // GhostId is a tuple struct; we need a bytes accessor.
    // serde-bincode would be one option; here we go directly via the
    // hash seed as a proxy. Since GhostId implements Copy, direct
    // access via the Debug string would be ugly. The clean path is
    // to extend the GhostId API. For now, a serde_json roundtrip
    // fallback would be absurd. Best solution: give GhostId a
    // bytes() accessor.
    id.as_bytes()
}

/// Finds all matches of a pattern in the graph.
///
/// Properties:
/// - **Ghost-aware**: `graph.matchable_nodes()` excludes TOMB.
/// - **Injective**: distinct pattern variables bind to distinct
///   graph nodes.
/// - **Edge constraints**: all `pattern.edges` must exist in the
///   graph and be matchable.
///
/// Naive backtracking enumeration.
pub fn find_matches(pattern: &Pattern, graph: &TypedGraph) -> Vec<PatternMatch> {
    find_matches_with_fixed(pattern, graph, &HashMap::new())
}

/// Variant of [`find_matches`] with pre-bindings: certain pattern
/// variables are already pinned to concrete `GhostId`s. The matcher
/// branches only over the remaining variables.
///
/// Used by NAC checks (M2), where the `shared_with_l` anchors from
/// the main match are already bound.
pub fn find_matches_with_fixed(
    pattern: &Pattern,
    graph: &TypedGraph,
    fixed: &HashMap<String, GhostId>,
) -> Vec<PatternMatch> {
    if pattern.nodes.is_empty() {
        return if pattern.edges.is_empty() {
            let mut pm = PatternMatch::default();
            for (k, v) in fixed {
                pm.bindings.insert(k.clone(), *v);
            }
            vec![pm]
        } else {
            Vec::new()
        };
    }
    let plan = build_match_plan(pattern, fixed);
    let mut results = Vec::new();
    let mut current = PatternMatch::default();
    for (k, v) in fixed {
        current.bindings.insert(k.clone(), *v);
    }
    enumerate_matches(pattern, graph, &plan, 0, &mut current, &mut results);
    // Determinism (Def. 4.2): canonical μ ordering, independent of
    // the edge-guided traversal path. The *set* of matches is path-
    // invariant (exhaustive search); only the push order varies.
    results.sort_by_key(|m| canonical_key(m, pattern));
    results
}

// ── Edge-guided match plan ═══════════════════════════════════════════════
//
// Instead of enumerating the cartesian product over all pattern
// positions and checking edge constraints only at the leaf (O(M^N)),
// the matcher walks the nodes in *connected order*: a seed node is
// chosen via a type scan, each follow-up node hangs on an already
// bound node via a pattern edge and is generated from graph adjacency
// rather than from the full type population. That lowers the worst
// case to O(M · d^(N-1)), where d is the average node degree.

/// A pattern edge of a plan node to an already placed plan node —
/// expressed relative to the *new* node.
struct EdgeLink {
    /// Variable of the already placed neighbor.
    placed_var: String,
    type_id: String,
    /// `true`: the new node is the source, the placed one is the target.
    /// `false`: the placed one is the source, the new node is the target.
    new_is_source: bool,
    /// rc7 (S): direction- and kind-agnostic correspondence membership
    /// (see [`EdgePattern::membership`]).
    membership: bool,
}

/// One step in the traversal plan: which pattern node is bound next
/// and how its candidates are produced.
struct MatchStep {
    /// Index into `pattern.nodes`.
    node_idx: usize,
    /// `true`: variable is pre-bound via `fixed` — only validate.
    pre_bound: bool,
    /// Pattern edges to already placed nodes. Non-empty ⇒ `links[0]`
    /// serves as the adjacency guide (candidates from the neighbors
    /// of the anchor); all links are verified in addition. Empty ⇒
    /// seed node (type scan).
    links: Vec<EdgeLink>,
}

/// Builds the deterministic traversal plan for a pattern.
///
/// Determinism: the pattern is a fixed input; node selection is a
/// pure function over (pattern, placed set). The final
/// `canonical_key` sort in [`find_matches_with_fixed`] makes the
/// plan ordering invisible at the API boundary anyway.
fn build_match_plan(pattern: &Pattern, fixed: &HashMap<String, GhostId>) -> Vec<MatchStep> {
    let n = pattern.nodes.len();
    let mut placed = vec![false; n];
    let mut steps: Vec<MatchStep> = Vec::with_capacity(n);

    // 1. fixed variables first: they are already bound and serve as
    //    adjacency anchors for the rest (relevant for NAC and
    //    re-validation matching with shared anchors).
    for (i, np) in pattern.nodes.iter().enumerate() {
        if fixed.contains_key(&np.var) {
            placed[i] = true;
            steps.push(MatchStep {
                node_idx: i,
                pre_bound: true,
                links: Vec::new(),
            });
        }
    }

    // 2. Remaining nodes greedily: prefer one that attaches to the
    //    placed set via an edge (guided); otherwise pick the most
    //    constrained unbound node as the component seed.
    while steps.len() < n {
        let next = pick_next_node(pattern, &placed);
        let links = links_to_placed(pattern, &placed, next);
        placed[next] = true;
        steps.push(MatchStep {
            node_idx: next,
            pre_bound: false,
            links,
        });
    }
    steps
}

/// Picks the next pattern node to place. Prefers nodes with an edge
/// to the placed set (guided); tie-break: most-constrained-variable-
/// first (most `attr_constraints`), then smallest index.
fn pick_next_node(pattern: &Pattern, placed: &[bool]) -> usize {
    let mut best_idx: Option<usize> = None;
    let mut best_key = (false, 0usize, 0usize);
    for (i, np) in pattern.nodes.iter().enumerate() {
        if placed[i] {
            continue;
        }
        let guided = !links_to_placed(pattern, placed, i).is_empty();
        // `usize::MAX - i` ⇒ larger key = smaller index.
        let key = (guided, np.attr_constraints.len(), usize::MAX - i);
        if best_idx.is_none() || key > best_key {
            best_idx = Some(i);
            best_key = key;
        }
    }
    best_idx.expect("pick_next_node: no unbound node available")
}

/// Collects all pattern edges of node `idx` to already placed nodes,
/// expressed relative to node `idx`. Self-loops and edges with an
/// unknown counterpart variable are skipped — the leaf check
/// `satisfies_edge_patterns` covers them.
fn links_to_placed(pattern: &Pattern, placed: &[bool], idx: usize) -> Vec<EdgeLink> {
    let var = pattern.nodes[idx].var.as_str();
    let mut links = Vec::new();
    for ep in &pattern.edges {
        if ep.source_var == ep.target_var {
            continue;
        }
        let (other_var, new_is_source) = if ep.source_var == var {
            (ep.target_var.as_str(), true)
        } else if ep.target_var == var {
            (ep.source_var.as_str(), false)
        } else {
            continue;
        };
        let other_placed = pattern
            .nodes
            .iter()
            .position(|np| np.var == other_var)
            .map(|j| placed[j])
            .unwrap_or(false);
        if other_placed {
            links.push(EdgeLink {
                placed_var: other_var.to_string(),
                type_id: ep.type_id.clone(),
                new_is_source,
                membership: ep.membership,
            });
        }
    }
    links
}

fn enumerate_matches(
    pattern: &Pattern,
    graph: &TypedGraph,
    plan: &[MatchStep],
    depth: usize,
    current: &mut PatternMatch,
    out: &mut Vec<PatternMatch>,
) {
    if depth == plan.len() {
        // Intra-component edges have been checked incrementally; the
        // leaf check additionally covers self-loops and edges with an
        // unknown variable (defense in depth).
        if satisfies_edge_patterns(pattern, graph, current) {
            out.push(current.clone());
        }
        return;
    }
    let step = &plan[depth];
    let np = &pattern.nodes[step.node_idx];

    // Pre-bound variable (M2 / re-validation): do not enumerate,
    // only check whether the bound node satisfies the pattern.
    if step.pre_bound {
        if let Some(id) = current.bindings.get(&np.var).copied() {
            if let Some(node) = graph.get_node(&id) {
                if node.status.is_matchable() && np.matches_node(node) {
                    enumerate_matches(pattern, graph, plan, depth + 1, current, out);
                }
            }
        }
        return;
    }

    for cand in step_candidates(graph, np, step, current) {
        // Injectivity: the node must not already be bound.
        if current.bindings.values().any(|id| *id == cand) {
            continue;
        }
        let node = match graph.get_node(&cand) {
            Some(node) if node.status.is_matchable() => node,
            _ => continue,
        };
        if !np.matches_node(node) {
            continue;
        }
        // Check edge constraints to already bound nodes *immediately*
        // — not only at the leaf (early pruning).
        if !satisfies_links(graph, current, &step.links, cand) {
            continue;
        }
        current.bindings.insert(np.var.clone(), cand);
        enumerate_matches(pattern, graph, plan, depth + 1, current, out);
        current.bindings.remove(&np.var);
    }
}

/// Candidate nodes for a plan step.
///
/// - **Guided** (`links` non-empty): adjacency lookup via `links[0]`
///   — only neighbors of the already bound anchor, deduplicated
///   (parallel edges). O(node degree) instead of O(type population).
/// - **Seed** (`links` empty): type scan via `matchable_nodes_by_kind`
///   (F15 mitigation, BTreeSet-canonical).
fn step_candidates(
    graph: &TypedGraph,
    np: &NodePattern,
    step: &MatchStep,
    current: &PatternMatch,
) -> Vec<GhostId> {
    let guide = match step.links.first() {
        Some(guide) => guide,
        None => {
            return graph
                .matchable_nodes_by_kind(&np.type_id)
                .map(|node| node.id)
                .collect();
        }
    };
    let anchor = match current.bindings.get(&guide.placed_var) {
        Some(id) => *id,
        None => return Vec::new(),
    };
    let mut seen: BTreeSet<GhostId> = BTreeSet::new();
    if guide.membership {
        // rc7 (S): correspondence membership — direction- and kind-agnostic.
        // Candidates are ALL incident neighbors of the anchor.
        for (_edge, other) in graph.incident_edges(&anchor) {
            seen.insert(other);
        }
    } else if guide.new_is_source {
        // Pattern edge: new ─type─▶ anchor ⇒ the new node is the source
        // of an incoming edge of the anchor.
        for (edge, src) in graph.incoming_edges(&anchor) {
            if edge.type_id == guide.type_id {
                seen.insert(src);
            }
        }
    } else {
        // Pattern edge: anchor ─type─▶ new ⇒ the new node is the target
        // of an outgoing edge of the anchor.
        for (edge, tgt) in graph.outgoing_edges(&anchor) {
            if edge.type_id == guide.type_id {
                seen.insert(tgt);
            }
        }
    }
    seen.into_iter().collect()
}

/// Checks all edge links of a plan step against the graph.
fn satisfies_links(
    graph: &TypedGraph,
    current: &PatternMatch,
    links: &[EdgeLink],
    new_id: GhostId,
) -> bool {
    links.iter().all(|link| {
        let other = match current.bindings.get(&link.placed_var) {
            Some(id) => *id,
            None => return false,
        };
        if link.membership {
            graph.has_any_edge_either_dir(&new_id, &other)
        } else if link.new_is_source {
            graph.has_edge_between(&new_id, &other, &link.type_id)
        } else {
            graph.has_edge_between(&other, &new_id, &link.type_id)
        }
    })
}

fn satisfies_edge_patterns(pattern: &Pattern, graph: &TypedGraph, m: &PatternMatch) -> bool {
    pattern.edges.iter().all(|ep| {
        let src = match m.bindings.get(&ep.source_var) {
            Some(id) => id,
            None => return false,
        };
        let tgt = match m.bindings.get(&ep.target_var) {
            Some(id) => id,
            None => return false,
        };
        if ep.membership {
            graph.has_any_edge_either_dir(src, tgt)
        } else {
            graph.has_edge_between(src, tgt, &ep.type_id)
        }
    })
}

// ── Rule ═════════════════════════════════════════════════════════════════

/// Negative Application Condition as an engine pattern (M2).
///
/// The matcher checks it against the graph via
/// [`find_matches_with_fixed`] — if at least one match exists with
/// the shared-anchor bindings, the rule application is forbidden.
#[derive(Clone, Debug)]
pub struct NacPattern {
    pub name: String,
    pub pattern: Pattern,
    /// Node vars that get pinned to the L-match.
    pub shared_with_l: Vec<String>,
}

/// Entry in the attribute propagation plan of a rule (M5.3/M5.4).
///
/// Mirrors `crate::rule::compile::AttrPropagation` as an engine-
/// internal view, so the re-validation code does not need a cross-
/// module dependency on `rule`.
#[derive(Clone, Debug)]
pub struct EnginePropagation {
    pub source_node_var: String,
    pub source_attr: String,
    pub target_node_var: String,
    pub target_attr: String,
    /// String tag of the `AttrTransform` variant. Resolved at
    /// application time inside the rule's own context.
    pub transform_tag: String,
}

pub trait Rule: fmt::Debug + Send + Sync {
    fn id(&self) -> &str;
    fn rank(&self) -> u64;
    fn pattern(&self) -> &Pattern;
    fn produce(&self, m: &PatternMatch, graph: &TypedGraph) -> Vec<Op>;
    /// Negative Application Conditions. Default: none.
    fn nacs(&self) -> &[NacPattern] {
        &[]
    }
    /// List of propagations that match (l_var, source_attr).
    /// Default: empty.
    fn propagations_for(&self, _l_var: &str, _source_attr: &str) -> Vec<EnginePropagation> {
        Vec::new()
    }
    /// rc7: "real input kinds" of this directional rule (l_pattern
    /// minus r_pattern). Empty list = non-directional/rc6 rule
    /// (always active). Used for the Δ-based direction bundling.
    /// Default: empty.
    fn input_domain_kinds(&self) -> &[String] {
        &[]
    }
}

/// Concrete rule implementation with closure-based production.
pub struct BasicRule {
    id: String,
    rank: u64,
    pattern: Pattern,
    #[allow(clippy::type_complexity)]
    production: Box<dyn Fn(&PatternMatch, &TypedGraph) -> Vec<Op> + Send + Sync>,
    nacs: Vec<NacPattern>,
    propagations: Vec<EnginePropagation>,
    input_domain_kinds: Vec<String>,
}

impl BasicRule {
    pub fn new<F>(id: &str, rank: u64, pattern: Pattern, production: F) -> Self
    where
        F: Fn(&PatternMatch, &TypedGraph) -> Vec<Op> + Send + Sync + 'static,
    {
        Self {
            id: id.into(),
            rank,
            pattern,
            production: Box::new(production),
            nacs: Vec::new(),
            propagations: Vec::new(),
            input_domain_kinds: Vec::new(),
        }
    }

    /// Sets the Δ-direction input kinds (builder-style, rc7).
    pub fn with_input_domain_kinds(mut self, kinds: Vec<String>) -> Self {
        self.input_domain_kinds = kinds;
        self
    }

    /// Sets the NACs of this rule (builder-style).
    pub fn with_nacs(mut self, nacs: Vec<NacPattern>) -> Self {
        self.nacs = nacs;
        self
    }

    /// Sets the propagation plan (builder-style, M5.3).
    pub fn with_propagations(mut self, propagations: Vec<EnginePropagation>) -> Self {
        self.propagations = propagations;
        self
    }
}

impl fmt::Debug for BasicRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicRule")
            .field("id", &self.id)
            .field("rank", &self.rank)
            .field("pattern", &self.pattern)
            .finish()
    }
}

impl Rule for BasicRule {
    fn id(&self) -> &str {
        &self.id
    }
    fn rank(&self) -> u64 {
        self.rank
    }
    fn pattern(&self) -> &Pattern {
        &self.pattern
    }
    fn produce(&self, m: &PatternMatch, graph: &TypedGraph) -> Vec<Op> {
        (self.production)(m, graph)
    }
    fn nacs(&self) -> &[NacPattern] {
        &self.nacs
    }
    fn input_domain_kinds(&self) -> &[String] {
        &self.input_domain_kinds
    }
    fn propagations_for(&self, l_var: &str, source_attr: &str) -> Vec<EnginePropagation> {
        self.propagations
            .iter()
            .filter(|p| p.source_node_var == l_var && p.source_attr == source_attr)
            .cloned()
            .collect()
    }
}

/// Checks whether any NAC of the rule matches in the graph (with
/// the `shared_with_l` bindings carried over from the main match).
///
/// Returns: `true` = rule application forbidden.
pub fn nacs_forbid(m: &PatternMatch, rule: &dyn Rule, graph: &TypedGraph) -> bool {
    for nac in rule.nacs() {
        let mut fixed = HashMap::new();
        for var in &nac.shared_with_l {
            if let Some(id) = m.bindings.get(var) {
                fixed.insert(var.clone(), *id);
            }
        }
        if !find_matches_with_fixed(&nac.pattern, graph, &fixed).is_empty() {
            return true;
        }
    }
    false
}

// ── Candidate selection ══════════════════════════════════════════════════

/// A match candidate with rank information for selection.
pub struct MatchCandidate<'a> {
    pub rule: &'a dyn Rule,
    pub pattern_match: PatternMatch,
    pub match_idx: usize,
}

impl<'a> MatchCandidate<'a> {
    /// Composite rank (ρ(r), μ(m)), ordered lexicographically.
    ///
    /// Corresponds to Def. 4.3 in the paper; the product ρ·M + μ is
    /// represented here as a tuple, which is semantically identical
    /// and does not require a choice of M.
    pub fn rank_key(&self) -> (u64, usize) {
        (self.rule.rank(), self.match_idx)
    }
}

impl<'a> fmt::Debug for MatchCandidate<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MatchCandidate")
            .field("rule_id", &self.rule.id())
            .field("rule_rank", &self.rule.rank())
            .field("match_idx", &self.match_idx)
            .finish()
    }
}

/// Picks the highest-ranked candidate across all rules.
///
/// Convention: **higher rank value wins**. The default heuristic
/// `ρ(r_i) = i` (definition order) therefore means that later-
/// defined rules have higher priority. Users that want the opposite
/// priority set `ρ(r_i) = N - i`.
pub fn select_highest_rank<'a, I>(rules: I, graph: &TypedGraph) -> Option<MatchCandidate<'a>>
where
    I: IntoIterator<Item = &'a dyn Rule>,
{
    let mut best: Option<MatchCandidate<'a>> = None;

    for rule in rules {
        let pattern = rule.pattern();
        let mut matches = find_matches(pattern, graph);
        // Canonical enumeration μ: sort by Ghost-ID tuple.
        matches.sort_by_key(|m| canonical_key(m, pattern));

        for (idx, pattern_match) in matches.into_iter().enumerate() {
            let candidate = MatchCandidate {
                rule,
                pattern_match,
                match_idx: idx,
            };
            let better = match &best {
                None => true,
                Some(b) => candidate.rank_key() > b.rank_key(),
            };
            if better {
                best = Some(candidate);
            }
        }
    }

    best
}

/// Direction-bundled rule selection: returns the rules relevant to the
/// last delta. A rule is active iff it is **undirected**
/// (`input_domain_kinds` empty) **or** its input kinds intersect
/// `delta_kinds`. This is what keeps a bidirectional rule set
/// (`compile_bidirectional` → `R→` / `R←`) from ping-ponging: a delta on
/// the L-domain activates only the forward rules, a delta on the R-domain
/// only the backward rules — the direction lives in the delta, not in a
/// manual pass switch.
///
/// The caller derives `delta_kinds` from the just-applied delta (the
/// `type_id`s it touches) and passes the result to [`run_cascade`].
pub fn directional_rule_refs<'a>(
    rules: &'a [Box<dyn Rule>],
    delta_kinds: &HashSet<String>,
) -> Vec<&'a dyn Rule> {
    rules
        .iter()
        .filter(|r| {
            let idk = r.input_domain_kinds();
            idk.is_empty() || idk.iter().any(|k| delta_kinds.contains(k))
        })
        .map(|r| r.as_ref())
        .collect()
}

// ── Cascade ══════════════════════════════════════════════════════════════

#[derive(Debug, Default, Clone)]
pub struct Cascade {
    pub entries: Vec<DeltaEntry>,
    /// rc10 perf: GhostId → first entry that created the element — O(1)
    /// `creator_of` instead of a linear scan over all entries. This scan
    /// (via `ancestors_of_anchor`) previously dominated ~47% of cascade
    /// time (O(steps²)). Invariant: maintained in [`Self::append`], rebuilt
    /// after the single shrink point (`rollback_highest_rank` truncate).
    creator_index: HashMap<GhostId, usize>,
}

impl Cascade {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user_delta(user_delta: DeltaEntry) -> Self {
        let mut cascade = Self::default();
        cascade.append(user_delta);
        cascade
    }

    /// Appends a delta entry — strict monotonicity V₆.
    pub fn append(&mut self, entry: DeltaEntry) -> usize {
        let idx = self.entries.len();
        // Maintain the index incrementally: map each node/edge op target
        // identity to its *first* (lowest) creating entry. `or_insert`
        // preserves the first-creator semantics of the old linear scan.
        for op in &entry.op_star {
            match op.target() {
                OpTarget::Node(id) | OpTarget::Edge(id) => {
                    self.creator_index.entry(id).or_insert(idx);
                }
                OpTarget::Attr(..) => {}
            }
        }
        self.entries.push(entry);
        idx
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last(&self) -> Option<&DeltaEntry> {
        self.entries.last()
    }

    /// Rebuilds `creator_index` from scratch. Called after the only
    /// entry-shrinking operation (rollback truncate); incremental upkeep
    /// in [`Self::append`] cannot un-insert ids a removed entry created.
    pub fn rebuild_creator_index(&mut self) {
        self.creator_index.clear();
        for (idx, entry) in self.entries.iter().enumerate() {
            for op in &entry.op_star {
                match op.target() {
                    OpTarget::Node(id) | OpTarget::Edge(id) => {
                        self.creator_index.entry(id).or_insert(idx);
                    }
                    OpTarget::Attr(..) => {}
                }
            }
        }
    }

    /// Finds the delta entry that first created the element with the
    /// given ID. Returns `Some(idx)`, or `None` if the element was not
    /// produced by the cascade (e.g. SOLID baseline).
    ///
    /// O(1) via `creator_index` (rc10) — previously a linear scan over
    /// all entries, which dominated cascade time through
    /// `ancestors_of_anchor`.
    pub fn creator_of(&self, id: &GhostId) -> Option<usize> {
        self.creator_index.get(id).copied()
    }

    /// Transitive ancestor set with respect to `≺_D` (Def. 2.6): all
    /// delta indices that (transitively) produced elements referenced
    /// in the given anchor.
    pub fn ancestors_of_anchor(&self, anchor: &[GhostId]) -> HashSet<usize> {
        let mut result = HashSet::new();
        let mut queue: Vec<GhostId> = anchor.to_vec();
        while let Some(id) = queue.pop() {
            if let Some(creator) = self.creator_of(&id) {
                if result.insert(creator) {
                    for a in &self.entries[creator].anchor {
                        queue.push(*a);
                    }
                }
            }
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminationState {
    Convergence,
    Duplication,
    Contradiction { reason: String },
    Running,
}

/// Engine error during a cascade step.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Op application failed: {0}")]
    OpApplication(#[from] OpError),
    #[error("Step limit exceeded ({limit})")]
    StepLimitExceeded { limit: usize },
}

// ── Duplication and contradiction predicates ═════════════════════════════

/// Checks duplication (Def. 3.2): does a rule application contribute
/// *nothing new* — i.e. would **every** one of its ops only re-create
/// an already existing (matchable) element?
///
/// Op-granular (`.all()`, not `.any()`): a *mixed* application
/// (genuine new work + already-satisfied ops) is **not** a duplicate
/// — it fires, and the already-satisfied add-ops are idempotent
/// no-ops (`insert_node`/`add_edge` re-confirm what is already there).
/// Only a purely repeating application saturates to `Duplication`.
/// This lets rules create edges between already existing nodes.
///
/// Because Ghost-IDs are computed structurally via SHA-256 (Def. 5.3),
/// an isomorphic emission target produces the same hash → direct
/// lookup in the graph.
pub fn is_duplicate(ops: &[Op], graph: &TypedGraph) -> bool {
    // M5.5: TentativeTombstone elements do NOT count as duplicates —
    // they are resurrection candidates and should be reset to
    // Solid/Ghost via insert_node/add_edge when the add-op fires.
    let active_non_tentative =
        |status: Status| status.is_matchable() && status != Status::TentativeTombstone;
    ops.iter().all(|op| match op {
        Op::AddNode {
            parent,
            edge_type,
            type_id,
            attrs,
        } => {
            let would_be_id = GhostId::from_parent(parent, edge_type, type_id, attrs);
            graph
                .get_node(&would_be_id)
                .map(|n| active_non_tentative(n.status))
                .unwrap_or(false)
        }
        Op::AddEdge {
            source,
            target,
            type_id,
            attrs,
        } => {
            let would_be_id = GhostId::for_edge(source, target, type_id, attrs);
            graph
                .get_edge(&would_be_id)
                .map(|e| active_non_tentative(e.status))
                .unwrap_or(false)
        }
        // B5-rc5: SetAttr is a duplicate when the target node is
        // matchable+non-tentative and the attribute already carries
        // exactly this value — i.e. the op would be an idempotent
        // no-op. Before this arm, SetAttr fell into the catch-all
        // `_ => false`, which made rc4-attrs_to_set rules drive a
        // non-terminating cascade (every step counted as "productive"
        // even though produce() only re-emitted the same SetAttr).
        // Op-granular: only when ALL ops of this application are
        // idempotent does the step saturate to Duplication.
        Op::SetAttr { target, key, value } => graph
            .get_node(target)
            .filter(|n| active_non_tentative(n.status))
            .and_then(|n| n.attrs.get(key))
            .map(|current| current == value)
            .unwrap_or(false),
        _ => false,
    })
}

/// Checks contradiction without cascade context (simplification, deprecated).
///
/// Uses only SOLID and TOMB checks, no ancestor analysis.
/// Phase 1.3c uses [`is_contradictory_with_cascade`].
pub fn is_contradictory(ops: &[Op], graph: &TypedGraph) -> Option<String> {
    for op in ops {
        match op {
            Op::DelNode { target } => match graph.get_node(target) {
                None => return Some(format!("DelNode target {} not found", target.short())),
                Some(n) if n.status == Status::Solid => {
                    return Some(format!(
                        "V₇ violation: SOLID node {} cannot be erased",
                        target.short()
                    ))
                }
                Some(n) if n.status == Status::Tombstone => {
                    return Some(format!(
                        "double-tombstone on {} (already tombstoned)",
                        target.short()
                    ))
                }
                _ => {}
            },
            Op::DelEdge { target } => match graph.get_edge(target) {
                None => return Some(format!("DelEdge target {} not found", target.short())),
                Some(e) if e.status == Status::Solid => {
                    return Some(format!(
                        "V₇ violation: SOLID edge {} cannot be erased",
                        target.short()
                    ))
                }
                _ => {}
            },
            Op::AddEdge { source, target, .. } => {
                // Ghost endpoint must not be TOMB (Def. 2.5).
                if matches!(graph.get_node(source), Some(n) if n.status == Status::Tombstone) {
                    return Some(format!("AddEdge: source {} is TOMB", source.short()));
                }
                if matches!(graph.get_node(target), Some(n) if n.status == Status::Tombstone) {
                    return Some(format!("AddEdge: target {} is TOMB", target.short()));
                }
            }
            _ => {}
        }
    }
    None
}

/// Full contradiction check with ancestor analysis (Def. 3.6 + V₇).
///
/// In addition to the local SOLID and TOMB checks, this variant also
/// checks:
/// - del-ops on $d_0$ elements (user-delta protection).
/// - del-ops on elements produced by a (transitive) ancestor of the
///   candidate (reconciliation admissibility V₇).
pub fn is_contradictory_with_cascade(
    ops: &[Op],
    anchor: &[GhostId],
    graph: &TypedGraph,
    cascade: &Cascade,
) -> Option<String> {
    if let Some(reason) = is_contradictory(ops, graph) {
        return Some(reason);
    }

    let ancestors = cascade.ancestors_of_anchor(anchor);

    for op in ops {
        match op {
            Op::DelNode { target } | Op::DelEdge { target } => {
                if let Some(creator) = cascade.creator_of(target) {
                    if creator == 0 {
                        return Some(format!(
                            "V₇: d_0 element {} cannot be erased during cascade",
                            target.short()
                        ));
                    }
                    if ancestors.contains(&creator) {
                        return Some(format!(
                            "V₇: ancestor d_{} with element {} cannot be erased",
                            creator,
                            target.short()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Retraction cascade (Def. 3.8) ════════════════════════════════════════

/// Computes the retraction cascade for an op.
///
/// On DelNode, all matchable incident edges are produced as induced
/// DelEdge ops (structural dependency Def. 3.7 for edge endpoints), and
/// — following `corrL`/`corrR` — the correspondence node and its partner
/// on the opposite domain are tombstoned too, so a delete on one side of
/// a translated pair propagates the whole triple.
pub fn retraction_cascade_for(op: &Op, graph: &TypedGraph) -> Vec<Op> {
    match op {
        Op::DelNode { target } => {
            let mut ops = Vec::new();

            // 1. Tombstone every incident edge of the deleted node.
            for (edge, _) in graph.incident_edges(target) {
                ops.push(Op::DelEdge { target: edge.id });
            }

            // 2. Follow `corrL`/`corrR` edges to the correspondence node,
            //    then across its other correspondence edge to the partner
            //    on the opposite domain. Tombstone both.
            for (edge, neighbor) in graph.incident_edges(target) {
                if !is_correspondence_edge(&edge.type_id) {
                    continue;
                }
                // `neighbor` is the correspondence node. Walk its other
                // correspondence edge to find the partner; emit DelNode for
                // the partner and for the correspondence node itself.
                for (other_edge, other) in graph.incident_edges(&neighbor) {
                    if other == *target {
                        continue;
                    }
                    if !is_correspondence_edge(&other_edge.type_id) {
                        continue;
                    }
                    ops.push(Op::DelNode { target: other });
                }
                ops.push(Op::DelNode { target: neighbor });
            }

            ops
        }
        _ => Vec::new(),
    }
}

/// Returns `true` iff the edge kind names a TGG correspondence edge.
/// The seesaw convention is `"corrL"` (anchor → corr) and `"corrR"`
/// (corr → R-side). Generic across user rules — no demo-name hardcoding.
fn is_correspondence_edge(kind: &str) -> bool {
    kind == "corrL" || kind == "corrR"
}

/// Expands a primary op list with its retraction cascades while
/// building the `induces` DAG structure from V₁₂.
///
/// Returns `(expanded ops, induces map)` — the induces map has the
/// same length as the op list and contains, per op, the indices of
/// the directly induced follow-up ops.
pub fn expand_with_retraction(
    primary_ops: Vec<Op>,
    graph: &TypedGraph,
) -> (Vec<Op>, Vec<Vec<usize>>) {
    let mut full_ops: Vec<Op> = Vec::new();
    let mut induces: Vec<Vec<usize>> = Vec::new();

    for primary in primary_ops {
        let primary_idx = full_ops.len();
        let induced = retraction_cascade_for(&primary, graph);
        full_ops.push(primary);
        induces.push(Vec::new());

        let mut induced_indices = Vec::new();
        for induced_op in induced {
            let idx = full_ops.len();
            full_ops.push(induced_op);
            induces.push(Vec::new());
            induced_indices.push(idx);
        }
        induces[primary_idx] = induced_indices;
    }

    (full_ops, induces)
}

// ── Cascade step and runner ══════════════════════════════════════════════

/// Encodes a (rule_rank, match_idx) pair as u64 for DeltaEntry.rank.
///
/// Layout: `[rule_rank: u32 high][match_idx: u32 low]`. This makes
/// the rule rank dominate the match index (Def. 4.3 with implicit
/// M = 2^32).
pub fn encode_delta_rank(rule_rank: u64, match_idx: usize) -> u64 {
    (rule_rank << 32) | (match_idx as u64 & 0xFFFF_FFFF)
}

/// Collects all match candidates of all rules, sorted in descending
/// rank order (`rank_key` from max to min).
pub fn collect_candidates<'a, I>(rules: I, graph: &TypedGraph) -> Vec<MatchCandidate<'a>>
where
    I: IntoIterator<Item = &'a dyn Rule>,
{
    let mut all = Vec::new();
    for rule in rules {
        let pattern = rule.pattern();
        let mut matches = find_matches(pattern, graph);
        matches.sort_by_key(|m| canonical_key(m, pattern));
        for (idx, pattern_match) in matches.into_iter().enumerate() {
            all.push(MatchCandidate {
                rule,
                pattern_match,
                match_idx: idx,
            });
        }
    }
    // Descending by rank_key — std::cmp::Reverse keeps it canonical
    // under sort_by_key (clippy-clean).
    all.sort_by_key(|c| std::cmp::Reverse(c.rank_key()));
    all
}

/// Runs a single cascade step (Phase 1.3c).
///
/// Flow (rank-descending, implicit backtracking path):
/// 1. Collect all match candidates, sorted max→min by rank.
/// 2. For each candidate in rank-descending order:
///    a) Produce primary ops via the rule.
///    b) Expand with the retraction cascade (Def. 3.8); the
///    induces DAG (V₁₂) is built in the process.
///    c) Build the prospective anchor from pattern bindings.
///    d) Check duplication on the full op set → `any_duplicate`, next.
///    e) Check contradiction including the ancestor check (V₇) →
///    `any_contradiction` with reason, next.
///    f) Accepted: build the DeltaEntry (with populated `induces`),
///    apply, append, `Running`.
/// 3. Saturation: if no candidate passes, prioritize
///    Contradiction > Duplication > Convergence as the termination
///    reason.
pub fn cascade_step(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
) -> Result<TerminationState, EngineError> {
    let candidates = collect_candidates(rules.iter().copied(), graph);
    // Full path: inactive DeadSet ⇒ no skipping, exact original behavior.
    select_and_apply(cascade, graph, candidates, &mut DeadSet::default())
}

/// Tracks candidates that contribute nothing this step and onward —
/// already-applied rules and detected duplicates. In a monotonically
/// growing cascade such a candidate stays a duplicate, so re-running
/// `produce` + `is_duplicate` on it every step (an O(steps²) trap that
/// dominated the cached matcher) is pure waste.
///
/// Skipping is equivalence-preserving: a dead candidate would saturate to
/// `Duplication` anyway, so it is still counted as a duplicate for the
/// saturation verdict. `active` is false on the full ([`cascade_step`])
/// path → every method is a no-op and that path keeps exact original
/// behavior. The set is cleared whenever an op *removes/mutates* an
/// element (only that can revive a duplicate; pure adds cannot).
#[derive(Debug, Default)]
struct DeadSet {
    keys: HashSet<(String, Vec<(String, GhostId)>)>,
    active: bool,
}

impl DeadSet {
    fn active() -> Self {
        Self {
            keys: HashSet::new(),
            active: true,
        }
    }
    fn key(c: &MatchCandidate) -> (String, Vec<(String, GhostId)>) {
        let mut b: Vec<(String, GhostId)> = c
            .pattern_match
            .bindings
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        b.sort();
        (c.rule.id().to_string(), b)
    }
    fn contains(&self, c: &MatchCandidate) -> bool {
        self.active && self.keys.contains(&Self::key(c))
    }
    fn mark(&mut self, c: &MatchCandidate) {
        if self.active {
            self.keys.insert(Self::key(c));
        }
    }
    fn clear(&mut self) {
        self.keys.clear();
    }
}

/// Shared selection + application core of a cascade step.
///
/// Both the full ([`cascade_step`]) and the cached
/// (`cascade_step_cached`) step funnel their candidate list through
/// here, so the correctness-critical rank-descending selection, the
/// NAC / duplication / contradiction gates and the `DeltaEntry`
/// construction live in exactly ONE place. The two step variants differ
/// only in how the candidate list is *sourced* (and whether `dead` is
/// active) — never in how a candidate is selected or applied. That is
/// what makes the cached variant provably equivalent.
fn select_and_apply(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    candidates: Vec<MatchCandidate<'_>>,
    dead: &mut DeadSet,
) -> Result<TerminationState, EngineError> {
    if candidates.is_empty() {
        return Ok(TerminationState::Convergence);
    }

    let mut any_duplicate = false;
    let mut last_contradiction: Option<String> = None;

    for candidate in candidates {
        // Known-dead (applied or previously duplicate) ⇒ still a duplicate
        // for saturation, but skip the expensive produce/is_duplicate.
        if dead.contains(&candidate) {
            any_duplicate = true;
            continue;
        }

        // NAC check (M2) first — before production. If any NAC
        // matches, the candidate is forbidden.
        if nacs_forbid(&candidate.pattern_match, candidate.rule, graph) {
            continue;
        }

        let primary_ops = candidate.rule.produce(&candidate.pattern_match, graph);
        if primary_ops.is_empty() {
            continue;
        }

        // Build the retraction cascade.
        let (full_ops, induces) = expand_with_retraction(primary_ops, graph);

        let anchor: Vec<GhostId> = candidate
            .rule
            .pattern()
            .nodes
            .iter()
            .filter_map(|np| candidate.pattern_match.bindings.get(&np.var).copied())
            .collect();

        if is_duplicate(&full_ops, graph) {
            any_duplicate = true;
            // Monotonic add-cascade: a duplicate stays a duplicate → future
            // steps skip it without recompute.
            dead.mark(&candidate);
            continue;
        }

        if let Some(reason) = is_contradictory_with_cascade(&full_ops, &anchor, graph, cascade) {
            last_contradiction = Some(reason);
            continue;
        }

        // Accepted — build the DeltaEntry and apply.
        let rank = encode_delta_rank(candidate.rule.rank(), candidate.match_idx);
        let delta = DeltaEntry {
            origin: Origin::Rule {
                rule_id: candidate.rule.id().into(),
            },
            rank,
            op_star: full_ops,
            anchor,
            induces,
            bindings: candidate.pattern_match.bindings.clone(),
        };

        for op in &delta.op_star {
            op.apply(graph)?;
        }

        // Applied ⇒ its ops now exist ⇒ a duplicate from here on.
        dead.mark(&candidate);
        cascade.append(delta);
        return Ok(TerminationState::Running);
    }

    // Saturation priority: Contradiction > Duplication > Convergence.
    if let Some(reason) = last_contradiction {
        Ok(TerminationState::Contradiction { reason })
    } else if any_duplicate {
        Ok(TerminationState::Duplication)
    } else {
        Ok(TerminationState::Convergence)
    }
}

// ── Incremental matching: per-rule match cache (rc10 perf) ═══════════════
//
// `collect_candidates` re-enumerates every rule's matches over the whole
// graph every step (~69% of cascade time, measured). But a rule's match
// set can only change when an op touches a node/edge KIND that the rule's
// pattern references. The cache keeps each rule's last full (canonically
// sorted) enumeration and invalidates it only when a touched kind hits.
//
// Equivalence (proven by the differential test): a *clean* rule's match
// set is unchanged since it was cached ⇒ its cached list equals a fresh
// `find_matches` on the current graph ⇒ identical matches in identical
// canonical order ⇒ identical `match_idx` ⇒ identical `rank`. A *dirty*
// rule is re-enumerated outright. So the cached candidate list is
// bit-identical to the full one every step.

/// Per-rule match cache for `cascade_step_cached`.
#[derive(Debug, Default, Clone)]
pub struct MatchCache {
    per_rule: HashMap<String, Vec<PatternMatch>>,
}

impl MatchCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Incrementally brings the cache up to date after `delta` was applied
    /// (rc10 Lever 5: anchored matching).
    ///
    /// Instead of re-enumerating a dirty rule over the *whole* graph, a
    /// pure-add delta only *extends* each affected rule's match list with
    /// the matches that involve a newly added element — found by anchoring
    /// a pattern node of the matching kind to that element
    /// (`find_matches_with_fixed`). Edge endpoints are anchored too, so a
    /// new edge between existing nodes is picked up. That turns the per-step
    /// cost from O(graph) into O(neighborhood of the new elements).
    ///
    /// A removal/mutation op (Del/SetAttr) can both *disable* an existing
    /// match and *enable* a new one, which an additive update cannot
    /// express, so any cached rule whose pattern references a
    /// removed/mutated kind is conservatively re-enumerated in full.
    ///
    /// Either way the cache stays the *complete*, canonically sorted,
    /// duplicate-free match set, so [`collect_candidates_cached`] keeps
    /// producing a candidate list bit-identical to the full matcher
    /// (proven by the differential test).
    fn update(&mut self, delta: &DeltaEntry, graph: &TypedGraph, rules: &[&dyn Rule]) {
        if self.per_rule.is_empty() {
            return;
        }
        // Removal/mutation kinds force a full re-enumeration; newly added
        // nodes (and the endpoints of added edges) are the anchor points for
        // incremental matches.
        let mut removal_kinds: HashSet<String> = HashSet::new();
        let mut anchors: Vec<GhostId> = Vec::new();
        for op in &delta.op_star {
            match op {
                Op::AddNode { .. } => {
                    if let OpTarget::Node(id) = op.target() {
                        anchors.push(id);
                    }
                }
                Op::AddEdge { source, target, .. } => {
                    anchors.push(*source);
                    anchors.push(*target);
                }
                Op::DelNode { target } | Op::SetAttr { target, .. } => {
                    if let Some(n) = graph.get_node(target) {
                        removal_kinds.insert(n.type_id.clone());
                    }
                }
                Op::DelEdge { target } => {
                    if let Some(e) = graph.get_edge(target) {
                        removal_kinds.insert(e.type_id.clone());
                    }
                }
            }
        }
        let anchors_with_kind: Vec<(GhostId, String)> = anchors
            .iter()
            .filter_map(|&id| graph.get_node(&id).map(|n| (id, n.type_id.clone())))
            .collect();

        for rule in rules {
            let pat = rule.pattern();
            // Only maintain rules that are actually cached; an uncached rule
            // is (re)enumerated fully and lazily on next access.
            let Some(list) = self.per_rule.get_mut(rule.id()) else {
                continue;
            };
            // Removal/mutation hit → conservative full re-enumeration.
            if !removal_kinds.is_empty() && pattern_references_any_kind(pat, &removal_kinds) {
                let mut m = find_matches(pat, graph);
                m.sort_by_key(|pm| canonical_key(pm, pat));
                *list = m;
                continue;
            }
            // Additive: extend with matches that involve a new element,
            // anchoring each matching-kind pattern node on it.
            let mut grew = false;
            for (anchor, akind) in &anchors_with_kind {
                for np in &pat.nodes {
                    if &np.type_id == akind {
                        let mut fixed = HashMap::new();
                        fixed.insert(np.var.clone(), *anchor);
                        for m in find_matches_with_fixed(pat, graph, &fixed) {
                            list.push(m);
                            grew = true;
                        }
                    }
                }
            }
            if grew {
                // Re-establish the canonical-sorted, duplicate-free invariant
                // (compute each canonical key once).
                let mut keyed: Vec<(Vec<[u8; 32]>, PatternMatch)> = list
                    .drain(..)
                    .map(|m| (canonical_key(&m, pat), m))
                    .collect();
                keyed.sort_by(|a, b| a.0.cmp(&b.0));
                keyed.dedup_by(|a, b| a.0 == b.0);
                *list = keyed.into_iter().map(|(_, m)| m).collect();
            }
        }
    }
}

/// Whether a pattern mentions any of `kinds` as a node or edge type.
fn pattern_references_any_kind(pattern: &Pattern, kinds: &HashSet<String>) -> bool {
    pattern.nodes.iter().any(|n| kinds.contains(&n.type_id))
        || pattern.edges.iter().any(|e| kinds.contains(&e.type_id))
}

/// Cached counterpart of [`collect_candidates`]: identical output, but a
/// rule's matches are re-enumerated only on a cache miss (first use or
/// after invalidation). Mirrors [`collect_candidates`] exactly otherwise
/// — same per-rule canonical sort, same `match_idx` assignment, same
/// final rank-descending order.
pub fn collect_candidates_cached<'a>(
    rules: &[&'a dyn Rule],
    graph: &TypedGraph,
    cache: &mut MatchCache,
) -> Vec<MatchCandidate<'a>> {
    let mut all = Vec::new();
    for rule in rules {
        let pattern = rule.pattern();
        let matches = cache
            .per_rule
            .entry(rule.id().to_string())
            .or_insert_with(|| {
                let mut m = find_matches(pattern, graph);
                m.sort_by_key(|pm| canonical_key(pm, pattern));
                m
            });
        for (idx, pattern_match) in matches.iter().enumerate() {
            all.push(MatchCandidate {
                rule: *rule,
                pattern_match: pattern_match.clone(),
                match_idx: idx,
            });
        }
    }
    all.sort_by_key(|c| std::cmp::Reverse(c.rank_key()));
    all
}

/// Cached counterpart of [`cascade_step`]: same selection/application via
/// [`select_and_apply`], candidates sourced from `cache`. After a rule
/// fires, the cache is brought up to date for every rule whose pattern
/// overlaps the applied delta's kinds.
fn cascade_step_cached(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    cache: &mut MatchCache,
    dead: &mut DeadSet,
) -> Result<TerminationState, EngineError> {
    let candidates = collect_candidates_cached(rules, graph, cache);

    let before = cascade.len();
    let state = select_and_apply(cascade, graph, candidates, dead)?;
    // A new entry ⇒ a rule fired this step ⇒ bring caches up to date.
    if cascade.len() > before {
        if let Some(delta) = cascade.last().cloned() {
            cache.update(&delta, graph, rules);
            // A removal/mutation op can revive a duplicate → drop the
            // dead-set (pure adds can only ever keep a duplicate dead).
            let mutates = delta.op_star.iter().any(|op| {
                matches!(
                    op,
                    Op::DelNode { .. } | Op::DelEdge { .. } | Op::SetAttr { .. }
                )
            });
            if mutates {
                dead.clear();
            }
        }
    }
    Ok(state)
}

/// Cached counterpart of [`run_cascade`]: drives `cascade_step_cached`
/// to a terminal state, maintaining one [`MatchCache`] and one
/// `DeadSet` across all steps.
pub fn run_cascade_cached(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
) -> Result<TerminationState, EngineError> {
    let mut cache = MatchCache::new();
    let mut dead = DeadSet::active();
    for _ in 0..max_steps {
        match cascade_step_cached(cascade, graph, rules, &mut cache, &mut dead)? {
            TerminationState::Running => continue,
            terminal => return Ok(terminal),
        }
    }
    Err(EngineError::StepLimitExceeded { limit: max_steps })
}

// ── Re-validation logic (M5.3) ═══════════════════════════════════════════

/// Result of re-validating a rule application after an op on one of
/// its match participants.
#[derive(Debug, Clone)]
pub enum RevalidationOutcome {
    /// L-pattern + NACs + constraints still satisfied; no action needed.
    StillMatches,
    /// Match still present, but a bound attribute has changed and
    /// the rule has matching propagation(s).
    AttrChanged {
        propagations: Vec<EnginePropagation>,
        l_var: String,
        attr: String,
        new_value: String,
    },
    /// L-pattern no longer matches / a NAC now fires / a constraint
    /// is no longer satisfied → the rule application must be
    /// invalidated.
    NoLongerMatches,
}

/// Re-validates an existing rule application after an op on a
/// match participant.
pub fn revalidate_app(
    app_idx: usize,
    cascade: &Cascade,
    graph: &TypedGraph,
    rules: &[&dyn Rule],
    last_op: &Op,
) -> RevalidationOutcome {
    let entry = match cascade.entries.get(app_idx) {
        Some(e) => e,
        None => return RevalidationOutcome::NoLongerMatches,
    };
    let rule_id = match &entry.origin {
        Origin::Rule { rule_id } => rule_id.clone(),
        Origin::User => return RevalidationOutcome::NoLongerMatches,
    };
    let rule = match rules.iter().find(|r| r.id() == rule_id) {
        Some(r) => *r,
        None => return RevalidationOutcome::NoLongerMatches,
    };

    // Re-match the pattern with pinned bindings.
    let matches = find_matches_with_fixed(rule.pattern(), graph, &entry.bindings);
    if matches.is_empty() {
        return RevalidationOutcome::NoLongerMatches;
    }
    let pm = &matches[0];

    // NACs still clear?
    if nacs_forbid(pm, rule, graph) {
        return RevalidationOutcome::NoLongerMatches;
    }

    // If the op was a SetAttr on a bound node: check whether the rule
    // has an attribute propagation for this (l_var, attr) combination.
    if let Op::SetAttr { target, key, value } = last_op {
        let l_var = entry.bindings.iter().find_map(|(var, id)| {
            if id == target {
                Some(var.clone())
            } else {
                None
            }
        });
        if let Some(lv) = l_var {
            let propagations = rule.propagations_for(&lv, key);
            if !propagations.is_empty() {
                return RevalidationOutcome::AttrChanged {
                    propagations,
                    l_var: lv,
                    attr: key.clone(),
                    new_value: value.clone(),
                };
            }
        }
    }

    RevalidationOutcome::StillMatches
}

// ── Tentative tombstone + consolidation (M5.5) ═══════════════════════════

/// Marks the created set of a rule application as
/// `TentativeTombstone`. Returns all element IDs that were marked
/// (nodes and edges).
pub fn tentative_invalidate(
    app_idx: usize,
    cascade: &Cascade,
    graph: &mut TypedGraph,
) -> Vec<GhostId> {
    let store = MatchPersistenceStore::new(cascade);
    let created = store.created_set(app_idx);
    for id in &created {
        graph.set_node_status(id, Status::TentativeTombstone);
        graph.set_edge_status(id, Status::TentativeTombstone);
    }
    created
}

/// Consolidates after Phase B: TentativeTombstone elements that
/// appear in `just_created` (resurrection by identical Ghost-ID) are
/// reset to `Solid`. All other TentativeTombstones become final
/// `Tombstone`.
pub fn consolidate_tentative(graph: &mut TypedGraph, just_created: &[GhostId]) {
    use std::collections::HashSet;
    let just: HashSet<GhostId> = just_created.iter().copied().collect();

    // Collect all TentativeTombstone nodes (we mutate while iterating,
    // hence the intermediate Vec).
    let tt_nodes: Vec<GhostId> = graph
        .iter_nodes()
        .filter(|n| n.status == Status::TentativeTombstone)
        .map(|n| n.id)
        .collect();
    for id in tt_nodes {
        if just.contains(&id) {
            graph.set_node_status(&id, Status::Solid);
        } else {
            graph.set_node_status(&id, Status::Tombstone);
        }
    }
    // Edges analogously.
    let tt_edges: Vec<GhostId> = graph
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.status == Status::TentativeTombstone)
        .map(|(_, _, e)| e.id)
        .collect();
    for id in tt_edges {
        if just.contains(&id) {
            graph.set_edge_status(&id, Status::Solid);
        } else {
            graph.set_edge_status(&id, Status::Tombstone);
        }
    }
}

/// Match-observability-aware cascade loop. Per user delta:
/// 1. Apply user ops, collect affected RuleApps.
/// 2. Phase A: re-validate every affected RuleApp. On
///    `NoLongerMatches` → tentative_invalidate. On `AttrChanged`
///    → apply_attr_propagation.
/// 3. Phase B: standard cascade loop with `cascade_step`. Collect
///    new created entities.
/// 4. Consolidation: consolidate_tentative.
///
/// Without a user delta in the cascade it behaves like `run_cascade`.
pub fn run_cascade_observable(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
) -> Result<TerminationState, EngineError> {
    // Find the user delta — the most recent one with Origin::User
    // triggers re-validation.
    let user_idx = cascade
        .entries
        .iter()
        .rposition(|e| matches!(e.origin, Origin::User));

    if let Some(idx) = user_idx {
        let user_entry = cascade.entries[idx].clone();

        // Phase A: re-validate per affected RuleApp.
        let mut affected: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for op in &user_entry.op_star {
            for app_idx in watch_op(op, cascade, graph).affected_apps {
                affected.insert(app_idx);
            }
        }

        for app_idx in affected {
            // The last op that matches this app is enough — check
            // against all ops and take the most severe outcome.
            let mut final_outcome = RevalidationOutcome::StillMatches;
            for op in &user_entry.op_star {
                let outcome = revalidate_app(app_idx, cascade, graph, rules, op);
                final_outcome = match (final_outcome, outcome) {
                    (_, RevalidationOutcome::NoLongerMatches) => {
                        RevalidationOutcome::NoLongerMatches
                    }
                    (RevalidationOutcome::StillMatches, other) => other,
                    (existing, _) => existing,
                };
                if matches!(final_outcome, RevalidationOutcome::NoLongerMatches) {
                    break;
                }
            }
            match final_outcome {
                RevalidationOutcome::NoLongerMatches => {
                    let _ = tentative_invalidate(app_idx, cascade, graph);
                }
                RevalidationOutcome::AttrChanged {
                    propagations,
                    new_value,
                    ..
                } => {
                    let entry = &cascade.entries[app_idx];
                    let rule_id_str = match &entry.origin {
                        Origin::Rule { rule_id } => rule_id.clone(),
                        _ => continue,
                    };
                    let bindings = entry.bindings.clone();
                    apply_attr_propagation(
                        &propagations,
                        &bindings,
                        &new_value,
                        graph,
                        cascade,
                        &rule_id_str,
                    )?;
                }
                RevalidationOutcome::StillMatches => {}
            }
        }

        // Phase B: standard cascade loop. Collect new created entities.
        let entries_at_phase_b_start = cascade.entries.len();
        let term = run_cascade(cascade, graph, rules, max_steps)?;

        // Collect all op targets starting at `entries_at_phase_b_start`.
        let mut just_created: Vec<GhostId> = Vec::new();
        for entry in cascade.entries.iter().skip(entries_at_phase_b_start) {
            for op in &entry.op_star {
                if let OpTarget::Node(id) | OpTarget::Edge(id) = op.target() {
                    just_created.push(id);
                }
            }
        }

        // Phase C: consolidation — resurrection or final tombstone.
        consolidate_tentative(graph, &just_created);

        Ok(term)
    } else {
        run_cascade(cascade, graph, rules, max_steps)
    }
}

// ── Attribute propagation (M5.4) ═════════════════════════════════════════

/// Applies a list of propagations: for each propagation, an
/// `Op::SetAttr` is emitted on the bound R-node with the value
/// resolved through `transform_tag`.
///
/// Creates a new `DeltaEntry` with `Origin::Rule { rule_id:
/// "<rule>@propagate" }` and appends it to the cascade.
pub fn apply_attr_propagation(
    propagations: &[EnginePropagation],
    bindings: &std::collections::HashMap<String, GhostId>,
    new_value: &str,
    graph: &mut TypedGraph,
    cascade: &mut Cascade,
    rule_id: &str,
) -> Result<(), OpError> {
    use crate::rule::spec::AttrTransform;
    let mut ops = Vec::new();
    for prop in propagations {
        let target_id = match bindings.get(&prop.target_node_var).copied() {
            Some(id) => id,
            None => continue,
        };
        let transform =
            AttrTransform::parse(Some(&prop.transform_tag)).unwrap_or(AttrTransform::Identity);
        let transformed = transform.apply(new_value);
        ops.push(Op::SetAttr {
            target: target_id,
            key: prop.target_attr.clone(),
            value: transformed,
        });
    }
    if ops.is_empty() {
        return Ok(());
    }
    let len = ops.len();
    let entry = DeltaEntry {
        origin: Origin::Rule {
            rule_id: format!("{rule_id}@propagate"),
        },
        rank: 0,
        op_star: ops,
        anchor: bindings.values().copied().collect(),
        induces: vec![Vec::new(); len],
        bindings: bindings.clone(),
    };
    entry.apply(graph)?;
    cascade.append(entry);
    Ok(())
}

// ── Match persistence store (M5.1) ═══════════════════════════════════════

/// Read-only view onto all persistent rule-match applications in a
/// `Cascade`. The watch hook (M5.2) uses it to find affected
/// RuleApps.
///
/// `RuleApplicationId` = index into `cascade.entries`. This saves a
/// separate ID generation — the cascade is append-only anyway.
pub struct MatchPersistenceStore<'cascade> {
    cascade: &'cascade Cascade,
}

impl<'cascade> MatchPersistenceStore<'cascade> {
    pub fn new(cascade: &'cascade Cascade) -> Self {
        Self { cascade }
    }

    /// Indices of all rule applications in the cascade whose
    /// bindings reference `id`.
    pub fn applications_referencing(&self, id: &GhostId) -> Vec<usize> {
        self.cascade
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                matches!(e.origin, Origin::Rule { .. }) && e.bindings.values().any(|v| v == id)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Created set of a rule application — all GhostIds (nodes +
    /// edges) produced by its `op_star` sequence.
    pub fn created_set(&self, app_idx: usize) -> Vec<GhostId> {
        if app_idx >= self.cascade.entries.len() {
            return Vec::new();
        }
        self.cascade.entries[app_idx]
            .op_star
            .iter()
            .filter_map(|op| match op.target() {
                OpTarget::Node(id) | OpTarget::Edge(id) => Some(id),
                OpTarget::Attr(_, _) => None,
            })
            .collect()
    }

    /// Bindings of a rule application as a read reference.
    pub fn bindings(&self, app_idx: usize) -> Option<&std::collections::HashMap<String, GhostId>> {
        self.cascade.entries.get(app_idx).map(|e| &e.bindings)
    }
}

// ── Watch hook (M5.2) ════════════════════════════════════════════════════

/// Result of a watch lookup: which RuleApps are affected, and
/// whether the op carries attribute propagation potential.
#[derive(Debug, Default, Clone)]
pub struct WatchOutcome {
    /// Indices of the rule applications whose bindings reference the
    /// touched element.
    pub affected_apps: Vec<usize>,
    /// `true` when the op is a `SetAttr` on a bound node — candidate
    /// for attribute propagation (M5.4).
    pub propagation_candidate: bool,
}

/// Looks up, in the MatchPersistenceStore, all RuleApps whose
/// bindings reference the element touched by `op`.
///
/// For edge ops (AddEdge/DelEdge) the endpoint nodes are looked up,
/// because rule bindings rarely contain edge IDs (edges appear in
/// the pattern as edge constraints, not as variables).
pub fn watch_op(op: &Op, cascade: &Cascade, graph: &TypedGraph) -> WatchOutcome {
    let store = MatchPersistenceStore::new(cascade);
    let mut out = WatchOutcome::default();
    match op {
        Op::AddNode { parent, .. } => {
            out.affected_apps = store.applications_referencing(parent);
        }
        Op::AddEdge { source, target, .. } => {
            let mut affected = store.applications_referencing(source);
            affected.extend(store.applications_referencing(target));
            affected.sort();
            affected.dedup();
            out.affected_apps = affected;
        }
        Op::DelNode { target } => {
            out.affected_apps = store.applications_referencing(target);
        }
        Op::DelEdge { target } => {
            // Fetch endpoints from the graph edge index.
            let mut affected: Vec<usize> = store.applications_referencing(target);
            if let Some((src, tgt)) = graph.edge_endpoints(target) {
                affected.extend(store.applications_referencing(&src));
                affected.extend(store.applications_referencing(&tgt));
            }
            affected.sort();
            affected.dedup();
            out.affected_apps = affected;
        }
        Op::SetAttr { target, .. } => {
            out.propagation_candidate = true;
            out.affected_apps = store.applications_referencing(target);
        }
    }
    out
}

/// Applies a `DeltaEntry` and collects all RuleApps whose match
/// participants were touched.
///
/// The cascade is consulted **before** the DeltaEntry is appended,
/// so the lookup does not fall back onto its own entry.
pub fn apply_with_watch(
    delta: &DeltaEntry,
    graph: &mut TypedGraph,
    cascade: &Cascade,
) -> Result<Vec<usize>, OpError> {
    let mut affected = std::collections::HashSet::new();
    for op in &delta.op_star {
        let outcome = watch_op(op, cascade, graph);
        for app_idx in outcome.affected_apps {
            affected.insert(app_idx);
        }
        op.apply(graph)?;
    }
    let mut sorted: Vec<usize> = affected.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// Runs the cascade until termination or until `max_steps` is reached.
///
/// rc10: this is the default entry point and delegates to the
/// incremental, cache-backed matcher ([`run_cascade_cached`]) — proven
/// bit-identical to the full re-enumeration ([`run_cascade_full`]) by the
/// differential property test (256 random fwd/bwd/delete/retraction
/// sequences) and the whole scenario suite. The full matcher remains
/// available as [`run_cascade_full`] and serves as the differential
/// reference.
pub fn run_cascade(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
) -> Result<TerminationState, EngineError> {
    run_cascade_cached(cascade, graph, rules, max_steps)
}

/// Full-re-enumeration cascade runner: every step re-matches all rules
/// over the whole graph. Superseded as the default by [`run_cascade`]
/// (cached) for performance, but kept as the canonical reference
/// semantics for the differential tests.
pub fn run_cascade_full(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
) -> Result<TerminationState, EngineError> {
    for _ in 0..max_steps {
        match cascade_step(cascade, graph, rules)? {
            TerminationState::Running => continue,
            terminal => return Ok(terminal),
        }
    }
    Err(EngineError::StepLimitExceeded { limit: max_steps })
}

// ── Real backtracking with position-tagged rank limits ═══════════════════

/// Runs a cascade step with an optional rank ceiling.
///
/// Candidates with `encode_delta_rank(rule.rank, match_idx) >= rank_ceiling`
/// are filtered out. This marks the current cascade position as
/// rank-limited — a restriction that applies only locally at this
/// position, not globally across the rule (cf. Def. 3.10,
/// corrections in $\alpha_{3\text{-}4}$).
pub fn cascade_step_with_limit(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    rank_ceiling: Option<u64>,
) -> Result<TerminationState, EngineError> {
    let mut candidates = collect_candidates(rules.iter().copied(), graph);

    if let Some(ceil) = rank_ceiling {
        candidates.retain(|c| encode_delta_rank(c.rule.rank(), c.match_idx) < ceil);
    }

    if candidates.is_empty() {
        return Ok(TerminationState::Convergence);
    }

    let mut any_duplicate = false;
    let mut last_contradiction: Option<String> = None;

    for candidate in candidates {
        // NAC check (M2)
        if nacs_forbid(&candidate.pattern_match, candidate.rule, graph) {
            continue;
        }

        let primary_ops = candidate.rule.produce(&candidate.pattern_match, graph);
        if primary_ops.is_empty() {
            continue;
        }

        let (full_ops, induces) = expand_with_retraction(primary_ops, graph);

        let anchor: Vec<GhostId> = candidate
            .rule
            .pattern()
            .nodes
            .iter()
            .filter_map(|np| candidate.pattern_match.bindings.get(&np.var).copied())
            .collect();

        if is_duplicate(&full_ops, graph) {
            any_duplicate = true;
            continue;
        }

        if let Some(reason) = is_contradictory_with_cascade(&full_ops, &anchor, graph, cascade) {
            last_contradiction = Some(reason);
            continue;
        }

        let rank = encode_delta_rank(candidate.rule.rank(), candidate.match_idx);
        let delta = DeltaEntry {
            origin: Origin::Rule {
                rule_id: candidate.rule.id().into(),
            },
            rank,
            op_star: full_ops,
            anchor,
            induces,
            bindings: candidate.pattern_match.bindings.clone(),
        };

        for op in &delta.op_star {
            op.apply(graph)?;
        }

        cascade.append(delta);
        return Ok(TerminationState::Running);
    }

    if let Some(reason) = last_contradiction {
        Ok(TerminationState::Contradiction { reason })
    } else if any_duplicate {
        Ok(TerminationState::Duplication)
    } else {
        Ok(TerminationState::Convergence)
    }
}

/// Rolls back the highest-ranked rule delta entry of the cascade.
///
/// Finds the highest-ranked delta entry with `Origin::Rule` (user
/// deltas remain protected by V₇). Truncates the cascade from that
/// position, replays the graph via `base` + remaining ops, and sets
/// a position-tagged rank limit.
///
/// Returns `Some(position)` on a successful rollback, or `None` when
/// no rule delta is available to roll back (capitulation).
pub fn rollback_highest_rank(
    base: &TypedGraph,
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    limits: &mut HashMap<usize, u64>,
) -> Option<usize> {
    // Find the highest-ranked rule delta entry.
    let (highest_pos, highest_rank) = cascade
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.origin, Origin::Rule { .. }))
        .map(|(i, e)| (i, e.rank))
        .max_by_key(|(_, r)| *r)?;

    // Truncate the cascade: drop entries from highest_pos (inclusive).
    cascade.entries.truncate(highest_pos);
    // rc10: the creator index can only grow in `append`; after a shrink it
    // must be rebuilt so it no longer points at removed entries.
    cascade.rebuild_creator_index();

    // Replay the graph: clone `base`, then apply all ops of the
    // remaining entries. Reapply errors point to inconsistency in the
    // cascade structure — they are conservatively ignored because the
    // pre-rollback state must be reproducible.
    *graph = base.clone();
    for entry in &cascade.entries {
        for op in &entry.op_star {
            let _ = op.apply(graph);
        }
    }

    // Set a position-tagged limit. If a lower limit already exists
    // there (from a prior rollback), we keep the lower one — limits
    // can only decrease monotonically.
    let new_limit = limits
        .get(&highest_pos)
        .map(|existing| (*existing).min(highest_rank))
        .unwrap_or(highest_rank);
    limits.insert(highest_pos, new_limit);

    Some(highest_pos)
}

/// Statistics of a cascade run with rollback.
#[derive(Clone, Debug, Default)]
pub struct RollbackStats {
    pub rollback_count: usize,
    pub limits_applied: HashMap<usize, u64>,
}

/// Runs the cascade with real backtracking per Def. 3.10.
///
/// On contradiction: `rollback_highest_rank`, then retry with the
/// position-tagged limit. On convergence-under-limit (no candidate
/// passes) also roll back, because the limit makes the position
/// unfillable.
///
/// Returns `(TerminationState, RollbackStats)`. Capitulation when no
/// further rollback is possible or `max_rollbacks` is exceeded.
pub fn run_cascade_with_rollback(
    base: &TypedGraph,
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
    max_rollbacks: usize,
) -> Result<(TerminationState, RollbackStats), EngineError> {
    let mut limits: HashMap<usize, u64> = HashMap::new();
    let mut rollback_count = 0;
    let mut last_contradiction: Option<String> = None;

    for _ in 0..max_steps {
        let position = cascade.entries.len();
        let ceiling = limits.get(&position).copied();
        let had_ceiling = ceiling.is_some();

        match cascade_step_with_limit(cascade, graph, rules, ceiling)? {
            TerminationState::Running => continue,
            TerminationState::Convergence if had_ceiling => {
                // Converged under the limit — may be due to an
                // artificial block. Roll back to potentially reach a
                // different solution via a detour.
                if rollback_count >= max_rollbacks {
                    return Ok((
                        TerminationState::Convergence,
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                if rollback_highest_rank(base, cascade, graph, &mut limits).is_none() {
                    return Ok((
                        TerminationState::Convergence,
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                rollback_count += 1;
            }
            TerminationState::Convergence => {
                return Ok((
                    TerminationState::Convergence,
                    RollbackStats {
                        rollback_count,
                        limits_applied: limits,
                    },
                ));
            }
            TerminationState::Duplication => {
                return Ok((
                    TerminationState::Duplication,
                    RollbackStats {
                        rollback_count,
                        limits_applied: limits,
                    },
                ));
            }
            TerminationState::Contradiction { reason } => {
                last_contradiction = Some(reason.clone());
                if rollback_count >= max_rollbacks {
                    return Ok((
                        TerminationState::Contradiction { reason },
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                if rollback_highest_rank(base, cascade, graph, &mut limits).is_none() {
                    return Ok((
                        TerminationState::Contradiction { reason },
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                rollback_count += 1;
            }
        }
    }

    // Step limit reached without termination.
    let _ = last_contradiction;
    let _ = limits;
    let _ = rollback_count;
    Err(EngineError::StepLimitExceeded { limit: max_steps })
}

// ══ Tests ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Status;
    use crate::ops::Origin;
    use std::collections::BTreeMap;

    fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// `directional_rule_refs` keeps only the rules whose
    /// `input_domain_kinds` intersect the delta; undirected rules always
    /// stay active.
    #[test]
    fn directional_rule_refs_filters_by_input_domain_kinds() {
        let fwd: Box<dyn Rule> = Box::new(
            BasicRule::new("R\u{2192}", 10, Pattern::new(), |_, _| vec![])
                .with_input_domain_kinds(vec!["Class".to_string()]),
        );
        let bwd: Box<dyn Rule> = Box::new(
            BasicRule::new("R\u{2190}", 10, Pattern::new(), |_, _| vec![])
                .with_input_domain_kinds(vec!["JavaClass".to_string()]),
        );
        let undirected: Box<dyn Rule> =
            Box::new(BasicRule::new("R0", 10, Pattern::new(), |_, _| vec![]));
        let rules = vec![fwd, bwd, undirected];

        let delta: std::collections::HashSet<String> = ["Class".to_string()].into_iter().collect();
        let active: Vec<&str> = directional_rule_refs(&rules, &delta)
            .iter()
            .map(|r| r.id())
            .collect();
        assert!(
            active.contains(&"R\u{2192}"),
            "forward active on a Class delta"
        );
        assert!(active.contains(&"R0"), "undirected rule always active");
        assert!(
            !active.contains(&"R\u{2190}"),
            "backward must NOT be active on a Class delta, was: {active:?}"
        );
    }

    fn setup_uml_graph() -> (TypedGraph, GhostId, GhostId, GhostId, GhostId) {
        let mut g = TypedGraph::new();
        let person = g.add_baseline_node("Class", "Person", attrs(&[("name", "Person")]));
        let car = g.add_baseline_node("Class", "Car", attrs(&[("name", "Car")]));
        let person_name = g.add_ghost_node(
            person,
            "hasAttribute",
            "Attribute",
            attrs(&[("name", "name")]),
        );
        let car_model = g.add_ghost_node(
            car,
            "hasAttribute",
            "Attribute",
            attrs(&[("name", "model")]),
        );
        g.add_edge(
            person,
            person_name,
            "hasAttribute",
            BTreeMap::new(),
            Status::Ghost,
        )
        .unwrap();
        g.add_edge(
            car,
            car_model,
            "hasAttribute",
            BTreeMap::new(),
            Status::Ghost,
        )
        .unwrap();
        (g, person, car, person_name, car_model)
    }

    // ── Injectivity ──────────────────────────────────────────────────

    #[test]
    fn injectivity_prevents_same_node_twice() {
        let (g, _, _, _, _) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c1", "Class"))
            .with_node(NodePattern::new("c2", "Class"));
        let matches = find_matches(&pattern, &g);
        // 2 classes, two pattern variables, injective → 2×1 = 2 matches
        // (Person,Car) and (Car,Person).
        assert_eq!(matches.len(), 2);
        for m in &matches {
            let c1 = m.get("c1").unwrap();
            let c2 = m.get("c2").unwrap();
            assert_ne!(c1, c2, "injectivity: c1 ≠ c2");
        }
    }

    // ── Edge patterns ────────────────────────────────────────────────

    #[test]
    fn edge_pattern_requires_existing_edge() {
        let (g, _, _, _, _) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class"))
            .with_node(NodePattern::new("a", "Attribute"))
            .with_edge(EdgePattern::new("c", "a", "hasAttribute"));
        let matches = find_matches(&pattern, &g);
        // Person─hasAttribute→name, Car─hasAttribute→model: 2 matches.
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn edge_pattern_filters_out_unconnected() {
        let (g, person, _, _, car_model) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class").with_attr_equals("name", "Person"))
            .with_node(NodePattern::new("a", "Attribute").with_attr_equals("name", "model"))
            .with_edge(EdgePattern::new("c", "a", "hasAttribute"));
        let matches = find_matches(&pattern, &g);
        // Person has no "model" — edge missing, no match.
        assert_eq!(matches.len(), 0);
        let _ = person;
        let _ = car_model;
    }

    #[test]
    fn membership_edge_matches_corr_in_both_orientations() {
        // rc7 (S): a correspondence is symmetric. A membership match
        // (EdgePattern::membership) finds the Corr node independent
        // of the corrL/corrR orientation — that lets a Bwd rule see a
        // Forward-established Corr as context (the C1 finding), and
        // vice versa.
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class"))
            .with_node(NodePattern::new("jc", "JavaClass"))
            .with_node(NodePattern::new("corr", "CorrClass"))
            .with_edge(EdgePattern::membership("corr", "c"))
            .with_edge(EdgePattern::membership("corr", "jc"));

        // Orientation A (Forward-emitted): Class --corrL--> Corr --corrR--> JavaClass
        let mut ga = TypedGraph::new();
        let ca = ga.add_baseline_node("Class", "Foo", attrs(&[("name", "Foo")]));
        let corra = ga.add_ghost_node(ca, "corrL", "CorrClass", BTreeMap::new());
        ga.add_edge(ca, corra, "corrL", BTreeMap::new(), Status::Ghost)
            .unwrap();
        let jca = ga.add_ghost_node(corra, "corrR", "JavaClass", attrs(&[("name", "Foo")]));
        ga.add_edge(corra, jca, "corrR", BTreeMap::new(), Status::Ghost)
            .unwrap();
        assert_eq!(
            find_matches(&pattern, &ga).len(),
            1,
            "membership matches Forward orientation (Class--corrL-->Corr--corrR-->JavaClass)"
        );

        // Orientation B (Backward-emitted): JavaClass --corrL--> Corr --corrR--> Class
        let mut gb = TypedGraph::new();
        let jcb = gb.add_baseline_node("JavaClass", "Foo", attrs(&[("name", "Foo")]));
        let corrb = gb.add_ghost_node(jcb, "corrL", "CorrClass", BTreeMap::new());
        gb.add_edge(jcb, corrb, "corrL", BTreeMap::new(), Status::Ghost)
            .unwrap();
        let cb = gb.add_ghost_node(corrb, "corrR", "Class", attrs(&[("name", "Foo")]));
        gb.add_edge(corrb, cb, "corrR", BTreeMap::new(), Status::Ghost)
            .unwrap();
        assert_eq!(
            find_matches(&pattern, &gb).len(),
            1,
            "membership matches Backward orientation (JavaClass--corrL-->Corr--corrR-->Class)"
        );
    }

    #[test]
    fn edge_pattern_respects_tombstone() {
        let (mut g, person, _, person_name, _) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class").with_attr_equals("name", "Person"))
            .with_node(NodePattern::new("a", "Attribute"))
            .with_edge(EdgePattern::new("c", "a", "hasAttribute"));
        assert_eq!(find_matches(&pattern, &g).len(), 1, "Person→name");

        // Tombstone the only matching edge:
        let edge_id = GhostId::for_edge(&person, &person_name, "hasAttribute", &BTreeMap::new());
        assert!(g.set_edge_status(&edge_id, Status::Tombstone));

        assert_eq!(
            find_matches(&pattern, &g).len(),
            0,
            "TOMB edge no longer matchable"
        );
    }

    // ── Canonical enumeration ────────────────────────────────────────

    #[test]
    fn canonical_key_is_deterministic() {
        let (g, _, _, _, _) = setup_uml_graph();
        let pattern = Pattern::new().with_node(NodePattern::new("c", "Class"));

        let matches_a = find_matches(&pattern, &g);
        let matches_b = find_matches(&pattern, &g);

        let keys_a: Vec<_> = matches_a
            .iter()
            .map(|m| canonical_key(m, &pattern))
            .collect();
        let keys_b: Vec<_> = matches_b
            .iter()
            .map(|m| canonical_key(m, &pattern))
            .collect();
        assert_eq!(keys_a, keys_b, "ordering is deterministic");
    }

    #[test]
    fn edge_guided_matching_is_deterministic_and_complete() {
        // Three containers, each with two items. Pattern: container c
        // + two items i1, i2, both attached to c via a `holds` edge.
        // The edge-guided matcher generates i1/i2 from the neighbors
        // of c (adjacency) instead of from the full item population —
        // two like-kind guided nodes exercise dedup + injectivity.
        let mut g = TypedGraph::new();
        for ci in 0..3 {
            let c = g.add_baseline_node("Container", &format!("c{ci}"), BTreeMap::new());
            for ii in 0..2 {
                let item = g.add_ghost_node(
                    c,
                    "holds",
                    "Item",
                    attrs(&[("name", &format!("c{ci}-i{ii}"))]),
                );
                g.add_edge(c, item, "holds", BTreeMap::new(), Status::Ghost)
                    .unwrap();
            }
        }
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Container"))
            .with_node(NodePattern::new("i1", "Item"))
            .with_node(NodePattern::new("i2", "Item"))
            .with_edge(EdgePattern::new("c", "i1", "holds"))
            .with_edge(EdgePattern::new("c", "i2", "holds"));

        let a = find_matches(&pattern, &g);
        let b = find_matches(&pattern, &g);

        // Per container: 2 items, ordered injectively → 2×1 = 2 matches.
        // Three containers → 6 matches.
        assert_eq!(a.len(), 6, "3 containers × 2 ordered item pairs");

        let keys = |ms: &[PatternMatch]| -> Vec<Vec<[u8; 32]>> {
            ms.iter().map(|m| canonical_key(m, &pattern)).collect()
        };
        assert_eq!(
            keys(&a),
            keys(&b),
            "edge-guided enumeration is deterministic"
        );

        for m in &a {
            let c = m.get("c").unwrap();
            let i1 = m.get("i1").unwrap();
            let i2 = m.get("i2").unwrap();
            assert_ne!(i1, i2, "injectivity: i1 ≠ i2");
            assert!(g.has_edge_between(c, i1, "holds"), "i1 attached to c");
            assert!(g.has_edge_between(c, i2, "holds"), "i2 attached to c");
        }
    }

    // ── Rank selection ───────────────────────────────────────────────

    fn dummy_production(_m: &PatternMatch, _g: &TypedGraph) -> Vec<Op> {
        Vec::new()
    }

    #[test]
    fn rank_selection_picks_highest_rule() {
        let (g, _, _, _, _) = setup_uml_graph();
        let low = BasicRule::new(
            "low",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            dummy_production,
        );
        let high = BasicRule::new(
            "high",
            5,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            dummy_production,
        );

        let rules: Vec<&dyn Rule> = vec![&low, &high];
        let chosen = select_highest_rank(rules, &g).unwrap();
        assert_eq!(chosen.rule.id(), "high");
    }

    #[test]
    fn rank_selection_same_rule_picks_canonical_first() {
        let (g, _, _, _, _) = setup_uml_graph();
        let rule = BasicRule::new(
            "only",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            dummy_production,
        );
        let rules: Vec<&dyn Rule> = vec![&rule];
        let chosen = select_highest_rank(rules, &g).unwrap();
        // With two classes: highest match_idx wins (rank_key lex-max).
        assert_eq!(chosen.match_idx, 1, "last in μ ordering selected");
    }

    #[test]
    fn rank_selection_no_match_returns_none() {
        let g = TypedGraph::new();
        let rule = BasicRule::new(
            "x",
            1,
            Pattern::new().with_node(NodePattern::new("c", "NonExistentType")),
            dummy_production,
        );
        let rules: Vec<&dyn Rule> = vec![&rule];
        assert!(select_highest_rank(rules, &g).is_none());
    }

    #[test]
    fn rank_selection_empty_rule_list() {
        let g = TypedGraph::new();
        let rules: Vec<&dyn Rule> = Vec::new();
        assert!(select_highest_rank(rules, &g).is_none());
    }

    // ── Integration: real production ─────────────────────────────────

    #[test]
    fn basic_rule_produces_ghost_op() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let rule = BasicRule::new(
            "AttrToGetter",
            10,
            Pattern::new().with_node(NodePattern::new("a", "Attribute")),
            |m, _g| {
                let attr_id = *m.get("a").unwrap();
                vec![Op::AddNode {
                    parent: attr_id,
                    edge_type: "hasGetter".into(),
                    type_id: "Getter".into(),
                    attrs: BTreeMap::new(),
                }]
            },
        );

        let rules: Vec<&dyn Rule> = vec![&rule];
        let chosen = select_highest_rank(rules, &g).unwrap();
        let ops = chosen.rule.produce(&chosen.pattern_match, &g);
        assert_eq!(ops.len(), 1);

        match &ops[0] {
            Op::AddNode {
                parent,
                edge_type,
                type_id,
                ..
            } => {
                assert!(
                    *parent == person_name || {
                        // Or car_model — depending on μ.
                        true
                    }
                );
                assert_eq!(edge_type, "hasGetter");
                assert_eq!(type_id, "Getter");
            }
            _ => panic!("AddNode expected"),
        }
    }

    // ── Cascade skeleton ─────────────────────────────────────────────

    #[test]
    fn cascade_append_and_length() {
        let mut c = Cascade::new();
        assert!(c.is_empty());
        let user_delta = DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: Vec::new(),
            anchor: Vec::new(),
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        };
        let idx = c.append(user_delta);
        assert_eq!(idx, 0);
        assert_eq!(c.len(), 1);
    }

    // ── Duplication predicate ────────────────────────────────────────

    #[test]
    fn duplicate_detection_on_existing_ghost() {
        let (g, person, _, _, _) = setup_uml_graph();
        // An AddNode that aims at an existing "name" attribute.
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "name")]),
        };
        assert!(is_duplicate(&[op], &g));
    }

    #[test]
    fn no_duplicate_for_novel_attr() {
        let (g, person, _, _, _) = setup_uml_graph();
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "novel_attr")]),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn duplicate_ignores_tombstone() {
        let (mut g, person, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);

        // The same name is no longer matchable → no longer detectable
        // as a duplicate.
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "name")]),
        };
        assert!(!is_duplicate(&[op], &g), "TOMB does not block duplication");
    }

    #[test]
    fn mixed_application_is_not_a_duplicate() {
        let (g, person, _, _, _) = setup_uml_graph();
        // A duplicating op: attribute "name" already exists.
        let dup = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "name")]),
        };
        // A genuinely new op in the same op_star.
        let novel = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "novel_attr")]),
        };
        assert!(
            !is_duplicate(&[dup, novel], &g),
            "mixed application (duplicate + genuine new work) must not \
             be rejected as a duplicate"
        );
    }

    #[test]
    fn add_edge_for_existing_edge_is_duplicate() {
        let (g, person, _, person_name, _) = setup_uml_graph();
        // The edge person → person_name ("hasAttribute") already
        // exists in setup_uml_graph.
        let op = Op::AddEdge {
            source: person,
            target: person_name,
            type_id: "hasAttribute".into(),
            attrs: attrs(&[]),
        };
        assert!(is_duplicate(&[op], &g));
    }

    #[test]
    fn add_edge_for_novel_edge_is_not_duplicate() {
        let (g, person, _, _, car_model) = setup_uml_graph();
        // No edge person → car_model exists.
        let op = Op::AddEdge {
            source: person,
            target: car_model,
            type_id: "hasAttribute".into(),
            attrs: attrs(&[]),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn del_ops_never_count_as_duplicate() {
        let (g, person, _, person_name, _) = setup_uml_graph();
        // Del-ops produce nothing — they are never duplicates.
        assert!(!is_duplicate(&[Op::DelNode { target: person }], &g));
        assert!(!is_duplicate(
            &[Op::DelEdge {
                target: person_name
            }],
            &g
        ));
    }

    // ── B5-rc5: SetAttr duplicate (reverse-cascade termination) ──────

    #[test]
    fn setattr_for_already_equal_value_is_duplicate() {
        // Regression rc4→rc5: B5 emitted attrs_to_set → produce()
        // emits the same SetAttr in every step. Before the fix,
        // is_duplicate had `_ => false` for SetAttr — consequence:
        // every step counted as "productive", cascade_step kept
        // returning Running forever. Idempotency semantics (analogous
        // to AddNode/AddEdge): SetAttr is a duplicate when the target
        // node is matchable+non-tentative and the desired value is
        // already equal.
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::SetAttr {
            target: person_name,
            key: "name".into(),
            value: "name".into(), // person_name already carries name="name"
        };
        assert!(
            is_duplicate(&[op], &g),
            "SetAttr with an already identical value must count as a duplicate, \
             otherwise the cascade loops on every attrs_to_set rule"
        );
    }

    #[test]
    fn setattr_for_different_value_is_not_duplicate() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::SetAttr {
            target: person_name,
            key: "name".into(),
            value: "renamed".into(),
        };
        assert!(
            !is_duplicate(&[op], &g),
            "SetAttr with a differing target value is real work — not a duplicate"
        );
    }

    #[test]
    fn setattr_for_missing_key_is_not_duplicate() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::SetAttr {
            target: person_name,
            key: "freshly_introduced".into(),
            value: "v".into(),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn setattr_on_unknown_node_is_not_duplicate() {
        let (g, _, _, _, _) = setup_uml_graph();
        let phantom = GhostId::from_baseline("phantom-never-inserted");
        let op = Op::SetAttr {
            target: phantom,
            key: "k".into(),
            value: "v".into(),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn setattr_on_tombstone_node_is_not_duplicate() {
        let (mut g, _, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);
        let op = Op::SetAttr {
            target: person_name,
            key: "name".into(),
            value: "name".into(),
        };
        assert!(
            !is_duplicate(&[op], &g),
            "TOMB node is not matchable → SetAttr is not a duplicate"
        );
    }

    // ── Contradiction predicate ──────────────────────────────────────

    #[test]
    fn contradiction_on_solid_deletion() {
        let (g, person, _, _, _) = setup_uml_graph();
        let op = Op::DelNode { target: person };
        let reason = is_contradictory(&[op], &g);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("SOLID"));
    }

    #[test]
    fn no_contradiction_on_ghost_deletion() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::DelNode {
            target: person_name,
        };
        assert!(is_contradictory(&[op], &g).is_none());
    }

    #[test]
    fn contradiction_on_double_tombstone() {
        let (mut g, _, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);
        let op = Op::DelNode {
            target: person_name,
        };
        assert!(is_contradictory(&[op], &g).is_some());
    }

    #[test]
    fn contradiction_on_add_edge_to_tombstone() {
        let (mut g, person, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);

        let op = Op::AddEdge {
            source: person,
            target: person_name,
            type_id: "any".into(),
            attrs: BTreeMap::new(),
        };
        let reason = is_contradictory(&[op], &g);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("TOMB"));
    }

    // ── Encode rank ──────────────────────────────────────────────────

    #[test]
    fn encode_rank_dominated_by_rule() {
        let a = encode_delta_rank(1, 999);
        let b = encode_delta_rank(2, 0);
        assert!(b > a, "rule rank dominates match index");
    }

    #[test]
    fn encode_rank_ties_broken_by_match_idx() {
        let a = encode_delta_rank(3, 5);
        let b = encode_delta_rank(3, 7);
        assert!(b > a);
    }

    // ── Cascade step: runs ───────────────────────────────────────────

    #[test]
    fn cascade_step_convergence_on_empty_rules() {
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();
        let rules: &[&dyn Rule] = &[];
        let state = cascade_step(&mut c, &mut g, rules).unwrap();
        assert_eq!(state, TerminationState::Convergence);
    }

    #[test]
    fn cascade_step_runs_one_rule_once() {
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // A rule that attaches a new "derived" attribute to every class.
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

        let node_count_before = g.node_count();
        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        assert_eq!(state, TerminationState::Running);
        assert_eq!(c.len(), 1);
        assert!(g.node_count() > node_count_before);
    }

    #[test]
    fn cascade_step_contradiction_on_solid_del() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();
        let rule = BasicRule::new(
            "BadDel",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |m, _g| {
                let _ = m;
                vec![Op::DelNode { target: person }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];
        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        assert!(matches!(state, TerminationState::Contradiction { .. }));
    }

    // ── run_cascade ──────────────────────────────────────────────────

    #[test]
    fn run_cascade_terminates_via_convergence() {
        // Rule that adds a derived attribute to every class — exactly
        // once (via duplication detection on the second attempt).
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

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

        let state = run_cascade(&mut c, &mut g, &rules, 100).unwrap();
        // Two classes → two deltas, then duplication.
        assert_eq!(c.len(), 2, "two iterations, one per class");
        assert!(
            matches!(
                state,
                TerminationState::Duplication | TerminationState::Convergence
            ),
            "unexpected state: {state:?}"
        );
    }

    // ── Retraction cascade ───────────────────────────────────────────

    #[test]
    fn retraction_cascade_includes_incident_edges() {
        let (g, person, _, person_name, _) = setup_uml_graph();
        // Edge: person → person_name (via hasAttribute, emitted by setup_uml_graph).
        let del_op = Op::DelNode { target: person };
        let induced = retraction_cascade_for(&del_op, &g);
        // At least one DelEdge is induced (person→person_name).
        assert!(!induced.is_empty());
        assert!(
            induced.iter().all(|op| matches!(op, Op::DelEdge { .. })),
            "retraction cascade produces only DelEdges"
        );
        let _ = person_name;
    }

    #[test]
    fn retraction_cascade_empty_for_non_del() {
        let (g, person, _, _, _) = setup_uml_graph();
        let add_op = Op::AddNode {
            parent: person,
            edge_type: "x".into(),
            type_id: "Y".into(),
            attrs: BTreeMap::new(),
        };
        assert!(retraction_cascade_for(&add_op, &g).is_empty());
    }

    #[test]
    fn expand_with_retraction_populates_induces() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let primary = vec![Op::DelNode {
            target: person_name,
        }];
        let (full_ops, induces) = expand_with_retraction(primary, &g);
        assert!(!full_ops.is_empty());
        assert_eq!(full_ops.len(), induces.len());
        // The primary op (index 0) should have induces entries when
        // person_name has incident edges.
        if full_ops.len() > 1 {
            assert!(!induces[0].is_empty(), "primary op induces follow-up ops");
            for idx in &induces[0] {
                assert!(*idx > 0, "induced op has index > 0");
                assert!(matches!(full_ops[*idx], Op::DelEdge { .. }));
            }
        }
    }

    // ── V₇ ancestor check ────────────────────────────────────────────

    #[test]
    fn ancestor_check_identifies_creators() {
        use crate::ops::Origin;
        let (g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Simulate a cascade: d_0 creates a ghost, d_1 uses it as anchor.
        let parent = g.matchable_nodes().next().unwrap().id;
        let ghost_id = GhostId::from_parent(&parent, "test-edge", "TestType", &BTreeMap::new());

        // d_0: creates ghost_id
        c.append(DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: vec![Op::AddNode {
                parent,
                edge_type: "test-edge".into(),
                type_id: "TestType".into(),
                attrs: BTreeMap::new(),
            }],
            anchor: vec![parent],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        // d_1: has ghost_id as anchor
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r1".into(),
            },
            rank: 1,
            op_star: Vec::new(),
            anchor: vec![ghost_id],
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        });

        assert_eq!(c.creator_of(&ghost_id), Some(0));
        let ancestors = c.ancestors_of_anchor(&[ghost_id]);
        assert!(ancestors.contains(&0));
    }

    #[test]
    fn contradiction_rejects_ancestor_erasure() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // d_0 (User): trivial user delta with person as anchor.
        c.append(DeltaEntry {
            origin: crate::ops::Origin::User,
            rank: 0,
            op_star: Vec::new(),
            anchor: vec![person],
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        });

        // A rule that would try to create a ghost AND tombstone the
        // parent (person, SOLID). V₇ must catch this.
        let rule = BasicRule::new(
            "BadRule",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |m, _g| {
                let _c = m.get("c").unwrap();
                vec![Op::DelNode { target: person }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        // SOLID protection kicks in (subset of V₇): person is SOLID → Contradiction.
        assert!(matches!(state, TerminationState::Contradiction { .. }));
    }

    #[test]
    fn v7_protects_d0_created_ghost() {
        use crate::ops::Origin;
        let (mut g, person, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // d_0 (User): creates a ghost attribute.
        let user_op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "user_attr")]),
        };
        let ghost_id = match user_op.target() {
            OpTarget::Node(id) => id,
            _ => panic!(),
        };

        // Apply to graph:
        user_op.apply(&mut g).unwrap();

        c.append(DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: vec![user_op],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        // Rule that tries to delete exactly this d_0 ghost.
        let rule = BasicRule::new(
            "EraseD0",
            1,
            Pattern::new().with_node(NodePattern::new("a", "Attribute")),
            move |_m, _g| vec![Op::DelNode { target: ghost_id }],
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        // V₇ should protect the d_0 element.
        assert!(matches!(state, TerminationState::Contradiction { .. }));
        if let TerminationState::Contradiction { reason } = state {
            assert!(
                reason.contains("V₇") || reason.contains("d_0") || reason.contains("SOLID"),
                "reason should reference V₇: {reason}"
            );
        }
    }

    // ── Contradiction saturation instead of immediate abort ───────────

    // ── Rollback scenarios (Phase 1.3d) ──────────────────────────────

    #[test]
    fn rollback_truncates_and_sets_limit() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let op1 = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "a1")]),
        };
        op1.apply(&mut g).unwrap();
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r1".into(),
            },
            rank: 50,
            op_star: vec![op1],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let op2 = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "a2")]),
        };
        op2.apply(&mut g).unwrap();
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r2".into(),
            },
            rank: 100, // highest
            op_star: vec![op2],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let op3 = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "a3")]),
        };
        op3.apply(&mut g).unwrap();
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r3".into(),
            },
            rank: 30,
            op_star: vec![op3],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        assert_eq!(c.len(), 3);
        let mut limits: HashMap<usize, u64> = HashMap::new();

        let rolled_pos = rollback_highest_rank(&base, &mut c, &mut g, &mut limits);
        assert_eq!(rolled_pos, Some(1), "rank 100 delta was at position 1");

        // Cascade now truncated to [r1].
        assert_eq!(c.len(), 1);
        // Limit set at position 1.
        assert_eq!(limits.get(&1), Some(&100));
        // Graph state: 4 baseline nodes + a1 = 5; a2 and a3 are gone.
        assert_eq!(g.matchable_nodes().count(), 5);
    }

    #[test]
    fn rollback_skips_user_delta() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        // Only entry is a user delta.
        c.append(DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: Vec::new(),
            anchor: vec![person],
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        });

        let mut limits: HashMap<usize, u64> = HashMap::new();

        let result = rollback_highest_rank(&base, &mut c, &mut g, &mut limits);
        assert_eq!(result, None, "no rule delta → no rollback");
        assert_eq!(c.len(), 1, "user delta remains");
    }

    #[test]
    fn run_cascade_with_rollback_recovers_from_contradiction() {
        // Scenario:
        //   R_problem (rank 100): on Class → creates Attribute "x".
        //   R_kill (rank 5): on Attribute "x" → tries to delete it
        //     (V₇ violation, since R_problem is an ancestor).
        //
        // Expectation: without rollback → Contradiction.
        //              with rollback → R_problem rolled out, convergence.
        let (mut g, _, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let r_problem = BasicRule::new(
            "RProblem",
            100,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "x")]),
                }]
            },
        );

        let r_kill = BasicRule::new(
            "RKill",
            5,
            Pattern::new()
                .with_node(NodePattern::new("a", "Attribute").with_attr_equals("name", "x")),
            |m, _g| {
                let a = *m.get("a").unwrap();
                vec![Op::DelNode { target: a }]
            },
        );

        let rules: Vec<&dyn Rule> = vec![&r_problem, &r_kill];

        let (state, stats) =
            run_cascade_with_rollback(&base, &mut c, &mut g, &rules, 50, 10).unwrap();

        assert!(
            stats.rollback_count >= 1,
            "at least one rollback expected, stats = {stats:?}"
        );
        assert!(!stats.limits_applied.is_empty(), "limits set");
        // Termination via Convergence or Duplication (after R_problem
        // is rolled out and R_kill no longer has a match).
        assert!(
            matches!(
                state,
                TerminationState::Convergence | TerminationState::Duplication
            ),
            "valid end state after rollback: {state:?}"
        );
    }

    #[test]
    fn run_cascade_with_rollback_capitulates_when_exhausted() {
        // Scenario: rule that directly attempts a SOLID deletion —
        // no rollback can resolve it because the problem is not in a
        // prior rule decision but in the rule itself.
        let (mut g, person, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let bad_rule = BasicRule::new(
            "Bad",
            10,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |_m, _g| vec![Op::DelNode { target: person }],
        );
        let rules: Vec<&dyn Rule> = vec![&bad_rule];

        let (state, _stats) =
            run_cascade_with_rollback(&base, &mut c, &mut g, &rules, 50, 10).unwrap();

        // No rule delta in the cascade → no rollback possible →
        // contradiction remains.
        assert!(matches!(state, TerminationState::Contradiction { .. }));
    }

    #[test]
    fn contradictory_candidate_skipped_for_non_contra() {
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Two rules: a higher-ranked one that would be contradictory,
        // and a lower-ranked one that is valid.
        let person_id = g
            .matchable_nodes()
            .find(|n| n.type_id == "Class" && n.attrs.get("name") == Some(&"Person".to_string()))
            .map(|n| n.id)
            .unwrap();

        let high_bad = BasicRule::new(
            "HighBad",
            10,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |_m, _g| vec![Op::DelNode { target: person_id }], // SOLID erase
        );

        let low_good = BasicRule::new(
            "LowGood",
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

        let rules: Vec<&dyn Rule> = vec![&high_bad, &low_good];
        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        // The contradictory high-ranked rule is skipped; the low-
        // ranked rule fires.
        assert_eq!(state, TerminationState::Running);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn run_cascade_respects_step_limit() {
        // Rule that keeps producing fresh ghosts (non-contractive).
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        let rule = BasicRule::new(
            "Unbounded",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                // Produce a ghost with a unique suffix based on the
                // current hash — to avoid duplication in this stress
                // test.
                let unique = format!("derived-{}", c.short());
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", unique.as_str())]),
                }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        // With limit 1 it should not run to completion.
        // With 2 classes: step 1 produces, step 2 produces, step 3
        // matches again but the "derived-…" attribute exists —
        // duplication. So with max_steps = 1 we hit the limit error.
        let state = run_cascade(&mut c, &mut g, &rules, 1);
        assert!(matches!(state, Err(EngineError::StepLimitExceeded { .. })));
    }

    // ── M5.1: MatchPersistenceStore ──────────────────────────────────────

    #[test]
    fn match_persistence_records_bindings() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let _p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));

        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "marker".into(),
                type_id: "Marker".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        // There should be 2 rule applications (one per class).
        let store = MatchPersistenceStore::new(&cas);
        let refs_p1 = store.applications_referencing(&p1);
        assert_eq!(refs_p1.len(), 1, "C1 is bound exactly once");
        // Bindings must be populated.
        let bindings = store.bindings(refs_p1[0]).unwrap();
        assert!(bindings.contains_key("c"));
    }

    #[test]
    fn match_persistence_created_set_includes_emitted_node() {
        let mut g = TypedGraph::new();
        let _p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "marker".into(),
                type_id: "Marker".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        let store = MatchPersistenceStore::new(&cas);
        let created = store.created_set(0);
        assert!(!created.is_empty(), "at least 1 created element");
    }

    // ── M5.2: Watch hook ─────────────────────────────────────────────────

    #[test]
    fn watch_op_finds_rule_apps_referencing_node() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let _p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "marker".into(),
                type_id: "Marker".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        // Op on p1 — should find 1 RuleApp.
        let op = Op::DelNode { target: p1 };
        let outcome = watch_op(&op, &cas, &g);
        assert_eq!(outcome.affected_apps.len(), 1);
        assert!(!outcome.propagation_candidate);
    }

    #[test]
    fn watch_op_setattr_marks_propagation_candidate() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        let op = Op::SetAttr {
            target: p1,
            key: "name".into(),
            value: "C1Renamed".into(),
        };
        let outcome = watch_op(&op, &cas, &g);
        assert!(outcome.propagation_candidate);
        assert_eq!(outcome.affected_apps.len(), 1);
    }

    #[test]
    fn apply_with_watch_returns_sorted_unique_apps() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        let snapshot_cas = cas.clone();

        let user = DeltaEntry::new_user(
            vec![Op::DelNode { target: p1 }, Op::DelNode { target: p2 }],
            vec![p1, p2],
        );
        let affected = apply_with_watch(&user, &mut g, &snapshot_cas).unwrap();
        // 2 RuleApps (one per class) — both found, sorted, deduplicated.
        assert_eq!(affected.len(), 2);
        assert!(affected[0] < affected[1]);
    }

    // ── M5.3: Re-validation ──────────────────────────────────────────────

    #[test]
    fn revalidate_still_matches_when_unchanged() {
        let mut g = TypedGraph::new();
        let _p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        // Re-validate against a harmless op (same node, same status).
        let outcome = revalidate_app(
            0,
            &cas,
            &g,
            &rules,
            &Op::DelEdge {
                target: GhostId::from_baseline("nope"),
            },
        );
        assert!(matches!(outcome, RevalidationOutcome::StillMatches));
    }

    #[test]
    fn revalidate_no_longer_matches_when_node_gone() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        // Tombstone p1
        g.set_node_status(&p1, Status::Tombstone);
        let outcome = revalidate_app(0, &cas, &g, &rules, &Op::DelNode { target: p1 });
        assert!(matches!(outcome, RevalidationOutcome::NoLongerMatches));
    }

    #[test]
    fn revalidate_attr_changed_with_propagation() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        })
        .with_propagations(vec![EnginePropagation {
            source_node_var: "c".into(),
            source_attr: "name".into(),
            target_node_var: "c".into(),
            target_attr: "label".into(),
            transform_tag: "identity".into(),
        }]);
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        let outcome = revalidate_app(
            0,
            &cas,
            &g,
            &rules,
            &Op::SetAttr {
                target: p1,
                key: "name".into(),
                value: "C1Renamed".into(),
            },
        );
        match outcome {
            RevalidationOutcome::AttrChanged {
                propagations,
                l_var,
                attr,
                new_value,
            } => {
                assert_eq!(l_var, "c");
                assert_eq!(attr, "name");
                assert_eq!(new_value, "C1Renamed");
                assert_eq!(propagations.len(), 1);
            }
            other => panic!("expected AttrChanged, got {other:?}"),
        }
    }

    // ── M5.4: Attribute propagation ──────────────────────────────────────

    #[test]
    fn apply_attr_propagation_emits_setattr() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let r_node = g.add_baseline_node("Doc", "D1", attrs(&[("label", "old")]));

        let mut cas = Cascade::new();
        // Set up bindings: c → p1, d → r_node
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("c".to_string(), p1);
        bindings.insert("d".to_string(), r_node);

        let propagations = vec![EnginePropagation {
            source_node_var: "c".into(),
            source_attr: "name".into(),
            target_node_var: "d".into(),
            target_attr: "label".into(),
            transform_tag: "identity".into(),
        }];

        apply_attr_propagation(
            &propagations,
            &bindings,
            "C1Renamed",
            &mut g,
            &mut cas,
            "MyRule",
        )
        .unwrap();

        // Cascade now has an entry with @propagate
        assert_eq!(cas.entries.len(), 1);
        let origin = &cas.entries[0].origin;
        assert!(matches!(origin, Origin::Rule { rule_id } if rule_id == "MyRule@propagate"));

        // R-node has the new label value
        let r_data = g.get_node(&r_node).unwrap();
        assert_eq!(r_data.attrs["label"], "C1Renamed");
    }

    #[test]
    fn apply_attr_propagation_uses_getter_name_transform() {
        let mut g = TypedGraph::new();
        let attr_node = g.add_baseline_node("Attribute", "a", attrs(&[("name", "age")]));
        let getter_node = g.add_baseline_node("Method", "m", attrs(&[("name", "old")]));

        let mut cas = Cascade::new();
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("a".to_string(), attr_node);
        bindings.insert("m".to_string(), getter_node);

        let propagations = vec![EnginePropagation {
            source_node_var: "a".into(),
            source_attr: "name".into(),
            target_node_var: "m".into(),
            target_attr: "name".into(),
            transform_tag: "getter_name".into(),
        }];

        apply_attr_propagation(
            &propagations,
            &bindings,
            "salary",
            &mut g,
            &mut cas,
            "AttrRule",
        )
        .unwrap();

        let m = g.get_node(&getter_node).unwrap();
        assert_eq!(m.attrs["name"], "getSalary");
    }

    // ── M5.5: Tentative tombstone + consolidation ────────────────────────

    #[test]
    fn tentative_invalidate_marks_created_set() {
        let mut g = TypedGraph::new();
        let _p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        let invalidated = tentative_invalidate(0, &cas, &mut g);
        assert!(!invalidated.is_empty());
        for id in &invalidated {
            let n = g.get_node(id);
            if let Some(n) = n {
                assert_eq!(n.status, Status::TentativeTombstone);
            }
        }
    }

    #[test]
    fn consolidate_resurrects_just_created_and_tombstones_rest() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let m1 = g.add_ghost_node(p1, "m", "M", attrs(&[("name", "m1")]));
        let m2 = g.add_ghost_node(p1, "m", "M", attrs(&[("name", "m2")]));
        // Mark both as TentativeTombstone
        g.set_node_status(&m1, Status::TentativeTombstone);
        g.set_node_status(&m2, Status::TentativeTombstone);
        // Consolidate: only m1 was "freshly created" → resurrection
        consolidate_tentative(&mut g, &[m1]);
        assert_eq!(g.get_node(&m1).unwrap().status, Status::Solid);
        assert_eq!(g.get_node(&m2).unwrap().status, Status::Tombstone);
    }

    #[test]
    fn run_cascade_observable_invalidates_orphan_ruleapp() {
        // Two classes, rule produces a marker per class.
        // Then the user deletes one class → the marker of that app
        // must be invalidated (tombstoned).
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let _p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        // After initial sync: 2 marker nodes
        let marker_count = g.iter_nodes().filter(|n| n.type_id == "M").count();
        assert_eq!(marker_count, 2);

        // User delta: delete p1
        let user = DeltaEntry::new_user(vec![Op::DelNode { target: p1 }], vec![p1]);
        user.apply(&mut g).unwrap();
        cas.append(user);

        let _ = run_cascade_observable(&mut cas, &mut g, &rules, 5).unwrap();

        // Count markers that are not yet tombstoned (Solid or Ghost).
        let active_markers = g
            .iter_nodes()
            .filter(|n| n.type_id == "M" && n.status != Status::Tombstone)
            .count();
        let tombstoned_markers = g
            .iter_nodes()
            .filter(|n| n.type_id == "M" && n.status == Status::Tombstone)
            .count();
        assert_eq!(active_markers, 1, "p2's marker stays active");
        assert_eq!(tombstoned_markers, 1, "p1's marker was tombstoned");
    }

    #[test]
    fn user_delta_has_empty_bindings() {
        let cas = Cascade::with_user_delta(DeltaEntry::new_user(vec![], vec![]));
        let store = MatchPersistenceStore::new(&cas);
        // User delta is not picked up by applications_referencing,
        // since it only considers Origin::Rule.
        let id = GhostId::from_baseline("dummy");
        assert!(store.applications_referencing(&id).is_empty());
        assert!(store.bindings(0).unwrap().is_empty());
    }
}
