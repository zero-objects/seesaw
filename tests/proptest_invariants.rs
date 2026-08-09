//! Eigenschaftsbasierte Tests der Kern-Invarianten V₄, V₆, V₁₆, V₁₇ gegen
//! die Generation die Engine (`seesaw_tgg::graph`).
//!
//! Portierung von `proptest_invariants.rs`, das an der alten Generation
//! (`engine`, `fold`, `graph::TypedGraph`, `ops`) hängt. Die Aussagen sind
//! dieselben, die Begriffe sind die des heutigen Modells:
//!
//! | Begriff first-generation                  | Begriff die Engine                                |
//! |-----------------------------|-------------------------------------------|
//! | `Cascade` (Liste `DeltaEntry`) | `Engine::cascade` (Liste `Entry`)      |
//! | `Op::apply` auf `TypedGraph` | `Engine::step` auf `graph::Graph`            |
//! | `fold::consolidate`          | `Engine::consolidate` (TT → Tombstone)   |
//! | `TypedGraph::materialize`    | `graph::Graph::materialize`                  |
//!
//! Jeder Test nennt am Kopf Datei und Lemma seines formalen Gegenstücks in
//! `proofs/Seesaw/`.

mod common;

use proptest::prelude::*;
use seesaw_tgg::engine::{Engine, Termination};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;
use std::collections::BTreeSet;

// ══ Regelsatz ════════════════════════════════════════════════════════════

/// Family→Person, zwei gerichtete Rollen mit verschiedenem Rang — damit die
/// Rang-Ordnung der To-do-Liste im Test überhaupt eine Rolle spielt.
fn regelsatz(g: &mut Graph) -> Vec<DirectedRule> {
    let specs: Vec<serde_json::Value> = [
        ("Father", "Male", "MaleCorr", 850u64),
        ("Mother", "Female", "FemaleCorr", 800u64),
    ]
    .into_iter()
    .map(|(rolle, ziel, corr, rank)| {
        serde_json::json!({
            "name": format!("{rolle}_2_{ziel}"),
            "rank": rank,
            "left": {
                "anchor": "fam",
                "nodes": [
                    {"name": "fam", "type": "Family"},
                    {"name": "rolle", "type": rolle},
                    {"name": "member", "type": "Member"},
                    {"name": "first", "type": "firstName"}
                ],
                "links": [["fam", "rolle"], ["rolle", "member"], ["member", "first"]]
            },
            "right": {
                "anchor": "ziel",
                "nodes": [
                    {"name": "ziel", "type": ziel},
                    {"name": "name", "type": "name"}
                ],
                "links": [["ziel", "name"]]
            },
            "corrs": [
                {"type": corr, "left": "member", "right": "ziel", "role": "establishes",
                 "bindings": [{"left": "first", "right": "name"}]}
            ]
        })
    })
    .collect();
    common::load("proptest_invariants", specs, g)
}

// ══ Strategien ═══════════════════════════════════════════════════════════

/// Eine Familie im Zufallsmodell: welche Rollen existieren und wie die
/// Vornamen lauten. Fehlende Rollen erzeugen unvollständige L-Pattern —
/// so matcht nicht jede Familie jede Regel.
#[derive(Debug, Clone)]
struct FamilieSpec {
    vater: Option<String>,
    mutter: Option<String>,
    /// Member ohne Vornamens-Blatt: das L-Pattern bricht an Position 3 ab.
    vater_blatt: bool,
    /// Zweiter Family-Knoten an DERSELBEN Vater-Kette. Damit hat das
    /// L-Pattern zwei Matches, die dieselbe Ziel-Identität erzeugen —
    /// die Identität eines erzeugten Knotens hängt am Member, nicht an
    /// der Familie. Fällt der erste Family-Knoten weg, zieht die
    /// Retraktion das Erzeugte tentativ zurück, und die zweite
    /// Herleitung REKLAMIERT es (M5-Resurrektion). Ohne diesen Fall
    /// liefe die Konsolidierung im Test nie gegen reklamiertes
    /// Material, und V₁₇ wäre nur halb geprüft.
    geteilte_kette: bool,
}

