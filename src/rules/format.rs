//! File structures of the rule format. Serde only, no logic.
//!
//! `deny_unknown_fields` everywhere: a field of an older format
//! (e.g. `nacs`)
//! is rejected instead of silently ignored.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFile {
    pub format: u32,
    #[serde(default)]
    pub name: String,
    pub rules: Vec<RuleDecl>,
}

impl RuleFile {
    pub fn from_json(s: &str) -> Result<RuleFile, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleDecl {
    pub name: String,
    pub rank: u64,
    #[serde(default)]
    pub documentation: Option<String>,
    pub left: SideDecl,
    pub right: SideDecl,
    #[serde(default)]
    pub corrs: Vec<CorrDecl>,
    /// Cross-side value joins: (node name on the left, node name on
    /// the right). A pure match constraint on value equality, NO
    /// value flow -- that's what bindings are for. `#[serde(default)]`
    /// so files without this field still load unchanged.
    #[serde(default)]
    pub joins: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideDecl {
    pub anchor: String,
    pub nodes: Vec<NodeDecl>,
    #[serde(default)]
    pub links: Vec<(String, String)>,
    /// Value-equality constraints WITHIN this side: (node name,
    /// node name). The cross-side case lives as `joins` on the rule.
    /// `#[serde(default)]`, same as `links`.
    #[serde(default)]
    pub same_value_links: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: String,
    #[serde(default)]
    pub predicate: Option<PredicateDecl>,
    #[serde(default)]
    pub context: bool,
    #[serde(default)]
    pub same_as: Option<String>,
    #[serde(default)]
    pub constant: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Establishes,
    References,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrDecl {
    #[serde(rename = "type")]
    pub typ: String,
    pub left: String,
    pub right: String,
    pub role: Role,
    #[serde(default)]
    pub bindings: Vec<BindingDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingDecl {
    /// Static source: node name on the left side.
    #[serde(default)]
    pub left: Option<String>,
    #[serde(default)]
    pub right: Option<String>,
    /// Dynamic source: leaf type name, looked up at the anchor.
    #[serde(default)]
    pub left_type: Option<String>,
    #[serde(default)]
    pub right_type: Option<String>,
    #[serde(default)]
    pub transform: Vec<PrimDecl>,
}

// `#[serde(deny_unknown_fields)]` directly on an internally tagged enum
// (`#[serde(tag = "...")]`) does NOT act on the variants: serde first
// reads the object for this representation into an internal value
// buffer (`Content`), looks for the tag field there, and only then
// deserializes the matching variant from that buffer -- a foreign
// field gets lost in this intermediate step instead of raising an
// error (a known serde limitation, see serde-rs/serde#1600).
// Empirically confirmed in the fix round for this file (tests below).
//
// So each variant carries its body not directly as a struct variant,
// but as its own named struct with `deny_unknown_fields` (newtype
// variant). The JSON field schema stays flat regardless, e.g.
// `{"op":"prefix","arg":"..."}` -- only the Rust-side access changes
// from `PrimDecl::Prefix { arg }` to `PrimDecl::Prefix(PrimArgArgs { arg })`.

/// Body of an argument-less variant (Identity/Capitalize/Decapitalize,
/// Exists). Wrapped as its own struct so `deny_unknown_fields`
/// applies -- a serde unit variant would otherwise let a foreign field
/// through unnoticed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

/// Body of `PrimDecl::Prefix`/`PrimDecl::Suffix`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimArgArgs {
    pub arg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PrimDecl {
    Identity(NoArgs),
    Capitalize(NoArgs),
    Decapitalize(NoArgs),
    Prefix(PrimArgArgs),
    Suffix(PrimArgArgs),
}

/// Body of `PredicateDecl::Equals`/`PredicateDecl::Prefix` -- both
/// carry a single `value` field, hence a shared struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredValueArgs {
    pub value: String,
}

/// Body of `PredicateDecl::Regex`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredRegexArgs {
    pub pattern: String,
}

/// Body of `PredicateDecl::NumericRange`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredNumericRangeArgs {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PredicateDecl {
    Exists(NoArgs),
    Equals(PredValueArgs),
    Prefix(PredValueArgs),
    Regex(PredRegexArgs),
    NumericRange(PredNumericRangeArgs),
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: &str = include_str!("../../tests/fixtures/rules/uml_java_min.json");

    #[test]
    fn fixture_parses_fully() {
        let f = RuleFile::from_json(MIN).expect("fixture must parse");
        assert_eq!(f.format, 3);
        assert_eq!(f.rules.len(), 1);
        let r = &f.rules[0];
        assert_eq!(r.name, "R_Class");
        assert_eq!(r.rank, 40);
        assert_eq!(r.left.anchor, "cls");
        assert_eq!(r.left.nodes.len(), 3);
        assert_eq!(
            r.left.links,
            vec![
                ("model".to_string(), "cls".to_string()),
                ("cls".to_string(), "cname".to_string()),
            ]
        );
        assert_eq!(r.right.nodes[0].same_as.as_deref(), Some("model"));
        assert_eq!(r.corrs[0].role, Role::Establishes);
        assert_eq!(r.corrs[0].bindings[0].left.as_deref(), Some("cname"));
    }

    /// `joins` and `same_value_links` were added later. Existing files
    /// don't know them -- the fixture stays unchanged and must still
    /// load, with empty fields. Without `#[serde(default)]`, this test
    /// goes red.
    #[test]
    fn missing_value_join_fields_stay_empty() {
        let f = RuleFile::from_json(MIN).expect("fixture without the new fields must parse");
        let r = &f.rules[0];
        assert!(r.joins.is_empty());
        assert!(r.left.same_value_links.is_empty());
        assert!(r.right.same_value_links.is_empty());
    }

    /// Converse check: if the fields are present in the file, they are
    /// read -- `deny_unknown_fields` would otherwise have rejected them.
    #[test]
    fn value_join_fields_are_read() {
        let s = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"},{"name":"a2","type":"A"}],
                "links":[],"same_value_links":[["a","a2"]]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[],"joins":[["a","b"]]}]}"#;
        let f = RuleFile::from_json(s).expect("file with the new fields must parse");
        let r = &f.rules[0];
        assert_eq!(r.joins, vec![("a".to_string(), "b".to_string())]);
        assert_eq!(
            r.left.same_value_links,
            vec![("a".to_string(), "a2".to_string())]
        );
    }

    #[test]
    fn unknown_field_is_rejected() {
        let bad = r#"{"format":3,"name":"x","rules":[],"nacs":[]}"#;
        assert!(
            RuleFile::from_json(bad).is_err(),
            "nacs must not pass through"
        );
    }

    #[test]
    fn role_is_required() {
        let bad = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[{"type":"C","left":"a","right":"b"}]}]}"#;
        assert!(
            RuleFile::from_json(bad).is_err(),
            "missing role must be an error"
        );
    }

    // PrimDecl is internally tagged (`#[serde(tag = "op")]`). An unknown
    // extra field on a variant -- whether argument-less (Identity) or
    // with fields (Prefix) -- must fail parsing, so a file doesn't pass
    // through in Rust that a Java loader with FAIL_ON_UNKNOWN_PROPERTIES
    // would reject.
    #[test]
    fn primdecl_unknown_field_is_rejected() {
        let unit_with_foreign_field = r#"{"op":"identity","unknown":true}"#;
        assert!(
            serde_json::from_str::<PrimDecl>(unit_with_foreign_field).is_err(),
            "a foreign field on an argument-less PrimDecl variant must be rejected"
        );

        let struct_with_foreign_field = r#"{"op":"prefix","arg":"get","unknown":true}"#;
        assert!(
            serde_json::from_str::<PrimDecl>(struct_with_foreign_field).is_err(),
            "a foreign field on a PrimDecl variant with fields must be rejected"
        );
    }

    // Same for PredicateDecl (`#[serde(tag = "kind")]`).
    #[test]
    fn predicatedecl_unknown_field_is_rejected() {
        let unit_with_foreign_field = r#"{"kind":"exists","unknown":true}"#;
        assert!(
            serde_json::from_str::<PredicateDecl>(unit_with_foreign_field).is_err(),
            "a foreign field on an argument-less PredicateDecl variant must be rejected"
        );

        let struct_with_foreign_field = r#"{"kind":"equals","value":"x","unknown":true}"#;
        assert!(
            serde_json::from_str::<PredicateDecl>(struct_with_foreign_field).is_err(),
            "a foreign field on a PredicateDecl variant with fields must be rejected"
        );
    }

    // Follow-up spot check (task item 4): are nested, ordinary structs
    // (no enum, no content buffering) actually strict too? This is the
    // normal case that serde's deny_unknown_fields is documented to
    // handle -- here empirically confirmed at every struct level of the
    // format instead of just assumed: RuleDecl, SideDecl, NodeDecl,
    // CorrDecl, BindingDecl (RuleFile already covered above). `Role` is
    // left out: a plain string value, not an object where a foreign
    // field could even occur. `links: Vec<(String, String)>` likewise:
    // a JSON array pair has no named fields.
    #[test]
    fn unknown_field_in_nested_struct_is_rejected() {
        // Foreign field on RuleDecl itself (one level below RuleFile).
        let rule_with_foreign_field = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "foreign":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[]}]}"#;
        assert!(
            RuleFile::from_json(rule_with_foreign_field).is_err(),
            "a foreign field on RuleDecl must be rejected"
        );

        // Foreign field on SideDecl (one level below RuleDecl).
        let side_with_foreign_field = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[],"foreign":1},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[]}]}"#;
        assert!(
            RuleFile::from_json(side_with_foreign_field).is_err(),
            "a foreign field on SideDecl must be rejected"
        );

        // Foreign field on NodeDecl (two levels below RuleDecl).
        let node_with_foreign_field = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A","foreign":1}],"links":[]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[]}]}"#;
        assert!(
            RuleFile::from_json(node_with_foreign_field).is_err(),
            "a foreign field on NodeDecl must be rejected"
        );

        // Foreign field on CorrDecl (one level below RuleDecl).
        let corr_with_foreign_field = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes","foreign":1}]}]}"#;
        assert!(
            RuleFile::from_json(corr_with_foreign_field).is_err(),
            "a foreign field on CorrDecl must be rejected"
        );

        // Foreign field on BindingDecl (two levels below RuleDecl).
        let binding_with_foreign_field = r#"{"format":3,"name":"x","rules":[{"name":"r","rank":1,
            "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
            "corrs":[{"type":"C","left":"a","right":"b","role":"establishes",
                "bindings":[{"left":"a","foreign":1}]}]}]}"#;
        assert!(
            RuleFile::from_json(binding_with_foreign_field).is_err(),
            "a foreign field on BindingDecl must be rejected"
        );
    }
}
