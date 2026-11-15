//! Workload "language pack": many rules over ONE structural type per
//! side and ONE node correspondence type. What tells the rules apart is
//! the value of a leaf next to the node (`rule` on the left, `feature`
//! on the right), and for half of the kinds the value of the parent's
//! leaf as well.
//!
//! Synthetic. It reproduces the shape of the rule sets a language pack
//! generates, not a particular pack.
//!
//! Rules for `kinds` kinds:
//! - `Root`: file and unit.
//! - `C<i>`: a child of kind `i`. For `i >= kinds / 2` the rule needs a
//!   parent of kind `i - kinds / 2`.
//! - `A<i>`: the flag leaf of a node of kind `i`.

use seesaw_tgg::engine::{DeltaDomain, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;

use crate::fingerprint;
use crate::measure::{self, Usage};
use crate::Outcome;

/// Size of one model.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Number of kinds; the rule set has `2 * kinds + 1` rules.
    pub kinds: usize,
    /// Number of language nodes below the root.
    pub nodes: usize,
}

/// xorshift64, fixed seed: the same model on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

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

fn rules_json(kinds: usize) -> String {
    let half = kinds / 2;
    let mut rules = vec![serde_json::json!({
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
    })];
    for i in 0..kinds {
        let mut left = vec![
            serde_json::json!({"name": "sp", "type": "J"}),
            serde_json::json!({"name": "a", "type": "J"}),
            leaf("a_rule", "rule", &format!("k{i}"), false),
        ];
        let mut links = vec![
            serde_json::json!(["sp", "a"]),
            serde_json::json!(["a", "a_rule"]),
        ];
        if i >= half {
            left.push(leaf("sp_rule", "rule", &format!("k{}", i - half), true));
            links.push(serde_json::json!(["sp", "sp_rule"]));
        }
        rules.push(serde_json::json!({
            "name": format!("C{i}"), "rank": 200,
            "left": {"anchor": "a", "nodes": left, "links": links},
            "right": {"anchor": "b", "nodes": [
                {"name": "sb", "type": "S"},
                {"name": "b", "type": "S"},
                leaf("b_feature", "feature", &format!("F{i}"), false)
            ], "links": [["sb", "b"], ["b", "b_feature"]]},
            "corrs": [
                {"type": "CN", "left": "sp", "right": "sb", "role": "references"},
                {"type": "CN", "left": "a", "right": "b", "role": "establishes"}
            ]
        }));
        rules.push(serde_json::json!({
            "name": format!("A{i}"), "rank": 100,
            "left": {"anchor": "al", "nodes": [
                {"name": "a", "type": "J"},
                leaf("a_rule", "rule", &format!("k{i}"), true),
                leaf("al", "flag", "yes", false)
            ], "links": [["a", "a_rule"], ["a", "al"]]},
            "right": {"anchor": "bl", "nodes": [
                {"name": "b", "type": "S"},
                leaf("b_feature", "feature", &format!("F{i}"), true),
                leaf("bl", "open", "on", false)
            ], "links": [["b", "b_feature"], ["b", "bl"]]},
            "corrs": [
                {"type": "CN", "left": "a", "right": "b", "role": "references"},
                {"type": "CA", "left": "al", "right": "bl", "role": "establishes"}
            ]
        }));
    }
    serde_json::json!({"format": 3, "name": "langpack", "rules": rules}).to_string()
}

/// One language node of the source model.
#[derive(Debug, Clone, Copy)]
struct Lang {
    node: GhostId,
    rule_leaf: GhostId,
    kind: usize,
}

/// A source model: project, file, unit and the tree below it.
struct Model {
    g: Graph,
    vs: ValueStore,
    /// Every baseline node, in creation order.
    all: Vec<GhostId>,
    /// Root first, then the language nodes.
    lang: Vec<Lang>,
    /// Language nodes per kind (for parents of dependent kinds).
    by_kind: Vec<Vec<usize>>,
    rng: Rng,
    spec: Spec,
}

/// Parents are drawn from this many most recent nodes: bounded fan-out,
/// growing depth.
const WINDOW: usize = 64;

impl Model {
    fn new(spec: Spec) -> Self {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        let _project = g.add_baseline("project", "P");
        let file = g.add_baseline("file", "F");
        let root = g.add_baseline("n0", "J");
        let leaf = g.add_baseline("n0/rule", "rule");
        vs.insert(leaf, "unit");
        g.connect(file, root, Status::Solid);
        g.connect(root, leaf, Status::Solid);
        let mut m = Self {
            g,
            vs,
            all: vec![file, root, leaf],
            lang: vec![Lang {
                node: root,
                rule_leaf: leaf,
                kind: usize::MAX,
            }],
            by_kind: vec![Vec::new(); spec.kinds],
            rng: Rng(0x9E37_79B9_7F4A_7C15),
            spec,
        };
        for _ in 0..spec.nodes {
            m.grow();
        }
        m
    }