fn arb_familie() -> impl Strategy<Value = FamilieSpec> {
    (
        prop::option::of("[A-Z][a-z]{2,5}"),
        prop::option::of("[A-Z][a-z]{2,5}"),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(vater, mutter, vater_blatt, geteilte_kette)| FamilieSpec {
            vater,
            mutter,
            vater_blatt,
            geteilte_kette,
        })
}

/// Die erste Familie ist immer vollständig (Vater mit Vornamens-Blatt)
/// und trägt immer die geteilte Kette. Ohne diese Zusicherung erzeugt ein
/// Teil der Zufallsmodelle gar keinen Match und keine Reklamation, und die
/// Invarianten würden über leeren Kaskaden geprüft.
fn arb_modell() -> impl Strategy<Value = Vec<FamilieSpec>> {
    (
        "[A-Z][a-z]{2,5}",
        prop::option::of("[A-Z][a-z]{2,5}"),
        prop::collection::vec(arb_familie(), 0..=5),
    )
        .prop_map(|(vater, mutter, rest)| {
            let mut v = vec![FamilieSpec {
                vater: Some(vater),
                mutter,
                vater_blatt: true,
                geteilte_kette: true,
            }];
            v.extend(rest);
            v
        })
}

/// Baut das Quellmodell (nur die L-Seite) aus der Spezifikation.
fn baue(spec: &[FamilieSpec]) -> (Graph, ValueStore) {
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    for (i, f) in spec.iter().enumerate() {
        let fam = g.add_baseline(&format!("f{i}"), "Family");
        for (rolle, name, mit_blatt) in [
            ("Father", f.vater.as_ref(), f.vater_blatt),
            ("Mother", f.mutter.as_ref(), true),
        ] {
            let Some(name) = name else { continue };
            let r = g.add_baseline(&format!("f{i}/{rolle}"), rolle);
            let m = g.add_baseline(&format!("f{i}/{rolle}/m"), "Member");
            g.connect(fam, r, Status::Solid);
            g.connect(r, m, Status::Solid);
            if f.geteilte_kette {
                // Zweiter Family-Knoten an derselben Rollen-Kette.
                let fam2 = g.add_baseline(&format!("f{i}b"), "Family");
                g.connect(fam2, r, Status::Solid);
            }
            if mit_blatt {
                let leaf = g.add_baseline(&format!("f{i}/{rolle}/m/fn"), "firstName");
                g.connect(m, leaf, Status::Solid);
                vs.insert(leaf, name.clone());
            }
        }
    }
    (g, vs)
}

// ══ Fingerabdrücke ═══════════════════════════════════════════════════════

/// Knoten-Fingerabdruck: (Id, Typ-Name, Status). Deterministisch sortiert.
fn knoten_fp(g: &Graph) -> Vec<(GhostId, String, u8)> {
    let mut v: Vec<_> = g
        .iter_nodes()
        .map(|n| (n.id, g.types.name(n.typ).to_string(), n.status as u8))
        .collect();
    v.sort();
    v
}

/// Alle Verbindungs-Ids des Graphen — über die Beteiligungs-Listen, weil
/// die Map selbst privat ist.
fn verbindungen(g: &Graph) -> BTreeSet<GhostId> {
    let mut out = BTreeSet::new();
    for n in g.iter_nodes() {
        for p in g.parts(&n.id) {
            out.insert(p.connection);
        }
    }
    out
}

/// Effektive Materialisierung im Sinne von `Consolidation.matAt`: der
/// Rollup-Gewinner an einer Identität ist entweder ein `present`-Wert
/// (Solid/Ghost) oder `absent`. Ein TentativeTombstone ist eine
/// zurückgezogene Herleitung, also `absent` — genau `effVal .absent`.
///
/// `Graph::materialize` selbst überspringt nur `Tombstone` und lässt TT
/// stehen; das ist die Materialisierung VOR der Konsolidierung. Die
/// effektive Sicht hier ist die, über die V₁₇ redet.
fn mat_effektiv(g: &Graph) -> (BTreeSet<GhostId>, BTreeSet<GhostId>) {
    let knoten: BTreeSet<GhostId> = g
        .iter_nodes()
        .filter(|n| matches!(n.status, Status::Solid | Status::Ghost))
        .map(|n| n.id)
        .collect();
    let kanten: BTreeSet<GhostId> = verbindungen(g)
        .into_iter()
        .filter(|c| {
            g.connection(c).is_some_and(|c| {
                matches!(c.status, Status::Solid | Status::Ghost)
                    && knoten.contains(&c.source)
                    && knoten.contains(&c.target)
            })
        })
        .collect();
    (knoten, kanten)
}

