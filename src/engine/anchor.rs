//! Choice of the anchor places for the delta-local expansion
//! (spec §1.6 add).
//!
//! Unfiltered, `expand_at` anchors every new node at every active rule
//! at every pattern position of the same type and runs one anchored
//! search per triple. In a rule set whose structural nodes share one
//! type (`sl:Node`, `java:Node`) and whose node correspondences share
//! one corr type, almost every rule has a position of the type of a new
//! node, and what tells the rules apart (the value of a leaf such as
//! `rule` or `feature`) lies several links away from it.
//!
//! [`AnchorIndex`] reads, per rule and position, the paths of the
//! pattern from that position: a spanning tree of the pattern links
//! rooted at the position, each tree edge as (direction, type, value
//! predicate) of the next node. The paths of all places of one type are
//! merged into one prefix tree. [`AnchorIndex::candidates`] walks the
//! graph from each new node along that prefix tree once and marks every
//! path a walk realises; a place is a candidate when all its paths are
//! marked and its own value predicate holds.
//!
//! Every path of a match is realised by the match itself, so the
//! condition is necessary for a match with the node at the position; it
//! is checked on the same graph state as the search. The walk reads
//! types, directions and values, not the status of the elements on a
//! path; that is left to the search. The triples come back
//! in the order of the unfiltered loop (rule, node, position), so the
//! recorded matches and their order stay the same.

use std::collections::VecDeque;

use std::sync::OnceLock;

use crate::engine::matcher::{plan_anchored, LinkKind, Pattern, Plan};
use crate::graph::{Graph, TypeId, ValueResolver};
use crate::hash::{FxBuildHasher, FxHashMap};
use crate::ident::GhostId;
use crate::plan::DirectedRule;
use crate::rules::predicate::Predicate;

/// Longest path read from a position (links). Paths are cut there; a
/// cut path is still a necessary condition.
const MAX_DEPTH: usize = 6;

/// Children of a prefix-tree node that share direction and type: one
/// participation range of the graph serves all of them.
#[derive(Debug)]
struct Group {
    /// `Some(true)`: the current node is the source of the edge.
    /// `Some(false)`: it is the target. `None`: direction-free.
    outgoing: Option<bool>,
    typ: TypeId,
    /// The child without a value predicate.
    any: Option<u32>,
    /// Children with `Predicate::Equals`, by value.
    equals: FxHashMap<String, u32>,
    /// Children with another predicate (with its `Debug` form as key).
    other: Vec<(String, Predicate, u32)>,
}

/// A node of the prefix tree: one path from a position of one type.
#[derive(Debug, Default)]
struct PathNode {
    groups: Vec<Group>,
}

/// One pattern position of one rule, as an anchor candidate.
#[derive(Debug)]
struct Place {
    rule: usize,
    pos: usize,
    /// Value predicate of the position itself, unless it is
    /// `Predicate::Equals`: such a place is filed by its value
    /// ([`Filed::equals`]) and reached only by a node that has it.
    value: Option<Predicate>,
    /// The prefix-tree nodes that end the paths from the position, as
    /// a range of `AnchorIndex::needs`.
    needs: std::ops::Range<u32>,
}

/// Places filed at one point of the index, split by their own value
/// predicate: a new node reaches the places that ask for its value
/// through `equals`, without looking at those that ask for another.
#[derive(Debug, Default)]
struct Filed {
    /// Places without a value predicate or with one that is not
    /// `Predicate::Equals`.
    any: Vec<u32>,
    /// Does `any` hold a place with a value predicate? Such a place is
    /// checked against the value of the node.
    valued: bool,
    /// Places with `Predicate::Equals`, by value.
    equals: FxHashMap<String, Vec<u32>>,
}

impl Filed {
    /// How many places a node with the value of `value` would look at
    /// here.
    fn load(&self, value: Option<&Predicate>) -> usize {
        match value {
            Some(Predicate::Equals(v)) => self.equals.get(v).map_or(0, Vec::len),
            _ => self.any.len(),
        }
    }

