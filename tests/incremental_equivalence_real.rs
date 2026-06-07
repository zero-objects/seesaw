//! Differential `run_cascade_full` == `run_cascade_cached` over the REAL
//! paper rule sets (not just the demo rules).
//!
//! Motivation: the repro tests drive only the full/observable path — the
//! cached path (touched by perf C and the SetAttr levers) is otherwise
//! covered ONLY by `proptest_invariants` with simple synthetic rules. These
//! real rule sets have NACs (fase2019) and attribute conditions (lmcs2024)
//! — exactly the cases where a SetAttr can enable/disable a match. This test
//! is therefore the oracle for the correctness of attribute-/NAC-sensitive
//! rules on the cached path.

use seesaw_tgg::engine::{run_cascade_cached, run_cascade_full, Cascade, Rule};
use seesaw_tgg::graph::{GhostId, TypedGraph};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};

// Shared fixtures expose several builders; this differential uses one each,
// so silence dead-code for the builders the other tests use.
#[allow(dead_code)]
#[path = "fixtures/class_doc_mm.rs"]
mod class_doc_mm;
#[allow(dead_code)]
#[path = "fixtures/house_plan_mm.rs"]
mod house_plan_mm;
#[allow(dead_code)]
#[path = "fixtures/java_javadoc_mm.rs"]
mod java_javadoc_mm;
#[allow(dead_code)]
#[path = "fixtures/package_folder_mm.rs"]
mod package_folder_mm;
#[allow(dead_code)]
#[path = "fixtures/sysml_machine_mm.rs"]
mod sysml_machine_mm;

fn rules(fixture: &str) -> Vec<Box<dyn Rule>> {
    parse_ruleset(fixture)
        .expect("ruleset parses")
        .rules
        .iter()
        .map(|r| instantiate(&compile(r).expect("compile")))
        .collect()
}

fn node_fingerprint(g: &TypedGraph) -> Vec<(GhostId, String, u8)> {
    let mut v: Vec<(GhostId, String, u8)> = g
        .iter_nodes()
        .map(|n| (n.id, n.type_id.clone(), n.status as u8))
        .collect();
    v.sort();
    v
}

/// Forward initial sync on `seed` once full, once cached — the delta
/// sequence (origin/rank/op_star/bindings) and the final graph must be
/// bit-identical.
fn assert_equiv(name: &str, owned: Vec<Box<dyn Rule>>, seed: TypedGraph) {
    let refs: Vec<&dyn Rule> = owned.iter().map(|b| b.as_ref()).collect();

    let mut g_full = seed.clone();
    let mut c_full = Cascade::new();
    let t_full = run_cascade_full(&mut c_full, &mut g_full, &refs, 1_000_000).expect("full");

    let mut g_cached = seed;
    let mut c_cached = Cascade::new();
    let t_cached =
        run_cascade_cached(&mut c_cached, &mut g_cached, &refs, 1_000_000).expect("cached");

    assert_eq!(t_full, t_cached, "[{name}] termination differs");
    assert_eq!(
        c_full.entries.len(),
        c_cached.entries.len(),
        "[{name}] step count differs: full={} cached={}",
        c_full.entries.len(),
        c_cached.entries.len()
    );
    for (i, (ef, ei)) in c_full
        .entries
        .iter()
        .zip(c_cached.entries.iter())
        .enumerate()
    {
        assert_eq!(ef.origin, ei.origin, "[{name}] origin @entry {i}");
        assert_eq!(ef.rank, ei.rank, "[{name}] rank @entry {i}");
        assert_eq!(ef.op_star, ei.op_star, "[{name}] op_star @entry {i}");
        assert_eq!(ef.bindings, ei.bindings, "[{name}] bindings @entry {i}");
    }
    assert_eq!(
        node_fingerprint(&g_full),
        node_fingerprint(&g_cached),
        "[{name}] final graph differs"
    );
}

#[test]
fn real_lmcs2024_attr_condition() {
    assert_equiv(
        "lmcs2024",
        rules(include_str!("fixtures/rules_lmcs2024_terrace.json")),
        house_plan_mm::build_fig3a_graph().0,
    );
}

#[test]
fn real_fase2019_nac() {
    assert_equiv(
        "fase2019",
        rules(include_str!("fixtures/rules_fase2019_3rule.json")),
        package_folder_mm::build_fig3a_graph().0,
    );
}

#[test]
fn real_sle2020() {
    assert_equiv(
        "sle2020",
        rules(include_str!("fixtures/rules_sle2020_class_doc.json")),
        class_doc_mm::build_pre_graph().0,
    );
}

#[test]
fn real_jot2022() {
    assert_equiv(
        "jot2022",
        rules(include_str!("fixtures/rules_jot2022_sysml.json")),
        sysml_machine_mm::build_dangling_graph().0,
    );
}

#[test]
fn real_faoc2021() {
    assert_equiv(
        "faoc2021",
        rules(include_str!("fixtures/rules_faoc2021_schema.json")),
        java_javadoc_mm::build_doc_two_entries().0,
    );
}