/// Materialisierung wie `Graph::materialize` sie liefert, als Mengen.
fn mat_graph(g: &Graph) -> (BTreeSet<GhostId>, BTreeSet<GhostId>) {
    let m = g.materialize();
    let knoten: BTreeSet<GhostId> = m.iter_nodes().map(|n| n.id).collect();
    let kanten: BTreeSet<GhostId> = verbindungen(&m);
    (knoten, kanten)
}

/// Ein Kaskaden-Eintrag als Vergleichswert: Regel, Rang, Ref-Folge (μ),
/// erzeugte Knoten, erzeugte Verbindungen.
type EintragFp = (usize, u64, Vec<GhostId>, Vec<GhostId>, Vec<GhostId>);

/// Cascade-Fingerabdruck: was angewandt wurde, in Anwendungs-Reihenfolge.
fn cascade_fp(e: &Engine<'_>) -> Vec<EintragFp> {
    e.cascade
        .iter()
        .map(|x| {
            (
                x.rule_ix,
                x.rank,
                x.refs.clone(),
                x.created.clone(),
                x.created_edges.clone(),
            )
        })
        .collect()
}

/// Cascade bis Sättigung, ohne erneutes `seed` (der Aufrufer hat entweder
/// `seed` oder `elements_added` gerufen).
fn bis_saettigung(e: &mut Engine<'_>, g: &mut Graph, vs: &ValueStore) -> usize {
    let mut schritte = 0;
    while e.step(g, vs).is_some() {
        schritte += 1;
        assert!(schritte < 100_000, "Sättigungs-Schranke");
    }
    schritte
}

/// Ein Δ auf das synchronisierte Modell, danach Re-Derivation bis
/// Sättigung. Die Konsolidierung bleibt dem Aufrufer.
///
/// Zwei Arten, weil sie die Konsolidierung an verschiedene Stellen führen:
///
/// * `Art::Knoten` — ein Baseline-Knoten fällt weg. Die Retraktion zieht
///   das daraus Erzeugte tentativ zurück, und nichts reklamiert es: die
///   Konsolidierung sieht ausschließlich TT-Material.
/// * `Art::Kante` — eine Verbindung zwischen zwei Teilnehmern eines
///   angewandten Matches fällt weg. `Engine::link_removed` ist hier
///   bewusst eine Über-Approximation (siehe dort): es zieht auch Matches
///   zurück, die die entfernte Kante gar nicht benutzt haben. Genau diese
///   Matches sind danach noch herleitbar und REKLAMIEREN das
///   Zurückgezogene (M5-Resurrektion). Die Konsolidierung läuft dann
///   gegen gemischtes Material — reklamiert und unreklamiert. Ohne diese
///   Art bliebe der Reklamations-Zweig von `consolidate` ungeprüft.
#[derive(Debug, Clone, Copy)]
enum Art {
    Knoten,
    Kante,
}

fn delta_anwenden(e: &mut Engine<'_>, g: &mut Graph, vs: &ValueStore, art: Art, ix: usize) -> bool {
    let geschehen = match art {
        Art::Kante if !e.cascade.is_empty() => {
            let refs = e.cascade[ix % e.cascade.len()].refs.clone();
            if refs.len() < 2 {
                false
            } else {
                let (a, b) = (refs[0], refs[refs.len() - 1]);
                e.link_removed(g, &a, &b);
                true
            }
        }
        _ => {
            let kandidaten: Vec<GhostId> = g
                .iter_nodes()
                .filter(|n| n.status == Status::Solid)
                .map(|n| n.id)
                .collect();
            if kandidaten.is_empty() {
                false
            } else {
                let opfer = kandidaten[ix % kandidaten.len()];
                g.set_node_status(&opfer, Status::Tombstone);
                e.element_removed(&opfer);
                e.retract_for(g, &opfer);
                true
            }
        }
    };
    if geschehen {
        // Re-Derivation: was noch begründbar ist, reklamiert sich.
        e.seed(g, vs);
        bis_saettigung(e, g, vs);
    }
    geschehen
}