    fn push(&mut self, value: Option<&Predicate>, place: u32) {
        match value {
            Some(Predicate::Equals(v)) => match self.equals.get_mut(v.as_str()) {
                Some(places) => places.push(place),
                None => {
                    self.equals.insert(v.clone(), vec![place]);
                }
            },
            _ => {
                self.valued |= value.is_some();
                self.any.push(place);
            }
        }
    }
}

/// Places and prefix tree of one position type.
#[derive(Debug)]
struct TypeIndex {
    /// Ascending by (rule, position).
    places: Vec<Place>,
    /// Root of the prefix tree (a node id into `AnchorIndex::nodes`).
    root: u32,
    /// The places without a path (a pattern of one node, or a position
    /// no link leaves): candidates for every node of the type.
    free: Filed,
}

/// Working memory of the walks, kept by the engine across expansions.
#[derive(Debug, Default)]
pub(crate) struct Walk {
    /// Per prefix-tree node: realised by the current walk.
    marked: Vec<bool>,
    /// The nodes marked by the current walk (for the reset).
    list: Vec<u32>,
    /// Starts of the triples per node of the current expansion.
    groups: Vec<usize>,
    /// (graph node, prefix-tree node) pairs already expanded.
    visited: std::collections::HashSet<(GhostId, u32), FxBuildHasher>,
}

/// Anchor conditions of a rule set for the delta-local expansion: per
/// rule and pattern position the paths of the pattern from there (see
/// the module documentation of `engine::anchor`).
///
/// [`Engine::new`](crate::engine::Engine::new) builds one on first use.
/// A host that runs several engines over the same rules builds it once
/// with [`AnchorIndex::build`] and hands it to
/// [`Engine::with_anchor_index`](crate::engine::Engine::with_anchor_index).
#[derive(Debug, Default)]
pub struct AnchorIndex {
    nodes: Vec<PathNode>,
    /// Per prefix-tree node: the places (indices into the `places` of
    /// the node's type) filed under it. Every place with a path is
    /// filed under exactly one of its `needs`, so a walk reaches the
    /// places it can satisfy through the nodes it marked, without
    /// looking at the other places of the type.
    filed: Vec<Filed>,
    /// The `needs` of all places, one range per place.
    needs: Vec<u32>,
    /// Per rule and position: the plan of the search anchored there,
    /// built when the first search at that place runs. Building all of
    /// them with the index costs more than a short run saves. The plans
    /// of rule `r` start at `plan_at[r]`.
    plans: Vec<OnceLock<Plan>>,
    plan_at: Vec<usize>,
    by_type: FxHashMap<TypeId, TypeIndex>,
    /// The rule slice the index was built for (address, length).
    built_for: (usize, usize),
}

/// Working memory of [`AnchorIndex::build`]: the breadth-first spanning
/// tree of the links of one pattern from one position (no value joins,
/// no self links), cut at [`MAX_DEPTH`]. A path of the position is the
/// way from it to a leaf of the tree; a leaf path covers its prefixes.
#[derive(Debug, Default)]
struct Tree {
    seen: Vec<bool>,
    /// Per reached position: the position it was reached from and the
    /// direction of the edge as seen from there (`None`: direction-free).
    parent: Vec<(usize, Option<bool>)>,
    depth: Vec<usize>,
    has_child: Vec<bool>,
    queue: VecDeque<usize>,
    /// The positions of one path, from the leaf back to the root.
    chain: Vec<usize>,
}

impl Tree {
    fn span(&mut self, pattern: &Pattern, pos: usize) {
        let n = pattern.nodes.len();
        self.seen.clear();
        self.seen.resize(n, false);
        self.parent.clear();
        self.parent.resize(n, (pos, None));
        self.depth.clear();
        self.depth.resize(n, 0);
        self.has_child.clear();
        self.has_child.resize(n, false);
        self.queue.clear();
        self.seen[pos] = true;
        self.queue.push_back(pos);
        while let Some(u) = self.queue.pop_front() {
            if self.depth[u] >= MAX_DEPTH {
                continue;
            }
            for l in &pattern.links {
                if l.kind == LinkKind::SameValue || l.from == l.to {
                    continue;
                }
                let (w, outgoing) = if l.from == u {
                    (l.to, true)
                } else if l.to == u {
                    (l.from, false)
                } else {
                    continue;
                };
                if w >= n || self.seen[w] {
                    continue;
                }
                self.seen[w] = true;
                self.has_child[u] = true;
                self.parent[w] = (u, (l.kind != LinkKind::Context).then_some(outgoing));
                self.depth[w] = self.depth[u] + 1;
                self.queue.push_back(w);
            }
        }
    }

