//! Golden-Erzeuger fuer den Java-Port: Identitaet abgeleiteter
//! Blaetter ueber die KETTEN-Kodierung (`Chain::ident_bytes`).
//!
//! Der Java-Port hasht dieselben Bytes; die Werte hier sind die
//! einzige Quelle dafuer — von Hand abgeschriebene Hex-Ketten waren
//! die Ursache der stillen Divergenz zwischen Rust und Java.
//!
//! Die Faelle sind bewusst ROH notiert (nicht in Normalform): die
//! Java-Seite muss dieselbe Normalisierung anwenden, sonst hashen
//! z. B. `[]` und `[Identity]` verschieden. Welche Faelle dieselbe
//! Identitaet ergeben muessen, steht in `gleich`.
//!
//! Lauf: `cargo test -p seesaw-core --test chain_ident_golden -- --ignored`

use seesaw_tgg::graph::{preview_derived_id, Graph, PlanTransform};
use seesaw_tgg::rules::transform::{Chain, Prim};
use serde_json::json;

/// Die Golden-Datei liegt in den Java-Test-Ressourcen: EINE Datei,
/// von Rust geschrieben, von beiden Seiten gelesen.
// Der Java-Baum liegt hier unter `java/`, nicht als Geschwister-Crate
// `../../seesaw-java` wie im Entwicklungsbaum.
const GOLDEN: &str = "java/src/test/resources/fixtures/chain_ident_golden.json";

const PARENT_EXTERNAL: &str = "uml:/Person";
const SOURCE_EXTERNAL: &str = "uml:/Person/name";
const TYP: &str = "getterName";

fn primj(p: &Prim) -> serde_json::Value {
    match p {
        Prim::Identity => json!({"op": "identity"}),
        Prim::Capitalize => json!({"op": "capitalize"}),
        Prim::Decapitalize => json!({"op": "decapitalize"}),
        Prim::Prefix(a) => json!({"op": "prefix", "arg": a}),
        Prim::Suffix(a) => json!({"op": "suffix", "arg": a}),
        Prim::StripPrefix(a) => json!({"op": "strip_prefix", "arg": a}),
        Prim::StripSuffix(a) => json!({"op": "strip_suffix", "arg": a}),
    }
}

fn transj(t: &PlanTransform) -> serde_json::Value {
    match t {
        // ROH, nicht normalisiert — siehe Modul-Kopf.
        PlanTransform::Chain(c) => json!(c.0.iter().map(primj).collect::<Vec<_>>()),
    }
}

fn faelle() -> Vec<(&'static str, PlanTransform)> {
    let ch = |p: Vec<Prim>| PlanTransform::Chain(Chain(p));
    let pre = |a: &str| Prim::Prefix(a.to_string());
    let suf = |a: &str| Prim::Suffix(a.to_string());
    vec![
        ("leer", ch(vec![])),
        ("identity", ch(vec![Prim::Identity])),
        (
            "identity_und_leere_affixe",
            ch(vec![
                Prim::Identity,
                pre(""),
                suf(""),
                Prim::StripPrefix(String::new()),
                Prim::StripSuffix(String::new()),
            ]),
        ),
        ("capitalize", ch(vec![Prim::Capitalize])),
        ("decapitalize", ch(vec![Prim::Decapitalize])),
        (
            "capitalize_dann_praefix",
            ch(vec![Prim::Capitalize, pre("get")]),
        ),
        (
            "praefix_dann_capitalize",
            ch(vec![pre("get"), Prim::Capitalize]),
        ),
        ("praefix_zweistufig_roh", ch(vec![pre("a"), pre("b")])),
        ("praefix_zusammengezogen", ch(vec![pre("ba")])),
        ("suffix", ch(vec![suf("_id")])),
        ("suffix_zweistufig_roh", ch(vec![suf("a"), suf("b")])),
        ("suffix_zusammengezogen", ch(vec![suf("ab")])),
        (
            "strip_praefix_dann_decapitalize",
            ch(vec![Prim::StripPrefix("get".into()), Prim::Decapitalize]),
        ),
        ("strip_suffix", ch(vec![Prim::StripSuffix("_id".into())])),
        ("nicht_ascii", ch(vec![pre("Ärger→"), suf("Größe_ß")])),
        (
            "nicht_ascii_zusammengezogen_roh",
            ch(vec![pre("ß"), pre("Ä")]),
        ),
        ("nicht_ascii_zusammengezogen", ch(vec![pre("Äß")])),
    ]
}

