//! Export of lowered creation plans.
//!
//! Purpose: NOT the transport path for rules into production — the
//! new format is declarative, both languages lower themselves from
//! the file. This exporter is a pure verification artifact for
//! equivalence assurance: Rust is the reference implementation, the
//! Java side must be able to check its own lowering against the plans
//! Rust produces. Hence: stable order, no reordering, no omissions —
//! a difference between the languages must show up in the diff, not
//! get swallowed by a normalization step.
//!
//! Historical trigger (see task brief): `rules_uml_java.json` sat in
//! the tree for months without a producer, a Rust/Java divergence
//! went unnoticed. Reproducibility from the tree is therefore not a
//! side condition here, but the actual purpose of this module.
//!
//! The actual Rust/Java equivalence suite (Java lowers the same rule
//! file itself and compares against the export here) is NOT part of
//! this module — Java has no lowering of its own yet (see the plan
//! companion document `specs/2026-08-07-regelformat-v3.md`, section
//! plan does not cover": "Plan 3: Java loader and Java lowering plus
//! equivalence suite against the exporter from task 7"). What IS
//! checked HERE AND NOW (`tests::plans_fixture_is_up_to_date`): that
//! the export itself doesn't drift unnoticed, against a comparison
//! artifact kept in the tree. This closes the "producer without
//! artifact" gap for this module; the Rust/Java chain closes plan 3.

use crate::engine::matcher::{Link, LinkKind, PatternNode};
use crate::graph::{PlanTransform, TypeTable};
use crate::plan::{CreateNode, DirectedRule, Ref};
use crate::rules::predicate::Predicate;
use crate::rules::transform::Prim;
use serde_json::{json, Value};

/// Serializes lowered plans as a JSON string. `types` is the type
/// table of the graph that was lowered against — `PatternNode.typ` is
/// a [`crate::graph::TypeId`] that can only be resolved into a readable
/// name via this table (`CreateNode.typ` and `corr_recognition`
/// already carry plain-text names).
///
/// Two calls with the same arguments return the same string: every
/// structure is walked in its given order, never iterated via a
/// HashMap.
pub fn plans_to_json(rules: &[DirectedRule], types: &TypeTable) -> String {
    let rules_json: Vec<Value> = rules.iter().map(|r| rule_to_json(r, types)).collect();
    serde_json::to_string_pretty(&json!({ "rules": rules_json }))
        .expect("a Value made only of strings/numbers/bools cannot fail to serialize")
}

fn rule_to_json(r: &DirectedRule, types: &TypeTable) -> Value {
    let pattern_nodes: Vec<Value> = r
        .pattern
        .nodes
        .iter()
        .map(|n| pattern_node_to_json(n, types))
        .collect();
    let pattern_links: Vec<Value> = r.pattern.links.iter().map(link_to_json).collect();
    let create_nodes: Vec<Value> = r.create_nodes.iter().map(create_node_to_json).collect();
    let create_links: Vec<Value> = r
        .create_links
        .iter()
        .map(|(a, b)| json!([ref_to_json(a), ref_to_json(b)]))
        .collect();
    let corr_recognition: Vec<Value> = r
        .corr_recognition
        .iter()
        .map(|(corr_typ, anchor, endpoint_typ)| {
            json!({
                "corr_type": corr_typ,
                "anchor": anchor,
                "endpoint_type": endpoint_typ,
            })
        })
        .collect();
    json!({
        "name": r.name,
        "rank": r.rank,
        "pattern_nodes": pattern_nodes,
        "pattern_links": pattern_links,
        "create_nodes": create_nodes,
        "create_links": create_links,
        "input_types": r.input_types,
        "corr_recognition": corr_recognition,
    })
}

fn pattern_node_to_json(n: &PatternNode, types: &TypeTable) -> Value {
    json!({
        "typ": types.name(n.typ),
        "value": n.value.as_ref().map(predicate_to_json),
    })
}

fn link_to_json(l: &Link) -> Value {
    let kind = match l.kind {
        LinkKind::Directed => "directed",
        LinkKind::Context => "context",
        LinkKind::SameValue => "same_value",
    };
    json!({ "from": l.from, "to": l.to, "kind": kind })
}