fn arb_art() -> impl Strategy<Value = Art> {
    prop_oneof![Just(Art::Knoten), Just(Art::Kante)]
}

// ══ V₄ — Projektionsstabilität ═══════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// **V₄ (Projektionsstabilität).**
    /// Formales Gegenstück: `proofs/Seesaw/Projection.lean`,
    /// `Seesaw.Applies_total` (Totalität) und `Seesaw.Applies_functional`
    /// (Determinismus), gestützt auf `Seesaw.Applies_iff`.
    ///
    /// Lean-Aussage: die Projektions-Relation `Applies σ ops σ'` hat für
    /// jedes Op-Skript mindestens ein (total) und höchstens ein
    /// (funktional) Ergebnis — φ_L ist eine Funktion, keine Relation.
    /// `Applies_iff` sagt zusätzlich, dass JEDE Herleitung dasselbe
    /// `applyScript` berechnet, unabhängig davon, wie sie zerlegt wurde.
    ///
    /// hiesige Form: die Projektion ist der Kaskaden-Lauf. Der alte Test hat
    /// nur zwei identische Läufe verglichen (Knoten-/Kantenzahl). Hier
    /// wird die stärkere Hälfte von `Applies_iff` geprüft: ZWEI
    /// VERSCHIEDENE Herleitungswege müssen dasselbe Ergebnis liefern.
    /// Weg A ist die Voll-Enumeration (`Engine::seed`), Weg B die
    /// delta-lokale Verankerung (`Engine::elements_added` über alle
    /// Baseline-Knoten). Beide Wege sind verschiedene Zerlegungen
    /// desselben Op-Skripts. Dasselbe Endergebnis heißt: die Projektion
    /// hängt am Eingabe-Graphen, nicht am Enumerations-Pfad.
    /// Totalität: beide Wege liefern eine Terminierung, keiner hängt.
    #[test]
    fn v4_projektion_ist_funktion(spec in arb_modell()) {
        // Weg A: Voll-Enumeration.
        let (mut ga, vsa) = baue(&spec);
        let rules_a = regelsatz(&mut ga);
        let mut ea = Engine::new(&rules_a);
        ea.seed(&ga, &vsa);
        bis_saettigung(&mut ea, &mut ga, &vsa);

        // Weg B: delta-lokale Verankerung jedes Baseline-Knotens.
        let (mut gb, vsb) = baue(&spec);
        let rules_b = regelsatz(&mut gb);
        let neu: Vec<GhostId> = gb.iter_nodes().map(|n| n.id).collect();
        let mut eb = Engine::new(&rules_b);
        eb.elements_added(&gb, &vsb, &neu);
        bis_saettigung(&mut eb, &mut gb, &vsb);

        // Totalität: beide Wege sind fertig geworden (kein Hänger, kein
        // Widerspruch) — `Applies_total`.
        prop_assert!(!ea.saw_contradiction, "Weg A: Widerspruch");
        prop_assert!(!eb.saw_contradiction, "Weg B: Widerspruch");

        // Funktionalität: identisches Ergebnis — `Applies_functional`.
        prop_assert_eq!(
            cascade_fp(&ea), cascade_fp(&eb),
            "Herleitungsweg verändert die Kaskade"
        );
        prop_assert_eq!(
            knoten_fp(&ga), knoten_fp(&gb),
            "Herleitungsweg verändert den Ergebnis-Graphen"
        );
        prop_assert_eq!(
            verbindungen(&ga), verbindungen(&gb),
            "Herleitungsweg verändert die Verbindungen"
        );
    }
}

