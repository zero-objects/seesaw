//! Differential test on the rule shapes of a language pack: one
//! structural type per side, one node correspondence type, rules told
//! apart by leaf values. The rule set has rules that compete for the
//! same node (a rule with a parent condition and one without, two rules
//! with the same left side), so that matches are suppressed as
//! duplicates and become applicable again when the competing
//! translation is retracted.
//!
//! A scenario is a tree, then a sequence of waves: add a node, change a
//! leaf value, delete a node or a leaf, delete something a rule
//! produced, seed again. Both engines run it; after every wave the
//! graph, the cascade and the sequence of `step` results are compared.

use proptest::prelude::*;

/// One language node of the source model.
#[derive(Debug, Clone)]
pub struct NodeSpec {
    rule: &'static str,
    /// Selects the parent among the nodes so far ([`pick`]).
    parent: usize,
    flag: Option<bool>,
}

/// The element a selector stands for in a collection of `len > 0`.
pub fn pick(selector: usize, len: usize) -> usize {
    selector % len
}

/// One wave after the initial one. The numbers select a node, a
/// cascade entry or a node of an entry ([`pick`]).
#[derive(Debug, Clone)]
pub enum Op {
    Add(NodeSpec),
    ChangeRule(usize, &'static str),
    ChangeFlag(usize, bool),
    DeleteNode(usize),
    DeleteFlag(usize),
    /// Deletes a node a cascade entry created (entry, node of the entry).
    DeleteProduct(usize, usize),
    /// Deletes a connection a cascade entry created (entry, connection
    /// of the entry).
    DeleteProductEdge(usize, usize),
    Seed,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    /// With the rule `Adopt` ([`rules_json`]).
    late_context: bool,
    initial: Vec<NodeSpec>,
    /// The initial wave through `seed` instead of `elements_added`.
    by_seed: bool,
    /// Every wave announces its domain (`admit_delta`); otherwise the
    /// waves run without a route.
    routed: bool,
    ops: Vec<Op>,
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

/// A child rule: a node with rule value `rule` below any node, or below
/// a node with rule value `parent_rule`.
fn child(
    name: &str,
    rank: u64,
    parent_rule: Option<&str>,
    rule: &str,
    feature: &str,
) -> serde_json::Value {
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
        "name": name, "rank": rank,
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
}

/// A flag rule: the flag leaf of a node, with or without a condition
/// on the node's rule value.
fn flag(name: &str, rank: u64, rule: Option<(&str, &str)>) -> serde_json::Value {
    let mut left = vec![serde_json::json!({"name": "a", "type": "J"})];
    let mut left_links = Vec::new();
    let mut right = vec![serde_json::json!({"name": "b", "type": "S"})];
    let mut right_links = Vec::new();
    if let Some((rule, feature)) = rule {
        left.push(leaf("a_rule", "rule", rule, true));
        left_links.push(serde_json::json!(["a", "a_rule"]));
        right.push(leaf("b_feature", "feature", feature, true));
        right_links.push(serde_json::json!(["b", "b_feature"]));
    }
    left.push(leaf("al", "flag", "yes", false));
    left_links.push(serde_json::json!(["a", "al"]));
    right.push(leaf("bl", "open", "on", false));
    right_links.push(serde_json::json!(["b", "bl"]));
    serde_json::json!({
        "name": name, "rank": rank,
        "left": {"anchor": "al", "nodes": left, "links": left_links},
        "right": {"anchor": "bl", "nodes": right, "links": right_links},
        "corrs": [
            {"type": "CN", "left": "a", "right": "b", "role": "references"},
            {"type": "CA", "left": "al", "right": "bl", "role": "establishes"}
        ]
    })
}

/// `late_context` adds the rule `Adopt`: a node below a translated
/// parent is translated when a sibling is. Its match comes into being
/// when the sibling's step creates the sibling's correspondence, at a
/// time when the node itself may already be translated by another
/// rule.
pub fn rules_json(late_context: bool) -> String {
    let mut rules = vec![
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
        child("Cls", 200, None, "cls", "Class"),
        // Competes with `MethodAny` for a method below a class.
        child("Method", 200, Some("cls"), "m", "Method"),
        child("MethodAny", 150, None, "m", "Free"),
        child("Other", 200, None, "x", "Other"),
        // Competes with `FlagAny` for the flag of a method.
        flag("Flag", 100, Some(("m", "Method"))),
        flag("FlagAny", 50, None),
    ];
    if late_context {
        rules.push(serde_json::json!({
            "name": "Adopt", "rank": 40,
            "left": {"anchor": "a", "nodes": [
                {"name": "sp", "type": "J"},
                {"name": "a", "type": "J"},
                {"name": "sib", "type": "J"}
            ], "links": [["sp", "a"], ["sp", "sib"]]},
            "right": {"anchor": "b", "nodes": [
                {"name": "sb", "type": "S"},
                {"name": "b", "type": "S"},
                {"name": "x", "type": "S"},
                leaf("b_feature", "feature", "Adopted", false)
            ], "links": [["sb", "b"], ["sb", "x"], ["b", "b_feature"]]},
            "corrs": [
                {"type": "CN", "left": "sp", "right": "sb", "role": "references"},
                {"type": "CN", "left": "sib", "right": "x", "role": "references"},
                {"type": "CN", "left": "a", "right": "b", "role": "establishes"}
            ]
        }));
    }
    serde_json::json!({"format": 3, "name": "difftest_lang", "rules": rules}).to_string()
}

/// The code that runs a scenario through one engine crate. Returns one
/// dump of graph and cascade per wave.
macro_rules! driver {
    ($module:ident, $krate:ident) => {
        pub mod $module {
            use super::{pick, NodeSpec, Op, Scenario};
            use $krate::engine::{DeltaDomain, Engine};
            use $krate::graph::{Graph, ValueStore};
            use $krate::ident::{GhostId, Status};

            struct Lang {
                node: GhostId,
                rule_leaf: GhostId,
                flag_leaf: Option<GhostId>,
            }

            struct Model {
                g: Graph,
                vs: ValueStore,
                lang: Vec<Lang>,
            }

            impl Model {
                /// Adds one language node; returns the new nodes.
                fn grow(&mut self, spec: &NodeSpec) -> Vec<GhostId> {
                    let ix = self.lang.len();
                    let parent = self.lang[pick(spec.parent, ix)].node;
                    let node = self.g.add_baseline(&format!("n{ix}"), "J");
                    self.g.connect(parent, node, Status::Solid);
                    let rule_leaf = self.g.add_baseline(&format!("n{ix}/rule"), "rule");
                    self.vs.insert(rule_leaf, spec.rule);
                    self.g.connect(node, rule_leaf, Status::Solid);
                    let mut new = vec![node, rule_leaf];
                    let flag_leaf = spec.flag.map(|yes| {
                        let l = self.g.add_baseline(&format!("n{ix}/flag"), "flag");
                        self.vs.insert(l, if yes { "yes" } else { "no" });
                        self.g.connect(node, l, Status::Solid);
                        new.push(l);
                        l
                    });
                    self.lang.push(Lang {
                        node,
                        rule_leaf,
                        flag_leaf,
                    });
                    new
                }

                fn alive(&self, id: &GhostId) -> bool {
                    self.g.node(id).is_some_and(|n| n.status.is_matchable())
                }
            }

            fn hex(id: &GhostId) -> String {
                id.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
            }

            /// Nodes (id, type, status, correspondence flag, value),
            /// connections (id, status) and the cascade in order.
            fn dump(m: &Model, e: &Engine<'_>) -> String {
                use std::fmt::Write;
                let mut out = String::new();
                let mut nodes: Vec<_> = m.g.nodes().collect();
                nodes.sort_by_key(|n| n.id);
                let mut connections: Vec<(GhostId, String)> = Vec::new();
                for n in &nodes {
                    writeln!(
                        out,
                        "node {} {} {:?} corr={} value={:?}",
                        hex(&n.id),
                        m.g.types.name(n.typ),
                        n.status,
                        n.is_corr,
                        m.g.resolve_value(&n.id, &m.vs)
                    )
                    .unwrap();
                    for p in m.g.parts(&n.id) {
                        if p.outgoing {
                            let status =
                                m.g.connection(&p.connection)
                                    .map(|c| format!("{:?}", c.status))
                                    .unwrap_or_default();
                            connections.push((p.connection, status));
                        }
                    }
                }
                connections.sort();
                for (id, status) in &connections {
                    writeln!(out, "connection {} {status}", hex(id)).unwrap();
                }
                for entry in &e.cascade {
                    write!(out, "entry rule={} rank={}", entry.rule_ix, entry.rank).unwrap();
                    for (name, list) in [
                        ("refs", &entry.refs),
                        ("created", &entry.created),
                        ("edges", &entry.created_edges),
                    ] {
                        write!(out, " {name}=[").unwrap();
                        for id in list {
                            write!(out, "{} ", &hex(id)[..12]).unwrap();
                        }
                        out.push(']');
                    }
                    out.push('\n');
                }
                writeln!(out, "contradiction={}", e.saw_contradiction).unwrap();
                out
            }

            /// Steps to saturation and consolidates. Returns the results
            /// of `step`, one letter each (applied, duplicate), and an
            /// `!` when the step limit was reached.
            fn settle(m: &mut Model, e: &mut Engine<'_>) -> String {
                let mut steps = String::new();
                loop {
                    match e.step(&mut m.g, &m.vs) {
                        Some(true) => steps.push('a'),
                        Some(false) => steps.push('d'),
                        None => break,
                    }
                    if steps.len() > 2_000 {
                        steps.push('!');
                        return steps;
                    }
                }
                e.consolidate(&mut m.g);
                steps
            }

            pub fn run(s: &Scenario) -> Vec<String> {
                let mut g = Graph::new();
                let mut vs = ValueStore::default();
                let _project = g.add_baseline("project", "P");
                let file = g.add_baseline("file", "F");
                let root = g.add_baseline("n0", "J");
                let root_rule = g.add_baseline("n0/rule", "rule");
                vs.insert(root_rule, "unit");
                g.connect(file, root, Status::Solid);
                g.connect(root, root_rule, Status::Solid);
                let rules = $krate::rules::load(&super::rules_json(s.late_context), &mut g)
                    .expect("rules load");
                let mut m = Model {
                    g,
                    vs,
                    lang: vec![Lang {
                        node: root,
                        rule_leaf: root_rule,
                        flag_leaf: None,
                    }],
                };
                let mut all = vec![file, root, root_rule];
                for spec in &s.initial {
                    all.extend(m.grow(spec));
                }
                let mut e = Engine::new(&rules);
                let source = |e: &mut Engine<'_>| {
                    if s.routed {
                        e.admit_delta(&[DeltaDomain::Source]);
                    }
                };
                if s.by_seed {
                    e.seed(&m.g, &m.vs);
                } else {
                    source(&mut e);
                    e.elements_added(&m.g, &m.vs, &all);
                }
                let mut dumps = Vec::new();
                let mut wave = |m: &mut Model, e: &mut Engine<'_>, what: String| {
                    let steps = settle(m, e);
                    dumps.push(format!("{what}\nsteps={steps}\n{}", dump(m, e)));
                };
                wave(&mut m, &mut e, "initial".into());
                for op in &s.ops {
                    let what = format!("{op:?}");
                    match op {
                        Op::Add(spec) => {
                            let new = m.grow(spec);
                            source(&mut e);
                            e.elements_added(&m.g, &m.vs, &new);
                        }
                        Op::ChangeRule(at, value) => {
                            let l = &m.lang[pick(*at, m.lang.len())];
                            let (node, leaf) = (l.node, l.rule_leaf);
                            if m.alive(&node) && m.alive(&leaf) {
                                m.vs.insert(leaf, *value);
                                source(&mut e);
                                e.element_removed(&leaf);
                                e.element_changed(&mut m.g, &leaf);
                                e.elements_added(&m.g, &m.vs, &[leaf]);
                            }
                        }
                        Op::ChangeFlag(at, yes) => {
                            let l = &m.lang[pick(*at, m.lang.len())];
                            if let (node, Some(leaf)) = (l.node, l.flag_leaf) {
                                if m.alive(&node) && m.alive(&leaf) {
                                    m.vs.insert(leaf, if *yes { "yes" } else { "no" });
                                    source(&mut e);
                                    e.element_removed(&leaf);
                                    e.element_changed(&mut m.g, &leaf);
                                    e.elements_added(&m.g, &m.vs, &[leaf]);
                                }
                            }
                        }
                        Op::DeleteNode(at) => {
                            let ix = pick(*at, m.lang.len());
                            let node = m.lang[ix].node;
                            if ix > 0 && m.alive(&node) {
                                m.g.set_node_status(&node, Status::Tombstone);
                                source(&mut e);
                                e.element_removed(&node);
                                e.element_deleted(&mut m.g, &node);
                            }
                        }
                        Op::DeleteFlag(at) => {
                            let l = &m.lang[pick(*at, m.lang.len())];
                            if let Some(leaf) = l.flag_leaf {
                                if m.alive(&leaf) {
                                    m.g.set_node_status(&leaf, Status::Tombstone);
                                    source(&mut e);
                                    e.element_removed(&leaf);
                                    e.element_deleted(&mut m.g, &leaf);
                                }
                            }
                        }
                        Op::DeleteProduct(entry, node) => {
                            let product = (!e.cascade.is_empty())
                                .then(|| &e.cascade[pick(*entry, e.cascade.len())].created)
                                .filter(|created| !created.is_empty())
                                .map(|created| created[pick(*node, created.len())]);
                            if let Some(id) = product {
                                if m.alive(&id) {
                                    m.g.set_node_status(&id, Status::Tombstone);
                                    if s.routed {
                                        e.admit_delta(&[DeltaDomain::Target]);
                                    }
                                    e.element_removed(&id);
                                    e.element_deleted(&mut m.g, &id);
                                }
                            }
                        }
                        Op::DeleteProductEdge(entry, edge) => {
                            let product = (!e.cascade.is_empty())
                                .then(|| &e.cascade[pick(*entry, e.cascade.len())].created_edges)
                                .filter(|edges| !edges.is_empty())
                                .map(|edges| edges[pick(*edge, edges.len())]);
                            if let Some(id) = product {
                                if m.g.connection(&id).is_some_and(|c| c.status.is_matchable()) {
                                    m.g.set_connection_status(&id, Status::Tombstone);
                                    if s.routed {
                                        e.admit_delta(&[DeltaDomain::Target]);
                                    }
                                    e.element_removed(&id);
                                    e.element_deleted(&mut m.g, &id);
                                }
                            }
                        }
                        Op::Seed => {
                            if s.routed {
                                e.admit_initial();
                            }
                            e.seed(&m.g, &m.vs);
                        }
                    }
                    wave(&mut m, &mut e, what);
                }
                dumps
            }
        }
    };
}

