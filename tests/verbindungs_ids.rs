//! Verbindungs-Ids an den Tueren der Engine, gemessen an 2.0.2.
//!
//! 2.0.2 legte jede erzeugte Verbindung in `by_product` ab und jeden
//! Match auch unter der Verbindung jedes Links seines Musters in
//! `by_element`. Die Engine legt dort seit der Auswahl der Optionen nur
//! Knoten ab und findet die Verbindungen aus ihren Enden, fuer eine Id,
//! die der Graph nicht kennt, aus eigenen Indizes. Diese Tests fahren
//! ueber die oeffentliche API die Ablaeufe, in denen eine Tuer die Id
//! einer Verbindung bekommt. Die erwarteten Werte sind die eines Laufs
//! dieser Datei gegen den Tag v2.0.2.
//!
//! Die Ablaeufe folgen den Java-Tests in `ErzeugerTest` und `LinksTest`.
//! Matches und `verdict` sind in Rust privat. Beobachtet wird deshalb,
//! was im Graphen steht, und was ein neuer Seed mit Saettigung meldet.

use std::collections::BTreeSet;

use seesaw_tgg::engine::{DeltaDomain, Engine, Termination};
use seesaw_tgg::graph::{preview_connection_id, Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;

/// `Root` setzt ein `A` in Korrespondenz zu einem `B`, `Child` ein `X`
/// unter dem `A` zu einem `Y` unter dem `B` und liest die Korrespondenz
/// von `Root` als Kontext. Das Muster von `Child` nennt den Link von
/// dieser Korrespondenz zum `A`, der Graph haelt die Verbindung
/// andersherum.
const CONTEXT_LINK: &str = r#"{
  "format": 3,
  "name": "context_link",
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
}"#;

/// Die Regeln von [`CONTEXT_LINK`] und `Pair`: zwei `X` mit einer
/// Verbindung, deren `Y` ueber ihre Korrespondenzen gematcht werden.
/// Die Verbindung zwischen den beiden `Y` erzeugt `Pair` zwischen zwei
/// gematchten Knoten.
const PAIR: &str = r#"{
  "format": 3,
  "name": "context_link_pair",
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
    },
    {
      "name": "Pair", "rank": 80,
      "left": {"anchor": "x1", "nodes": [
        {"name": "x1", "type": "X"}, {"name": "x2", "type": "X"}
      ], "links": [["x1", "x2"]]},
      "right": {"anchor": "y1", "nodes": [
        {"name": "y1", "type": "Y"}, {"name": "y2", "type": "Y"}
      ], "links": [["y1", "y2"]]},
      "corrs": [
        {"type": "XY", "left": "x1", "right": "y1", "role": "references"},
        {"type": "XY", "left": "x2", "right": "y2", "role": "references"}
      ]
    }
  ]
}"#;

/// `a` mit `n` Kindern vom Typ `X`, die Regeln von [`CONTEXT_LINK`]
/// darin geladen.
fn welt(n: usize) -> (Graph, GhostId, Vec<GhostId>, Vec<DirectedRule>) {
    let mut g = Graph::new();
    let a = g.add_baseline("a", "A");
    let mut xs = Vec::new();
    for i in 0..n {
        let x = g.add_baseline(&format!("a/x{i}"), "X");
        g.connect(a, x, Status::Solid);
        xs.push(x);
    }
    let rules = seesaw_tgg::rules::load(CONTEXT_LINK, &mut g).expect("Regeln laden");
    (g, a, xs, rules)
}

/// Eine Engine fuer eine Welle von der Quellseite.
fn vorwaerts(rules: &[DirectedRule]) -> Engine<'_> {
    let mut e = Engine::new(rules);
    e.admit_delta(&[DeltaDomain::Source]);
    e
}

/// Schritte ohne neuen Seed, bis keiner mehr geht.
fn saettigen(e: &mut Engine<'_>, g: &mut Graph, vs: &ValueStore) {
    for _ in 0..2000 {
        if e.step(g, vs).is_none() {
            return;
        }
    }
    panic!("keine Saettigung nach 2000 Schritten");
}

/// Wie viele Knoten und wie viele Verbindungen von `g` Tombstones sind.
fn grabsteine(g: &Graph) -> (usize, usize) {
    let knoten = g
        .iter_nodes()
        .filter(|n| n.status == Status::Tombstone)
        .count();
    let ids: BTreeSet<GhostId> = g
        .iter_nodes()
        .flat_map(|n| g.parts(&n.id).map(|p| p.connection).collect::<Vec<_>>())
        .collect();
    let verbindungen = ids
        .iter()
        .filter(|id| {
            g.connection(id)
                .is_some_and(|c| c.status == Status::Tombstone)
        })
        .count();
    (knoten, verbindungen)
}

