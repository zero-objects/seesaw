//! Rule-Spec-Deserialisierung.
//!
//! Die Structs spiegeln exakt das JSON-Format, das
//! `RuleSetJsonExporter` in `seesaw-core-java` produziert. Kontrakt:
//!
//! - **snake_case**-Feldnamen (Standard in Rust-serde).
//! - **leere Arrays** statt `null` für Listen (Array-Defaults via
//!   `#[serde(default)]`).
//! - **optionale Strings** als `Option<String>` — der Java-Exporter
//!   lässt Null-Felder weg, daher muss `Option<T>` korrekt
//!   deserialisieren wenn der Key fehlt.
//!
//! Tests im Submodul `tests` verifizieren die Kompatibilität mit
//! demselben Demo-Rule-Set, das die Java-Tests produzieren.

use serde::{Deserialize, Serialize};

/// Toplevel-Spec für eine `.seesaw-rules`-Datei.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSetSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub rules: Vec<RuleSpec>,
}

/// Einzelne TGG-Regel.
///
/// `l_pattern` und `r_pattern` sind gleichwertig (Paper-Bijektivität);
/// die Engine entscheidet aus dem Match-Zustand, welche Seite zu
/// komplettieren ist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSpec {
    pub name: String,
    pub rank: i32,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub l_pattern: Option<PatternSpec>,
    #[serde(default)]
    pub r_pattern: Option<PatternSpec>,
    #[serde(default)]
    pub correspondence_links: Vec<CorrespondenceLinkSpec>,
    /// Negative Application Conditions (M2). Wenn irgendeine NAC
    /// im Graph matchbar ist — mit den Match-Bindings aus
    /// `shared_with_l` als Anker fixiert —, wird der Rule-Kandidat
    /// verworfen.
    #[serde(default)]
    pub nacs: Vec<NegativeApplicationCondition>,
}

/// Negative Application Condition — ein "verbotenes" Sub-Pattern.
///
/// `shared_with_l` listet NodePattern-IDs, die sowohl im NAC als
/// auch im `l_pattern` der Rule vorkommen. Beim NAC-Check werden
/// diese an die Match-Bindings der Rule gekoppelt — der Rest
/// bleibt frei matchbar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NegativeApplicationCondition {
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<NodePatternSpec>,
    #[serde(default)]
    pub edges: Vec<EdgePatternSpec>,
    /// NodePattern-IDs, die mit dem `l_pattern` der Rule geteilt
    /// sind (Anker-Bindings werden beim NAC-Check fixiert).
    #[serde(default)]
    pub shared_with_l: Vec<String>,
}

/// Graph-Match-Muster (Knoten + Kanten).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PatternSpec {
    #[serde(default)]
    pub nodes: Vec<NodePatternSpec>,
    #[serde(default)]
    pub edges: Vec<EdgePatternSpec>,
}

/// Knoten-Match mit optionalen Literal-Constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePatternSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub constraints: Vec<AttrConstraintSpec>,
}

/// Kanten-Match via Knoten-IDs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePatternSpec {
    pub kind: String,
    pub source_node_id: String,
    pub target_node_id: String,
}

/// Attribut-Bedingung mit getaggtem Matcher (M1).
///
/// Legacy-Feld `expected_value` wurde zur M1-Umstellung entfernt —
/// Literal-Bedingungen schreiben jetzt
/// `{ "name": "...", "matcher": { "type": "literal", "value": "..." } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttrConstraintSpec {
    pub name: String,
    pub matcher: AttrMatcherSpec,
}