/// Die zusammengesetzten Ketten, die frueher als benanntes Vokabular im
/// Kern standen (`getter_name`, `cmd_prefix` und so fort). Die Namen
/// sind aus dem Kern verschwunden (Spec §5), die KETTEN gibt es
/// weiterhin, und ihre Identitaet muss in beiden Sprachen dieselbe
/// sein. Diese Faelle nageln das fest.
fn vokabular() -> Vec<(&'static str, PlanTransform)> {
    let c = |p: Vec<Prim>| PlanTransform::Chain(Chain(p));
    let pre = |a: &str| Prim::Prefix(a.to_string());
    let strip = |a: &str| Prim::StripPrefix(a.to_string());
    vec![
        ("identity", c(vec![])),
        ("capitalize", c(vec![Prim::Capitalize])),
        ("decapitalize", c(vec![Prim::Decapitalize])),
        ("getter_name", c(vec![Prim::Capitalize, pre("get")])),
        ("getter_strip", c(vec![strip("get"), Prim::Decapitalize])),
        ("setter_name", c(vec![Prim::Capitalize, pre("set")])),
        ("setter_strip", c(vec![strip("set"), Prim::Decapitalize])),
        ("cmd_prefix", c(vec![pre("C ")])),
        ("cmd_strip", c(vec![strip("C ")])),
        ("tm_prefix", c(vec![pre("T ")])),
        ("tm_strip", c(vec![strip("T ")])),
    ]
}

/// Faelle, die dieselbe Identitaet ergeben MUESSEN (Normalform).
const GLEICH: [[&str; 2]; 5] = [
    ["leer", "identity"],
    ["leer", "identity_und_leere_affixe"],
    ["praefix_zweistufig_roh", "praefix_zusammengezogen"],
    ["suffix_zweistufig_roh", "suffix_zusammengezogen"],
    [
        "nicht_ascii_zusammengezogen_roh",
        "nicht_ascii_zusammengezogen",
    ],
];

#[test]
#[ignore = "manuell: schreibt das Java-Golden fuer die Ketten-Identitaet"]
fn schreibt_java_golden() {
    let mut g = Graph::new();
    let parent = g.add_baseline(PARENT_EXTERNAL, "Class");
    let source = g.add_baseline(SOURCE_EXTERNAL, "name");

    let mut cases = Vec::new();
    for (name, t) in faelle() {
        cases.push(json!({
            "name": name,
            "transform": transj(&t),
            "id": preview_derived_id(&parent, TYP, &source, &t).hex(),
        }));
    }

    let mut vokabeln = Vec::new();
    for (name, plan) in vokabular() {
        vokabeln.push(json!({
            "name": name,
            "transform": transj(&plan),
            "id": preview_derived_id(&parent, TYP, &source, &plan).hex(),
        }));
    }

    let out = json!({
        "parent_external": PARENT_EXTERNAL,
        "parent": parent.hex(),
        "source_external": SOURCE_EXTERNAL,
        "source": source.hex(),
        "typ": TYP,
        "cases": cases,
        "gleich": GLEICH,
        "vokabular": vokabeln,
    });
    let path = std::path::Path::new(GOLDEN);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
    eprintln!("Ketten-Golden geschrieben nach {GOLDEN}");
}