    /// Is `i` the end of a path from `pos`?
    fn is_leaf(&self, pos: usize, i: usize) -> bool {
        i != pos && self.seen[i] && !self.has_child[i]
    }
}

impl AnchorIndex {
    /// Reads the paths from every position of the directed rules.
    #[must_use]
    pub fn build(rules: &[DirectedRule]) -> Self {
        let mut plan_at = Vec::with_capacity(rules.len());
        let mut positions = 0;
        for r in rules {
            plan_at.push(positions);
            positions += r.pattern.nodes.len();
        }
        let mut index = Self {
            nodes: Vec::new(),
            filed: Vec::new(),
            needs: Vec::new(),
            plans: (0..positions).map(|_| OnceLock::new()).collect(),
            plan_at,
            by_type: FxHashMap::default(),
            built_for: (rules.as_ptr() as usize, rules.len()),
        };
        let mut tree = Tree::default();
        for (ri, rule) in rules.iter().enumerate() {
            let pattern = &rule.pattern;
            for (pos, pn) in pattern.nodes.iter().enumerate() {
                let root = match index.by_type.get(&pn.typ) {
                    Some(ti) => ti.root,
                    None => {
                        let root = index.new_node();
                        index.by_type.insert(
                            pn.typ,
                            TypeIndex {
                                places: Vec::new(),
                                root,
                                free: Filed::default(),
                            },
                        );
                        root
                    }
                };
                // One path per leaf of the spanning tree, the leaves in
                // ascending position.
                tree.span(pattern, pos);
                let from = index.needs.len();
                for leaf in 0..pattern.nodes.len() {
                    if !tree.is_leaf(pos, leaf) {
                        continue;
                    }
                    tree.chain.clear();
                    let mut w = leaf;
                    while w != pos {
                        tree.chain.push(w);
                        w = tree.parent[w].0;
                    }
                    let mut cur = root;
                    for &w in tree.chain.iter().rev() {
                        let next = &pattern.nodes[w];
                        cur = index.child(cur, tree.parent[w].1, next.typ, next.value.as_ref());
                    }
                    if !index.needs[from..].contains(&cur) {
                        index.needs.push(cur);
                    }
                }
                let needs = from as u32..index.needs.len() as u32;
                // File the place under the need that carries the fewest
                // places so far: a path many rules share (the link to
                // the parent) fills up, a path few rules share (a leaf
                // value) stays short.
                let key = index.needs[from..]
                    .iter()
                    .copied()
                    .min_by_key(|&n| index.filed[n as usize].load(pn.value.as_ref()));
                let ti = index.by_type.get_mut(&pn.typ).expect("inserted above");
                let place_ix = ti.places.len() as u32;
                match key {
                    Some(n) => index.filed[n as usize].push(pn.value.as_ref(), place_ix),
                    None => ti.free.push(pn.value.as_ref(), place_ix),
                }
                ti.places.push(Place {
                    rule: ri,
                    pos,
                    value: match &pn.value {
                        Some(Predicate::Equals(_)) => None,
                        other => other.clone(),
                    },
                    needs,
                });
            }
        }
        index
    }

    fn new_node(&mut self) -> u32 {
        self.nodes.push(PathNode::default());
        self.filed.push(Filed::default());
        (self.nodes.len() - 1) as u32
    }

