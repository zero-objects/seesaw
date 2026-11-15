//! Differential test on small typed graphs with every kind of delta:
//! nodes, links, values, deletions on either side, seeds, runs with a
//! step budget, and waves that stop before saturation. Three rule sets:
//!
//! - `followup`: a root rule and a rule that references its
//!   correspondence. A second root next to a translated child gives a
//!   match whose anchor is already translated.
//! - `family`: a rule with a value binding and a competing variant of
//!   it with another correspondence type and a lower rank.
//! - `predicates`: one rule per kind of value predicate on a context
//!   leaf, all establishing at the same node.
//!
//! Both engines run the scenario. After every wave the graph, the
//! cascade, the sequence of `step` results and the verdicts of `run`
//! are compared.

use proptest::prelude::*;

/// The element a selector stands for in a collection of `len > 0`.
pub fn pick(selector: usize, len: usize) -> usize {
    selector % len
}

/// One node type of a world.
#[derive(Debug, Clone)]
pub struct Kind {
    name: &'static str,
    /// Values a node of the type can carry; empty: no value.
    values: &'static [&'static str],
    /// The type belongs to the target side.
    target: bool,
}

/// A rule set with the node types and links a model of it uses.
#[derive(Debug, Clone)]
pub struct World {
    rules: String,
    kinds: Vec<Kind>,
    /// Links a model may have: (kind of the source, kind of the target).
    links: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub enum Op {
    /// A new node (kind, value) with links to existing nodes (link of
    /// the world, node of the other kind).
    AddNode(usize, usize, Vec<(usize, usize)>),
    /// A new link between two existing nodes (link of the world, source,
    /// target).
    AddLink(usize, usize, usize),
    /// Removes a link (node, participation of the node).
    RemoveLink(usize, usize),
    DeleteNode(usize),
    /// A new value for a node that carries one (node, value).
    ChangeValue(usize, usize),
    /// Deletes a node a cascade entry created (entry, node of the entry).
    DeleteProduct(usize, usize),
    Seed,
    /// `Engine::run` with a step budget.
    Run(usize),
}

/// One wave: the delta, then steps. `limit`: at most that many steps
/// and no consolidation, so the next delta meets a todo that is not
/// empty; `None`: steps to saturation, then consolidation.
#[derive(Debug, Clone)]
pub struct Wave {
    op: Op,
    limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    world: World,
    /// Every delta announces its domain (`admit_delta`).
    routed: bool,
    waves: Vec<Wave>,
}

fn followup() -> World {
    let rules = serde_json::json!({
        "format": 3,
        "name": "difftest_followup",
        "rules": [
            {
                "name": "Root", "rank": 100,
                "left": {"anchor": "a", "nodes": [{"name": "a", "type": "A"}]},
                "right": {"anchor": "b", "nodes": [{"name": "b", "type": "B"}]},
                "corrs": [{"type": "AB", "left": "a", "right": "b", "role": "establishes"}]
            },
            {
                "name": "Child", "rank": 90,
                "left": {"anchor": "a", "nodes": [
                    {"name": "a", "type": "A"}, {"name": "x", "type": "X"}
                ], "links": [["a", "x"]]},
                "right": {"anchor": "b", "nodes": [
                    {"name": "b", "type": "B"}, {"name": "y", "type": "Y"}
                ], "links": [["b", "y"]]},
                "corrs": [
                    {"type": "AB", "left": "a", "right": "b", "role": "references"},
                    {"type": "XY", "left": "x", "right": "y", "role": "establishes"}
                ]
            }
        ]
    });
    World {
        rules: rules.to_string(),
        kinds: vec![
            Kind {
                name: "A",
                values: &[],
                target: false,
            },
            Kind {
                name: "X",
                values: &[],
                target: false,
            },
            Kind {
                name: "B",
                values: &[],
                target: true,
            },
            Kind {
                name: "Y",
                values: &[],
                target: true,
            },
        ],
        links: vec![(0, 1), (0, 1), (2, 3)],
    }
}

fn family() -> World {
    let rule = |name: &str, rank: u64, corr: &str| {
        serde_json::json!({
            "name": name, "rank": rank,
            "left": {"anchor": "fam", "nodes": [
                {"name": "fam", "type": "Family"},
                {"name": "father", "type": "Father"},
                {"name": "member", "type": "Member"},
                {"name": "first", "type": "firstName"}
            ], "links": [["fam", "father"], ["father", "member"], ["member", "first"]]},
            "right": {"anchor": "male", "nodes": [
                {"name": "male", "type": "Male"},
                {"name": "name", "type": "name"}
            ], "links": [["male", "name"]]},
            "corrs": [
                {"type": corr, "left": "member", "right": "male", "role": "establishes",
                 "bindings": [{"left": "first", "right": "name"}]}
            ]
        })
    };
    let rules = serde_json::json!({
        "format": 3,
        "name": "difftest_family",
        "rules": [
            rule("Father_2_Male", 850, "PersonCorr"),
            rule("Father_2_Male_Variant", 800, "PersonCorr"),
            rule("Father_2_Male_Other", 700, "OtherCorr"),
        ]
    });
    const NAMES: &[&str] = &["Ada", "Bob", "Cy"];
    World {
        rules: rules.to_string(),
        kinds: vec![
            Kind {
                name: "Family",
                values: &[],
                target: false,
            },
            Kind {
                name: "Father",
                values: &[],
                target: false,
            },
            Kind {
                name: "Member",
                values: &[],
                target: false,
            },
            Kind {
                name: "firstName",
                values: NAMES,
                target: false,
            },
            Kind {
                name: "Male",
                values: &[],
                target: true,
            },
            Kind {
                name: "name",
                values: NAMES,
                target: true,
            },
        ],
        links: vec![(0, 1), (1, 2), (2, 3), (4, 5)],
    }
}

fn predicates() -> World {
    let rule = |name: &str, rank: u64, predicate: serde_json::Value| {
        serde_json::json!({
            "name": name, "rank": rank,
            "left": {"anchor": "item", "nodes": [
                {"name": "item", "type": "Item"},
                {"name": "attr", "type": "Attr", "context": true, "predicate": predicate}
            ], "links": [["item", "attr"]]},
            "right": {"anchor": "marker", "nodes": [{"name": "marker", "type": "Marker"}]},
            "corrs": [{"type": "C", "left": "item", "right": "marker", "role": "establishes"}]
        })
    };
    let rules = serde_json::json!({
        "format": 3,
        "name": "difftest_predicates",
        "rules": [
            rule("Equals", 500, serde_json::json!({"kind": "equals", "value": "yes"})),
            rule("Prefix", 400, serde_json::json!({"kind": "prefix", "value": "get"})),
            rule("Regex", 300, serde_json::json!({"kind": "regex", "pattern": "Abstract.*"})),
            rule("Range", 200,
                 serde_json::json!({"kind": "numeric_range", "min": 0.0, "max": 100.0})),
            rule("Exists", 100, serde_json::json!({"kind": "exists"})),
        ]
    });
    World {
        rules: rules.to_string(),
        kinds: vec![
            Kind {
                name: "Item",
                values: &[],
                target: false,
            },
            Kind {
                name: "Attr",
                values: &[
                    "yes",
                    "getAge",
                    "AbstractList",
                    "42",
                    "no",
                    "setAge",
                    "7000",
                ],
                target: false,
            },
            Kind {
                name: "Marker",
                values: &[],
                target: true,
            },
        ],
        links: vec![(0, 1)],
    }
}

/// The code that runs a scenario through one engine crate. Returns one
/// dump per wave.
macro_rules! driver {
    ($module:ident, $krate:ident) => {
        pub mod $module {
            use super::{pick, Op, Scenario};
            use $krate::engine::{DeltaDomain, Engine};
            use $krate::graph::{Graph, ValueStore};
            use $krate::ident::{GhostId, Status};

            struct Model {
                g: Graph,
                vs: ValueStore,
                /// The baseline nodes with their kind.
                nodes: Vec<(GhostId, usize)>,
            }

            impl Model {
                fn alive(&self, id: &GhostId) -> bool {
                    self.g.node(id).is_some_and(|n| n.status.is_matchable())
                }
            }

            fn hex(id: &GhostId) -> String {
                id.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
            }

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
                        &hex(&n.id)[..12],
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
                    writeln!(out, "connection {} {status}", &hex(id)[..12]).unwrap();
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

            pub fn run(s: &Scenario) -> Vec<String> {
                let world = &s.world;
                let mut g = Graph::new();
                let rules = $krate::rules::load(&world.rules, &mut g).expect("rules load");
                let mut m = Model {
                    g,
                    vs: ValueStore::default(),
                    nodes: Vec::new(),
                };
                let mut e = Engine::new(&rules);
                let admit = |e: &mut Engine<'_>, target: bool| {
                    if s.routed {
                        e.admit_delta(&[if target {
                            DeltaDomain::Target
                        } else {
                            DeltaDomain::Source
                        }]);
                    }
                };
                let mut dumps = Vec::new();
                for (wave_ix, wave) in s.waves.iter().enumerate() {
                    let mut note = String::new();
                    match &wave.op {
                        Op::AddNode(kind, value, links) => {
                            let k = pick(*kind, world.kinds.len());
                            let kind = &world.kinds[k];
                            let id = m.g.add_baseline(&format!("n{wave_ix}"), kind.name);
                            if !kind.values.is_empty() {
                                m.vs.insert(id, kind.values[pick(*value, kind.values.len())]);
                            }
                            for (link, other) in links {
                                let (from, to) = world.links[pick(*link, world.links.len())];
                                let wanted = if from == k {
                                    to
                                } else if to == k {
                                    from
                                } else {
                                    continue;
                                };
                                let others: Vec<GhostId> = m
                                    .nodes
                                    .iter()
                                    .filter(|(o, ok)| *ok == wanted && m.alive(o))
                                    .map(|(o, _)| *o)
                                    .collect();
                                if others.is_empty() {
                                    continue;
                                }
                                let other = others[pick(*other, others.len())];
                                if from == k {
                                    m.g.connect(id, other, Status::Solid);
                                } else {
                                    m.g.connect(other, id, Status::Solid);
                                }
                            }
                            m.nodes.push((id, k));
                            admit(&mut e, kind.target);
                            e.elements_added(&m.g, &m.vs, &[id]);
                        }
                        Op::AddLink(link, a, b) => {
                            let (from, to) = world.links[pick(*link, world.links.len())];
                            let of = |k: usize| -> Vec<GhostId> {
                                m.nodes
                                    .iter()
                                    .filter(|(o, ok)| *ok == k && m.alive(o))
                                    .map(|(o, _)| *o)
                                    .collect()
                            };
                            let (sources, targets) = (of(from), of(to));
                            if !sources.is_empty() && !targets.is_empty() {
                                let a = sources[pick(*a, sources.len())];
                                let b = targets[pick(*b, targets.len())];
                                if a != b && !m.g.connected(&a, &b) {
                                    m.g.connect(a, b, Status::Solid);
                                    admit(&mut e, world.kinds[from].target);
                                    e.link_added(&m.g, &m.vs, &a, &b);
                                    note = "linked".into();
                                }
                            }
                        }
                        Op::RemoveLink(node, part) => {
                            if !m.nodes.is_empty() {
                                let (a, k) = m.nodes[pick(*node, m.nodes.len())];
                                let live: Vec<(GhostId, GhostId)> =
                                    m.g.parts(&a)
                                        .filter(|p| p.outgoing)
                                        .filter(|p| {
                                            m.g.connection(&p.connection)
                                                .is_some_and(|c| c.status == Status::Solid)
                                        })
                                        .map(|p| (p.other, p.connection))
                                        .collect();
                                if !live.is_empty() {
                                    let (b, connection) = live[pick(*part, live.len())];
                                    m.g.set_connection_status(&connection, Status::Tombstone);
                                    admit(&mut e, world.kinds[k].target);
                                    e.link_removed(&mut m.g, &a, &b);
                                    note = "unlinked".into();
                                }
                            }
                        }
                        Op::DeleteNode(node) => {
                            if !m.nodes.is_empty() {
                                let (id, k) = m.nodes[pick(*node, m.nodes.len())];
                                if m.alive(&id) {
                                    m.g.set_node_status(&id, Status::Tombstone);
                                    admit(&mut e, world.kinds[k].target);
                                    e.element_removed(&id);
                                    e.element_deleted(&mut m.g, &id);
                                    note = "deleted".into();
                                }
                            }
                        }
                        Op::ChangeValue(node, value) => {
                            let valued: Vec<(GhostId, usize)> = m
                                .nodes
                                .iter()
                                .filter(|(id, k)| !world.kinds[*k].values.is_empty() && m.alive(id))
                                .copied()
                                .collect();
                            if !valued.is_empty() {
                                let (id, k) = valued[pick(*node, valued.len())];
                                let values = world.kinds[k].values;
                                m.vs.insert(id, values[pick(*value, values.len())]);
                                admit(&mut e, world.kinds[k].target);
                                e.element_removed(&id);
                                e.element_changed(&mut m.g, &id);
                                e.elements_added(&m.g, &m.vs, &[id]);
                                note = "changed".into();
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
                                    admit(&mut e, true);
                                    e.element_removed(&id);
                                    e.element_deleted(&mut m.g, &id);
                                    note = "product deleted".into();
                                }
                            }
                        }
                        Op::Seed => {
                            if s.routed {
                                e.admit_initial();
                            }
                            e.seed(&m.g, &m.vs);
                        }
                        Op::Run(budget) => {
                            if s.routed {
                                e.admit_initial();
                            }
                            note = format!("{:?}", e.run(&mut m.g, &m.vs, *budget));
                        }
                    }
                    // The results of `step`, one letter each.
                    let mut steps = String::new();
                    let limit = wave.limit.unwrap_or(2_000);
                    while steps.len() < limit {
                        match e.step(&mut m.g, &m.vs) {
                            Some(true) => steps.push('a'),
                            Some(false) => steps.push('d'),
                            None => break,
                        }
                    }
                    if wave.limit.is_none() {
                        e.consolidate(&mut m.g);
                    }
                    dumps.push(format!(
                        "{:?} {note}\nsteps={steps}\n{}",
                        wave.op,
                        dump(&m, &e)
                    ));
                }
                dumps
            }
        }
    };
}

