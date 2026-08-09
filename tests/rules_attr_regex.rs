//! Nachfolger des geloeschten Vorgaengers auf der Kette:
//! Regeldatei -> `RuleFile::from_json` -> `validate` -> `lower_all` ->
//! Matchen. Prueft alle fuenf `PredicateDecl`-Varianten (`Exists`,
//! `Equals`, `Prefix`, `Regex`, `NumericRange`) end-to-end sowie den
//! Fehlerfall bei ungueltigem regulaerem Ausdruck.
//!
//! Zwei Abweichungen gegenueber der alten (erste Generation) Fassung, siehe
//! Task-3-Bericht:
//!
//! 1. Fuenf Praedikat-Arten statt vier: `PredicateDecl::Exists` kommt
//!    neu hinzu (first-generation hatte nur Regex/Prefix/NumericRange/Literal). Der
//!    Regex-Zweig hat eine EIGENE Normierung (`rules::predicate::Predicate::
//!    parse_regex`): Vollmatch (`\A(?:...)\z`) statt Teilmatch, KEINE
//!    Anker (`^`/`$`) im rohen Muster (der Vollmatch macht sie
//!    ueberfluessig und ein im Muster stehender Anker waere ohnehin
//!    doppelt verankert), eingeschraenkte Syntax (kein Lookaround, keine
//!    benannten Gruppen, keine Backreferences, keine possessiven
//!    Quantoren). Der alte Test benutzte "^Abstract" als GUELTIGES
//!    Muster (Teiltreffer-Semantik) -- das ist unter das Regelformat ein Ladefehler
//!    (`PredicateError::ForbiddenSyntax`), siehe
//!    `regex_mit_anker_wird_als_verbotene_syntax_abgelehnt` unten. Der
//!    alte Test benutzte "(" als UNGUELTIGES Muster und erwartete eine
//!    Fehlermeldung, die "InvalidRegex" oder "Regex" enthaelt -- das gilt
//!    unter das Regelformat unveraendert, aber ueber einen eigenen Fehlertyp
//!    (`PredicateError::BadRegex`), siehe
//!    `unbalancierte_klammer_wird_als_bad_regex_abgelehnt` unten.
//!
//! 2. Ein Wert-Praedikat auf einem Knoten, den das Lowering in einer der
//!    beiden Richtungen ERZEUGT, ist ein Ladefehler
//!    (`LoadError::PredicateOnCreatedNode`) -- ausser der engen
//!    Doppelrolle (Gleichheits-Praedikat mit Wert == `constant` desselben
//!    Knotens, siehe `rules::validate::check_value_roles`). Diese Datei
//!    testet reine Matcher-Praedikate, keine Doppelrolle -- deshalb sitzt
//!    das Praedikat hier an einem Knoten mit `"context": true` auf der
//!    LINKEN (Eingangs-)Seite: ein Kontextknoten wird in KEINER Richtung
//!    erzeugt (`validate::is_created` liefert fuer `context`-Knoten
//!    immer `false`), er wird nur gematcht -- genau die Rolle, die das
//!    alte `find_matches`-Pattern direkt hatte.

use seesaw_tgg::engine::matcher::find_matches;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::format::RuleFile;
use seesaw_tgg::rules::lower::lower_all;
use seesaw_tgg::rules::predicate::PredicateError;
use seesaw_tgg::rules::validate::{validate, LoadError};

/// Eine `RuleDecl`, minimal: Anker `item` (Typ `Item`) verlinkt auf ein
/// Kontext-Blatt `attr` (Typ `Attr`) mit dem gegebenen Praedikat; rechts
/// ein `Marker`-Knoten, per `establishes`-Corr an `item` gebunden. Das
/// Praedikat ist die einzige Variable zwischen den Tests.
fn rule_decl(name: &str, predicate_json: &str) -> String {
    format!(
        r#"{{"name":"{name}","rank":1,
        "left":{{"anchor":"item","nodes":[
            {{"name":"item","type":"Item"}},
            {{"name":"attr","type":"Attr","context":true,"predicate":{predicate_json}}}],
          "links":[["item","attr"]]}},
        "right":{{"anchor":"marker","nodes":[{{"name":"marker","type":"Marker"}}]}},
        "corrs":[{{"type":"C","left":"item","right":"marker","role":"establishes"}}]}}"#
    )
}

/// Eine `RuleFile` mit genau der einen Regel aus `rule_decl`.
fn one_predicate_rule(predicate_json: &str) -> String {
    format!(
        r#"{{"format":3,"name":"pred-test","rules":[{}]}}"#,
        rule_decl("R", predicate_json)
    )
}

