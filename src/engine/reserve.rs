//! Room in the stores of graph and engine before a run (feature
//! `opt_b`).
//!
//! The maps and vectors of graph and engine grow by doubling, and a
//! map that doubles moves every entry it holds. With `opt_b` a host
//! that knows how large a run gets says so before it starts
//! ([`Engine::reserve`], [`Graph::reserve`]; [`Engine::sizes`] tells
//! what a run reached), and the engine makes room for what the matches
//! it has recorded and not yet applied would create
//! ([`Engine::reserve_for_recorded`]).
//! `run` does the latter between the seed and the first step. Without
//! the feature none of this exists and `run` reserves nothing.
//!
//! Capacity is not part of what the engine does: graph, cascade and
//! verdicts are the same with and without a hint.

use super::Engine;
use crate::graph::Graph;

/// Sizes of the engine's stores: what a run reached
/// ([`Engine::sizes`]), or what a host expects a run to add
/// ([`Engine::reserve`]).
#[cfg(feature = "opt_b")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sizes {
    /// Matches recorded, whether applied, recognised as duplicates,
    /// dead or still to do.
    pub matches: usize,
    /// Elements under which recorded matches are filed: the nodes of
    /// the matches and, without the feature `opt_a`, the connections
    /// their patterns ask for. A value read with one setting of
    /// `opt_a` is a hint of the wrong size for the other.
    pub elements: usize,
    /// Nodes created by applied steps.
    pub products: usize,
    /// Applied steps (entries of the cascade).
    pub steps: usize,
}

#[cfg(feature = "opt_b")]
impl Engine<'_> {
    /// The sizes the engine's stores have reached. After a run they
    /// are the hint for the next run of the same rules over a model of
    /// the same size.
    #[must_use]
    pub fn sizes(&self) -> Sizes {
        Sizes {
            matches: self.matches.len(),
            elements: self.by_element.len(),
            products: self.by_product.len(),
            steps: self.cascade.len(),
        }
    }

    /// Makes room for `more` on top of what the stores hold. A hint:
    /// fewer or more may follow.
    pub fn reserve(&mut self, more: Sizes) {
        self.matches.reserve(more.matches);
        self.by_key.reserve(more.matches);
        // A recorded match ends in `applied`, as applied or as a
        // duplicate.
        self.applied.reserve(more.matches);
        self.by_element.reserve(more.elements);
        self.by_product.reserve(more.products);
        self.cascade.reserve(more.steps);
    }

    /// Makes room, in the engine and in `g`, for what the matches in
    /// the todo would create if every one of them were applied: the
    /// nodes and connections of their rules' plans and one cascade
    /// entry each. Reads the todo once.
    pub fn reserve_for_recorded(&mut self, g: &mut Graph) {
        // As at every door with a graph: what `element_removed` left
        // open comes first, so that the todo counted below is the one
        // the next step sees.
        self.settle_removed(g);
        // Per rule: nodes and connections of its plan. A dynamic leaf
        // with an attribute correspondence adds that correspondence
        // and its two connections.
        let plans: Vec<(usize, usize)> = self
            .rules
            .iter()
            .map(|rule| {
                let dynamic = rule
                    .create_nodes
                    .iter()
                    .filter(|cn| cn.attr_corr.is_some())
                    .count();
                (
                    rule.create_nodes.len() + dynamic,
                    rule.create_links.len() + 2 * dynamic,
                )
            })
            .collect();
        let (mut pending, mut nodes, mut connections) = (0, 0, 0);
        for rule_ix in self.pending_rules() {
            pending += 1;
            nodes += plans[rule_ix].0;
            connections += plans[rule_ix].1;
        }
        g.reserve(nodes, connections);
        self.by_product.reserve(nodes);
        self.cascade.reserve(pending);
    }

    /// The rule of every match in the todo.
    fn pending_rules(&self) -> impl Iterator<Item = usize> + '_ {
        self.todo.rules()
    }
}

impl Engine<'_> {
    /// What `run` does between the seed and the first step.
    #[cfg(feature = "opt_b")]
    #[inline]
    pub(super) fn room_after_seed(&mut self, g: &mut Graph) {
        self.reserve_for_recorded(g);
    }

    /// What `run` does between the seed and the first step: nothing.
    #[cfg(not(feature = "opt_b"))]
    #[inline(always)]
    pub(super) fn room_after_seed(&mut self, _g: &mut Graph) {}
}