impl AttrConstraintSpec {
    /// Konvenienz: Literal-Constraint.
    pub fn literal(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            matcher: AttrMatcherSpec::Literal {
                value: value.into(),
            },
        }
    }

    pub fn regex(name: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            matcher: AttrMatcherSpec::Regex {
                pattern: pattern.into(),
            },
        }
    }

    pub fn prefix(name: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            matcher: AttrMatcherSpec::Prefix {
                prefix: prefix.into(),
            },
        }
    }

    pub fn suffix(name: impl Into<String>, suffix: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            matcher: AttrMatcherSpec::Suffix {
                suffix: suffix.into(),
            },
        }
    }

    pub fn numeric_range(name: impl Into<String>, min: f64, max: f64) -> Self {
        Self {
            name: name.into(),
            matcher: AttrMatcherSpec::NumericRange { min, max },
        }
    }
}

/// Matcher-Varianten für Attribut-Constraints (M1).
///
/// Serialize/Deserialize via `#[serde(tag = "type")]` — in JSON
/// erscheint ein `type`-Feld + Typ-spezifische Felder:
///
/// ```json
/// { "type": "regex", "pattern": "^Abstract.*$" }
/// { "type": "prefix", "prefix": "get" }
/// { "type": "numeric_range", "min": 0.0, "max": 100.0 }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttrMatcherSpec {
    /// Exakte Wert-Gleichheit.
    Literal { value: String },
    /// Regex-Match (Rust `regex`-Crate-Semantik, nicht-gleich "find").
    Regex { pattern: String },
    /// String-Präfix-Match.
    Prefix { prefix: String },
    /// String-Suffix-Match.
    Suffix { suffix: String },
    /// Numerischer Bereich (inklusiv). Wert wird via `parse::<f64>()`
    /// konvertiert. Parse-Fehler → kein Match.
    NumericRange { min: f64, max: f64 },
}

/// Persistente L↔R-Bindung, optional mit Attribut-Sync-Bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrespondenceLinkSpec {
    pub l_node_id: String,
    pub r_node_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub attribute_bindings: Vec<AttrBindingSpec>,
}

/// Attribut-Sync-Bindung zwischen zwei über `CorrespondenceLinkSpec`
/// verknüpften Knoten.
///
/// Die Transformations-Semantik ist in [`AttrTransform`] dokumentiert;
/// das Spec-Feld hält nur den String-Tag, damit das JSON stabil und
/// engine-versions-unabhängig bleibt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttrBindingSpec {
    pub l_attr_name: String,
    pub r_attr_name: String,
    #[serde(default)]
    pub transformation: Option<String>,
}

/// Interpretierte Transformations-Funktion über einem Attribut-Wert.
/// Die Umkehrbarkeit ist Voraussetzung für TGG-Bijektivität — irre-
/// versible Transformationen gehören nicht in den Spec-Layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrTransform {
    /// Kein-Op; Wert 1:1 übernehmen.
    Identity,
    /// Erster Buchstabe großschreiben.
    Capitalize,
    /// Java-Bean-Getter-Name: `"name"` → `"getName"`. Umkehrung:
    /// strippe `"get"`-Präfix, erster Buchstabe klein.
    GetterName,
    /// Java-Bean-Setter-Name: `"name"` → `"setName"`.
    SetterName,
}

impl AttrTransform {
    pub fn parse(tag: Option<&str>) -> Result<Self, UnknownTransform> {
        match tag {
            None | Some("") | Some("identity") => Ok(Self::Identity),
            Some("capitalize") => Ok(Self::Capitalize),
            Some("getter_name") => Ok(Self::GetterName),
            Some("setter_name") => Ok(Self::SetterName),
            Some(other) => Err(UnknownTransform(other.to_string())),
        }
    }

