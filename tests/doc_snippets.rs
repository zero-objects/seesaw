//! Uebersetzungspruefung der Doku-Beispiele.
//!
//! Die Beispiele in `docs/using.md` bauen aufeinander auf, wie in
//! Dokumentation ueblich. Sie werden deshalb in einem gemeinsamen
//! Kontext geprueft, nicht einzeln. Der Test fuehrt nichts aus; er
//! stellt sicher, dass niemand ein Beispiel schreibt, das nicht
//! uebersetzt.
#![allow(
    unused_imports,
    unused_variables,
    dead_code,
    unreachable_code,
    non_snake_case
)]

use seesaw_tgg::engine::{Engine, Termination};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::LoadError;

#[allow(unused_mut)]
fn doku_beispiele() {
    // Platzhalter fuer das, was die Beispiele aus ihrem Umfeld
    // voraussetzen (Regeltext, Graph, Bezeichner aus vorherigen
    // Beispielen).
    let source: &str = "";
    let MIN: &str = "";
    let rule_file: &str = "";
    let mut g = Graph::default();
    let mut g2 = Graph::default();
    let lowered: Vec<DirectedRule> = Vec::new();
    let mut vs = ValueStore::default();
    let mut vs2 = ValueStore::default();
    let jname = GhostId::from_raw([0; 32]);
    let mname = GhostId::from_raw([0; 32]);
    let an2 = GhostId::from_raw([0; 32]);
    let chain = seesaw_tgg::rules::transform::Chain::default();
    let target: &str = "";
    fn count_of_type(_g: &Graph, _typ: &str) -> usize {
        0
    }
    fn accept(_s: String) {}
    fn reject(_s: &str) {}

    // ── Beispiel 1 ──
    {
        use seesaw_tgg::graph::Graph;

        let mut g = Graph::default();
        let lowered = seesaw_tgg::rules::load(source, &mut g).expect("rule file loads");
    }
    // ── Beispiel 2 ──
    {
        use seesaw_tgg::rules::LoadError;

        match seesaw_tgg::rules::load(source, &mut g) {
            Ok(rules) => { /* … */ }
            Err(LoadError::Parse(e)) => eprintln!("not the JSON this format expects: {e}"),
            Err(LoadError::Validate(e)) => eprintln!("the file says something inconsistent: {e:?}"),
            Err(LoadError::Lower(e)) => eprintln!("consistent, but not lowerable: {e:?}"),
        }
    }
    // ── Beispiel 3 ──
    {
        assert_eq!(lowered.len(), 2, "one rule, two directions");
        assert_eq!(lowered[0].name, "R_Class→");
        assert_eq!(lowered[1].name, "R_Class←");
    }
    // ── Beispiel 4 ──
    {
        use seesaw_tgg::engine::{Engine, Termination};
        use seesaw_tgg::graph::{Graph, ValueStore};
        use seesaw_tgg::ident::Status;

        let mut g = Graph::default();
        let lowered = seesaw_tgg::rules::load(MIN, &mut g).expect("rule file loads");

        // Seed: Model -> Class -> name leaf.
        let model = g.add_baseline("m", "Model");
        let cls = g.add_baseline("m/Person", "Class");
        let cname = g.add_baseline("m/Person/name", "name");
        g.connect(model, cls, Status::Solid);
        g.connect(cls, cname, Status::Solid);

        let mut vs = ValueStore::default();
        vs.insert(cname, "Person");

        let mut engine = Engine::new(&lowered);
        let verdict = engine.run(&mut g, &vs, 1000);
        assert!(matches!(
            verdict,
            Termination::Convergence | Termination::Duplication
        ));
    }
    // ── Beispiel 5 ──
    {
        // Live nodes of one type.
        fn count_of_type(g: &Graph, typ: &str) -> usize {
            g.types
                .lookup(typ)
                .map(|t| {
                    g.nodes_of_type(t)
                        .filter(|n| n.status.is_matchable())
                        .count()
                })
                .unwrap_or(0)
        }

        assert_eq!(count_of_type(&g, "JavaClass"), 1);
        assert_eq!(count_of_type(&g, "CorrClass"), 1);
    }
    // ── Beispiel 6 ──
    {
        let jcls = {
            let t = g.types.lookup("JavaClass").expect("type exists");
            g.nodes_of_type(t)
                .find(|n| n.status.is_matchable())
                .expect("one JavaClass")
                .id
        };
        let jname = g
            .child_leaf_of_type(&jcls, "name")
            .expect("JavaClass has a name leaf");
        assert_eq!(g.resolve_value(&jname, &vs).as_deref(), Some("Person"));
    }
    // ── Beispiel 7 ──
    {
        let solid = g.materialize();
        assert_eq!(count_of_type(&solid, "JavaClass"), 1);
        assert_eq!(solid.resolve_value(&jname, &vs).as_deref(), Some("Person"));
    }
    // ── Beispiel 8 ──
    {
        let lowered = seesaw_tgg::rules::load(source, &mut g).expect("rule file loads");
        let names: Vec<&str> = lowered.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["R_Class→", "R_Class←", "R_Attr→", "R_Attr←"]);
    }
    // ── Beispiel 9 ──
    {
        let model = g.add_baseline("m", "Model");
        let cls = g.add_baseline("m/Person", "Class");
        let cname = g.add_baseline("m/Person/name", "name");
        let attr = g.add_baseline("m/Person/age", "Attribute");
        let aname = g.add_baseline("m/Person/age/name", "name");
        g.connect(model, cls, Status::Solid);
        g.connect(cls, cname, Status::Solid);
        g.connect(cls, attr, Status::Solid);
        g.connect(attr, aname, Status::Solid);

        let mut vs = ValueStore::default();
        vs.insert(cname, "Person");
        vs.insert(aname, "age");

        let mut engine = Engine::new(&lowered);
        let verdict = engine.run(&mut g, &vs, 10_000);
    }
    // ── Beispiel 10 ──
    {
        assert_eq!(count_of_type(&g, "JavaClass"), 1);
        assert_eq!(count_of_type(&g, "CorrClass"), 1);
        assert_eq!(count_of_type(&g, "JavaField"), 1);
        assert_eq!(count_of_type(&g, "CorrAttr"), 1);
        assert_eq!(count_of_type(&g, "Attribute"), 1, "no second Attribute");
        assert_eq!(count_of_type(&g, "Class"), 1, "no second Class");

        let field = {
            let t = g.types.lookup("JavaField").expect("type exists");
            g.nodes_of_type(t)
                .find(|n| n.status.is_matchable())
                .expect("one JavaField")
                .id
        };
        let fname = g.child_leaf_of_type(&field, "name").expect("name leaf");
        assert_eq!(g.resolve_value(&fname, &vs).as_deref(), Some("age"));
        let vis = g
            .child_leaf_of_type(&field, "visibility")
            .expect("visibility leaf");
        assert_eq!(g.resolve_value(&vis, &vs).as_deref(), Some("private"));
    }
    // ── Beispiel 11 ──
    {
        assert_eq!(g.resolve_value(&mname, &vs).as_deref(), Some("getAge"));
        // ... and on a graph seeded from the Java side:
        assert_eq!(g2.resolve_value(&an2, &vs2).as_deref(), Some("age"));
    }
    // ── Beispiel 12 ──
    {
        match chain.invert_checked(target) {
            Some(source) => accept(source),
            None => reject(target), // not producible by this rule
        }
    }
    // ── Beispiel 13 ──
    {
        use seesaw_tgg::engine::Engine;
        use seesaw_tgg::graph::{Graph, ValueStore};
        use seesaw_tgg::ident::Status;
        use seesaw_tgg::plan::DirectedRule;

        fn run(rule_file: &str) {
            let mut g = Graph::default();
            let rules = seesaw_tgg::rules::load(rule_file, &mut g).expect("rule file loads");

            // A source model: Model → Class → name leaf.
            let model = g.add_baseline("m", "Model");
            let cls = g.add_baseline("m/Person", "Class");
            let cname = g.add_baseline("m/Person/name", "name");
            g.connect(model, cls, Status::Solid);
            g.connect(cls, cname, Status::Solid);

            // Values live in the host, not in the graph.
            let mut values = ValueStore::default();
            values.insert(cname, "Person");

            let rules: &'static [DirectedRule] = Box::leak(rules.into_boxed_slice());
            Engine::new(rules).run(&mut g, &values, 1000);
            // The graph now holds a JavaClass, a CorrClass, and a name leaf
            // that resolves to "Person" without ever storing it twice.
        }
    }
}