#[cfg(all(test, feature = "opt_b"))]
mod tests {
    use super::Sizes;
    use crate::engine::{Engine, Termination};
    use crate::graph::{Graph, ValueStore};
    use crate::ident::{GhostId, Status};
    use crate::rules::format::RuleFile;

    fn rules(g: &mut Graph) -> Vec<crate::plan::DirectedRule> {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3, "name": "reserve_tests",
            "rules": [{
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
            }]
        }))
        .expect("rule file parses");
        crate::rules::load_file(&file, g).expect("rule file loads")
    }

    /// What a batch run leaves behind, in every order a caller can
    /// rely on.
    type Run = (
        Termination,
        Vec<(usize, Vec<GhostId>, Vec<GhostId>, Vec<GhostId>)>,
        Vec<(GhostId, Status)>,
        Vec<(GhostId, GhostId, bool)>,
        Sizes,
    );

    fn run(families: usize, graph_hint: Option<(usize, usize)>, engine_hint: Option<Sizes>) -> Run {
        let mut g = Graph::new();
        if let Some((nodes, connections)) = graph_hint {
            g.reserve(nodes, connections);
        }
        let mut vs = ValueStore::default();
        for i in 0..families {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            let r = g.add_baseline(&format!("f{i}/father"), "Father");
            let m = g.add_baseline(&format!("f{i}/father/m"), "Member");
            let leaf = g.add_baseline(&format!("f{i}/father/m/fn"), "firstName");
            g.connect(f, r, Status::Solid);
            g.connect(r, m, Status::Solid);
            g.connect(m, leaf, Status::Solid);
            vs.insert(leaf, format!("John{i}"));
        }
        let rules = rules(&mut g);
        let mut e = Engine::new(&rules);
        if let Some(more) = engine_hint {
            e.reserve(more);
        }
        let verdict = e.run(&mut g, &vs, 100_000);
        let cascade = e
            .cascade
            .iter()
            .map(|c| {
                (
                    c.rule_ix,
                    c.refs.clone(),
                    c.created.clone(),
                    c.created_edges.clone(),
                )
            })
            .collect();
        let nodes: Vec<(GhostId, Status)> = g.iter_nodes().map(|n| (n.id, n.status)).collect();
        let parts = nodes
            .iter()
            .flat_map(|(id, _)| g.parts(id).map(|p| (*id, p.other, p.outgoing)))
            .collect();
        (verdict, cascade, nodes, parts, e.sizes())
    }

    /// Capacity is no part of the result: the same cascade, the same
    /// graph in insertion order, the same participations, with a hint
    /// that is too small, one that fits and one that is far too large.
    #[test]
    fn a_hint_changes_no_result() {
        let plain = run(300, None, None);
        assert_eq!(plain.0, Termination::Duplication);
        assert_eq!(plain.4.steps, 300);
        let exact = plain.4;
        for (graph, engine) in [
            (Some((10, 10)), Some(Sizes::default())),
            (Some((4000, 4000)), Some(exact)),
            (
                Some((1 << 20, 1 << 20)),
                Some(Sizes {
                    matches: 1 << 18,
                    elements: 1 << 18,
                    products: 1 << 18,
                    steps: 1 << 18,
                }),
            ),
            (None, Some(exact)),
            (Some((4000, 4000)), None),
        ] {
            assert_eq!(run(300, graph, engine), plain, "{graph:?} {engine:?}");
        }
    }

    /// `reserve_for_recorded` makes room for the plans of the pending
    /// matches, and for nothing once they are applied.
    #[test]
    fn room_for_the_recorded_matches() {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        for i in 0..10 {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            let r = g.add_baseline(&format!("f{i}/father"), "Father");
            let m = g.add_baseline(&format!("f{i}/father/m"), "Member");
            let leaf = g.add_baseline(&format!("f{i}/father/m/fn"), "firstName");
            g.connect(f, r, Status::Solid);
            g.connect(r, m, Status::Solid);
            g.connect(m, leaf, Status::Solid);
            vs.insert(leaf, format!("John{i}"));
        }
        let rules = rules(&mut g);
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        assert_eq!(e.sizes().matches, 10);
        e.reserve_for_recorded(&mut g);
        assert!(e.cascade.capacity() >= 10);
        while e.step(&mut g, &vs).is_some() {}
        assert_eq!(e.sizes().steps, 10);
        let before = e.cascade.capacity();
        e.reserve_for_recorded(&mut g);
        assert_eq!(e.cascade.capacity(), before);
    }
}
