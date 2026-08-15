//! Ende-zu-Ende: Regeldatei laden, lowern, Kaskade bis Terminal fahren.
//!
//! Zeigt die volle Kette Datei -> Validierung -> Erzeugungsplaene -> Engine
//! anhand der Fixture `uml_java_min.json` (Regel `R_Class`, Task 5). Der
//! Testkoerper folgt der Skizze aus Task-6-Brief; `validate`/`lower_all`
//! sind an die tatsaechlichen Signaturen aus Task 5 angepasst -- die
//! Interning-Tabelle fuer Transformations-Ketten gehoert zum
//! `Resolved`-Ergebnis, nicht mehr einem separaten Parameter (siehe
//! `rules::validate::Resolved::chains`).

use std::collections::BTreeSet;

use seesaw_tgg::engine::{DeltaDomain, Engine, Termination};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::format::RuleFile;
use seesaw_tgg::rules::lower::lower_all;
use seesaw_tgg::rules::validate::validate;

const MIN: &str = include_str!("fixtures/rules/uml_java_min.json");

/// Lebende Knoten eines Typs. `Graph::types` ist oeffentlich, siehe
/// tests/case01_least_changing_move_v2.rs:206.
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

/// Der einzige lebende Knoten eines Typs. Schlaegt fehl, wenn keiner
/// oder mehr als einer existiert.
fn only_of_type(g: &Graph, typ: &str) -> GhostId {
    let t = g.types.lookup(typ).expect("Typ muss existieren");
    let mut it = g.nodes_of_type(t).filter(|n| n.status.is_matchable());
    let id = it.next().unwrap_or_else(|| panic!("kein {typ}-Knoten")).id;
    assert!(it.next().is_none(), "mehr als ein {typ}-Knoten");
    id
}

#[test]
fn regeldatei_treibt_eine_kaskade() {
    let file = RuleFile::from_json(MIN).expect("parst");
    let resolved = validate(&file).expect("validiert");

    let mut g = Graph::default();
    let lowered = lower_all(&resolved, &mut g).expect("lowert");
    assert_eq!(lowered.len(), 2, "eine Regel, zwei Richtungen");

    // Seed: Model -> Class -> name-Blatt.
    let model = g.add_baseline("m", "Model");
    let cls = g.add_baseline("m/Person", "Class");
    let cname = g.add_baseline("m/Person/name", "name");
    g.connect(model, cls, Status::Solid);
    g.connect(cls, cname, Status::Solid);

    let mut vs = ValueStore::default();
    vs.insert(cname, "Person");

    let rules: &'static [DirectedRule] = Box::leak(lowered.into_boxed_slice());

    let mut engine = Engine::new(rules);
    let verdict = engine.run(&mut g, &vs, 1000);
    assert!(
        matches!(verdict, Termination::Convergence | Termination::Duplication),
        "Kaskade muss regulaer terminieren, war {verdict:?}"
    );

    assert_eq!(count_of_type(&g, "JavaClass"), 1, "genau eine Java-Klasse");
    assert_eq!(
        count_of_type(&g, "CorrClass"),
        1,
        "genau eine Korrespondenz"
    );

    // Abgeleitetes Namensblatt loest auf den erwarteten Wert auf
    // (Binding cname -> jname, transform: [] = Identitaet).
    let jcls = only_of_type(&g, "JavaClass");
    let jname = g
        .child_leaf_of_type(&jcls, "name")
        .expect("JavaClass hat ein name-Blatt");
    assert_eq!(
        g.resolve_value(&jname, &vs).as_deref(),
        Some("Person"),
        "Namensblatt muss auf den Quellwert aufloesen"
    );

    // Idempotenz: ein zweiter Lauf, mit FRISCHEM Engine-Zustand (leeres
    // Duplikat-Gedaechtnis), auf demselben Graphen darf nichts Neues
    // erzeugen -- die Konvergenz muss aus der Graph-Identitaet kommen,
    // nicht nur aus dem Anwendungs-Gedaechtnis der ersten Instanz.
    let mut engine2 = Engine::new(rules);
    let verdict2 = engine2.run(&mut g, &vs, 1000);
    assert!(
        matches!(
            verdict2,
            Termination::Convergence | Termination::Duplication
        ),
        "zweiter Lauf muss regulaer terminieren, war {verdict2:?}"
    );
    assert_eq!(
        count_of_type(&g, "JavaClass"),
        1,
        "zweiter Lauf erzeugt keine weitere Java-Klasse"
    );
    assert_eq!(
        count_of_type(&g, "CorrClass"),
        1,
        "zweiter Lauf erzeugt keine weitere Korrespondenz"
    );
}