driver!(released, old);
driver!(current, new);

fn op() -> impl Strategy<Value = Op> {
    let n = 0usize..32;
    prop_oneof![
        8 => (n.clone(), n.clone(), prop::collection::vec((n.clone(), n.clone()), 0..4))
            .prop_map(|(k, v, links)| Op::AddNode(k, v, links)),
        3 => (n.clone(), n.clone(), n.clone()).prop_map(|(l, a, b)| Op::AddLink(l, a, b)),
        2 => (n.clone(), n.clone()).prop_map(|(a, p)| Op::RemoveLink(a, p)),
        3 => n.clone().prop_map(Op::DeleteNode),
        3 => (n.clone(), n.clone()).prop_map(|(a, v)| Op::ChangeValue(a, v)),
        2 => (n.clone(), n.clone()).prop_map(|(e, c)| Op::DeleteProduct(e, c)),
        2 => Just(Op::Seed),
        1 => (0usize..6).prop_map(Op::Run),
    ]
}

fn wave() -> impl Strategy<Value = Wave> {
    (
        op(),
        prop_oneof![4 => Just(None), 1 => (0usize..3).prop_map(Some)],
    )
        .prop_map(|(op, limit)| Wave { op, limit })
}

fn scenario(world: World, waves: usize) -> impl Strategy<Value = Scenario> {
    (any::<bool>(), prop::collection::vec(wave(), 0..waves)).prop_map(move |(routed, waves)| {
        Scenario {
            world: world.clone(),
            routed,
            waves,
        }
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

    #[test]
    fn followup_agrees(s in scenario(followup(), 14)) {
        compare(&s)?;
    }

    #[test]
    fn predicates_agree(s in scenario(predicates(), 14)) {
        compare(&s)?;
    }
}

proptest! {
    // A case of this world takes about a tenth of a second (measured:
    // 400 cases in 45 s), so fewer of them by default.
    #![proptest_config(cases(128))]

    #[test]
    fn family_agrees(s in scenario(family(), 24)) {
        compare(&s)?;
    }
}

/// The run a reading of the code predicted to differ when a match
/// whose anchor is already translated is left out of the match store:
/// a second root next to a translated child, then the first root is
/// deleted, then a seed.
#[test]
fn a_suppressed_match_stays_suppressed_after_the_first_root_is_deleted() {
    let wave = |op| Wave { op, limit: None };
    let s = Scenario {
        world: followup(),
        routed: false,
        waves: vec![
            wave(Op::AddNode(1, 0, vec![])),       // x
            wave(Op::AddNode(0, 0, vec![(0, 0)])), // a1 → x
            wave(Op::AddNode(0, 0, vec![(0, 0)])), // a2 → x
            wave(Op::DeleteNode(1)),               // a1
            wave(Op::Seed),
        ],
    };
    let dumps = released::run(&s);
    let entries = |dump: &str| dump.lines().filter(|l| l.starts_with("entry ")).count();
    // Root(a1), Child(a1, x), Root(a2); Child(a2, x) is a duplicate.
    assert_eq!(entries(&dumps[2]), 3, "{}", dumps[2]);
    assert_eq!(dumps, current::run(&s));
}