    /// String-Tag der Variante (Inverses zu `parse`).
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Capitalize => "capitalize",
            Self::GetterName => "getter_name",
            Self::SetterName => "setter_name",
        }
    }

    pub fn apply(&self, value: &str) -> String {
        match self {
            Self::Identity => value.to_string(),
            Self::Capitalize => capitalize_first(value),
            Self::GetterName => format!("get{}", capitalize_first(value)),
            Self::SetterName => format!("set{}", capitalize_first(value)),
        }
    }

    /// Umkehrfunktion — Grundlage für Bijektivität.
    pub fn apply_inverse(&self, value: &str) -> String {
        match self {
            Self::Identity => value.to_string(),
            Self::Capitalize => lowercase_first(value),
            Self::GetterName => lowercase_first(value.strip_prefix("get").unwrap_or(value)),
            Self::SetterName => lowercase_first(value.strip_prefix("set").unwrap_or(value)),
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Fehler beim Parsen eines unbekannten Transformations-Tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTransform(pub String);

impl std::fmt::Display for UnknownTransform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unbekannte Attribut-Transformation: {}", self.0)
    }
}

impl std::error::Error for UnknownTransform {}

/// Convenience: lädt ein RuleSet aus JSON-String.
pub fn parse_ruleset(json: &str) -> Result<RuleSetSpec, serde_json::Error> {
    serde_json::from_str(json)
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_ruleset_deserialisiert() {
        let json = r#"{ "name": "rs", "rules": [] }"#;
        let rs = parse_ruleset(json).unwrap();
        assert_eq!(rs.name.as_deref(), Some("rs"));
        assert_eq!(rs.rules.len(), 0);
    }

    #[test]
    fn fehlende_optionals_werden_zu_none_und_leeren_vecs() {
        let rs = parse_ruleset("{}").unwrap();
        assert_eq!(rs.name, None);
        assert_eq!(rs.description, None);
        assert!(rs.rules.is_empty());
    }

    #[test]
    fn rule_mit_patterns_und_kanten() {
        let json = r#"{
            "name": "demo",
            "rules": [{
                "name": "R_Test", "rank": 5,
                "l_pattern": {
                    "nodes": [
                        {"id": "c", "kind": "Class", "constraints": []},
                        {"id": "a", "kind": "Attribute", "constraints": []}
                    ],
                    "edges": [
                        {"kind": "attributes", "source_node_id": "c", "target_node_id": "a"}
                    ]
                },
                "r_pattern": {
                    "nodes": [{"id": "jc", "kind": "JavaClass", "constraints": []}],
                    "edges": []
                },
                "correspondence_links": []
            }]
        }"#;
        let rs = parse_ruleset(json).unwrap();
        assert_eq!(rs.rules.len(), 1);
        let r = &rs.rules[0];
        assert_eq!(r.rank, 5);
        let l = r.l_pattern.as_ref().unwrap();
        assert_eq!(l.nodes.len(), 2);
        assert_eq!(l.edges.len(), 1);
        assert_eq!(l.edges[0].kind, "attributes");
    }

    #[test]
    fn correspondence_link_mit_attribute_bindings() {
        let json = r#"{
            "rules": [{
                "name": "R_Bind", "rank": 1,
                "l_pattern": {"nodes": [], "edges": []},
                "r_pattern": {"nodes": [], "edges": []},
                "correspondence_links": [{
                    "l_node_id": "a", "r_node_id": "b", "kind": "CorrAB",
                    "attribute_bindings": [
                        {"l_attr_name": "name", "r_attr_name": "name",
                         "transformation": "identity"},
                        {"l_attr_name": "name", "r_attr_name": "Name",
                         "transformation": "capitalize"}
                    ]
                }]
            }]
        }"#;
        let rs = parse_ruleset(json).unwrap();
        let cl = &rs.rules[0].correspondence_links[0];
        assert_eq!(cl.l_node_id, "a");
        assert_eq!(cl.r_node_id, "b");
        assert_eq!(cl.kind.as_deref(), Some("CorrAB"));
        assert_eq!(cl.attribute_bindings.len(), 2);
        assert_eq!(
            cl.attribute_bindings[1].transformation.as_deref(),
            Some("capitalize")
        );
    }

    #[test]
    fn attr_transform_parse_und_apply() {
        let identity = AttrTransform::parse(Some("identity")).unwrap();
        assert_eq!(identity, AttrTransform::Identity);
        assert_eq!(identity.apply("hello"), "hello");

        let cap = AttrTransform::parse(Some("capitalize")).unwrap();
        assert_eq!(cap, AttrTransform::Capitalize);
        assert_eq!(cap.apply("hello"), "Hello");
        assert_eq!(cap.apply(""), "");
    }

    #[test]
    fn attr_transform_fehlender_und_leerer_tag_ist_identity() {
        assert_eq!(AttrTransform::parse(None).unwrap(), AttrTransform::Identity);
        assert_eq!(
            AttrTransform::parse(Some("")).unwrap(),
            AttrTransform::Identity
        );
    }

    #[test]
    fn attr_transform_unbekannter_tag_ist_fehler() {
        let err = AttrTransform::parse(Some("sha256")).unwrap_err();
        assert_eq!(err.0, "sha256");
    }

    #[test]
    fn attr_transform_inverse_kehrt_capitalize_um() {
        let cap = AttrTransform::Capitalize;
        assert_eq!(cap.apply_inverse("Hello"), "hello");
        assert_eq!(cap.apply_inverse("N"), "n");
    }

    #[test]
    fn malformed_json_schlaegt_fehl() {
        let err = parse_ruleset("{not json").unwrap_err();
        // nur irgendein Fehler — konkrete Meldung hängt von serde_json-Version ab
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn demo_ruleset_json_roundtrip() {
        // Das ist das JSON-Format, das `DemoRules.buildDemoRuleSet()` auf
        // der Java-Seite produziert. Die konkreten Strings hier sind das
        // P7-Austauschformat und werden von den Java-Tests symmetrisch
        // validiert.
        let json = r#"{
            "name": "seesaw-demo",
            "rules": [
                {
                    "name": "R_Class", "rank": 40,
                    "l_pattern": {
                        "nodes": [
                            {"id":"m","kind":"Model","constraints":[]},
                            {"id":"c","kind":"Class","constraints":[]}
                        ],
                        "edges": [
                            {"kind":"classes","source_node_id":"m","target_node_id":"c"}
                        ]
                    },
                    "r_pattern": {
                        "nodes": [
                            {"id":"m","kind":"Model","constraints":[]},
                            {"id":"jc","kind":"JavaClass","constraints":[]}
                        ],
                        "edges": [
                            {"kind":"javaClasses","source_node_id":"m","target_node_id":"jc"}
                        ]
                    },
                    "correspondence_links": [
                        {"l_node_id":"c","r_node_id":"jc","kind":"CorrClass",
                         "attribute_bindings":[
                             {"l_attr_name":"name","r_attr_name":"name","transformation":"identity"}
                         ]}
                    ]
                }
            ]
        }"#;
        let rs = parse_ruleset(json).unwrap();
        assert_eq!(rs.rules.len(), 1);
        let r = &rs.rules[0];
        assert_eq!(r.name, "R_Class");
        assert_eq!(r.rank, 40);

        // Shared anchor: der Model-Knoten mit id='m' erscheint auf L und R
        let l = r.l_pattern.as_ref().unwrap();
        let r_pat = r.r_pattern.as_ref().unwrap();
        let l_has_m = l.nodes.iter().any(|n| n.id == "m" && n.kind == "Model");
        let r_has_m = r_pat.nodes.iter().any(|n| n.id == "m" && n.kind == "Model");
        assert!(
            l_has_m && r_has_m,
            "Model-Node muss auf beiden Seiten da sein"
        );

        // AttrBinding mit identity auf name ↔ name
        let cl = &r.correspondence_links[0];
        assert_eq!(cl.attribute_bindings.len(), 1);
        assert_eq!(cl.attribute_bindings[0].l_attr_name, "name");
        assert_eq!(
            cl.attribute_bindings[0].transformation.as_deref(),
            Some("identity")
        );
    }
}