    /// The child of `parent` for one path step; created if missing.
    fn child(
        &mut self,
        parent: u32,
        outgoing: Option<bool>,
        typ: TypeId,
        pred: Option<&Predicate>,
    ) -> u32 {
        let groups = &self.nodes[parent as usize].groups;
        let gi = match groups
            .iter()
            .position(|g| g.outgoing == outgoing && g.typ == typ)
        {
            Some(gi) => gi,
            None => {
                self.nodes[parent as usize].groups.push(Group {
                    outgoing,
                    typ,
                    any: None,
                    equals: FxHashMap::default(),
                    other: Vec::new(),
                });
                self.nodes[parent as usize].groups.len() - 1
            }
        };
        let found = {
            let group = &self.nodes[parent as usize].groups[gi];
            match pred {
                None => group.any,
                Some(Predicate::Equals(v)) => group.equals.get(v.as_str()).copied(),
                Some(p) => {
                    let key = format!("{p:?}");
                    group
                        .other
                        .iter()
                        .find(|(k, _, _)| *k == key)
                        .map(|(_, _, c)| *c)
                }
            }
        };
        if let Some(c) = found {
            return c;
        }
        let c = self.new_node();
        let group = &mut self.nodes[parent as usize].groups[gi];
        match pred {
            None => group.any = Some(c),
            Some(Predicate::Equals(v)) => {
                group.equals.insert(v.clone(), c);
            }
            Some(p) => group.other.push((format!("{p:?}"), p.clone(), c)),
        }
        c
    }

    /// The plan of the search anchored at position `pos` of `pattern`,
    /// the pattern of rule `rule` of the rules the index was built for.
    pub(crate) fn plan(&self, rule: usize, pos: usize, pattern: &Pattern) -> &Plan {
        self.plans[self.plan_at[rule] + pos].get_or_init(|| plan_anchored(pattern, pos))
    }

    /// Was the index built for this rule slice?
    #[must_use]
    pub fn built_for(&self, rules: &[DirectedRule]) -> bool {
        self.built_for == (rules.as_ptr() as usize, rules.len())
    }

    /// Marks every prefix-tree node below `at` that a walk from the
    /// graph node `x` realises without entering a node `skip` names.
    fn walk(
        &self,
        g: &Graph,
        v: &dyn ValueResolver,
        x: GhostId,
        at: u32,
        st: &mut Walk,
        skip: &dyn Fn(&GhostId) -> bool,
    ) {
        if !st.visited.insert((x, at)) {
            return;
        }
        trace!(anchor_walk_visits);
        for group in &self.nodes[at as usize].groups {
            let valued = !group.equals.is_empty() || !group.other.is_empty();
            for p in g.parts_by_other_type(&x, group.typ) {
                if group.outgoing.is_some_and(|o| o != p.outgoing) {
                    continue;
                }
                // A node the search will leave out cannot stand on a
                // path of a match it finds.
                if skip(&p.other) {
                    continue;
                }
                // The status of the edge and of the neighbour is not
                // read here. A path over a retracted element is marked
                // although no match can use it; the condition stays
                // necessary, and the search at that place fails on the
                // element. Reading the status costs two map lookups per
                // participation of every walk, the retracted elements
                // are few. The type is that of the participation range.
                if let Some(id) = group.any {
                    self.enter(g, v, p.other, id, st, skip);
                }
                if valued {
                    g.with_value(&p.other, v, |val| {
                        if let Some(&id) = val.and_then(|s| group.equals.get(s)) {
                            self.enter(g, v, p.other, id, st, skip);
                        }
                        for (_, pred, id) in &group.other {
                            if pred.matches(val) {
                                self.enter(g, v, p.other, *id, st, skip);
                            }
                        }
                    });
                }
            }
        }
    }

    fn enter(
        &self,
        g: &Graph,
        v: &dyn ValueResolver,
        x: GhostId,
        id: u32,
        st: &mut Walk,
        skip: &dyn Fn(&GhostId) -> bool,
    ) {
        if !st.marked[id as usize] {
            st.marked[id as usize] = true;
            st.list.push(id);
        }
        // The end of a path: nothing to walk from here.
        if self.nodes[id as usize].groups.is_empty() {
            return;
        }
        self.walk(g, v, x, id, st, skip);
    }