    /// Adds one language node with its leaves; returns the new baseline
    /// nodes.
    fn grow(&mut self) -> Vec<GhostId> {
        let half = self.spec.kinds / 2;
        let ix = self.lang.len();
        let mut kind = self.rng.below(self.spec.kinds);
        let parent = if kind >= half && !self.by_kind[kind - half].is_empty() {
            let of_kind = &self.by_kind[kind - half];
            let from = of_kind.len().saturating_sub(WINDOW);
            of_kind[from + self.rng.below(of_kind.len() - from)]
        } else {
            if kind >= half {
                kind -= half;
            }
            let from = ix.saturating_sub(WINDOW);
            from + self.rng.below(ix - from)
        };
        let node = self.g.add_baseline(&format!("n{ix}"), "J");
        self.g.connect(self.lang[parent].node, node, Status::Solid);
        let rule_leaf = self.g.add_baseline(&format!("n{ix}/rule"), "rule");
        self.vs.insert(rule_leaf, format!("k{kind}"));
        self.g.connect(node, rule_leaf, Status::Solid);
        let mut new = vec![node, rule_leaf];
        let flag = self.rng.below(3);
        if flag > 0 {
            let l = self.g.add_baseline(&format!("n{ix}/flag"), "flag");
            self.vs.insert(l, if flag == 1 { "yes" } else { "no" });
            self.g.connect(node, l, Status::Solid);
            new.push(l);
        }
        self.lang.push(Lang {
            node,
            rule_leaf,
            kind,
        });
        self.by_kind[kind].push(ix);
        self.all.extend(&new);
        new
    }
}

fn saturate(e: &mut Engine<'_>, g: &mut Graph, vs: &ValueStore) -> u64 {
    let mut steps = 0;
    while e.step(g, vs).is_some() {
        steps += 1;
    }
    steps
}

fn outcome(g: &Graph, e: &Engine<'_>, vs: &ValueStore, steps: u64, usage: Usage) -> Outcome {
    Outcome {
        usage,
        steps,
        applied: e.cascade.len() as u64,
        nodes: g.node_count() as u64,
        fingerprint: fingerprint::of(g, e, vs),
    }
}

fn load(spec: Spec, g: &mut Graph) -> Vec<DirectedRule> {
    seesaw_tgg::rules::load(&rules_json(spec.kinds), g).expect("langpack rules load")
}

