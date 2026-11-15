//! The matcher: enumerating matches over the participation lists.
//!
//! Spec §1.5 verbatim: "Pattern = typed nodes (+ value constraints on
//! leaves) + directed anonymous connectivity. Candidate expansion at
//! node k: its participation list, sorted by (counterpart type, id) —
//! the order lives IN the list, no global view alongside it. NACs
//! unchanged (structure + value constraints on leaves)."
//!
//! Variables are POSITIONAL (spec §1: a match = a ref sequence in
//! pattern order — there are no variable names anymore).
//!
//! [OPEN, reported to Sandra: seed steps (first pattern position
//! without a bound neighbor) need "all nodes of type T" — the spec
//! names no type index; for now a scan over insertion order (correct,
//! unoptimized). With stage 4 (add stream = initial enumeration) seeds
//! mostly disappear anyway.]

use crate::ident::GhostId;
use crate::rules::predicate::Predicate;

use crate::graph::{Graph, TypeId, ValueResolver};

/// Pattern node: type + optional leaf constraint. The position in the
/// `nodes` vec IS the variable.
///
/// The value constraint is [`crate::rules::predicate::Predicate`] — the
/// cross-language normalized form; the earlier own `ValuePredicate`
/// is gone, with no replacement needed.
#[derive(Clone, Debug)]
pub struct PatternNode {
    pub typ: TypeId,
    pub value: Option<Predicate>,
}

/// Link flavor (spec §3b.3, Sandra): containment is directed; toward a
/// context corr the link is DIRECTION-FREE — matching is anchored at
/// the delta node, the creation direction is provenance for reading,
/// not a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Directed,
    Context,
    /// Value equality of two leaf positions (§1.5 addition, reported:
    /// BX-standard need — the Benchmarx E decision needs the join
    /// "famName == last"). Values stay in the original, compared only
    /// while matching (like Equals today).
    SameValue,
}

/// One pattern link between two positions.
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub kind: LinkKind,
}

impl Link {
    pub fn directed(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            kind: LinkKind::Directed,
        }
    }

    pub fn context(a: usize, b: usize) -> Self {
        Self {
            from: a,
            to: b,
            kind: LinkKind::Context,
        }
    }

    pub fn same_value(a: usize, b: usize) -> Self {
        Self {
            from: a,
            to: b,
            kind: LinkKind::SameValue,
        }
    }
}

/// Pattern: typed nodes + anonymous connectivity (indices into `nodes`).
#[derive(Clone, Debug, Default)]
pub struct Pattern {
    pub nodes: Vec<PatternNode>,
    pub links: Vec<Link>,
}

/// Match: a ref sequence in pattern order (spec §1) — it IS the μ key;
/// no separate key, no names.
pub type Bindings = Vec<GhostId>;

// ── Plan (connected order, fixed first) ────────────────────────────────────

#[derive(Debug)]
struct Step {
    node_ix: usize,
    pre_bound: bool,
    /// Links to already placed positions: (placed position, direction
    /// stance of the NEW node). `None` = context link (direction-free).
    links: Vec<(usize, Option<bool>)>,
}

/// Search plan of a pattern for one set of pre-bound positions.
#[derive(Debug)]
pub(crate) struct Plan {
    steps: Vec<Step>,
    /// The links no step checks, as indices into `Pattern::links`:
    /// value joins, self links and links between two pre-bound
    /// positions. Every other link is checked by the step that places
    /// the later of its two ends.
    closing: Vec<usize>,
}