driver!(released, old);
driver!(current, new);

fn rule_value() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("cls"), Just("m"), Just("x"), Just("unit")]
}

fn node_spec() -> impl Strategy<Value = NodeSpec> {
    (
        rule_value(),
        0usize..64,
        prop_oneof![Just(None), Just(Some(true)), Just(Some(false))],
    )
        .prop_map(|(rule, parent, flag)| NodeSpec { rule, parent, flag })
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => node_spec().prop_map(Op::Add),
        4 => (0usize..64, rule_value()).prop_map(|(at, v)| Op::ChangeRule(at, v)),
        2 => (0usize..64, any::<bool>()).prop_map(|(at, yes)| Op::ChangeFlag(at, yes)),
        2 => (0usize..64).prop_map(Op::DeleteNode),
        1 => (0usize..64).prop_map(Op::DeleteFlag),
        2 => (0usize..64, 0usize..64).prop_map(|(e, n)| Op::DeleteProduct(e, n)),
        2 => (0usize..64, 0usize..64).prop_map(|(e, n)| Op::DeleteProductEdge(e, n)),
        1 => Just(Op::Seed),
    ]
}

fn scenario(late_context: bool, nodes: usize, ops: usize) -> impl Strategy<Value = Scenario> {
    (
        prop::collection::vec(node_spec(), 0..nodes),
        any::<bool>(),
        any::<bool>(),
        prop::collection::vec(op(), 0..ops),
    )
        .prop_map(move |(initial, by_seed, routed, ops)| Scenario {
            late_context,
            initial,
            by_seed,
            routed,
            ops,
        })
}