/// Die Golden-Datei ist auch fuer die RUST-Seite verbindlich: aendert
/// sich `Chain::ident_bytes`, `Chain::normalized` oder
/// `Transform::plan`, faellt dieser Test, ohne dass jemand vorher den
/// Export laufen laesst. Ohne ihn waere das Golden ein Golden, das
/// niemand liest — dieselbe Luecke, die die Divergenz erzeugt hat,
/// eine Ebene hoeher.
#[test]
fn golden_datei_deckt_sich_mit_der_rust_seite() {
    let text = std::fs::read_to_string(GOLDEN)
        .unwrap_or_else(|e| panic!("Golden {GOLDEN} nicht lesbar: {e}"));
    let g: serde_json::Value = serde_json::from_str(&text).expect("Golden ist kein JSON");

    let mut graph = Graph::new();
    let parent = graph.add_baseline(PARENT_EXTERNAL, "Class");
    let source = graph.add_baseline(SOURCE_EXTERNAL, "name");
    assert_eq!(g["parent_external"], PARENT_EXTERNAL);
    assert_eq!(g["source_external"], SOURCE_EXTERNAL);
    assert_eq!(g["typ"], TYP);
    assert_eq!(g["parent"], parent.hex(), "Anker-Id");
    assert_eq!(g["source"], source.hex(), "Quell-Id");

    // Beide Bloecke: Namen und Reihenfolge wie im Code, die notierte
    // Kette wie `transj` sie schreibt, die Id wie `ident::derived` sie
    // rechnet.
    let bloecke: [(&str, Vec<(&str, PlanTransform)>); 2] =
        [("cases", faelle()), ("vokabular", vokabular())];
    for (block, erwartet) in bloecke {
        let eintraege = g[block]
            .as_array()
            .unwrap_or_else(|| panic!("{block} fehlt"));
        assert_eq!(
            eintraege.len(),
            erwartet.len(),
            "{block}: Golden ist nicht mehr aktuell (Export fehlt)"
        );
        for (eintrag, (name, t)) in eintraege.iter().zip(erwartet) {
            assert_eq!(eintrag["name"], name, "{block}: Reihenfolge/Name");
            assert_eq!(eintrag["transform"], transj(&t), "{block}/{name}: Kette");
            assert_eq!(
                eintrag["id"],
                preview_derived_id(&parent, TYP, &source, &t).hex(),
                "{block}/{name}: Identitaet"
            );
        }
    }

    let gleich = g["gleich"].as_array().expect("gleich fehlt");
    assert_eq!(gleich.len(), GLEICH.len(), "gleich-Paare");
    for (paar, [a, b]) in gleich.iter().zip(GLEICH) {
        assert_eq!(paar[0], a);
        assert_eq!(paar[1], b);
    }
}

/// Laeuft im normalen Testlauf mit: die `gleich`-Paare duerfen nicht
/// auseinanderlaufen (sonst waere das Golden selbst inkonsistent).
#[test]
fn normalform_faelle_teilen_die_identitaet() {
    let mut g = Graph::new();
    let parent = g.add_baseline(PARENT_EXTERNAL, "Class");
    let source = g.add_baseline(SOURCE_EXTERNAL, "name");
    let ids: Vec<(String, String)> = faelle()
        .iter()
        .map(|(n, t)| {
            (
                n.to_string(),
                preview_derived_id(&parent, TYP, &source, t).hex(),
            )
        })
        .collect();
    let id_of = |name: &str| {
        ids.iter()
            .find(|(n, _)| n == name)
            .map(|(_, h)| h.clone())
            .unwrap_or_else(|| panic!("Fall {name} fehlt"))
    };
    for [a, b] in GLEICH {
        assert_eq!(id_of(a), id_of(b), "{a} und {b} muessen dieselbe Id haben");
    }
    // Und die Reihenfolge bleibt unterscheidbar.
    assert_ne!(
        id_of("capitalize_dann_praefix"),
        id_of("praefix_dann_capitalize")
    );
}