/// Wie viele lebendige Korrespondenzen an `x` haengen.
fn lebendige_korrespondenzen(g: &Graph, x: &GhostId) -> usize {
    g.parts(x)
        .filter(|p| {
            g.node(&p.other)
                .is_some_and(|n| n.is_corr && n.status.is_matchable())
        })
        .count()
}

/// Die Korrespondenz vom Typ `AB`, die `Root` erzeugt.
fn korrespondenz_ab(g: &Graph) -> GhostId {
    let mut found = g
        .iter_nodes()
        .filter(|n| n.is_corr && g.types.name(n.typ) == "AB")
        .map(|n| n.id);
    let ab = found.next().expect("eine Korrespondenz AB");
    assert!(found.next().is_none(), "genau eine Korrespondenz AB");
    ab
}

/// Der Host macht mit der materialisierten Kopie weiter. Sie laesst eine
/// Verbindung weg, die `Root` erzeugt hat und die der Host vorher zum
/// Tombstone gemacht hat, und der Host gibt deren Id an eine Tuer.
/// 2.0.2 fand den Erzeuger in `by_product`, zog den Match von `Root`
/// zurueck und mit ihm alles, was auf seiner Korrespondenz aufbaut. Je
/// Tuer (`element_deleted`, `element_changed`, `retract_for`) und je
/// Verbindung von `Root` eine frische Welt.
///
/// Beobachtet je Lauf: Tombstones unter den Knoten und den Verbindungen
/// der Kopie, wie viele Erzeugnisse der Kaskade Tombstones sind, das
/// Verdikt eines neuen Seeds mit Saettigung und die Laenge der Kaskade
/// danach.
#[test]
#[allow(deprecated)]
fn eine_verbindung_die_die_kopie_weglaesst_findet_ihren_erzeuger() {
    let mut beobachtet = Vec::new();
    for tuer in 0..3 {
        for kante in 0..2 {
            let at = format!("Tuer {tuer}, Verbindung {kante}");
            let (mut g, _a, _xs, rules) = welt(2);
            let vs = ValueStore::default();
            let mut e = vorwaerts(&rules);
            e.seed(&g, &vs);
            saettigen(&mut e, &mut g, &vs);
            e.consolidate(&mut g);
            assert_eq!(e.cascade.len(), 3, "{at}: Root und zwei Child");
            assert_eq!(
                e.cascade[0].created_edges.len(),
                2,
                "{at}: die Verbindungen von Root"
            );
            let edge = e.cascade[0].created_edges[kante];
            g.set_connection_status(&edge, Status::Tombstone);
            let mut copy = g.materialize();
            assert!(
                copy.connection(&edge).is_none() && copy.node(&edge).is_none(),
                "{at}: die Kopie haelt sie nicht"
            );
            match tuer {
                0 => e.element_deleted(&mut copy, &edge),
                1 => e.element_changed(&mut copy, &edge),
                _ => e.retract_for(&mut copy, &edge),
            }
            e.consolidate(&mut copy);
            let (knoten, verbindungen) = grabsteine(&copy);
            let erzeugnisse = e
                .cascade
                .iter()
                .flat_map(|en| en.created.iter())
                .filter(|c| copy.node(c).is_some_and(|n| n.status == Status::Tombstone))
                .count();
            let verdikt = e.run(&mut copy, &vs, 2000);
            let laenge = e.cascade.len();
            println!(
                "BEOBACHTUNG erzeuger_kopie tuer={tuer} kante={kante} knoten_grab={knoten} \
                 verbindungen_grab={verbindungen} erzeugnisse_grab={erzeugnisse} \
                 verdikt={verdikt:?} kaskade={laenge}"
            );
            beobachtet.push((knoten, verbindungen, erzeugnisse, verdikt, laenge));
        }
    }
    // Je Lauf: die sechs Knoten von Root und beiden Child, ihre sieben
    // Verbindungen ohne die weggelassene, jedes Erzeugnis der Kaskade.
    let erwartet = vec![(6, 7, 6, Termination::Contradiction, 3); 6];
    assert_eq!(beobachtet, erwartet);
}