/// Runs the scenario through both engines and compares wave by wave.
fn compare(s: &Scenario) -> Result<(), TestCaseError> {
    let released = released::run(s);
    let current = current::run(s);
    prop_assert_eq!(released.len(), current.len());
    for (wave, (old, new)) in released.iter().zip(&current).enumerate() {
        prop_assert!(
            old == new,
            "wave {} differs\n--- 2.0.2\n{}\n--- current\n{}",
            wave,
            old,
            new
        );
    }
    Ok(())
}

/// Cases per property; `DIFFTEST_CASES` overrides it.
fn cases(default: u32) -> ProptestConfig {
    let cases = std::env::var("DIFFTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default);
    ProptestConfig::with_cases(cases)
}

proptest! {
    #![proptest_config(cases(512))]

    /// Adds only: the initial wave and added nodes.
    #[test]
    fn adds_agree(
        initial in prop::collection::vec(node_spec(), 0..24),
        by_seed in any::<bool>(),
        routed in any::<bool>(),
        added in prop::collection::vec(node_spec(), 0..8),
    ) {
        let s = Scenario {
            late_context: false,
            initial,
            by_seed,
            routed,
            ops: added.into_iter().map(Op::Add).collect(),
        };
        compare(&s)?;
    }

    /// Adds, changes, deletions and seeds in any order.
    #[test]
    fn waves_agree(s in scenario(false, 16, 12)) {
        compare(&s)?;
    }

    /// The same with the rule `Adopt`, whose matches come into being
    /// on nodes a step has just created while their anchor is already
    /// translated.
    #[test]
    fn waves_agree_with_a_late_context(s in scenario(true, 10, 12)) {
        compare(&s)?;
    }
}

/// A fixed scenario, as a check that the driver does what it says: the
/// cascade grows and both engines are driven.
#[test]
fn the_driver_reaches_the_cascade() {
    let s = Scenario {
        late_context: false,
        initial: vec![
            NodeSpec {
                rule: "cls",
                parent: 0,
                flag: None,
            },
            NodeSpec {
                rule: "m",
                parent: 1,
                flag: Some(true),
            },
        ],
        by_seed: false,
        routed: false,
        ops: vec![Op::ChangeRule(1, "x"), Op::DeleteNode(2), Op::Seed],
    };
    let dumps = current::run(&s);
    let entries = |dump: &str| dump.lines().filter(|l| l.starts_with("entry ")).count();
    // Root, Cls, Method (the competing MethodAny is a duplicate), Flag.
    assert!(entries(&dumps[0]) >= 4, "cascade too short:\n{}", dumps[0]);
    assert!(dumps.iter().all(|d| !d.contains('!')));
    assert_eq!(dumps, released::run(&s));
}
