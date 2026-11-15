//! Workload "Families to Persons" (Benchmarx): five rules, each with
//! its own node types, over a wide and shallow model. Four members per
//! family, so a model of `n` families takes `4 n + 1` applied steps.
//!
//! The rule set and the model are those of the scaling anchors of the
//! implementation report.

use seesaw_tgg::engine::{DeltaDomain, Engine, Termination};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;

use crate::fingerprint;
use crate::hints;
use crate::measure;
use crate::Outcome;

fn member_rule(name: &str, role: &str, rank: u64) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "rank": rank,
        "left": {
            "anchor": "fam",
            "nodes": [
                {"name": "fam", "type": "Family"},
                {"name": "role", "type": role},
                {"name": "member", "type": "Member"},
                {"name": "first", "type": "firstName"}
            ],
            "links": [["fam", "role"], ["role", "member"], ["member", "first"]]
        },
        "right": {
            "anchor": "person",
            "nodes": [
                {"name": "person",
                 "type": if role == "Father" || role == "Sons" { "Male" } else { "Female" }},
                {"name": "pname", "type": "name"}
            ],
            "links": [["person", "pname"]]
        },
        "corrs": [
            {"type": "PersonCorr", "left": "member", "right": "person",
             "role": "establishes",
             "bindings": [{"left": "first", "right": "pname"}]}
        ]
    })
}

fn register_rule() -> serde_json::Value {
    serde_json::json!({
        "name": "Register", "rank": 1000,
        "left": {
            "anchor": "l0",
            "nodes": [
                {"name": "l0", "type": "FamilyRegister"},
                {"name": "l1", "type": "regName"}
            ],
            "links": [["l0", "l1"]]
        },
        "right": {
            "anchor": "r0",
            "nodes": [
                {"name": "r0", "type": "PersonRegister"},
                {"name": "r1", "type": "name"}
            ],
            "links": [["r0", "r1"]]
        },
        "corrs": [
            {"type": "RegisterCorr", "left": "l0", "right": "r0", "role": "establishes",
             "bindings": [{"left": "l1", "right": "r1"}]}
        ]
    })
}

fn rules(g: &mut Graph) -> Vec<DirectedRule> {
    let file = serde_json::json!({
        "format": 3, "name": "f2p",
        "rules": [
            register_rule(),
            member_rule("Father_2_Male", "Father", 850),
            member_rule("Mother_2_Female", "Mother", 840),
            member_rule("Sons_2_Male", "Sons", 830),
            member_rule("Daughters_2_Female", "Daughters", 820),
        ]
    });
    seesaw_tgg::rules::load(&file.to_string(), g).expect("f2p rules load")
}

struct Model {
    g: Graph,
    vs: ValueStore,
    reg: GhostId,
    all: Vec<GhostId>,
    /// The `Member` nodes, in creation order.
    members: Vec<GhostId>,
    families: usize,
}

impl Model {
    fn new(families: usize) -> Self {
        let mut g = Graph::new();
        // 13 nodes and 13 connections per family, and the register.
        hints::graph(&mut g, 13 * families + 2, 13 * families + 1, &hints::F2P);
        let mut vs = ValueStore::default();
        let reg = g.add_baseline("reg", "FamilyRegister");
        let regname = g.add_baseline("reg/name", "regName");
        g.connect(reg, regname, Status::Solid);
        vs.insert(regname, "R");
        let mut m = Self {
            g,
            vs,
            reg,
            all: vec![reg, regname],
            members: Vec::new(),
            families: 0,
        };
        for _ in 0..families {
            m.grow();
        }
        m
    }