/// Batch over the whole model through `seed`.
pub fn seed(spec: Spec) -> Outcome {
    let mut m = Model::new(spec);
    let rules = load(spec, &mut m.g);
    let mut e = Engine::new(&rules);
    let (steps, usage) = measure::section(|| {
        e.seed(&m.g, &m.vs);
        saturate(&mut e, &mut m.g, &m.vs)
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// The seed alone on `threads` threads (`Engine::seed_with_threads`;
/// one thread is `Engine::seed`). The steps to saturation follow after
/// the measured section, so the fingerprint is that of [`seed`].
#[cfg(feature = "opt_j")]
pub fn seed_search(spec: Spec, threads: usize) -> Outcome {
    let mut m = Model::new(spec);
    let rules = load(spec, &mut m.g);
    let mut e = Engine::new(&rules);
    let ((), usage) = measure::section(|| e.seed_with_threads(&m.g, &m.vs, threads));
    let steps = saturate(&mut e, &mut m.g, &m.vs);
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// An engine that starts on a model that is already translated: a
/// second engine seeds the synchronised model on `threads` threads.
/// Measured is that seed alone; it finds the matches of every rule.
#[cfg(feature = "opt_j")]
pub fn reseed(spec: Spec, threads: usize) -> Outcome {
    let (mut m, rules) = synchronised(spec);
    let mut first = Engine::new(&rules);
    first.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut first, &mut m.g, &m.vs);
    let mut e = Engine::new(&rules);
    let ((), usage) = measure::section(|| e.seed_with_threads(&m.g, &m.vs, threads));
    let steps = saturate(&mut e, &mut m.g, &m.vs);
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// [`seed`] with the seed on `threads` threads.
#[cfg(feature = "opt_j")]
pub fn seed_par(spec: Spec, threads: usize) -> Outcome {
    let mut m = Model::new(spec);
    let rules = load(spec, &mut m.g);
    let mut e = Engine::new(&rules);
    let (steps, usage) = measure::section(|| {
        e.seed_with_threads(&m.g, &m.vs, threads);
        saturate(&mut e, &mut m.g, &m.vs)
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// The whole model as one add delta (`elements_added`).
pub fn add(spec: Spec) -> Outcome {
    let mut m = Model::new(spec);
    let rules = load(spec, &mut m.g);
    let mut e = Engine::new(&rules);
    let (steps, usage) = measure::section(|| {
        e.elements_added(&m.g, &m.vs, &m.all);
        saturate(&mut e, &mut m.g, &m.vs)
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// `engines` batches one after the other in one process, each with a
/// model, a rule set and an engine of its own: a caller that loads the
/// rules and builds an engine per input. Models and rules are built
/// before the measured section; the fingerprint covers all runs.
pub fn engines(spec: Spec, engines: usize) -> Outcome {
    let mut models = Vec::with_capacity(engines);
    let mut rule_sets = Vec::with_capacity(engines);
    for _ in 0..engines {
        let mut m = Model::new(spec);
        rule_sets.push(load(spec, &mut m.g));
        models.push(m);
    }
    let mut runs: Vec<Engine<'_>> = Vec::with_capacity(engines);
    let (steps, usage) = measure::section(|| {
        let mut steps = 0;
        for (m, rules) in models.iter_mut().zip(&rule_sets) {
            let mut e = Engine::new(rules);
            e.seed(&m.g, &m.vs);
            steps += saturate(&mut e, &mut m.g, &m.vs);
            runs.push(e);
        }
        steps
    });
    let mut all = String::new();
    let (mut applied, mut nodes) = (0, 0);
    for (m, e) in models.iter().zip(&runs) {
        applied += e.cascade.len() as u64;
        nodes += m.g.node_count() as u64;
        all.push_str(&fingerprint::of(&m.g, e, &m.vs));
    }
    Outcome {
        usage,
        steps,
        applied,
        nodes,
        fingerprint: fingerprint::of_text(&all),
    }
}

/// `engines` engines one after the other, each over rules loaded anew,
/// each with one expansion that finds nothing: what it costs an engine
/// to come by its anchor index.
pub fn index_only(spec: Spec, engines: usize) -> Outcome {
    let mut g = Graph::new();
    let vs = ValueStore::default();
    let lone = g.add_baseline("lone", "Lone");
    let rule_sets: Vec<Vec<DirectedRule>> = (0..engines).map(|_| load(spec, &mut g)).collect();
    let mut runs: Vec<Engine<'_>> = Vec::with_capacity(engines);
    let ((), usage) = measure::section(|| {
        for rules in &rule_sets {
            let mut e = Engine::new(rules);
            e.elements_added(&g, &vs, &[lone]);
            runs.push(e);
        }
    });
    let e = runs.last().expect("at least one engine");
    outcome(&g, e, &vs, 0, usage)
}

/// A synchronised model and engine, as the start of the delta cells.
fn synchronised(spec: Spec) -> (Model, Vec<DirectedRule>) {
    let mut m = Model::new(spec);
    let rules = load(spec, &mut m.g);
    (m, rules)
}

/// `delta` new nodes on a synchronised model, one wave per node.
pub fn delta_add(spec: Spec, delta: usize) -> Outcome {
    let (mut m, rules) = synchronised(spec);
    let mut e = Engine::new(&rules);
    e.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut e, &mut m.g, &m.vs);
    let (steps, usage) = measure::section(|| {
        let mut steps = 0;
        for _ in 0..delta {
            let new = m.grow();
            e.admit_delta(&[DeltaDomain::Source]);
            e.elements_added(&m.g, &m.vs, &new);
            steps += saturate(&mut e, &mut m.g, &m.vs);
            e.consolidate(&mut m.g);
        }
        steps
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// `delta` value changes of a `rule` leaf (the node changes its kind),
/// one wave per change.
pub fn delta_change(spec: Spec, delta: usize) -> Outcome {
    let (mut m, rules) = synchronised(spec);
    let mut e = Engine::new(&rules);
    e.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut e, &mut m.g, &m.vs);
    let half = spec.kinds / 2;
    let (steps, usage) = measure::section(|| {
        let mut steps = 0;
        for _ in 0..delta {
            let ix = 1 + m.rng.below(m.lang.len() - 1);
            let l = m.lang[ix];
            // Another kind without a parent condition.
            let kind = (l.kind % half + 1) % half;
            m.lang[ix].kind = kind;
            m.vs.insert(l.rule_leaf, format!("k{kind}"));
            e.admit_delta(&[DeltaDomain::Source]);
            e.element_removed(&l.rule_leaf);
            e.element_changed(&mut m.g, &l.rule_leaf);
            e.elements_added(&m.g, &m.vs, &[l.rule_leaf]);
            steps += saturate(&mut e, &mut m.g, &m.vs);
            e.consolidate(&mut m.g);
        }
        steps
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// `delta` value changes of ONE `rule` leaf, which goes back and forth
/// between two kinds, one wave per change. A match that is retracted
/// and found again gets a new record while the old one stays in the
/// engine's lists, so the cost of the k-th change of one place shows
/// whether it grows with k.
pub fn delta_change_same(spec: Spec, delta: usize) -> Outcome {
    let (mut m, rules) = synchronised(spec);
    let mut e = Engine::new(&rules);
    e.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut e, &mut m.g, &m.vs);
    let half = spec.kinds / 2;
    let ix = 1 + m.rng.below(m.lang.len() - 1);
    let l = m.lang[ix];
    let kinds = [l.kind % half, (l.kind % half + 1) % half];
    let (steps, usage) = measure::section(|| {
        let mut steps = 0;
        for i in 0..delta {
            let kind = kinds[(i + 1) % 2];
            m.vs.insert(l.rule_leaf, format!("k{kind}"));
            e.admit_delta(&[DeltaDomain::Source]);
            e.element_removed(&l.rule_leaf);
            e.element_changed(&mut m.g, &l.rule_leaf);
            e.elements_added(&m.g, &m.vs, &[l.rule_leaf]);
            steps += saturate(&mut e, &mut m.g, &m.vs);
            e.consolidate(&mut m.g);
        }
        steps
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// `delta` deletions of a language node, one wave per deletion.
pub fn delta_delete(spec: Spec, delta: usize) -> Outcome {
    let (mut m, rules) = synchronised(spec);
    let mut e = Engine::new(&rules);
    e.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut e, &mut m.g, &m.vs);
    let (steps, usage) = measure::section(|| {
        let mut steps = 0;
        for _ in 0..delta {
            let ix = 1 + m.rng.below(m.lang.len() - 1);
            let node = m.lang[ix].node;
            if !m.g.node(&node).is_some_and(|n| n.status.is_matchable()) {
                continue;
            }
            m.g.set_node_status(&node, Status::Tombstone);
            e.admit_delta(&[DeltaDomain::Source]);
            e.element_removed(&node);
            e.element_deleted(&mut m.g, &node);
            steps += saturate(&mut e, &mut m.g, &m.vs);
            e.consolidate(&mut m.g);
        }
        steps
    });
    outcome(&m.g, &e, &m.vs, steps, usage)
}

/// The forward result as the baseline of a second graph, then the
/// whole target side as one add delta: the backward wave.
pub fn backward(spec: Spec) -> Outcome {
    // Forward once (not measured) to obtain a target model.
    let (mut m, rules) = synchronised(spec);
    let mut e = Engine::new(&rules);
    e.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut e, &mut m.g, &m.vs);

    // Copy the target side (types S, feature, open and the project)
    // into a fresh graph as baseline nodes.
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    let target = ["P", "S", "feature", "open"];
    let mut nodes: Vec<_> =
        m.g.nodes()
            .filter(|n| n.status.is_matchable() && target.contains(&m.g.types.name(n.typ)))
            .collect();
    nodes.sort_by_key(|n| n.id);
    let mut copy = std::collections::BTreeMap::new();
    let mut all = Vec::new();
    for n in &nodes {
        let id = g.add_baseline(&n.id.hex(), m.g.types.name(n.typ));
        if let Some(v) = m.g.resolve_value(&n.id, &m.vs) {
            vs.insert(id, v);
        }
        copy.insert(n.id, id);
        all.push(id);
    }
    for n in &nodes {
        for p in m.g.parts(&n.id) {
            if p.outgoing && m.g.connected(&n.id, &p.other) {
                if let Some(&to) = copy.get(&p.other) {
                    g.connect(copy[&n.id], to, Status::Solid);
                }
            }
        }
    }
    let rules = load(spec, &mut g);
    let mut e = Engine::new(&rules);
    let (steps, usage) = measure::section(|| {
        e.admit_delta(&[DeltaDomain::Target]);
        e.elements_added(&g, &vs, &all);
        saturate(&mut e, &mut g, &vs)
    });
    outcome(&g, &e, &vs, steps, usage)
}
