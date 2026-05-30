//! Rule spec deserialization.
//!
//! The structs mirror exactly the JSON format that
//! `RuleSetJsonExporter` in `seesaw-core-java` produces. Contract:
//!
//! - **snake_case** field names (the default for Rust serde).
//! - **empty arrays** instead of `null` for lists (array defaults via
//!   `#[serde(default)]`).
//! - **optional strings** as `Option<String>` — the Java exporter
//!   omits null fields, so `Option<T>` must deserialize correctly
//!   when the key is missing.
//!
//! Tests in the `tests` submodule verify compatibility with the
//! same demo rule set that the Java tests produce.

use serde::{Deserialize, Serialize};

/// Top-level spec for a `.seesaw-rules` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSetSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub rules: Vec<RuleSpec>,
}

/// A single TGG rule.
///
/// `l_pattern` and `r_pattern` are equivalent (paper bijectivity);
/// the engine decides from the match state which side has to
/// be completed.
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
    /// Negative Application Conditions (M2). If any NAC is
    /// matchable in the graph — pinned by the match bindings from
    /// `shared_with_l` as anchors — the rule candidate is
    /// discarded.
    #[serde(default)]
    pub nacs: Vec<NegativeApplicationCondition>,
}

/// Negative Application Condition — a "forbidden" sub-pattern.
///
/// `shared_with_l` lists NodePattern IDs that appear both in the
/// NAC and in the rule's `l_pattern`. During the NAC check these
/// are bound to the rule's match bindings — the rest remains
/// freely matchable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NegativeApplicationCondition {
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<NodePatternSpec>,
    #[serde(default)]
    pub edges: Vec<EdgePatternSpec>,
    /// NodePattern IDs that are shared with the rule's `l_pattern`
    /// (anchor bindings are pinned during the NAC check).
    #[serde(default)]
    pub shared_with_l: Vec<String>,
}

/// Graph match pattern (nodes + edges).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PatternSpec {
    #[serde(default)]
    pub nodes: Vec<NodePatternSpec>,
    #[serde(default)]
    pub edges: Vec<EdgePatternSpec>,
}

/// Node match with optional literal constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePatternSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub constraints: Vec<AttrConstraintSpec>,
}

/// Edge match via node IDs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePatternSpec {
    pub kind: String,
    pub source_node_id: String,
    pub target_node_id: String,
}

/// Attribute constraint with a tagged matcher (M1).
///
/// The legacy `expected_value` field was removed for the M1
/// migration — literal constraints now write
/// `{ "name": "...", "matcher": { "type": "literal", "value": "..." } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttrConstraintSpec {
    pub name: String,
    pub matcher: AttrMatcherSpec,
}

impl AttrConstraintSpec {
    /// Convenience: literal constraint.
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

/// Matcher variants for attribute constraints (M1).
///
/// Serialize/Deserialize via `#[serde(tag = "type")]` — in JSON
/// a `type` field appears together with type-specific fields:
///
/// ```json
/// { "type": "regex", "pattern": "^Abstract.*$" }
/// { "type": "prefix", "prefix": "get" }
/// { "type": "numeric_range", "min": 0.0, "max": 100.0 }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttrMatcherSpec {
    /// Exact value equality.
    Literal { value: String },
    /// Regex match (Rust `regex` crate semantics, not the same as "find").
    Regex { pattern: String },
    /// String prefix match.
    Prefix { prefix: String },
    /// String suffix match.
    Suffix { suffix: String },
    /// Numeric range (inclusive). The value is converted via
    /// `parse::<f64>()`. Parse errors → no match.
    NumericRange { min: f64, max: f64 },
}

/// Role of a correspondence link (rc7): decouples context-vs-creation
/// from the span anchor.
///
/// - `Establishes` — the link establishes a new correspondence; the
///   output-side endpoint is created in the respective direction.
/// - `References` — the link references an already existing
///   correspondence; both endpoints are context (match) in the
///   respective direction. Carries span bindings for GhostId
///   identity without creating anything.
///
/// `role == None` in [`CorrespondenceLinkSpec`] ⟹ rc6 fallback
/// (`attribute_bindings.is_empty()` ⟹ References).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorrRole {
    Establishes,
    References,
}

/// Persistent L↔R binding, optionally with attribute sync bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrespondenceLinkSpec {
    pub l_node_id: String,
    pub r_node_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub attribute_bindings: Vec<AttrBindingSpec>,
    /// rc7: explicit CorrRole. `None` ⟹ rc6 fallback from
    /// `attribute_bindings.is_empty()`.
    #[serde(default)]
    pub role: Option<CorrRole>,
}

/// Attribute sync binding between two nodes linked via
/// `CorrespondenceLinkSpec`.
///
/// The transformation semantics are documented in [`AttrTransform`];
/// the spec field only holds the string tag so that the JSON stays
/// stable and independent of engine versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttrBindingSpec {
    pub l_attr_name: String,
    pub r_attr_name: String,
    #[serde(default)]
    pub transformation: Option<String>,
}

/// Interpreted transformation function over an attribute value.
/// Invertibility is a precondition for TGG bijectivity —
/// irreversible transformations do not belong in the spec layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrTransform {
    /// No-op; take the value 1:1.
    Identity,
    /// Uppercase the first letter.
    Capitalize,
    /// Java bean getter name: `"name"` → `"getName"`. Inverse:
    /// strip the `"get"` prefix, lowercase the first letter.
    GetterName,
    /// Java bean setter name: `"name"` → `"setName"`.
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

    /// String tag of the variant (inverse of `parse`).
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

    /// Inverse function — foundation for bijectivity.
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

/// Error when parsing an unknown transformation tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTransform(pub String);

impl std::fmt::Display for UnknownTransform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unbekannte Attribut-Transformation: {}", self.0)
    }
}

impl std::error::Error for UnknownTransform {}

/// Convenience: loads a RuleSet from a JSON string.
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
    fn corr_role_deserializes_explicit_and_defaults_to_none() {
        let with_role: CorrespondenceLinkSpec = serde_json::from_str(
            r#"{"l_node_id":"c","r_node_id":"jc","role":"References","attribute_bindings":[]}"#,
        )
        .unwrap();
        assert_eq!(with_role.role, Some(CorrRole::References));

        let without: CorrespondenceLinkSpec =
            serde_json::from_str(r#"{"l_node_id":"c","r_node_id":"jc","attribute_bindings":[]}"#)
                .unwrap();
        assert_eq!(without.role, None);
    }

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
        // just any error — the concrete message depends on the serde_json version
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn demo_ruleset_json_roundtrip() {
        // This is the JSON format that `DemoRules.buildDemoRuleSet()`
        // produces on the Java side. The concrete strings here are the
        // P7 interchange format and are symmetrically validated by the
        // Java tests.
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

        // Shared anchor: the Model node with id='m' appears on both L and R
        let l = r.l_pattern.as_ref().unwrap();
        let r_pat = r.r_pattern.as_ref().unwrap();
        let l_has_m = l.nodes.iter().any(|n| n.id == "m" && n.kind == "Model");
        let r_has_m = r_pat.nodes.iter().any(|n| n.id == "m" && n.kind == "Model");
        assert!(
            l_has_m && r_has_m,
            "Model-Node muss auf beiden Seiten da sein"
        );

        // AttrBinding with identity on name ↔ name
        let cl = &r.correspondence_links[0];
        assert_eq!(cl.attribute_bindings.len(), 1);
        assert_eq!(cl.attribute_bindings[0].l_attr_name, "name");
        assert_eq!(
            cl.attribute_bindings[0].transformation.as_deref(),
            Some("identity")
        );
    }
}