/// Ein Eintrag legt eine Verbindung zwischen zwei gematchten Knoten an
/// (`Pair`: `y0 → y1`), der Host loescht sie. Ihre Erzeuger kommen aus
/// ihren Enden: aus den Erzeugern der Enden und den Eintraegen der
/// Matches, die sie binden. Keines der Enden hat der Eintrag von `Pair`
/// erzeugt, ihn findet die Provenienzkante an seinem Match. 2.0.2 fand
/// ihn in `by_product`, zog den Match von `Pair` zurueck, und ein neuer
/// Seed wendet ihn wieder an.
///
/// Der Ablauf von Java `ErzeugerTest` (der Host ruft `retractMatch` fuer
/// einen toten Match, dessen Schluessel ein anderer traegt) ist in Rust
/// nicht erreichbar, `retract_match` und `record` sind privat. Hier der
/// Teil, den die oeffentliche API erreicht.
#[test]
fn eine_verbindung_zwischen_gematchten_knoten_findet_ihren_erzeuger() {
    let mut g = Graph::new();
    let a = g.add_baseline("a", "A");
    let x0 = g.add_baseline("a/x0", "X");
    let x1 = g.add_baseline("a/x1", "X");
    g.connect(a, x0, Status::Solid);
    g.connect(a, x1, Status::Solid);
    g.connect(x0, x1, Status::Solid);
    let rules = seesaw_tgg::rules::load(PAIR, &mut g).expect("Regeln laden");
    let vs = ValueStore::default();
    let mut e = vorwaerts(&rules);
    e.seed(&g, &vs);
    saettigen(&mut e, &mut g, &vs);
    e.consolidate(&mut g);
    assert_eq!(e.cascade.len(), 4, "Root, zwei Child und Pair");
    let pair: Vec<usize> = (0..e.cascade.len())
        .filter(|&i| rules[e.cascade[i].rule_ix].name.starts_with("Pair"))
        .collect();
    assert_eq!(pair.len(), 1, "ein Eintrag von Pair");
    let eix = pair[0];
    assert!(
        e.cascade[eix].created.is_empty(),
        "Pair erzeugt keinen Knoten"
    );
    assert_eq!(e.cascade[eix].created_edges.len(), 1, "y0 → y1");
    let edge = e.cascade[eix].created_edges[0];
    let ende = g.connection(&edge).expect("y0 → y1 im Graphen");
    let (y0, y1) = (ende.source, ende.target);
    g.set_connection_status(&edge, Status::Tombstone);
    e.element_deleted(&mut g, &edge);
    e.consolidate(&mut g);
    let (knoten, verbindungen) = grabsteine(&g);
    let verdikt = e.run(&mut g, &vs, 2000);
    let laenge = e.cascade.len();
    let status = g.connection(&edge).map(|c| c.status);
    let lebend = (
        lebendige_korrespondenzen(&g, &y0),
        lebendige_korrespondenzen(&g, &y1),
    );
    println!(
        "BEOBACHTUNG erzeuger_paar knoten_grab={knoten} verbindungen_grab={verbindungen} \
         verdikt={verdikt:?} kaskade={laenge} status={status:?} korrespondenzen={lebend:?}"
    );
    assert_eq!(
        (knoten, verbindungen),
        (0, 1),
        "nur die geloeschte Verbindung"
    );
    assert_eq!(laenge, 5, "ein neuer Match von Pair wird angewandt");
    assert_eq!(verdikt, Termination::Duplication);
    assert_eq!(lebend, (1, 1));
}