fn build_plan(pattern: &Pattern, fixed: &[Option<GhostId>]) -> Plan {
    trace!(plan_builds);
    let n = pattern.nodes.len();
    let mut placed = vec![false; n];
    let mut steps = Vec::with_capacity(n);
    for (i, f) in fixed.iter().enumerate() {
        if f.is_some() {
            placed[i] = true;
            steps.push(Step {
                node_ix: i,
                pre_bound: true,
                links: Vec::new(),
            });
        }
    }
    while steps.len() < n {
        // Deterministic: the smallest unplaced position that connects
        // to something placed; otherwise the smallest unplaced (seed).
        let mut next: Option<usize> = None;
        for i in 0..n {
            if placed[i] {
                continue;
            }
            let connected = pattern.links.iter().any(|l| {
                l.kind != LinkKind::SameValue
                    && ((l.from == i && placed[l.to]) || (l.to == i && placed[l.from]))
            });
            if connected {
                next = Some(i);
                break;
            }
            if next.is_none() {
                next = Some(i);
            }
        }
        let i = next.expect("an unplaced position exists");
        let links = pattern
            .links
            .iter()
            .filter_map(|l| {
                if l.kind == LinkKind::SameValue {
                    return None; // Value joins are closing links.
                }
                let dirless = l.kind == LinkKind::Context;
                if l.from == i && placed[l.to] {
                    Some((l.to, if dirless { None } else { Some(true) }))
                } else if l.to == i && placed[l.from] {
                    Some((l.from, if dirless { None } else { Some(false) }))
                } else {
                    None
                }
            })
            .collect();
        placed[i] = true;
        steps.push(Step {
            node_ix: i,
            pre_bound: false,
            links,
        });
    }
    let bound = |i: usize| fixed.get(i).is_some_and(Option::is_some);
    let closing = pattern
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            l.kind == LinkKind::SameValue || l.from == l.to || (bound(l.from) && bound(l.to))
        })
        .map(|(i, _)| i)
        .collect();
    Plan { steps, closing }
}

// ── Enumeration ──────────────────────────────────────────────────────────

fn node_ok(g: &Graph, resolver: &dyn ValueResolver, pn: &PatternNode, id: &GhostId) -> bool {
    trace!(node_checks);
    let Some(node) = g.node(id) else {
        return false;
    };
    if !node.status.is_matchable() || node.typ != pn.typ {
        return false;
    }
    match &pn.value {
        None => true,
        Some(p) => g.with_node_value(node, resolver, |v| p.matches(v)),
    }
}

fn links_ok(
    g: &Graph,
    cur: &[Option<GhostId>],
    links: &[(usize, Option<bool>)],
    cand: GhostId,
) -> bool {
    links.iter().all(|&(placed, dir)| {
        let other = cur[placed].expect("placed");
        match dir {
            Some(true) => g.connected(&cand, &other),
            Some(false) => g.connected(&other, &cand),
            // Context link: direction-free (provenance is data).
            None => g.connected(&cand, &other) || g.connected(&other, &cand),
        }
    })
}

/// The links of `plan.closing` on the complete binding.
fn closing_links_ok(
    g: &Graph,
    resolver: &dyn ValueResolver,
    pattern: &Pattern,
    plan: &Plan,
    cur: &[Option<GhostId>],
) -> bool {
    plan.closing
        .iter()
        .map(|&li| &pattern.links[li])
        .all(|l| match (cur[l.from], cur[l.to]) {
            (Some(s), Some(t)) => match l.kind {
                LinkKind::Directed => g.connected(&s, &t),
                LinkKind::Context => g.connected(&s, &t) || g.connected(&t, &s),
                LinkKind::SameValue => {
                    let a = g.resolve_value(&s, resolver);
                    a.is_some() && a == g.resolve_value(&t, resolver)
                }
            },
            _ => false,
        })
}