/// Kanonischer Endzustand der Kaskade, fuer den Sprachvergleich.
///
/// Statt eines Hashs die Sache selbst: lebende Knoten mit Typ, Wert
/// und ausgehenden Verbindungen, nach Id sortiert. Ein Unterschied
/// zwischen den Sprachen ist damit lesbar statt nur ungleich.
#[cfg(test)]
const CASCADE_FIXTURE: &str =
    "../../seesaw-java/src/test/resources/fixtures/uml_java_min.cascade.json";

fn cascade_state(g: &Graph, vs: &ValueStore) -> serde_json::Value {
    let mut nodes: Vec<serde_json::Value> = g
        .nodes()
        .filter(|n| n.status.is_matchable())
        .map(|n| {
            let mut outs: Vec<String> = g
                .parts(&n.id)
                .filter(|p| p.outgoing)
                .map(|p| p.other.hex())
                .collect();
            outs.sort();
            serde_json::json!({
                "id": n.id.hex(),
                "typ": g.types.name(n.typ),
                "value": g.resolve_value(&n.id, vs),
                "out": outs,
            })
        })
        .collect();
    nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    serde_json::json!({ "alive": nodes.len(), "nodes": nodes })
}

/// Vollständige operationelle Folge, nicht nur Anzahl oder Endzustand.
/// Damit wird eine Abweichung der Kandidatenauswahl zwischen Rust und
/// Java unmittelbar sichtbar.
fn cascade_trace(engine: &Engine<'_>) -> serde_json::Value {
    serde_json::Value::Array(
        engine
            .cascade
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "rule": engine.rules[entry.rule_ix].name,
                    "rank": entry.rank,
                    "refs": entry.refs.iter().map(GhostId::hex).collect::<Vec<_>>(),
                    "created": entry.created.iter().map(GhostId::hex).collect::<Vec<_>>(),
                    "created_edges": entry
                        .created_edges
                        .iter()
                        .map(GhostId::hex)
                        .collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Faehrt die Fixture und liefert den Endzustand.
fn run_min_cascade() -> serde_json::Value {
    let file = RuleFile::from_json(MIN).expect("parst");
    let mut g = Graph::default();
    let lowered = seesaw_tgg::rules::load_file(&file, &mut g).expect("laedt");
    let model = g.add_baseline("m", "Model");
    let cls = g.add_baseline("m/Person", "Class");
    let cname = g.add_baseline("m/Person/name", "name");
    g.connect(model, cls, Status::Solid);
    g.connect(cls, cname, Status::Solid);
    let mut vs = ValueStore::default();
    vs.insert(cname, "Person");
    let rules: &'static [DirectedRule] = Box::leak(lowered.into_boxed_slice());
    let mut engine = Engine::new(rules);
    engine.admit_delta(&[DeltaDomain::Source]);
    engine.run(&mut g, &vs, 1000);
    let mut state = cascade_state(&g, &vs);
    state["cascade"] = cascade_trace(&engine);
    state
}

#[test]
#[ignore = "manuell: schreibt den Kaskaden-Endzustand fuer den Sprachvergleich"]
fn schreibt_kaskaden_endzustand() {
    let s = serde_json::to_string_pretty(&run_min_cascade()).expect("serialisiert");
    std::fs::write(CASCADE_FIXTURE, s).expect("Schreibversuch");
    eprintln!("Kaskaden-Endzustand nach {CASCADE_FIXTURE} geschrieben");
}

/// Die Datei im Java-Baum deckt sich mit einem frischen Lauf.
#[test]
fn kaskaden_endzustand_ist_aktuell() {
    let Ok(on_disk) = std::fs::read_to_string(CASCADE_FIXTURE) else {
        eprintln!("SKIPPED: {CASCADE_FIXTURE} fehlt");
        return;
    };
    let on_disk: serde_json::Value = serde_json::from_str(&on_disk).expect("parst");
    assert_eq!(
        on_disk,
        run_min_cascade(),
        "der Kaskaden-Endzustand im Java-Baum ist veraltet -- neu schreiben mit \
         `cargo test -p seesaw-core --test format -- --ignored`"
    );
}

/// Ein materialisiertes Erzeugnis wird zurückgezogen, ein
/// Baseline-Element nicht.
///
/// Beide Hälften zählen. Die erste ist der Fix vom 2026-08-10: bis
/// dahin prüfte `retract_match` auf `Status::Ghost`, also blieb ein
/// gefaltetes Erzeugnis (`Solid`) stehen und dasselbe Delta setzte
/// keinen Tombstone. Die zweite hält fest, dass der Fix nicht zu viel
/// zurückzieht: was der Host selbst angelegt hat, steht in keiner
/// Produktmenge und bleibt unberührt. Genau das ist die Aussage, die
/// die Kaskaden-Isolation schützen soll.
///
/// Knoten UND Kanten, weil der Fix beide betrifft und Kanten vorher
/// von niemandem gemessen wurden.
#[test]
fn materialisiertes_erzeugnis_wird_zurueckgezogen() {
    for materialisieren in [false, true] {
        let file = RuleFile::from_json(MIN).expect("parst");
        let mut g = Graph::default();
        let lowered = seesaw_tgg::rules::load_file(&file, &mut g).expect("laedt");
        let model = g.add_baseline("m", "Model");
        let cls = g.add_baseline("m/Person", "Class");
        let cname = g.add_baseline("m/Person/name", "name");
        g.connect(model, cls, Status::Solid);
        g.connect(cls, cname, Status::Solid);
        let mut vs = ValueStore::default();
        vs.insert(cname, "Person");
        let rules: &'static [DirectedRule] = Box::leak(lowered.into_boxed_slice());
        let mut e = Engine::new(rules);
        e.run(&mut g, &vs, 1000);

        // Das Erzeugnis der Kaskade und die Kante, die zu ihm führt.
        let jt = g.types.lookup("JavaClass").expect("JavaClass existiert");
        let jcls = g.nodes_of_type(jt).next().expect("eine JavaClass").id;
        let corr_t = g.types.lookup("CorrClass").expect("CorrClass existiert");
        let corr = g.nodes_of_type(corr_t).next().expect("eine Corr").id;
        let kante = g
            .parts(&corr)
            .find(|p| p.outgoing && p.other == jcls)
            .expect("Corr zeigt auf die JavaClass")
            .connection;

        if materialisieren {
            g = g.materialize();
            assert_eq!(
                g.node(&jcls).expect("ueberlebt").status,
                Status::Solid,
                "die Materialisierung macht das Erzeugnis solid"
            );
            assert_eq!(
                g.connection(&kante).expect("ueberlebt").status,
                Status::Solid,
                "die Materialisierung macht auch die Kante solid"
            );
        }

        // Delta: die Quelle fällt weg.
        g.set_node_status(&cls, Status::Tombstone);
        e.element_removed(&cls);
        e.element_deleted(&mut g, &cls);
        e.consolidate(&mut g);

        assert_eq!(
            g.node(&jcls).map(|n| n.status),
            Some(Status::Tombstone),
            "materialisiert={materialisieren}: das Erzeugnis muss zurückgezogen werden"
        );
        assert_eq!(
            g.connection(&kante).map(|c| c.status),
            Some(Status::Tombstone),
            "materialisiert={materialisieren}: die erzeugte Kante muss mitfallen"
        );

        // Die Gegenrichtung: Baseline-Elemente stehen in keiner
        // Produktmenge und bleiben unberührt.
        assert_eq!(
            g.node(&model).map(|n| n.status),
            Some(Status::Solid),
            "materialisiert={materialisieren}: der Baseline-Anker bleibt unberührt"
        );
        assert_eq!(
            g.node(&cname).map(|n| n.status),
            Some(Status::Solid),
            "materialisiert={materialisieren}: das Baseline-Blatt bleibt unberührt"
        );
    }
}
/// Erzeuger des Java-Golden für die Identitäts-Ableitung.
///
/// Die sechs Ableitungen mit festen Eingaben. Bis 2026-08-10 standen
/// diese Werte nur hartkodiert im Java-Test, mit dem Vermerk, sie
/// stammten aus dem Rust-Pfad — ohne Erzeuger, der das belegt. Genau
/// dieser Mangel schlug beim Ändern der Kodierung zu.
#[test]
#[ignore = "manuell: schreibt das Java-Golden der Identitaets-Ableitung"]
fn schreibt_ident_golden() {
    let path = "../../seesaw-java/src/test/resources/fixtures/ident_golden.json";
    let cls = seesaw_tgg::graph::preview_baseline_id("uml:/Person");
    let name = seesaw_tgg::graph::preview_baseline_id("uml:/Person/name");
    let getter =
        seesaw_tgg::graph::PlanTransform::Chain(seesaw_tgg::rules::transform::Chain(vec![
            seesaw_tgg::rules::transform::Prim::Capitalize,
            seesaw_tgg::rules::transform::Prim::Prefix("get".into()),
        ]));
    let j = serde_json::json!({
        "baseline_class": cls.hex(),
        "baseline_name": name.hex(),
        "ghost": seesaw_tgg::graph::preview_ghost_id(&cls, "Member").hex(),
        "derived": seesaw_tgg::graph::preview_derived_id(&cls, "getterName", &name, &getter).hex(),
        "connection": seesaw_tgg::graph::preview_connection_id(&cls, &name).hex(),
        "corr": seesaw_tgg::graph::preview_corr_id(&cls, "Corr", &[cls, name]).hex(),
        "konst": seesaw_tgg::graph::preview_konst_id(&cls, "Op", "mkOp", 3).hex(),
    });
    std::fs::write(
        path,
        serde_json::to_string_pretty(&j).expect("serialisiert"),
    )
    .expect("Schreibversuch");
    eprintln!("Ident-Golden nach {path} geschrieben");
}

/// **Löschen entlang der Korrespondenz: der Host löscht das Erzeugnis.**
///
/// Die Gegenrichtung zu `materialisiertes_erzeugnis_wird_zurueckgezogen`.
/// Dort fällt die Quelle und das Erzeugnis muss mitfallen; hier fällt das
/// Erzeugnis und die Quelle muss mitfallen.
///
/// Entscheidung Sandra 2026-08-11: wer die erzeugte Java-Klasse löscht,
/// löscht die UML-Klasse. Eine Korrespondenz, die eine Übersetzung
/// bezeugt, deren Ergebnis nicht mehr existiert, ist kein zulässiger
/// Ruhezustand. Wer das Erzeugnis behalten will, nimmt die Löschung
/// zurück (Undo), statt dass der Sync einen Sonderfall kennt.
#[test]
fn geloeschtes_erzeugnis_reisst_die_quelle_mit() {
    for materialisieren in [false, true] {
        let file = RuleFile::from_json(MIN).expect("parst");
        let mut g = Graph::default();
        let lowered = seesaw_tgg::rules::load_file(&file, &mut g).expect("laedt");
        let model = g.add_baseline("m", "Model");
        let cls = g.add_baseline("m/Person", "Class");
        let cname = g.add_baseline("m/Person/name", "name");
        g.connect(model, cls, Status::Solid);
        g.connect(cls, cname, Status::Solid);
        let mut vs = ValueStore::default();
        vs.insert(cname, "Person");
        let rules: &'static [DirectedRule] = Box::leak(lowered.into_boxed_slice());
        let mut e = Engine::new(rules);
        e.run(&mut g, &vs, 1000);

        let jt = g.types.lookup("JavaClass").expect("JavaClass existiert");
        let jcls = g.nodes_of_type(jt).next().expect("eine JavaClass").id;
        let corr_t = g.types.lookup("CorrClass").expect("CorrClass existiert");
        let corr = g.nodes_of_type(corr_t).next().expect("eine Corr").id;

        if materialisieren {
            g = g.materialize();
        }
        let fall = format!("materialisiert={materialisieren}");

        // Delta: der Host löscht das ERZEUGNIS.
        g.set_node_status(&jcls, Status::Tombstone);
        e.element_removed(&jcls);
        e.element_deleted(&mut g, &jcls);
        e.consolidate(&mut g);

        assert_eq!(
            g.node(&corr).map(|n| n.status),
            Some(Status::Tombstone),
            "{fall}: die Korrespondenz darf keine Uebersetzung mehr bezeugen"
        );
        assert_eq!(
            g.node(&cls).map(|n| n.status),
            Some(Status::Tombstone),
            "{fall}: die Quelle faellt entlang der Korrespondenz mit"
        );
        // Und die Abgrenzung: was an keiner gefallenen Korrespondenz
        // haengt, bleibt. Der Baseline-Anker traegt keine.
        assert_eq!(
            g.node(&model).map(|n| n.status),
            Some(Status::Solid),
            "{fall}: der Baseline-Anker bleibt unberuehrt"
        );
    }
}

/// **Der Zweck der ganzen Reihe, als Test:** der Host löscht ein
/// erzeugtes Blatt, und die Löschung trägt auf die Quellseite.
///
/// Das erzeugte Namensblatt der Java-Klasse ist kein Endpunkt der
/// Klassen-Korrespondenz. Es steht in einer EIGENEN Korrespondenz, der
/// Attribut-Korrespondenz zwischen den beiden Namensblättern (Sandra
/// 2026-08-12). Über die trägt die Löschung.
#[test]
fn geloeschtes_erzeugtes_blatt_traegt_zur_quelle() {
    let file = RuleFile::from_json(MIN).expect("parst");
    let mut g = Graph::default();
    let lowered = seesaw_tgg::rules::load_file(&file, &mut g).expect("laedt");
    let model = g.add_baseline("m", "Model");
    let cls = g.add_baseline("m/Person", "Class");
    let cname = g.add_baseline("m/Person/name", "name");
    g.connect(model, cls, Status::Solid);
    g.connect(cls, cname, Status::Solid);
    let mut vs = ValueStore::default();
    vs.insert(cname, "Person");
    let rules: &'static [DirectedRule] = Box::leak(lowered.into_boxed_slice());
    let mut e = Engine::new(rules);
    e.run(&mut g, &vs, 1000);

    let jt = g.types.lookup("JavaClass").expect("JavaClass existiert");
    let jcls = g.nodes_of_type(jt).next().expect("eine JavaClass").id;
    let jname = g
        .child_leaf_of_type(&jcls, "name")
        .expect("die JavaClass hat ein erzeugtes Namensblatt");
    let at = g
        .types
        .lookup("CorrClass_name")
        .expect("die Attribut-Korrespondenz existiert");
    let acorr = g
        .nodes_of_type(at)
        .next()
        .expect("eine Attribut-Korrespondenz")
        .id;

    g.set_node_status(&jname, Status::Tombstone);
    e.element_removed(&jname);
    e.element_deleted(&mut g, &jname);
    e.consolidate(&mut g);

    assert_eq!(
        g.node(&acorr).map(|n| n.status),
        Some(Status::Tombstone),
        "die Attribut-Korrespondenz darf kein fehlendes Blatt bezeugen"
    );
    assert_eq!(
        g.node(&cname).map(|n| n.status),
        Some(Status::Tombstone),
        "das Quellblatt faellt entlang der Attribut-Korrespondenz mit"
    );
    assert_eq!(
        g.node(&model).map(|n| n.status),
        Some(Status::Solid),
        "der Baseline-Anker bleibt unberuehrt"
    );
}

const DYN: &str = include_str!("fixtures/rules/uml_java_dyn.json");

/// **Der dynamische Fall: `left_type`/`right_type` statt Musterposition.**
///
/// Dieselbe Aussage wie im statischen Fall, nur wird die Quelle erst
/// beim Anwenden über `child_leaf_of_type` gefunden. Die
/// Attribut-Korrespondenz entsteht deshalb in `apply_creation` statt im
/// Lowering — anderer Ort, gleiche Sache. Ohne diesen Test wäre der
/// Zweig unbelegt: dynamische Constraints kommen sonst nur in den
/// dry-cleaner-Regelsätzen vor, deren Roundtrip `#[ignore]` trägt.
#[test]
fn dynamische_bindung_erzeugt_blatt_korrespondenz() {
    let file = RuleFile::from_json(DYN).expect("parst");
    let mut g = Graph::default();
    let lowered = seesaw_tgg::rules::load_file(&file, &mut g).expect("laedt");
    let model = g.add_baseline("m", "Model");
    let cls = g.add_baseline("m/Person", "Class");
    let cname = g.add_baseline("m/Person/name", "name");
    g.connect(model, cls, Status::Solid);
    g.connect(cls, cname, Status::Solid);
    let mut vs = ValueStore::default();
    vs.insert(cname, "Person");
    let rules: &'static [DirectedRule] = Box::leak(lowered.into_boxed_slice());
    let mut e = Engine::new(rules);
    e.run(&mut g, &vs, 1000);

    let jt = g.types.lookup("JavaClass").expect("JavaClass existiert");
    let jcls = g.nodes_of_type(jt).next().expect("eine JavaClass").id;
    let jname = g
        .child_leaf_of_type(&jcls, "name")
        .expect("die JavaClass hat ein erzeugtes Namensblatt");
    assert_eq!(
        g.resolve_value(&jname, &vs).as_deref(),
        Some("Person"),
        "das dynamisch gebundene Blatt traegt den Quellwert"
    );

    // Die Blatt-Korrespondenz existiert und haengt an BEIDEN Blättern.
    let at = g
        .types
        .lookup("CorrClass_name")
        .expect("die Attribut-Korrespondenz des dynamischen Falls");
    let acorr = g
        .nodes_of_type(at)
        .next()
        .expect("eine Attribut-Korrespondenz")
        .id;
    let enden: BTreeSet<GhostId> = g.parts(&acorr).map(|p| p.other).collect();
    assert!(
        enden.contains(&cname) && enden.contains(&jname),
        "die Attribut-Korrespondenz haelt Quell- und Zielblatt, hat aber {enden:?}"
    );

    // Und sie trägt: das erzeugte Blatt löschen holt das Quellblatt mit.
    g.set_node_status(&jname, Status::Tombstone);
    e.element_removed(&jname);
    e.element_deleted(&mut g, &jname);
    e.consolidate(&mut g);
    assert_eq!(
        g.node(&cname).map(|n| n.status),
        Some(Status::Tombstone),
        "das Quellblatt faellt entlang der dynamischen Attribut-Korrespondenz"
    );
    assert_eq!(
        g.node(&model).map(|n| n.status),
        Some(Status::Solid),
        "der Baseline-Anker bleibt unberuehrt"
    );
}