    /// The (rule, index into `nodes`, position) triples at which an
    /// anchored search can find a match, for the rules `active` admits,
    /// ascending by (rule, node index, position) — the order of the
    /// unfiltered loop.
    ///
    /// `skip(ni, id)`: the search anchored at `nodes[ni]` leaves the
    /// node `id` out. The walk from `nodes[ni]` does not enter such a
    /// node, so a place whose paths are realised only through one is
    /// not a candidate: the search there would find nothing. A node the
    /// search leaves out as its own anchor (`skip(ni, &nodes[ni])`) has
    /// no triples.
    pub(crate) fn candidates(
        &self,
        g: &Graph,
        v: &dyn ValueResolver,
        nodes: &[GhostId],
        st: &mut Walk,
        active: impl Fn(usize) -> bool,
        skip: impl Fn(usize, &GhostId) -> bool,
    ) -> Vec<(usize, usize, usize)> {
        let mut out: Vec<(usize, usize, usize)> = Vec::new();
        if st.marked.len() != self.nodes.len() {
            st.marked = vec![false; self.nodes.len()];
            st.list.clear();
        }
        // Where the triples of each node start; kept in the working
        // memory, an expansion of a few nodes allocates nothing for it.
        let mut groups = std::mem::take(&mut st.groups);
        groups.clear();
        for (ni, x) in nodes.iter().enumerate() {
            groups.push(out.len());
            if skip(ni, x) {
                continue;
            }
            let Some(node) = g.node(x) else { continue };
            let Some(ti) = self.by_type.get(&node.typ) else {
                continue;
            };
            if !node.status.is_matchable() {
                continue;
            }
            st.visited.clear();
            self.walk(g, v, *x, ti.root, st, &|id| skip(ni, id));
            // The places filed under a marked node, and the places
            // without a path. Each place is filed once, so none comes
            // twice; the triples are sorted below. Of the places that
            // ask for a value of the node itself, only those that ask
            // for the value it has.
            let reached = || {
                st.list
                    .iter()
                    .map(|&id| &self.filed[id as usize])
                    .chain(std::iter::once(&ti.free))
            };
            // The value of the node, where a reached place asks for it:
            // by value (`equals`) or with another predicate (`valued`).
            let value = if reached().any(|f| !f.equals.is_empty() || f.valued) {
                g.resolve_value(x, v)
            } else {
                None
            };
            let mut check = |place_ix: u32, value_holds: bool| {
                trace!(anchor_places);
                let place = &ti.places[place_ix as usize];
                let needs = &self.needs[place.needs.start as usize..place.needs.end as usize];
                if !needs.iter().all(|&n| st.marked[n as usize]) {
                    return;
                }
                if !value_holds {
                    if let Some(pred) = &place.value {
                        if !pred.matches(value.as_deref()) {
                            return;
                        }
                    }
                }
                if active(place.rule) {
                    out.push((place.rule, ni, place.pos));
                }
            };
            for f in reached() {
                for &place_ix in &f.any {
                    check(place_ix, false);
                }
                if let Some(places) = value.as_deref().and_then(|s| f.equals.get(s)) {
                    for &place_ix in places {
                        check(place_ix, true);
                    }
                }
            }
            // Reset only what this walk marked.
            for id in st.list.drain(..) {
                st.marked[id as usize] = false;
            }
        }
        let out = Self::in_order(out, &groups);
        st.groups = groups;
        out
    }

    /// Brings the triples into the order (rule, node index, position).
    /// `groups` are the starts of the triples of each node, which come
    /// with ascending node index. A short list is sorted as a whole. A
    /// long one (a delta of many nodes) is sorted within each node and
    /// then distributed by rule, which keeps the node order: linear
    /// instead of one comparison sort over all triples.
    fn in_order(
        mut out: Vec<(usize, usize, usize)>,
        groups: &[usize],
    ) -> Vec<(usize, usize, usize)> {
        const WHOLE: usize = 256;
        if out.len() <= WHOLE {
            out.sort_unstable();
            return out;
        }
        for (i, &from) in groups.iter().enumerate() {
            let to = groups.get(i + 1).copied().unwrap_or(out.len());
            out[from..to].sort_unstable();
        }
        let rules = out.iter().map(|t| t.0).max().map_or(0, |m| m + 1);
        let mut start = vec![0usize; rules + 1];
        for t in &out {
            start[t.0 + 1] += 1;
        }
        for r in 0..rules {
            start[r + 1] += start[r];
        }
        let mut sorted = vec![(0, 0, 0); out.len()];
        for t in out {
            sorted[start[t.0]] = t;
            start[t.0] += 1;
        }
        sorted
    }
}