// The recursion carries the whole search state: graph, resolver, pattern,
// plan, depth, current binding, optional sink, hit flag, stop mode and
// the candidates to leave out.
// Bundling them into a struct would add an indirection on the hot path.
#[allow(clippy::too_many_arguments)]
fn enumerate(
    g: &Graph,
    resolver: &dyn ValueResolver,
    pattern: &Pattern,
    plan: &Plan,
    depth: usize,
    cur: &mut Vec<Option<GhostId>>,
    out: &mut Option<&mut Found>,
    found_any: &mut bool,
    stop_first: bool,
    skip: Option<&dyn Fn(&GhostId) -> bool>,
) {
    if depth == plan.steps.len() {
        if closing_links_ok(g, resolver, pattern, plan, cur) {
            *found_any = true;
            if let Some(out) = out.as_deref_mut() {
                out.ids.extend(cur.iter().map(|o| o.expect("complete")));
                out.count += 1;
            }
        }
        return;
    }
    let step = &plan.steps[depth];
    let pn = &pattern.nodes[step.node_ix];

    if step.pre_bound {
        let id = cur[step.node_ix].expect("pre-bound");
        if node_ok(g, resolver, pn, &id) {
            enumerate(
                g,
                resolver,
                pattern,
                plan,
                depth + 1,
                cur,
                out,
                found_any,
                stop_first,
                skip,
            );
        }
        return;
    }

    // Candidates: the participation list of the first placed neighbor,
    // via range by counterpart type (spec §1.5); without a link: seed
    // scan.
    //
    // A candidate from a participation list comes with the connection
    // that realises the first link, so that link is checked on the
    // connection itself; `links_ok` checks the remaining ones.
    //
    // The candidates are tried as they are read, without collecting
    // them first; the graph does not change during a search.
    let rest = &step.links[step.links.len().min(1)..];
    let first = step
        .links
        .first()
        .map(|&(placed, dir)| (cur[placed].expect("placed"), dir));
    // Tries one candidate; `true` = stop (first hit wanted and found).
    let mut try_candidate = |cand: GhostId, via: Option<GhostId>| -> bool {
        trace!(candidates);
        if cur.iter().flatten().any(|id| *id == cand) {
            return false; // injective
        }
        if skip.is_some_and(|skip| skip(&cand)) {
            return false;
        }
        if via.is_some_and(|c| !g.connection(&c).is_some_and(|c| c.status.is_matchable())) {
            return false;
        }
        if !node_ok(g, resolver, pn, &cand) {
            return false;
        }
        if !links_ok(g, cur, rest, cand) {
            return false;
        }
        cur[step.node_ix] = Some(cand);
        enumerate(
            g,
            resolver,
            pattern,
            plan,
            depth + 1,
            cur,
            out,
            found_any,
            stop_first,
            skip,
        );
        cur[step.node_ix] = None;
        stop_first && *found_any
    };
    match first {
        Some((anchor, dir)) => {
            for p in g.parts_by_other_type(&anchor, pn.typ) {
                if dir.is_some_and(|cand_is_source| p.outgoing == cand_is_source) {
                    continue;
                }
                if try_candidate(p.other, Some(p.connection)) {
                    return;
                }
            }
        }
        None => {
            for n in g.nodes_of_type(pn.typ) {
                if try_candidate(n.id, None) {
                    return;
                }
            }
        }
    }
}

/// All matches, canonically sorted by the ref sequence (μ order).
pub fn find_matches(g: &Graph, resolver: &dyn ValueResolver, pattern: &Pattern) -> Vec<Bindings> {
    find_matches_with_fixed(g, resolver, pattern, &vec![None; pattern.nodes.len()])
}

/// Matches with pre-bound positions.
pub fn find_matches_with_fixed(
    g: &Graph,
    resolver: &dyn ValueResolver,
    pattern: &Pattern,
    fixed: &[Option<GhostId>],
) -> Vec<Bindings> {
    find_matches_without(g, resolver, pattern, fixed, None)
}