/// Laedt, validiert, lowert -- liefert die VORWAERTS-Richtung (Eingang
/// `item`+`attr` matcht, `marker` entsteht) plus den Graphen, in dessen
/// Typtabelle die Regel gelowert wurde (dieselbe Instanz muss fuer die
/// Seed-Daten weiterverwendet werden, sonst passen die `TypeId`s nicht).
fn setup(predicate_json: &str) -> (DirectedRule, Graph) {
    let file = RuleFile::from_json(&one_predicate_rule(predicate_json)).expect("parst");
    let resolved = validate(&file).expect("validiert");
    let mut g = Graph::default();
    let mut lowered = lower_all(&resolved, &mut g).expect("lowert");
    assert_eq!(lowered.len(), 2, "eine Regel, zwei Richtungen");
    let fwd = lowered.remove(0);
    (fwd, g)
}

// ── Voller Umlauf: alle fuenf Praedikat-Arten in einer Datei ───────────

#[test]
fn fuenf_praedikat_arten_parsen_validieren_und_lowern() {
    let s = format!(
        r#"{{"format":3,"name":"pred-kinds","rules":[{},{},{},{},{}]}}"#,
        rule_decl("R_exists", r#"{"kind":"exists"}"#),
        rule_decl("R_equals", r#"{"kind":"equals","value":"yes"}"#),
        rule_decl("R_prefix", r#"{"kind":"prefix","value":"get"}"#),
        rule_decl("R_regex", r#"{"kind":"regex","pattern":"Abstract.*"}"#),
        rule_decl(
            "R_numeric_range",
            r#"{"kind":"numeric_range","min":0.0,"max":100.0}"#
        ),
    );
    let file = RuleFile::from_json(&s).expect("parst");
    assert_eq!(file.rules.len(), 5);
    let resolved = validate(&file).expect("validiert");
    let mut g = Graph::default();
    let lowered = lower_all(&resolved, &mut g).expect("lowert");
    assert_eq!(lowered.len(), 10, "fuenf Regeln, je zwei Richtungen");
}

// ── Matcher-Verhalten je Praedikat-Art ──────────────────────────────────

#[test]
fn exists_matcht_nur_knoten_mit_wert() {
    let (fwd, mut g) = setup(r#"{"kind":"exists"}"#);
    let item = g.add_baseline("item", "Item");
    let mit_wert = g.add_baseline("item/mit_wert", "Attr");
    let ohne_wert = g.add_baseline("item/ohne_wert", "Attr");
    g.connect(item, mit_wert, Status::Solid);
    g.connect(item, ohne_wert, Status::Solid);
    let mut vs = ValueStore::default();
    vs.insert(mit_wert, "irgendein-wert");
    // ohne_wert bleibt unbesetzt -- resolve_value liefert None.

    let matches = find_matches(&g, &vs, &fwd.pattern);
    assert_eq!(
        matches.len(),
        1,
        "nur der Knoten mit gesetztem Wert matcht Exists"
    );
    assert_eq!(matches[0][1], mit_wert);
}

#[test]
fn equals_matcht_exakten_wert() {
    let (fwd, mut g) = setup(r#"{"kind":"equals","value":"yes"}"#);
    let item = g.add_baseline("item", "Item");
    let treffer = g.add_baseline("item/treffer", "Attr");
    let fehltreffer = g.add_baseline("item/fehltreffer", "Attr");
    g.connect(item, treffer, Status::Solid);
    g.connect(item, fehltreffer, Status::Solid);
    let mut vs = ValueStore::default();
    vs.insert(treffer, "yes");
    vs.insert(fehltreffer, "no");

    let matches = find_matches(&g, &vs, &fwd.pattern);
    assert_eq!(matches.len(), 1, "nur 'yes' matcht Equals('yes')");
    assert_eq!(matches[0][1], treffer);
}

#[test]
fn prefix_matcht_nur_praefix_treffer() {
    let (fwd, mut g) = setup(r#"{"kind":"prefix","value":"get"}"#);
    let item = g.add_baseline("item", "Item");
    let get_age = g.add_baseline("item/getAge", "Attr");
    let set_age = g.add_baseline("item/setAge", "Attr");
    let is_active = g.add_baseline("item/isActive", "Attr");
    for a in [get_age, set_age, is_active] {
        g.connect(item, a, Status::Solid);
    }
    let mut vs = ValueStore::default();
    vs.insert(get_age, "getAge");
    vs.insert(set_age, "setAge");
    vs.insert(is_active, "isActive");

    let matches = find_matches(&g, &vs, &fwd.pattern);
    assert_eq!(matches.len(), 1, "nur getAge matcht Prefix('get')");
    assert_eq!(matches[0][1], get_age);
}

#[test]
fn regex_matcht_vollstaendig_nicht_teilweise() {
    // "Abstract.*" statt (wie in der Fassung der ersten Generation) "^Abstract": das Regelformat ist
    // Vollmatch, ein Anker im Muster waere verbotene Syntax (siehe
    // Dateikopf, Abweichung 1).
    let (fwd, mut g) = setup(r#"{"kind":"regex","pattern":"Abstract.*"}"#);
    let item = g.add_baseline("item", "Item");
    let abstract_base = g.add_baseline("item/AbstractBase", "Attr");
    let concrete_class = g.add_baseline("item/ConcreteClass", "Attr");
    g.connect(item, abstract_base, Status::Solid);
    g.connect(item, concrete_class, Status::Solid);
    let mut vs = ValueStore::default();
    vs.insert(abstract_base, "AbstractBase");
    vs.insert(concrete_class, "ConcreteClass");

    let matches = find_matches(&g, &vs, &fwd.pattern);
    assert_eq!(
        matches.len(),
        1,
        "nur 'AbstractBase' matcht 'Abstract.*' vollstaendig"
    );
    assert_eq!(matches[0][1], abstract_base);
}

#[test]
fn numeric_range_akzeptiert_im_bereich_lehnt_ausserhalb_und_nicht_numerisch_ab() {
    let (fwd, mut g) = setup(r#"{"kind":"numeric_range","min":0.0,"max":100.0}"#);
    let item = g.add_baseline("item", "Item");
    let im_bereich = g.add_baseline("item/r1", "Attr");
    let ausserhalb = g.add_baseline("item/r2", "Attr");
    let nicht_numerisch = g.add_baseline("item/r3", "Attr");
    for a in [im_bereich, ausserhalb, nicht_numerisch] {
        g.connect(item, a, Status::Solid);
    }
    let mut vs = ValueStore::default();
    vs.insert(im_bereich, "42.5");
    vs.insert(ausserhalb, "150.0");
    vs.insert(nicht_numerisch, "foo");

    let matches = find_matches(&g, &vs, &fwd.pattern);
    assert_eq!(matches.len(), 1, "nur r1 matcht Range [0..100]");
    assert_eq!(matches[0][1], im_bereich);
}

// ── Fehlerfall: ungueltiger regulaerer Ausdruck ─────────────────────────

/// Unbalancierte Klammer: bricht keine der verbotenen Syntaxregeln
/// (kein Anker, kein Lookaround, ...), scheitert aber an
/// `regex::Regex::new` selbst -- `PredicateError::BadRegex`. Dieselbe
/// Musterwahl wie in der geloeschten Fassung der ersten Generation
/// (`invalid_regex_reports_compile_error`), dort noch mit der Erwartung
/// "InvalidRegex" oder "Regex" in der Fehlermeldung prosaisch geprueft;
/// hier gegen den tatsaechlichen Fehlertyp.
#[test]
fn unbalancierte_klammer_wird_als_bad_regex_abgelehnt() {
    let file = RuleFile::from_json(&one_predicate_rule(r#"{"kind":"regex","pattern":"("}"#))
        .expect("parst");
    match validate(&file) {
        Err(LoadError::Predicate { rule, node, err }) => {
            assert_eq!(rule, "R");
            assert_eq!(node, "attr");
            assert!(
                matches!(err, PredicateError::BadRegex(_)),
                "erwartet BadRegex, war {err:?}"
            );
        }
        other => panic!("erwartet LoadError::Predicate, war {other:?}"),
    }
}

/// "^Abstract" war das GUELTIGE Beispiel in der geloeschten Fassung der ersten Generation
/// (Teiltreffer-Semantik: matcht jeden String, der mit "Abstract"
/// beginnt). Unter das Regelformat ist ein Anker im rohen Muster verbotene Syntax --
/// der Vollmatch-Wrapper macht ihn ueberfluessig, siehe Dateikopf,
/// Abweichung 1.
#[test]
fn regex_mit_anker_wird_als_verbotene_syntax_abgelehnt() {
    let file = RuleFile::from_json(&one_predicate_rule(
        r#"{"kind":"regex","pattern":"^Abstract"}"#,
    ))
    .expect("parst");
    match validate(&file) {
        Err(LoadError::Predicate { rule, node, err }) => {
            assert_eq!(rule, "R");
            assert_eq!(node, "attr");
            assert!(
                matches!(err, PredicateError::ForbiddenSyntax(_)),
                "erwartet ForbiddenSyntax, war {err:?}"
            );
        }
        other => panic!("erwartet LoadError::Predicate, war {other:?}"),
    }
}