/// `link_removed` mit der Id einer Verbindung als `a` oder `b`. 2.0.2
/// legte jeden Match auch unter der Verbindung jedes Links seines
/// Musters ab und zog die Matches zurueck, die unter `a` und unter `b`
/// standen. Vier Lagen in der Welt mit zwei Kindern, je eine frische
/// Welt:
///
/// - 0: `corr → a` (der Graph haelt sie andersherum) und `a`,
/// - 1: `a` und `corr → a`,
/// - 2: `a → x0` (gehalten) und `x0`,
/// - 3: `a → x0` auf der materialisierten Kopie, die sie als Tombstone
///   weglaesst, und `x0`.
///
/// Die Matches von `Child`, die auf der Verbindung ruhen, fallen, die
/// Konsolidierung macht ihre Erzeugnisse zu Tombstones, `Root` bleibt.
/// Ein neuer Seed traegt sie wieder ein, und ihr Schritt widerspricht
/// der Historie, ausser auf der Kopie.
///
/// Beobachtet je Lage: Tombstones unter den Knoten und den Verbindungen,
/// ob die Korrespondenz von `Root` lebt, die lebendigen Korrespondenzen
/// an `x0` und `x1`, das Verdikt eines neuen Seeds mit Saettigung und
/// die Laenge der Kaskade danach.
#[test]
fn eine_verbindungs_id_in_link_removed_zieht_die_matches_auf_ihr_zurueck() {
    let mut beobachtet = Vec::new();
    for lage in 0..4 {
        let at = format!("Lage {lage}");
        let (mut g, a, xs, rules) = welt(2);
        let (x0, x1) = (xs[0], xs[1]);
        let vs = ValueStore::default();
        let mut e = vorwaerts(&rules);
        e.seed(&g, &vs);
        saettigen(&mut e, &mut g, &vs);
        e.consolidate(&mut g);
        assert_eq!(e.cascade.len(), 3, "{at}: Root und zwei Child");
        let ab = korrespondenz_ab(&g);
        let named = preview_connection_id(&ab, &a);
        let held = preview_connection_id(&a, &x0);
        assert!(
            g.connection(&named).is_none() && g.node(&named).is_none(),
            "{at}: corr → a haelt der Graph nicht"
        );
        assert!(g.connection(&held).is_some(), "{at}: a → x0");
        let mut h = match lage {
            0 => {
                e.link_removed(&mut g, &named, &a);
                g
            }
            1 => {
                e.link_removed(&mut g, &a, &named);
                g
            }
            2 => {
                e.link_removed(&mut g, &held, &x0);
                g
            }
            _ => {
                g.set_connection_status(&held, Status::Tombstone);
                let mut h = g.materialize();
                assert!(
                    h.connection(&held).is_none() && h.node(&held).is_none(),
                    "{at}: die Kopie haelt a → x0 nicht"
                );
                e.link_removed(&mut h, &held, &x0);
                h
            }
        };
        e.consolidate(&mut h);
        let (knoten, verbindungen) = grabsteine(&h);
        let root = h.node(&ab).is_some_and(|n| n.status.is_matchable());
        let lebend = (
            lebendige_korrespondenzen(&h, &x0),
            lebendige_korrespondenzen(&h, &x1),
        );
        let verdikt = e.run(&mut h, &vs, 2000);
        let laenge = e.cascade.len();
        println!(
            "BEOBACHTUNG link_removed lage={lage} knoten_grab={knoten} \
             verbindungen_grab={verbindungen} root={root} korrespondenzen={lebend:?} \
             verdikt={verdikt:?} kaskade={laenge}"
        );
        beobachtet.push((knoten, verbindungen, root, lebend, verdikt, laenge));
    }
    // Lagen 0 und 1: beide Child fallen, je zwei Knoten und drei
    // Verbindungen. Lagen 2 und 3: das Child von x0.
    let erwartet = vec![
        (4, 6, true, (0, 0), Termination::Contradiction, 3),
        (4, 6, true, (0, 0), Termination::Contradiction, 3),
        (2, 3, true, (0, 1), Termination::Contradiction, 3),
        (2, 3, true, (0, 1), Termination::Duplication, 3),
    ];
    assert_eq!(beobachtet, erwartet);
}

/// `link_removed` mit einem Knoten des Graphen, den kein Match bindet,
/// als `a` oder `b`. 2.0.2 kehrte sofort zurueck. Vier Lagen in der Welt
/// mit zwei Kindern und einem Knoten `z` vom Typ `Z`, den keine Regel
/// liest, je eine frische Welt: `(a, z)`, `(z, a)`, `(z, corr → a)` und
/// `(corr → a, z)`. Keine Wirkung. Dass dabei kein Index der nicht
/// gehaltenen Links entsteht, ist von aussen nicht sichtbar, das prueft
/// `link_removed_with_a_node_no_match_binds_returns_at_once` in
/// `src/engine/mod.rs`.
#[test]
fn ein_knoten_ohne_match_in_link_removed_hat_keine_wirkung() {
    let mut beobachtet = Vec::new();
    for lage in 0..4 {
        let (mut g, a, xs, rules) = welt(2);
        let z = g.add_baseline("z", "Z");
        let vs = ValueStore::default();
        let mut e = vorwaerts(&rules);
        e.seed(&g, &vs);
        saettigen(&mut e, &mut g, &vs);
        e.consolidate(&mut g);
        assert_eq!(e.cascade.len(), 3, "Lage {lage}: Root und zwei Child");
        let ab = korrespondenz_ab(&g);
        let named = preview_connection_id(&ab, &a);
        let (x, y) = [(a, z), (z, a), (z, named), (named, z)][lage];
        e.link_removed(&mut g, &x, &y);
        e.consolidate(&mut g);
        let (knoten, verbindungen) = grabsteine(&g);
        let root = g.node(&ab).is_some_and(|n| n.status.is_matchable());
        let lebend = (
            lebendige_korrespondenzen(&g, &xs[0]),
            lebendige_korrespondenzen(&g, &xs[1]),
        );
        let verdikt = e.run(&mut g, &vs, 2000);
        let laenge = e.cascade.len();
        println!(
            "BEOBACHTUNG knoten_ohne_match lage={lage} knoten_grab={knoten} \
             verbindungen_grab={verbindungen} root={root} korrespondenzen={lebend:?} \
             verdikt={verdikt:?} kaskade={laenge}"
        );
        beobachtet.push((knoten, verbindungen, root, lebend, verdikt, laenge));
    }
    let erwartet = vec![(0, 0, true, (1, 1), Termination::Duplication, 3); 4];
    assert_eq!(beobachtet, erwartet);
}