/// [`find_matches_with_fixed`] without the matches that bind a node
/// `skip` names at a position that is not pre-bound. The search leaves
/// such a node out as a candidate, so it does not descend below it.
pub(crate) fn find_matches_without(
    g: &Graph,
    resolver: &dyn ValueResolver,
    pattern: &Pattern,
    fixed: &[Option<GhostId>],
    skip: Option<&dyn Fn(&GhostId) -> bool>,
) -> Vec<Bindings> {
    let plan = build_plan(pattern, fixed);
    search(g, resolver, pattern, &plan, fixed.to_vec(), skip)
}

/// The plan of the searches that pre-bind exactly `pos`. It depends on
/// the pattern and the position only, so it can be kept
/// ([`find_matches_planned`]).
pub(crate) fn plan_anchored(pattern: &Pattern, pos: usize) -> Plan {
    let mut fixed = vec![None; pattern.nodes.len()];
    fixed[pos] = Some(GhostId::from_raw([0; 32]));
    build_plan(pattern, &fixed)
}

/// The matches of one search, one after the other in one vector.
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// `count` ref sequences of the pattern's length each.
    ids: Vec<GhostId>,
    count: usize,
}

/// Working memory of the anchored searches, kept by the engine across
/// searches so that a search allocates nothing.
#[derive(Debug, Default)]
pub(crate) struct Scratch {
    cur: Vec<Option<GhostId>>,
    found: Found,
    /// The matches of the last search as offsets into `found.ids`, in
    /// μ order.
    order: Vec<usize>,
}

impl Scratch {
    /// The matches of the last [`find_matches_planned`], in μ order.
    pub(crate) fn matches(&self, pattern: &Pattern) -> impl Iterator<Item = &[GhostId]> + '_ {
        let n = pattern.nodes.len();
        self.order
            .iter()
            .map(move |&at| &self.found.ids[at..at + n])
    }
}

/// [`find_matches_without`] with `id` at `pos` and nothing else
/// pre-bound, along a plan from [`plan_anchored`] for that position.
/// The matches are left in `scratch` ([`Scratch::matches`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn find_matches_planned(
    g: &Graph,
    resolver: &dyn ValueResolver,
    pattern: &Pattern,
    plan: &Plan,
    pos: usize,
    id: GhostId,
    skip: Option<&dyn Fn(&GhostId) -> bool>,
    scratch: &mut Scratch,
) {
    let n = pattern.nodes.len();
    scratch.cur.clear();
    scratch.cur.resize(n, None);
    scratch.cur[pos] = Some(id);
    scratch.found.ids.clear();
    scratch.found.count = 0;
    let mut found = false;
    enumerate(
        g,
        resolver,
        pattern,
        plan,
        0,
        &mut scratch.cur,
        &mut Some(&mut scratch.found),
        &mut found,
        false,
        skip,
    );
    let ids = &scratch.found.ids;
    scratch.order.clear();
    scratch
        .order
        .extend((0..scratch.found.count).map(|i| i * n));
    if scratch.order.len() > 1 {
        scratch
            .order
            .sort_by(|&a, &b| ids[a..a + n].cmp(&ids[b..b + n]));
    }
}

fn search(
    g: &Graph,
    resolver: &dyn ValueResolver,
    pattern: &Pattern,
    plan: &Plan,
    mut cur: Vec<Option<GhostId>>,
    skip: Option<&dyn Fn(&GhostId) -> bool>,
) -> Vec<Bindings> {
    let mut out = Found::default();
    let mut found = false;
    enumerate(
        g,
        resolver,
        pattern,
        plan,
        0,
        &mut cur,
        &mut Some(&mut out),
        &mut found,
        false,
        skip,
    );
    let n = pattern.nodes.len();
    let mut result: Vec<Bindings> = (0..out.count)
        .map(|i| out.ids[i * n..(i + 1) * n].to_vec())
        .collect();
    result.sort();
    result
}