// ══ V₆ — strikte Längen-Monotonie ════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// **V₆ (Strikte Monotonie der Länge).**
    /// Formales Gegenstück: `proofs/Seesaw/Delta.lean`,
    /// `Seesaw.deltaRun_length` (|D_x| = x + 1) und
    /// `Seesaw.append1_strict_mono` (jeder Schritt wächst strikt),
    /// zusammen mit `Seesaw.append1_length`.
    ///
    /// Lean-Aussage: der Delta-Graph ist append-only; ein Kaskaden-Schritt
    /// hängt genau einen Eintrag an, also gilt nach x Schritten |D_x| =
    /// x + 1 und |D_x| < |D_{x+1}|.
    ///
    /// hiesige Form: der Delta-Graph ist `Engine::cascade`. Der alte Test hat
    /// `Cascade::append` direkt gerufen und die Länge gezählt — geprüft
    /// wurde also die Datenstruktur. Hier wird die ENGINE getrieben, denn
    /// in die Engine ist `append` nicht öffentlich: nur `step` schreibt in die
    /// Kaskade. `step` liefert `Some(true)` für eine Anwendung,
    /// `Some(false)` für ein erkanntes Duplikat (Def. der Nullifikation:
    /// ein Duplikat ist kein Kaskaden-Eintrag) und `None` bei Sättigung.
    /// Geprüft wird deshalb dreierlei:
    ///   1. jede Anwendung hängt GENAU einen Eintrag an (`append1_length`),
    ///   2. keine Anwendung verkürzt (`append1_strict_mono`),
    ///   3. das bereits geschriebene Präfix bleibt unverändert — das ist
    ///      die Append-only-Eigenschaft, die in Lean in der Konstruktion
    ///      `append1 D e = D ++ [e]` steckt.
    /// Zusammen: |D| = Zahl der Anwendungen (das x + 1 der Lean-Aussage,
    /// verschoben um den Start-Eintrag d₀, den die Engine nicht kennt — die
    /// Kaskade startet leer).
    #[test]
    fn v6_kaskade_ist_append_only(spec in arb_modell()) {
        let (mut g, vs) = baue(&spec);
        let rules = regelsatz(&mut g);
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);

        let mut anwendungen = 0usize;
        prop_assert_eq!(e.cascade.len(), 0, "Kaskade startet leer");

        loop {
            let vorher_len = e.cascade.len();
            let vorher_fp = cascade_fp(&e);
            match e.step(&mut g, &vs) {
                None => break,
                Some(angewandt) => {
                    let nachher_fp = cascade_fp(&e);
                    // (3) Append-only: das Präfix ist unangetastet.
                    prop_assert_eq!(
                        &nachher_fp[..vorher_len], &vorher_fp[..],
                        "Kaskaden-Präfix wurde verändert"
                    );
                    if angewandt {
                        anwendungen += 1;
                        // (1) genau ein Eintrag …
                        prop_assert_eq!(
                            e.cascade.len(), vorher_len + 1,
                            "Anwendung hängt nicht genau einen Eintrag an"
                        );
                        // (2) … also strikt gewachsen.
                        prop_assert!(vorher_len < e.cascade.len());
                    } else {
                        // Duplikat: nullifiziert, kein Eintrag.
                        prop_assert_eq!(
                            e.cascade.len(), vorher_len,
                            "Duplikat hat einen Eintrag angehängt"
                        );
                    }
                }
            }
            prop_assert!(anwendungen < 100_000, "Sättigungs-Schranke");
        }

        // |D| = Zahl der Anwendungen.
        prop_assert_eq!(
            e.cascade.len(), anwendungen,
            "Kaskaden-Länge ≠ Zahl der Anwendungen"
        );
    }
}