fn predicate_to_json(p: &Predicate) -> Value {
    match p {
        Predicate::Exists => json!({ "kind": "exists" }),
        Predicate::Equals(v) => json!({ "kind": "equals", "value": v }),
        Predicate::Prefix(v) => json!({ "kind": "prefix", "value": v }),
        // `as_str()` returns the already full-match-framed pattern
        // (`\A(?:...)\z`, see `Predicate::parse_regex`) — exactly what
        // the Java side must match against, not the raw original.
        Predicate::Regex(rx) => json!({ "kind": "regex", "pattern": rx.as_str() }),
        Predicate::NumericRange { min, max } => {
            json!({ "kind": "numeric_range", "min": min, "max": max })
        }
    }
}

fn ref_to_json(r: &Ref) -> Value {
    match r {
        Ref::Matched(i) => json!({ "matched": i }),
        Ref::New(i) => json!({ "new": i }),
    }
}

/// Primitive chain of a [`PlanTransform`].
/// the two non-chain variants are handled.
fn transform_to_json(t: &PlanTransform) -> Value {
    match t {
        PlanTransform::Chain(c) => Value::Array(c.0.iter().map(prim_to_json).collect()),
    }
}

fn prim_to_json(p: &Prim) -> Value {
    match p {
        Prim::Identity => json!({ "op": "identity" }),
        Prim::Capitalize => json!({ "op": "capitalize" }),
        Prim::Decapitalize => json!({ "op": "decapitalize" }),
        Prim::Prefix(a) => json!({ "op": "prefix", "arg": a }),
        Prim::Suffix(a) => json!({ "op": "suffix", "arg": a }),
        Prim::StripPrefix(a) => json!({ "op": "strip_prefix", "arg": a }),
        Prim::StripSuffix(a) => json!({ "op": "strip_suffix", "arg": a }),
    }
}