// ══ Tests (stage 2) ═══════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::*;
    use crate::graph::{PlanTransform, ValueStore};
    use crate::ident::Status;
    use crate::rules::transform::{Chain, Prim};

    /// F2P shape: register → family → role node → member, name
    /// leaf at the member. Everything connected anonymously.
    fn f2p_v2(n: usize) -> (Graph, ValueStore) {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        let reg = g.add_baseline("reg", "FamilyRegister");
        for i in 0..n {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            g.connect(reg, f, Status::Solid);
            for role in ["Father", "Mother", "Sons", "Daughters"] {
                let r = g.add_baseline(&format!("f{i}/{role}"), role);
                g.connect(f, r, Status::Solid);
                let m = g.add_baseline(&format!("f{i}/{role}/m"), "Member");
                g.connect(r, m, Status::Solid);
                let leaf = g.add_baseline(&format!("f{i}/{role}/m/firstName"), "firstName");
                g.connect(m, leaf, Status::Solid);
                vs.insert(leaf, format!("{role}-{i}"));
            }
        }
        (g, vs)
    }

    fn pat(g: &mut Graph, spec: &[(&str, Option<Predicate>)], links: &[(usize, usize)]) -> Pattern {
        Pattern {
            nodes: spec
                .iter()
                .map(|(t, v)| PatternNode {
                    typ: g.types.intern(t),
                    value: v.clone(),
                })
                .collect(),
            links: links.iter().map(|&(a, b)| Link::directed(a, b)).collect(),
        }
    }

    #[test]
    fn role_pattern_finds_the_fathers() {
        let (mut g, vs) = f2p_v2(10);
        // Family → Father role → Member
        let p = pat(
            &mut g,
            &[("Family", None), ("Father", None), ("Member", None)],
            &[(0, 1), (1, 2)],
        );
        let m = find_matches(&g, &vs, &p);
        assert_eq!(m.len(), 10);
        // Canonical μ order: sorted by the ref sequence.
        let mut sorted = m.clone();
        sorted.sort();
        assert_eq!(m, sorted);
    }

    #[test]
    fn value_constraint_via_the_resolver() {
        let (mut g, vs) = f2p_v2(5);
        let p = pat(
            &mut g,
            &[
                ("Member", None),
                ("firstName", Some(Predicate::Equals("Father-3".into()))),
            ],
            &[(0, 1)],
        );
        let m = find_matches(&g, &vs, &p);
        assert_eq!(m.len(), 1, "exactly one member is named Father-3");
    }

    #[test]
    fn value_constraint_on_a_derived_leaf() {
        // The constraint checks the DERIVED value (ref chain), with no
        // value stored anywhere.
        let (mut g, vs) = f2p_v2(2);
        let member_leaf = g
            .iter_nodes()
            .find(|n| g.types.name(n.typ) == "firstName")
            .unwrap()
            .id;
        let parent = member_leaf; // The derivation hangs off the leaf itself.
        let getter = g.add_derived_leaf(
            &parent,
            "getterName",
            member_leaf,
            &PlanTransform::Chain(Chain(vec![Prim::Capitalize, Prim::Prefix("get".into())])),
        );
        let gt = g.types.lookup("getterName").unwrap();
        let p = Pattern {
            nodes: vec![PatternNode {
                typ: gt,
                value: Some(Predicate::Regex(regex::Regex::new("^getFather").unwrap())),
            }],
            links: vec![],
        };
        let m = find_matches(&g, &vs, &p);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0][0], getter);
    }

    #[test]
    fn hub_expansion_via_the_participation_range() {
        // Corr at the hub: pattern register→?corr expands via the
        // sorted participation list, not over all n families.
        let (mut g, vs) = f2p_v2(50);
        let reg = g.iter_nodes().next().unwrap().id;
        let corr = g.add_ghost(&reg, "Corr");
        g.connect(corr, reg, Status::Ghost);
        let p = pat(
            &mut g,
            &[("FamilyRegister", None), ("Corr", None)],
            &[(1, 0)],
        );
        let m = find_matches(&g, &vs, &p);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0], vec![reg, corr]);
    }
}