// ══ V₁₆ — Terminierung der Konsolidierung ════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// **V₁₆ (Terminierung der Konsolidierung).**
    /// Formales Gegenstück: `proofs/Seesaw/Fold.lean`,
    /// `Seesaw.fold_fixpoint`.
    ///
    /// Lean-Aussage: sei `c i` die Größe der Nullifikations-Menge C nach
    /// i Fold-Iterationen. Ist `c` durch B beschränkt (`bound`) und wächst
    /// jede Iteration entweder gar nicht oder um mindestens 1
    /// (`progress`), dann gibt es ein i ≤ B mit c(i+1) = c(i) — der
    /// Fixpunkt wird innerhalb von B Iterationen erreicht.
    ///
    /// hiesige Form: die Nullifikations-Menge C ist die Menge der Elemente
    /// (Knoten und Verbindungen) mit Status `Tombstone` — das ist genau,
    /// was `Engine::consolidate` schreibt. B ist die Gesamtzahl der
    /// Teilnehmer im Modell, denn mehr Elemente kann C nicht enthalten.
    /// Der alte Test war schwächer: er rief `fold::consolidate` einmal
    /// auf und prüfte nur, dass ein Ergebnis kam. Hier werden die beiden
    /// Hypothesen des Lemmas GEMESSEN (Schranke, Monotonie) und die
    /// Konklusion geprüft (Fixpunkt-Index ≤ B).
    #[test]
    fn v16_konsolidierung_erreicht_fixpunkt(
        spec in arb_modell(),
        art in arb_art(),
        loesch_ix in 0..64usize,
    ) {
        let (mut g, vs) = baue(&spec);
        let rules = regelsatz(&mut g);
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        bis_saettigung(&mut e, &mut g, &vs);

        // Ein Δ — erst dann hat die Konsolidierung überhaupt Arbeit.
        delta_anwenden(&mut e, &mut g, &vs, art, loesch_ix);

        // B = Gesamtzahl der Teilnehmer. C ⊆ Teilnehmer, also c i ≤ B.
        let b = g.node_count() + verbindungen(&g).len();

        // c 0 … c (B+1): Konsolidierung iterieren und C messen.
        let c = |g: &Graph| -> usize {
            let tote_knoten = g
                .iter_nodes()
                .filter(|n| n.status == Status::Tombstone)
                .count();
            let tote_kanten = verbindungen(g)
                .into_iter()
                .filter(|id| {
                    g.connection(id).is_some_and(|x| x.status == Status::Tombstone)
                })
                .count();
            tote_knoten + tote_kanten
        };

        let mut reihe = vec![c(&g)];
        for _ in 0..=b {
            e.consolidate(&mut g);
            reihe.push(c(&g));
        }

        // Hypothese `bound`: ∀ i, c i ≤ B.
        for (i, &ci) in reihe.iter().enumerate() {
            prop_assert!(ci <= b, "c {} = {} überschreitet die Schranke B = {}", i, ci, b);
        }
        // Hypothese `progress`: c (i+1) = c i ∨ c i + 1 ≤ c (i+1). Über
        // natürlichen Zahlen ist die zweite Hälfte `c i < c (i+1)`.
        // (Die Nullifikations-Menge schrumpft nie — Tombstone ist final.)
        for i in 0..reihe.len() - 1 {
            prop_assert!(
                reihe[i + 1] == reihe[i] || reihe[i] < reihe[i + 1],
                "c {} = {} → c {} = {} ist weder Fixpunkt noch Wachstum",
                i, reihe[i], i + 1, reihe[i + 1]
            );
        }
        // Konklusion `fold_fixpoint`: ∃ i ≤ B mit c (i+1) = c i.
        let fixpunkt = (0..reihe.len() - 1).find(|&i| reihe[i + 1] == reihe[i]);
        prop_assert!(fixpunkt.is_some(), "kein Fixpunkt innerhalb von B = {}", b);
        prop_assert!(
            fixpunkt.unwrap() <= b,
            "Fixpunkt erst bei i = {}, Schranke B = {}", fixpunkt.unwrap(), b
        );
    }
}