fn create_node_to_json(cn: &CreateNode) -> Value {
    let derived = cn
        .derived
        .as_ref()
        .map(|(src, t)| json!({ "source": src, "transform": transform_to_json(t) }));
    let derived_dyn = cn
        .derived_dyn
        .as_ref()
        .map(|(anchor, attr, t)| {
            json!({ "anchor": anchor, "attr": attr, "transform": transform_to_json(t) })
        });
    json!({
        "typ": cn.typ,
        "parent": ref_to_json(&cn.parent),
        "derived": derived,
        "konst": cn.konst,
        "derived_dyn": derived_dyn,
        "corr_full_match": cn.corr_full_match,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::rules::format::RuleFile;
    use crate::rules::lower::lower_all;
    use crate::rules::validate::validate;

    const MIN: &str = include_str!("../../tests/fixtures/rules/uml_java_min.json");

    /// Absolute path to the comparison artifact, independent of the
    /// working directory `cargo test` is started from —
    /// `include_str!` resolves at compile time relative to this file,
    /// while `std::fs::write`/`read_to_string` need a path valid at
    /// runtime; `CARGO_MANIFEST_DIR` is the stable anchor for that.
    const PLANS_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rules/uml_java_min.plans.json"
    );

    fn compute_min_export() -> String {
        let f = RuleFile::from_json(MIN).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let rules = lower_all(&res, &mut g).unwrap();
        plans_to_json(&rules, &g.types)
    }

    #[test]
    fn export_is_deterministic_and_complete() {
        let f = RuleFile::from_json(MIN).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let rules = lower_all(&res, &mut g).unwrap();

        let a = plans_to_json(&rules, &g.types);
        let b = plans_to_json(&rules, &g.types);
        assert_eq!(a, b, "two runs, same bytes");

        let v: serde_json::Value = serde_json::from_str(&a).unwrap();
        let arr = v["rules"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "R_Class lowers in both directions");
        for r in arr {
            for key in [
                "name",
                "rank",
                "pattern_nodes",
                "pattern_links",
                "create_nodes",
                "create_links",
                "input_types",
                "corr_recognition",
            ] {
                assert!(r.get(key).is_some(), "{key} is missing from the export");
            }
            // `nacs` does not exist in this format (purely
            // positive, see the `rules` module doc) — DirectedRule doesn't
            // even carry a matching field that could show up here.
            assert!(r.get("nacs").is_none(), "nacs must not be exported");
        }
    }

    /// Confirms field by field that the export loses nothing the
    /// creation plan carries — beyond the plain key-presence check
    /// above. `R_Class` covers establishes+derived (leaf binding);
    /// `konst`/`derived_dyn` stay `null` here because the fixture
    /// doesn't use them (see `konst_and_derived_dyn_...` below for the
    /// positive case).
    #[test]
    fn export_forward_mirrors_the_lowered_plan_content() {
        let f = RuleFile::from_json(MIN).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let rules = lower_all(&res, &mut g).unwrap();
        let fwd = &rules[0];
        assert_eq!(fwd.name, "R_Class→");

        let s = plans_to_json(&rules, &g.types);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let r = &v["rules"][0];

        assert_eq!(r["name"], "R_Class→");
        assert_eq!(r["rank"], 40);

        // Pattern: Model, Class, name — type names resolved from the
        // TypeTable, not the internal TypeIds.
        let pn = r["pattern_nodes"].as_array().unwrap();
        assert_eq!(pn.len(), fwd.pattern.nodes.len());
        assert_eq!(pn[0]["typ"], "Model");
        assert_eq!(pn[1]["typ"], "Class");
        assert_eq!(pn[2]["typ"], "name");
        for n in pn {
            assert!(n["value"].is_null(), "the fixture sets no predicate");
        }

        let pl = r["pattern_links"].as_array().unwrap();
        assert_eq!(pl.len(), fwd.pattern.links.len());
        assert_eq!(pl[0]["from"], 0);
        assert_eq!(pl[0]["to"], 1);
        assert_eq!(pl[0]["kind"], "directed");

        // Creation plan: JavaClass (the corr's structural child) +
        // jname (leaf derived from cname, empty chain = []).
        let cn = r["create_nodes"].as_array().unwrap();
        assert_eq!(cn.len(), fwd.create_nodes.len());
        let jcls = cn.iter().find(|n| n["typ"] == "JavaClass").unwrap();
        assert_eq!(jcls["konst"], serde_json::Value::Null);
        assert_eq!(jcls["derived_dyn"], serde_json::Value::Null);
        assert_eq!(jcls["corr_full_match"], false);
        let jname = cn.iter().find(|n| n["typ"] == "name").unwrap();
        assert_eq!(jname["derived"]["source"], 2, "cname is at position 2");
        assert_eq!(
            jname["derived"]["transform"],
            serde_json::json!([]),
            "an empty chain in the binding stays empty"
        );

        assert_eq!(
            r["create_links"].as_array().unwrap().len(),
            fwd.create_links.len()
        );

        let cr = r["corr_recognition"].as_array().unwrap();
        assert_eq!(cr.len(), 1);
        assert_eq!(cr[0]["corr_type"], "CorrClass");
        assert_eq!(cr[0]["endpoint_type"], "JavaClass");

        assert_eq!(r["input_types"], serde_json::json!(fwd.input_types));
    }

    /// `konst` and `derived_dyn` are `CreateNode` fields that the
    /// `uml_java_min` fixture never triggers (no `constant`, no type
    /// binding). Without a dedicated positive test, it would stay
    /// unproven that the export writes them at all, rather than just
    /// happening to show `null` for lack of occurrence in `MIN`.
    #[test]
    fn konst_and_derived_dyn_appear_when_the_plan_carries_them() {
        let json = r#"{"format":3,"name":"k","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
            "right":{"anchor":"b","nodes":[
                {"name":"b","type":"B"},
                {"name":"k","type":"lit","constant":"x"}],
              "links":[["b","k"]]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes",
                "bindings":[{"left_type":"src","right_type":"dyn","transform":[]}]}]}]}"#;
        let f = RuleFile::from_json(json).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let rules = lower_all(&res, &mut g).unwrap();
        let s = plans_to_json(&rules, &g.types);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let fwd = &v["rules"][0];

        let cn = fwd["create_nodes"].as_array().unwrap();
        let lit = cn.iter().find(|n| n["typ"] == "lit").unwrap();
        assert_eq!(lit["konst"], "x");
        assert_eq!(lit["derived"], serde_json::Value::Null);

        let dynn = cn.iter().find(|n| n["typ"] == "dyn").unwrap();
        assert_eq!(dynn["derived_dyn"]["anchor"], 0);
        assert_eq!(dynn["derived_dyn"]["attr"], "src");
        assert_eq!(dynn["derived_dyn"]["transform"], serde_json::json!([]));
    }

    /// Finding 1 (fix round 1): `link_to_json` distinguishes three
    /// kinds, but until now no test checked `"context"` or
    /// `"same_value"` — only `"directed"` was covered. A rule with a
    /// `same_value_links` constraint on the input side AND a
    /// `references` corr forces all three at once in the forward
    /// pattern: `directed` from the ordinary input links, `same_value`
    /// from `same_value_links`, `context` from the reference corr
    /// (`rules::lower::lower_directed`: one context link each
    /// corr→anchor and corr→context-endpoint).
    #[test]
    fn all_three_link_kinds_appear_in_the_export() {
        let json = r#"{"format":3,"name":"link_kinds","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},
                {"name":"n1","type":"name"},
                {"name":"n2","type":"name"}],
              "links":[["a","n1"],["a","n2"]],
              "same_value_links":[["n1","n2"]]},
            "right":{"anchor":"b","nodes":[
                {"name":"b","type":"B"},
                {"name":"refd","type":"Other"}],
              "links":[]},
            "corrs":[
                {"type":"CEst","left":"a","right":"b","role":"establishes"},
                {"type":"CRef","left":"a","right":"refd","role":"references"}]}]}"#;
        let f = RuleFile::from_json(json).unwrap();
        let res = validate(&f).unwrap();
        let mut g = Graph::default();
        let rules = lower_all(&res, &mut g).unwrap();
        assert_eq!(rules[0].name, "R→");

        let s = plans_to_json(&rules, &g.types);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();

        // Collected over BOTH directions, so the test doesn't depend
        // on which direction carries which combination — it should
        // only prove that the export writes all three kinds
        // distinguishably.
        let mut kinds = std::collections::BTreeSet::new();
        for rule in v["rules"].as_array().unwrap() {
            for l in rule["pattern_links"].as_array().unwrap() {
                kinds.insert(l["kind"].as_str().unwrap().to_string());
            }
        }
        assert_eq!(
            kinds,
            ["context", "directed", "same_value"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            "all three link kinds must appear distinguishably in the export"
        );
    }

    /// Finding 2 (fix round 1): `plans_to_json` had no caller besides
    /// its own tests — a producer without an artifact, the mirror
    /// image of the original finding (artifact without a producer).
    /// This test WRITES the comparison artifact; it doesn't run
    /// automatically (`--ignored`), because it modifies a file in the
    /// tree. Counterpart: `plans_fixture_is_up_to_date` below, which
    /// runs without `--ignored` and goes red as soon as the export
    /// diverges from the checked-in state.
    #[test]
    #[ignore = "manual: writes/updates tests/fixtures/rules/uml_java_min.plans.json"]
    fn write_plans_fixture() {
        std::fs::write(PLANS_FIXTURE, compute_min_export()).expect("fixture write attempt");
        eprintln!("plans fixture written to {PLANS_FIXTURE}");
    }

    /// Runs on every `cargo test`, without `--ignored`: reads the
    /// checked-in artifact and compares it byte-for-byte against a
    /// freshly computed export from `uml_java_min.json`. If the export
    /// diverges from the checked-in state (exporter changed, fixture
    /// not regenerated — or the reverse, hand-edited), this test goes
    /// red. This is the chain that was previously missing: the
    /// artifact can no longer go stale without being noticed.
    #[test]
    fn plans_fixture_is_up_to_date() {
        let on_disk = std::fs::read_to_string(PLANS_FIXTURE).unwrap_or_else(|e| {
            panic!(
                "fixture {PLANS_FIXTURE} is missing or unreadable ({e}) — generate it with \
                 `cargo test -p seesaw-core --lib rules::export::tests::write_plans_fixture -- --ignored`"
            )
        });
        let fresh = compute_min_export();
        assert_eq!(
            on_disk, fresh,
            "tests/fixtures/rules/uml_java_min.plans.json is stale relative to the export — \
             regenerate with `cargo test -p seesaw-core --lib rules::export::tests::write_plans_fixture -- --ignored`"
        );
    }
}