    /// Adds one family with its four members; returns the new nodes.
    fn grow(&mut self) -> Vec<GhostId> {
        let i = self.families;
        self.families += 1;
        let f = self.g.add_baseline(&format!("f{i}"), "Family");
        self.g.connect(self.reg, f, Status::Solid);
        let mut new = vec![f];
        for role in ["Father", "Mother", "Sons", "Daughters"] {
            let r = self.g.add_baseline(&format!("f{i}/{role}"), role);
            self.g.connect(f, r, Status::Solid);
            let m = self.g.add_baseline(&format!("f{i}/{role}/m"), "Member");
            self.g.connect(r, m, Status::Solid);
            let leaf = self
                .g
                .add_baseline(&format!("f{i}/{role}/m/fn"), "firstName");
            self.g.connect(m, leaf, Status::Solid);
            self.vs.insert(leaf, format!("{role}-{i}"));
            self.members.push(m);
            new.extend([r, m, leaf]);
        }
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

/// Batch through `Engine::run` (seed, then steps to saturation).
pub fn batch(families: usize) -> Outcome {
    let mut m = Model::new(families);
    let rules = rules(&mut m.g);
    let mut e = Engine::new(&rules);
    hints::engine(&mut e, m.all.len(), &hints::F2P);
    let (t, usage) = measure::section(|| e.run(&mut m.g, &m.vs, 100_000_000));
    assert_eq!(t, Termination::Duplication);
    assert_eq!(e.cascade.len(), 4 * families + 1);
    hints::report("f2p/batch", &m.g, &e, m.all.len());
    Outcome {
        usage,
        steps: e.cascade.len() as u64,
        applied: e.cascade.len() as u64,
        nodes: m.g.node_count() as u64,
        fingerprint: fingerprint::of(&m.g, &e, &m.vs),
    }
}

/// The whole model as one add delta.
pub fn add(families: usize) -> Outcome {
    let mut m = Model::new(families);
    let rules = rules(&mut m.g);
    let mut e = Engine::new(&rules);
    hints::engine(&mut e, m.all.len(), &hints::F2P);
    let (steps, usage) = measure::section(|| {
        e.elements_added(&m.g, &m.vs, &m.all);
        saturate(&mut e, &mut m.g, &m.vs)
    });
    assert_eq!(e.cascade.len(), 4 * families + 1);
    hints::report("f2p/add", &m.g, &e, m.all.len());
    Outcome {
        usage,
        steps,
        applied: e.cascade.len() as u64,
        nodes: m.g.node_count() as u64,
        fingerprint: fingerprint::of(&m.g, &e, &m.vs),
    }
}

/// `delta` new families on a synchronised model, one wave per family.
pub fn delta_add(families: usize, delta: usize) -> Outcome {
    let mut m = Model::new(families);
    let rules = rules(&mut m.g);
    let mut e = Engine::new(&rules);
    hints::engine(&mut e, m.all.len(), &hints::F2P);
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
    assert_eq!(e.cascade.len(), 4 * (families + delta) + 1);
    Outcome {
        usage,
        steps,
        applied: e.cascade.len() as u64,
        nodes: m.g.node_count() as u64,
        fingerprint: fingerprint::of(&m.g, &e, &m.vs),
    }
}

/// `delta` deletions of a member on a synchronised model, one wave per
/// deletion. The members are spread evenly over the model.
pub fn delta_delete(families: usize, delta: usize) -> Outcome {
    let mut m = Model::new(families);
    let rules = rules(&mut m.g);
    let mut e = Engine::new(&rules);
    hints::engine(&mut e, m.all.len(), &hints::F2P);
    e.elements_added(&m.g, &m.vs, &m.all);
    saturate(&mut e, &mut m.g, &m.vs);
    let stride = (m.members.len() / delta).max(1);
    let (steps, usage) = measure::section(|| {
        let mut steps = 0;
        for i in 0..delta {
            let member = m.members[(i * stride) % m.members.len()];
            m.g.set_node_status(&member, Status::Tombstone);
            e.admit_delta(&[DeltaDomain::Source]);
            e.element_removed(&member);
            e.element_deleted(&mut m.g, &member);
            steps += saturate(&mut e, &mut m.g, &m.vs);
            e.consolidate(&mut m.g);
        }
        steps
    });
    Outcome {
        usage,
        steps,
        applied: e.cascade.len() as u64,
        nodes: m.g.node_count() as u64,
        fingerprint: fingerprint::of(&m.g, &e, &m.vs),
    }
}