// ══ V₁₇ — semantische Treue der Konsolidierung ═══════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// **V₁₇ (Semantische Treue der Konsolidierung).**
    /// Formales Gegenstück: `proofs/Seesaw/Consolidation.lean`,
    /// `Seesaw.consolidation_faithful` (∀ t, matAt (D.filter keep) t =
    /// matAt D t) und die Punktfrei-Fassung
    /// `Seesaw.consolidation_effect_equiv`; zusätzlich geprüft wird
    /// `Seesaw.consolidation_idempotent`.
    ///
    /// Lean-Aussage: entfernt die Konsolidierung nur dominierte oder
    /// annullierte Ops und behält an jeder Identität den Rollup-Gewinner,
    /// dann ist die Materialisierung unverändert.
    ///
    /// hiesige Form: `matAt` an einer Identität ist ihr aktueller Rollup-
    /// Gewinner. `Solid`/`Ghost` ist `Effect.present`, ein
    /// `TentativeTombstone` ist eine zurückgezogene Herleitung, also
    /// `Effect.absent` — genau wie ein `Tombstone`. `Engine::consolidate`
    /// ist der `filter`: es schreibt TT → Tombstone und lässt alles
    /// andere stehen. Beide Status sind unter `matAt` `absent`, folglich
    /// darf die effektive Materialisierung sich NICHT ändern. Das ist
    /// wörtlich `mat(φ_L(D_x)) = mat(φ_L(D̂_x))`.
    ///
    /// Der alte Test hat V₁₇ auf eine Ungleichung abgeschwächt
    /// (`fold`-Baseline hat ≤ so viele Knoten wie die Voll-Anwendung).
    /// Hier steht die Gleichung selbst, als Mengen-Gleichheit über
    /// Knoten UND Verbindungen.
    ///
    /// Zweite Zusage im selben Test: nach der Konsolidierung stimmt die
    /// naive Materialisierung `Graph::materialize` (die nur `Tombstone`
    /// überspringt und TT stehen lässt) mit der effektiven überein —
    /// die Konsolidierung ist genau der Schritt, der beide zur Deckung
    /// bringt. Dritte Zusage: ein zweiter Lauf ändert nichts mehr
    /// (`consolidation_idempotent`).
    #[test]
    fn v17_konsolidierung_erhaelt_materialisierung(
        spec in arb_modell(),
        art in arb_art(),
        loesch_ix in 0..64usize,
    ) {
        let (mut g, vs) = baue(&spec);
        let rules = regelsatz(&mut g);
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        bis_saettigung(&mut e, &mut g, &vs);

        // Δ + Re-Derivation — erzeugt das (teils reklamierte)
        // TT-Material, an dem die Konsolidierung arbeitet.
        prop_assume!(delta_anwenden(&mut e, &mut g, &vs, art, loesch_ix));

        // Nicht-Leerlauf-Zeuge: solange TT-Material offen ist, weicht die
        // naive `materialize`-Sicht von der effektiven ab — die
        // Konsolidierung hat also wirklich etwas zu tun. Ohne diese
        // Zusicherung könnte der Test über einem TT-freien Graphen
        // trivial grün sein.
        let tt: Vec<GhostId> = g
            .iter_nodes()
            .filter(|n| n.status == Status::TentativeTombstone)
            .map(|n| n.id)
            .collect();
        let vor = mat_effektiv(&g);
        if !tt.is_empty() {
            let naiv = mat_graph(&g);
            for id in &tt {
                prop_assert!(naiv.0.contains(id), "materialize lässt TT stehen");
                prop_assert!(!vor.0.contains(id), "effektive Sicht zählt TT als absent");
            }
        }

        e.consolidate(&mut g);
        let nach = mat_effektiv(&g);

        // V₁₇: die effektive Materialisierung ist unverändert.
        prop_assert_eq!(
            &vor.0, &nach.0,
            "Konsolidierung hat Knoten-Materialisierung verändert"
        );
        prop_assert_eq!(
            &vor.1, &nach.1,
            "Konsolidierung hat Kanten-Materialisierung verändert"
        );

        // Nach der Konsolidierung deckt sich `materialize` mit der
        // effektiven Sicht: es gibt kein unentschiedenes TT mehr.
        prop_assert_eq!(
            mat_graph(&g), nach.clone(),
            "materialize weicht nach der Konsolidierung ab"
        );

        // `consolidation_idempotent`: zweiter Lauf ändert nichts.
        e.consolidate(&mut g);
        prop_assert_eq!(
            mat_effektiv(&g), nach,
            "Konsolidierung ist nicht idempotent"
        );
    }
}

// ══ Rahmen-Zusicherung ═══════════════════════════════════════════════════

/// Kein Invarianten-Test, sondern die Zusicherung, dass der Regelsatz
/// oben überhaupt arbeitet — sonst prüften die vier Tests über leeren
/// Kaskaden und wären wertlos.
#[test]
fn regelsatz_erzeugt_kaskade() {
    let spec = vec![
        FamilieSpec {
            vater: Some("Ann".into()),
            mutter: Some("Bea".into()),
            vater_blatt: true,
            geteilte_kette: false,
        },
        FamilieSpec {
            vater: Some("Cyd".into()),
            mutter: None,
            vater_blatt: true,
            geteilte_kette: false,
        },
    ];
    let (mut g, vs) = baue(&spec);
    let rules = regelsatz(&mut g);
    let mut e = Engine::new(&rules);
    let t = e.run(&mut g, &vs, 10_000);
    assert_eq!(t, Termination::Duplication);
    assert_eq!(e.cascade.len(), 3, "2 Väter + 1 Mutter übersetzt");
}