// ══ Tests ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    //! Rule shapes of a language pack (one structural type per side, one
    //! node corr type for all rules, the distinguishing leaf values two
    //! to three links from the parent): root, child, grandchild with a
    //! parent condition, attribute with a context leaf. Every expansion
    //! of the cascades below runs `Engine::check_candidates`, which
    //! compares the index against the unfiltered loop.

    use super::AnchorIndex;
    use crate::engine::{Engine, Termination};
    use crate::graph::{Graph, ValueStore};
    use crate::ident::{GhostId, Status};
    use proptest::prelude::*;

    fn leaf(name: &str, typ: &str, value: &str, context: bool) -> serde_json::Value {
        let mut n = serde_json::json!({
            "name": name, "type": typ,
            "predicate": {"kind": "equals", "value": value}
        });
        if context {
            n["context"] = serde_json::json!(true);
        } else {
            n["constant"] = serde_json::json!(value);
        }
        n
    }

    fn rules_json() -> String {
        let child = |name: &str, parent_rule: Option<&str>, rule: &str, feature: &str| {
            let mut left = vec![
                serde_json::json!({"name": "sp", "type": "J"}),
                serde_json::json!({"name": "a", "type": "J"}),
                leaf("a_rule", "rule", rule, false),
            ];
            let mut links = vec![
                serde_json::json!(["sp", "a"]),
                serde_json::json!(["a", "a_rule"]),
            ];
            if let Some(pr) = parent_rule {
                left.push(leaf("sp_rule", "rule", pr, true));
                links.push(serde_json::json!(["sp", "sp_rule"]));
            }
            serde_json::json!({
                "name": name, "rank": 200,
                "left": {"anchor": "a", "nodes": left, "links": links},
                "right": {"anchor": "b", "nodes": [
                    {"name": "sb", "type": "S"},
                    {"name": "b", "type": "S"},
                    leaf("b_feature", "feature", feature, false)
                ], "links": [["sb", "b"], ["b", "b_feature"]]},
                "corrs": [
                    {"type": "CN", "left": "sp", "right": "sb", "role": "references"},
                    {"type": "CN", "left": "a", "right": "b", "role": "establishes"}
                ]
            })
        };
        let rules = vec![
            serde_json::json!({
                "name": "Root", "rank": 900,
                "left": {"anchor": "n", "nodes": [
                    {"name": "f", "type": "F"},
                    {"name": "n", "type": "J"},
                    leaf("n_rule", "rule", "unit", false)
                ], "links": [["f", "n"], ["n", "n_rule"]]},
                "right": {"anchor": "m", "nodes": [
                    {"name": "p", "type": "P", "context": true},
                    {"name": "m", "type": "S"},
                    leaf("m_feature", "feature", "Unit", false)
                ], "links": [["p", "m"], ["m", "m_feature"]]},
                "corrs": [
                    {"type": "CN", "left": "n", "right": "m", "role": "establishes"},
                    {"type": "CF", "left": "f", "right": "m", "role": "establishes"}
                ]
            }),
            child("Cls", None, "cls", "Class"),
            child("Method", Some("cls"), "m", "Method"),
            child("Other", None, "x", "Other"),
            serde_json::json!({
                "name": "Flag", "rank": 100,
                "left": {"anchor": "al", "nodes": [
                    {"name": "a", "type": "J"},
                    leaf("a_rule", "rule", "m", true),
                    leaf("al", "flag", "yes", false)
                ], "links": [["a", "a_rule"], ["a", "al"]]},
                "right": {"anchor": "bl", "nodes": [
                    {"name": "b", "type": "S"},
                    leaf("b_feature", "feature", "Method", true),
                    leaf("bl", "open", "on", false)
                ], "links": [["b", "b_feature"], ["b", "bl"]]},
                "corrs": [
                    {"type": "CN", "left": "a", "right": "b", "role": "references"},
                    {"type": "CA", "left": "al", "right": "bl", "role": "establishes"}
                ]
            }),
        ];
        serde_json::json!({"format": 3, "name": "anchor_tests", "rules": rules}).to_string()
    }

    /// A language tree: per node the rule value, the parent (index) and
    /// whether it carries the flag leaf.
    #[derive(Debug, Clone)]
    struct Tree {
        nodes: Vec<(String, usize, Option<bool>)>,
    }

    fn tree() -> impl Strategy<Value = Tree> {
        let rule = prop_oneof![
            Just("cls".to_string()),
            Just("m".to_string()),
            Just("x".to_string()),
            Just("unit".to_string())
        ];
        let flag = prop_oneof![Just(None), Just(Some(true)), Just(Some(false))];
        prop::collection::vec((rule, any::<prop::sample::Index>(), flag), 0..14).prop_map(|v| {
            Tree {
                nodes: v
                    .into_iter()
                    .enumerate()
                    .map(|(i, (r, p, f))| (r, p.index(i + 1), f))
                    .collect(),
            }
        })
    }

    /// Graph with file, root and the tree; returns the language nodes.
    fn build(t: &Tree, g: &mut Graph, vs: &mut ValueStore) -> Vec<GhostId> {
        let _p = g.add_baseline("project", "P");
        let f = g.add_baseline("file", "F");
        let root = g.add_baseline("n0", "J");
        let rl = g.add_baseline("n0/rule", "rule");
        vs.insert(rl, "unit");
        g.connect(f, root, Status::Solid);
        g.connect(root, rl, Status::Solid);
        let mut all = vec![f, root, rl];
        let mut ids = vec![root];
        for (i, (rule, parent, flag)) in t.nodes.iter().enumerate() {
            let n = g.add_baseline(&format!("n{}", i + 1), "J");
            g.connect(ids[*parent], n, Status::Solid);
            let r = g.add_baseline(&format!("n{}/rule", i + 1), "rule");
            vs.insert(r, rule.clone());
            g.connect(n, r, Status::Solid);
            all.extend([n, r]);
            if let Some(yes) = flag {
                let l = g.add_baseline(&format!("n{}/flag", i + 1), "flag");
                vs.insert(l, if *yes { "yes" } else { "no" });
                g.connect(n, l, Status::Solid);
                all.push(l);
            }
            ids.push(n);
        }
        all
    }

    fn cascade(t: &Tree, delta: bool) -> usize {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        let all = build(t, &mut g, &mut vs);
        let rules = crate::rules::load(&rules_json(), &mut g).expect("rules load");
        let mut e = Engine::new(&rules);
        if delta {
            e.elements_added(&g, &vs, &all);
        } else {
            e.seed(&g, &vs);
        }
        let mut steps = 0;
        while e.step(&mut g, &vs).is_some() {
            steps += 1;
            assert!(steps < 10_000, "no saturation");
        }
        assert_ne!(e.verdict(), Termination::Contradiction);
        e.cascade.len()
    }

    #[test]
    fn the_index_keeps_the_matches_of_a_fixed_tree() {
        let t = Tree {
            nodes: vec![
                ("cls".into(), 0, None),
                ("m".into(), 1, Some(true)),
                ("m".into(), 1, Some(false)),
                ("x".into(), 0, None),
                ("m".into(), 4, Some(true)),
            ],
        };
        // Root, Cls, Method ×2, Other, Flag ×1; the method under `x`
        // has no Method rule and so no flag rule either.
        assert_eq!(cascade(&t, true), 6);
        assert_eq!(cascade(&t, false), 6);
    }

    /// One rule: an `Item` with a context leaf `Attr` that carries the
    /// given predicate; the right side is a marker.
    fn predicate_rule(predicate: &str) -> String {
        format!(
            r#"{{"format":3,"name":"anchor_predicates","rules":[{{"name":"R","rank":1,
            "left":{{"anchor":"item","nodes":[
                {{"name":"item","type":"Item"}},
                {{"name":"attr","type":"Attr","context":true,"predicate":{predicate}}}],
              "links":[["item","attr"]]}},
            "right":{{"anchor":"marker","nodes":[{{"name":"marker","type":"Marker"}}]}},
            "corrs":[{{"type":"C","left":"item","right":"marker","role":"establishes"}}]}}]}}"#
        )
    }

    /// Applied steps for one item with one leaf of the given value. The
    /// delta is `[attr]`, `[attr, item]` or `[item, attr]`, or the
    /// engine seeds; `by_index` forces the seed through the index.
    fn predicate_cascade(predicate: &str, value: &str, delta: Option<&[usize]>) -> usize {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        let item = g.add_baseline("item", "Item");
        let attr = g.add_baseline("attr", "Attr");
        g.connect(item, attr, Status::Solid);
        vs.insert(attr, value);
        let rules = crate::rules::load(&predicate_rule(predicate), &mut g).expect("rules load");
        let mut e = Engine::new(&rules);
        match delta {
            Some(order) => {
                let nodes: Vec<GhostId> = order.iter().map(|&i| [item, attr][i]).collect();
                e.elements_added(&g, &vs, &nodes);
            }
            None => e.seed_by_index(&g, &vs),
        }
        while e.step(&mut g, &vs).is_some() {}
        e.cascade.len()
    }

    /// The value predicate of the anchored position itself, for every
    /// kind of predicate: a leaf that stands alone or first in the
    /// delta is anchored at its own position, and the place there asks
    /// for the value of the leaf.
    #[test]
    fn the_index_checks_every_kind_of_predicate_on_the_anchored_node() {
        let kinds = [
            (r#"{"kind":"exists"}"#, "anything", None),
            (r#"{"kind":"equals","value":"yes"}"#, "yes", Some("no")),
            (
                r#"{"kind":"prefix","value":"get"}"#,
                "getAge",
                Some("setAge"),
            ),
            (
                r#"{"kind":"regex","pattern":"Abstract.*"}"#,
                "AbstractList",
                Some("List"),
            ),
            (
                r#"{"kind":"numeric_range","min":0.0,"max":100.0}"#,
                "42",
                Some("420"),
            ),
        ];
        let deltas: [Option<&[usize]>; 4] = [Some(&[1]), Some(&[1, 0]), Some(&[0, 1]), None];
        for (predicate, holds, fails) in kinds {
            for delta in deltas {
                assert_eq!(
                    predicate_cascade(predicate, holds, delta),
                    1,
                    "{predicate} on {holds:?}, delta {delta:?}"
                );
                if let Some(fails) = fails {
                    assert_eq!(
                        predicate_cascade(predicate, fails, delta),
                        0,
                        "{predicate} on {fails:?}, delta {delta:?}"
                    );
                }
            }
        }
    }

    proptest! {
        /// The distribution by rule gives the order of a sort.
        #[test]
        fn in_order_is_a_sort(
            per_node in prop::collection::vec(
                prop::collection::vec((0usize..40, 0usize..6), 0..12),
                0..120,
            ),
        ) {
            let mut out = Vec::new();
            let mut groups = Vec::new();
            for (ni, places) in per_node.iter().enumerate() {
                groups.push(out.len());
                let mut seen = std::collections::BTreeSet::new();
                for &(rule, pos) in places {
                    if seen.insert((rule, pos)) {
                        out.push((rule, ni, pos));
                    }
                }
            }
            let mut expected = out.clone();
            expected.sort_unstable();
            prop_assert_eq!(AnchorIndex::in_order(out, &groups), expected);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]
        #[test]
        fn the_index_keeps_every_match(t in tree()) {
            let a = cascade(&t, true);
            let b = cascade(&t, false);
            prop_assert_eq!(a, b);
        }
    }
}
