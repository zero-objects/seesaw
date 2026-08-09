//! Validation and name resolution. Errors carry their location.

use std::collections::BTreeSet;

use crate::hash::FxHashMap;
use crate::rules::format::{
    BindingDecl, NodeDecl, PredicateDecl, PrimDecl, Role, RuleDecl, RuleFile, SideDecl,
};
use crate::rules::predicate::{Predicate, PredicateError};
use crate::rules::transform::{ChainId, ChainTable, Prim};
use crate::rules::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    Version {
        found: u32,
        expected: u32,
    },
    /// Two rules of the same set carry the same name. The rule name
    /// enters the GhostId identity (`ident::konst` hashes the
    /// producing rule name into constant leaves, `graph.rs`;
    /// `DirectedRule.name` comes from `RuleDecl.name`, `rules::lower`).
    /// Two same-named rules with a constant at the same plan index
    /// under the same parent otherwise produce the same GhostId -- a
    /// name conflict is therefore a load error, not just a style
    /// break.
    DuplicateRuleName {
        name: String,
    },
    DuplicateNode {
        rule: String,
        side: String,
        name: String,
    },
    /// Fix round 2, task 1: a lint rescued from the removed positional path
    /// (the removed positional path, hard-checked there via
    /// `assert!(dup_links.is_empty())`). Two identical `(a,b)` links on
    /// the same rule side are functionally a no-op
    /// (`Graph::connect` is content-addressed/idempotent), but a
    /// regression symptom in the converter (the converter) -- without this
    /// check a bug there could produce duplicate links unnoticed.
    /// Anchored here, where it holds for EVERY loaded file, not just
    /// for a single corpus in one test.
    DuplicateLink {
        rule: String,
        side: String,
        a: String,
        b: String,
    },
    /// Like `DuplicateLink`, the same idea for `same_value_links`.
    DuplicateSameValueLink {
        rule: String,
        side: String,
        a: String,
        b: String,
    },
    UnknownNode {
        rule: String,
        side: String,
        name: String,
    },
    UnknownAnchor {
        rule: String,
        side: String,
        name: String,
    },
    SameAsOnLeft {
        rule: String,
        name: String,
    },
    UnknownSameAs {
        rule: String,
        name: String,
    },
    AmbiguousBinding {
        rule: String,
        corr: String,
    },
    EmptyBinding {
        rule: String,
        corr: String,
    },
    /// A binding whose one side is static (node name) and whose other
    /// side is dynamic (leaf type name). The format assigns no meaning
    /// to this case (no layer knows it) -- until now it only
    /// surfaced during lowering, as an untyped string error
    /// (`lower.rs`). Caught here, with rule and correspondence instead
    /// of just text.
    MixedBinding {
        rule: String,
        corr: String,
    },
    Predicate {
        rule: String,
        node: String,
        err: PredicateError,
    },
    /// A value predicate on a node that lowering CREATES in one of the
    /// two directions. It's never read there — the rule would only
    /// hold half the time. Exactly one form is allowed: an equality
    /// predicate whose value matches the `constant` of the same node
    /// (the earlier dual role of an equality constraint). Every other predicate
    /// falls here, even with `constant` set.
    PredicateOnCreatedNode {
        rule: String,
        side: String,
        node: String,
    },
    /// An equality predicate and `constant` on the same created node,
    /// but with different values: the match direction wants one, the
    /// creation direction writes the other.
    ConstantPredicateMismatch {
        rule: String,
        side: String,
        node: String,
    },
    /// A constant on a node that lowering NEVER creates (`context`,
    /// `same_as` partner, endpoint of a references corr). It would
    /// fall through in both directions.
    ConstantOnMatchedNode {
        rule: String,
        side: String,
        node: String,
    },
}

#[derive(Debug, Clone)]
pub enum BindingSource {
    Node(usize),
    LeafType(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedBinding {
    pub left: BindingSource,
    pub right: BindingSource,
    pub chain: ChainId,
}

#[derive(Debug, Clone)]
pub struct ResolvedCorr {
    pub typ: String,
    pub left: usize,
    pub right: usize,
    pub role: Role,
    pub bindings: Vec<ResolvedBinding>,
}

#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub name: String,
    pub typ: String,
    pub predicate: Option<Predicate>,
    pub context: bool,
    pub same_as: Option<usize>,
    pub constant: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedSide {
    pub anchor: usize,
    pub nodes: Vec<ResolvedNode>,
    pub links: Vec<(usize, usize)>,
    /// Value-equality constraints within the side, names resolved to
    /// positions of this side.
    pub same_value_links: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct ResolvedRule {
    pub name: String,
    pub rank: u64,
    pub left: ResolvedSide,
    pub right: ResolvedSide,
    pub corrs: Vec<ResolvedCorr>,
    /// Cross-side value joins: (position left, position right). The
    /// left name is resolved against the left, the right name against
    /// the right side index.
    pub joins: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub name: String,
    pub rules: Vec<ResolvedRule>,
    /// The chain table that `ResolvedBinding.chain` points into. It
    /// belongs to the result, not alongside it: resolving an id
    /// against a foreign table gives at best a panic, at worst
    /// silently the wrong chain.
    pub chains: ChainTable,
}

pub fn validate(file: &RuleFile) -> Result<Resolved, LoadError> {
    if file.format != FORMAT_VERSION {
        return Err(LoadError::Version {
            found: file.format,
            expected: FORMAT_VERSION,
        });
    }
    // Rule names must be unique within the set -- see the
    // documentation on `LoadError::DuplicateRuleName`. Checked before
    // the actual resolution, so a name conflict doesn't first surface
    // somewhere in the middle of `validate_rule`.
    let mut rule_names: FxHashMap<&str, ()> = FxHashMap::default();
    for r in &file.rules {
        if rule_names.insert(r.name.as_str(), ()).is_some() {
            return Err(LoadError::DuplicateRuleName {
                name: r.name.clone(),
            });
        }
    }
    let mut chains = ChainTable::default();
    let mut rules = Vec::with_capacity(file.rules.len());
    for r in &file.rules {
        rules.push(validate_rule(r, &mut chains)?);
    }
    Ok(Resolved {
        name: file.name.clone(),
        rules,
        chains,
    })
}

fn index_of(
    map: &FxHashMap<String, usize>,
    name: &str,
    rule: &str,
    side: &str,
) -> Result<usize, LoadError> {
    map.get(name)
        .copied()
        .ok_or_else(|| LoadError::UnknownNode {
            rule: rule.to_string(),
            side: side.to_string(),
            name: name.to_string(),
        })
}

/// Builds the name index of a side exactly once. The caller keeps the
/// result and hands it to both `resolve_side` (for anchor/links/
/// same_as targets) and the corr/binding resolution, instead of
/// rebuilding it at every use site.
fn side_index(
    side: &SideDecl,
    rule: &str,
    tag: &str,
) -> Result<FxHashMap<String, usize>, LoadError> {
    let mut map: FxHashMap<String, usize> = FxHashMap::default();
    for (i, n) in side.nodes.iter().enumerate() {
        if map.insert(n.name.clone(), i).is_some() {
            return Err(LoadError::DuplicateNode {
                rule: rule.to_string(),
                side: tag.to_string(),
                name: n.name.clone(),
            });
        }
    }
    Ok(map)
}

fn to_prims(decl: &[PrimDecl]) -> Vec<Prim> {
    decl.iter()
        .map(|p| match p {
            PrimDecl::Identity(_) => Prim::Identity,
            PrimDecl::Capitalize(_) => Prim::Capitalize,
            PrimDecl::Decapitalize(_) => Prim::Decapitalize,
            PrimDecl::Prefix(a) => Prim::Prefix(a.arg.clone()),
            PrimDecl::Suffix(a) => Prim::Suffix(a.arg.clone()),
        })
        .collect()
}

fn to_predicate(decl: &PredicateDecl, rule: &str, node: &str) -> Result<Predicate, LoadError> {
    let p = match decl {
        PredicateDecl::Exists(_) => Predicate::Exists,
        PredicateDecl::Equals(a) => Predicate::Equals(a.value.clone()),
        PredicateDecl::Prefix(a) => Predicate::Prefix(a.value.clone()),
        PredicateDecl::NumericRange(a) => Predicate::NumericRange {
            min: a.min,
            max: a.max,
        },
        PredicateDecl::Regex(a) => {
            Predicate::parse_regex(&a.pattern).map_err(|err| LoadError::Predicate {
                rule: rule.to_string(),
                node: node.to_string(),
                err,
            })?
        }
    };
    Ok(p)
}

/// Resolves a side against an already-built name index (see
/// `side_index`). `left_index` is set only for the right side and is
/// used for resolving `same_as` against the left side.
fn resolve_side(
    side: &SideDecl,
    index: &FxHashMap<String, usize>,
    rule: &str,
    tag: &str,
    left_index: Option<&FxHashMap<String, usize>>,
) -> Result<ResolvedSide, LoadError> {
    let anchor = index
        .get(&side.anchor)
        .copied()
        .ok_or_else(|| LoadError::UnknownAnchor {
            rule: rule.to_string(),
            side: tag.to_string(),
            name: side.anchor.clone(),
        })?;

    let mut links = Vec::with_capacity(side.links.len());
    for (a, b) in &side.links {
        links.push((
            index_of(index, a, rule, tag)?,
            index_of(index, b, rule, tag)?,
        ));
    }
    check_no_duplicate_pairs(&links, side, rule, tag, false)?;

    let mut same_value_links = Vec::with_capacity(side.same_value_links.len());
    for (a, b) in &side.same_value_links {
        same_value_links.push((
            index_of(index, a, rule, tag)?,
            index_of(index, b, rule, tag)?,
        ));
    }
    check_no_duplicate_pairs(&same_value_links, side, rule, tag, true)?;

    let mut nodes = Vec::with_capacity(side.nodes.len());
    for n in &side.nodes {
        let same_as = resolve_same_as(n, rule, tag, left_index)?;
        let predicate = match &n.predicate {
            None => None,
            Some(d) => Some(to_predicate(d, rule, &n.name)?),
        };
        nodes.push(ResolvedNode {
            name: n.name.clone(),
            typ: n.typ.clone(),
            predicate,
            context: n.context,
            same_as,
            constant: n.constant.clone(),
        });
    }
    Ok(ResolvedSide {
        anchor,
        nodes,
        links,
        same_value_links,
    })
}

/// Fix round 2, task 1: a lint rescued from
/// the removed positional path (see the `LoadError::DuplicateLink`
/// documentation), for both `links` AND `same_value_links` -- two
/// identical `(a,b)` pairs (positions, already resolved against the
/// name index) on the same side are a load error. Self-loops (`a ==
/// b`, occurring once) are NOT affected -- this check only finds
/// REPEATED pairs, regardless of whether `a == b` holds; whether a
/// single `(a, a)` is allowed is a separate question, deliberately
/// left open here.
fn check_no_duplicate_pairs(
    pairs: &[(usize, usize)],
    side: &SideDecl,
    rule: &str,
    tag: &str,
    same_value: bool,
) -> Result<(), LoadError> {
    let mut seen = BTreeSet::new();
    for &(a, b) in pairs {
        if !seen.insert((a, b)) {
            let na = side.nodes[a].name.clone();
            let nb = side.nodes[b].name.clone();
            return Err(if same_value {
                LoadError::DuplicateSameValueLink {
                    rule: rule.to_string(),
                    side: tag.to_string(),
                    a: na,
                    b: nb,
                }
            } else {
                LoadError::DuplicateLink {
                    rule: rule.to_string(),
                    side: tag.to_string(),
                    a: na,
                    b: nb,
                }
            });
        }
    }
    Ok(())
}

fn resolve_same_as(
    n: &NodeDecl,
    rule: &str,
    tag: &str,
    left_index: Option<&FxHashMap<String, usize>>,
) -> Result<Option<usize>, LoadError> {
    let Some(target) = n.same_as.as_ref() else {
        return Ok(None);
    };
    let Some(left) = left_index else {
        return Err(LoadError::SameAsOnLeft {
            rule: rule.to_string(),
            name: n.name.clone(),
        });
    };
    debug_assert_eq!(tag, "right");
    left.get(target)
        .copied()
        .map(Some)
        .ok_or_else(|| LoadError::UnknownSameAs {
            rule: rule.to_string(),
            name: target.clone(),
        })
}

fn validate_rule(r: &RuleDecl, chains: &mut ChainTable) -> Result<ResolvedRule, LoadError> {
    // Each side index is built exactly once and then reused both for
    // the side itself (anchor/links/same_as) and for the corrs/
    // bindings -- unlike the original version, which called
    // `side_index` twice per side.
    let left_index = side_index(&r.left, &r.name, "left")?;
    let left = resolve_side(&r.left, &left_index, &r.name, "left", None)?;
    let right_index = side_index(&r.right, &r.name, "right")?;
    let right = resolve_side(&r.right, &right_index, &r.name, "right", Some(&left_index))?;

    let mut corrs = Vec::with_capacity(r.corrs.len());
    for c in &r.corrs {
        let l = index_of(&left_index, &c.left, &r.name, "left")?;
        let rr = index_of(&right_index, &c.right, &r.name, "right")?;
        let mut bindings = Vec::with_capacity(c.bindings.len());
        for b in &c.bindings {
            bindings.push(resolve_binding(
                b,
                &r.name,
                &c.typ,
                &left_index,
                &right_index,
                chains,
            )?);
        }
        corrs.push(ResolvedCorr {
            typ: c.typ.clone(),
            left: l,
            right: rr,
            role: c.role,
            bindings,
        });
    }
    // Cross-side value joins: the left name belongs in the left, the
    // right one in the right side index. A typo therefore reports the
    // side it's on.
    let mut joins = Vec::with_capacity(r.joins.len());
    for (a, b) in &r.joins {
        joins.push((
            index_of(&left_index, a, &r.name, "left")?,
            index_of(&right_index, b, &r.name, "right")?,
        ));
    }

    let rule = ResolvedRule {
        name: r.name.clone(),
        rank: r.rank,
        left,
        right,
        corrs,
        joins,
    };
    check_value_roles(&rule)?;
    Ok(rule)
}

/// Does lowering create position `i` of side `side`?
///
/// Every side is the input side in exactly ONE direction. There, every
/// node is created — unless it is context (`context`), the partner of
/// a `same_as` binding, or the endpoint of a references corr. These
/// three are matched, never created (see `rules::lower`,
/// `out_ctx_pattern_pos`).
fn is_created(rule: &ResolvedRule, links: bool, i: usize) -> bool {
    let side = if links { &rule.left } else { &rule.right };
    if side.nodes[i].context {
        return false;
    }
    let same_as_partner = if links {
        rule.right.nodes.iter().any(|n| n.same_as == Some(i))
    } else {
        side.nodes[i].same_as.is_some()
    };
    if same_as_partner {
        return false;
    }
    !rule
        .corrs
        .iter()
        .any(|c| c.role == Role::References && i == if links { c.left } else { c.right })
}

/// Predicate and constant must match the node's role: a predicate is
/// only read while matching, a constant only while creating. If they
/// don't fit, the rule only holds in one direction — silently, because
/// lowering simply doesn't read the surplus part.
fn check_value_roles(rule: &ResolvedRule) -> Result<(), LoadError> {
    for (links, side) in [(true, &rule.left), (false, &rule.right)] {
        let tag = if links { "left" } else { "right" };
        for (i, n) in side.nodes.iter().enumerate() {
            let created = is_created(rule, links, i);
            if created {
                if let Some(p) = &n.predicate {
                    // Allowed is EXACTLY the earlier dual role: it accepted
                    // only an equality constraint on a created
                    // input node and used the same `value` as the
                    // constant. Everything else the earlier form
                    // rejected with CompileError.
                    match (p, &n.constant) {
                        (Predicate::Equals(v), Some(c)) if v == c => {}
                        (Predicate::Equals(_), Some(_)) => {
                            return Err(LoadError::ConstantPredicateMismatch {
                                rule: rule.name.clone(),
                                side: tag.to_string(),
                                node: n.name.clone(),
                            })
                        }
                        _ => {
                            return Err(LoadError::PredicateOnCreatedNode {
                                rule: rule.name.clone(),
                                side: tag.to_string(),
                                node: n.name.clone(),
                            })
                        }
                    }
                }
            }
            if !created && n.constant.is_some() {
                return Err(LoadError::ConstantOnMatchedNode {
                    rule: rule.name.clone(),
                    side: tag.to_string(),
                    node: n.name.clone(),
                });
            }
        }
    }
    Ok(())
}

fn resolve_binding(
    b: &BindingDecl,
    rule: &str,
    corr: &str,
    left_index: &FxHashMap<String, usize>,
    right_index: &FxHashMap<String, usize>,
    chains: &mut ChainTable,
) -> Result<ResolvedBinding, LoadError> {
    let left = pick_source(
        b.left.as_deref(),
        b.left_type.as_deref(),
        rule,
        corr,
        left_index,
        "left",
    )?;
    let right = pick_source(
        b.right.as_deref(),
        b.right_type.as_deref(),
        rule,
        corr,
        right_index,
        "right",
    )?;
    // Mixed binding: one side static (node), the other dynamic (leaf
    // type). `pick_source` only checks per side -- the mix between the
    // sides has no meaning here (see the `LoadError::MixedBinding`
    // documentation).
    if matches!(
        (&left, &right),
        (BindingSource::Node(_), BindingSource::LeafType(_))
            | (BindingSource::LeafType(_), BindingSource::Node(_))
    ) {
        return Err(LoadError::MixedBinding {
            rule: rule.to_string(),
            corr: corr.to_string(),
        });
    }
    Ok(ResolvedBinding {
        left,
        right,
        chain: chains.intern(&to_prims(&b.transform)),
    })
}

fn pick_source(
    node: Option<&str>,
    leaf_type: Option<&str>,
    rule: &str,
    corr: &str,
    index: &FxHashMap<String, usize>,
    side: &str,
) -> Result<BindingSource, LoadError> {
    match (node, leaf_type) {
        (Some(_), Some(_)) => Err(LoadError::AmbiguousBinding {
            rule: rule.to_string(),
            corr: corr.to_string(),
        }),
        (None, None) => Err(LoadError::EmptyBinding {
            rule: rule.to_string(),
            corr: corr.to_string(),
        }),
        (Some(n), None) => Ok(BindingSource::Node(index_of(index, n, rule, side)?)),
        (None, Some(t)) => Ok(BindingSource::LeafType(t.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::rules::format::RuleFile;
    use crate::rules::lower::lower_rule;
    use std::collections::BTreeSet;

    const MIN: &str = include_str!("../../tests/fixtures/rules/uml_java_min.json");

    fn load(s: &str) -> Result<Resolved, LoadError> {
        let f = RuleFile::from_json(s).expect("json");
        validate(&f)
    }

    #[test]
    fn fixture_validates_and_resolves_names() {
        let r = load(MIN).expect("must validate");
        let rule = &r.rules[0];
        assert_eq!(rule.left.anchor, 1, "cls is at position 1");
        assert_eq!(rule.left.links, vec![(0, 1), (1, 2)]);
        assert_eq!(rule.right.nodes[0].same_as, Some(0), "jmodel is model");
        assert_eq!(rule.corrs[0].left, 1);
        assert_eq!(rule.corrs[0].right, 1);
    }

    #[test]
    fn wrong_format_version_is_rejected() {
        let s = MIN.replace("\"format\": 3", "\"format\": 2");
        assert!(matches!(load(&s), Err(LoadError::Version { .. })));
    }

    #[test]
    fn unknown_node_name_reports_location() {
        let s = MIN.replace("[\"model\", \"cls\"]", "[\"typo\", \"cls\"]");
        match load(&s) {
            Err(LoadError::UnknownNode { rule, side, name }) => {
                assert_eq!(rule, "R_Class");
                assert_eq!(side, "left");
                assert_eq!(name, "typo");
            }
            other => panic!("expected UnknownNode, was {other:?}"),
        }
    }

    #[test]
    fn missing_anchor_is_rejected() {
        let s = MIN.replace("\"anchor\": \"cls\"", "\"anchor\": \"gibtsnicht\"");
        assert!(matches!(load(&s), Err(LoadError::UnknownAnchor { .. })));
    }

    #[test]
    fn same_as_on_left_side_is_forbidden() {
        let s = MIN.replace(
            "{ \"name\": \"cname\", \"type\": \"name\" }",
            "{ \"name\": \"cname\", \"type\": \"name\", \"same_as\": \"cls\" }",
        );
        assert!(matches!(load(&s), Err(LoadError::SameAsOnLeft { .. })));
    }

    #[test]
    fn binding_with_static_and_dynamic_source_is_forbidden() {
        let s = MIN.replace(
            "{ \"left\": \"cname\", \"right\": \"jname\", \"transform\": [] }",
            "{ \"left\": \"cname\", \"left_type\": \"name\", \"right\": \"jname\", \"transform\": [] }",
        );
        assert!(matches!(load(&s), Err(LoadError::AmbiguousBinding { .. })));
    }

    /// `cname` is an ordinary node of the left side, so it gets created
    /// in the backward direction. A predicate is never read there —
    /// the rule would only hold half the time.
    #[test]
    fn predicate_on_created_node_is_rejected() {
        let s = MIN.replace(
            "{ \"name\": \"cname\", \"type\": \"name\" }",
            "{ \"name\": \"cname\", \"type\": \"name\", \"predicate\": { \"kind\": \"equals\", \"value\": \"x\" } }",
        );
        match load(&s) {
            Err(LoadError::PredicateOnCreatedNode { rule, side, node }) => {
                assert_eq!(rule, "R_Class");
                assert_eq!(side, "left");
                assert_eq!(node, "cname");
            }
            other => panic!("expected PredicateOnCreatedNode, was {other:?}"),
        }
    }

    /// Builds the fixture with `constant` and/or `predicate` on
    /// `cname`, the ordinary (i.e. created) leaf of the left side.
    fn cname_with(constant: Option<&str>, predicate: Option<&str>) -> Result<Resolved, LoadError> {
        let mut fields = String::new();
        if let Some(c) = constant {
            fields.push_str(&format!(", \"constant\": \"{c}\""));
        }
        if let Some(p) = predicate {
            fields.push_str(&format!(", \"predicate\": {p}"));
        }
        load(&MIN.replace(
            "{ \"name\": \"cname\", \"type\": \"name\" }",
            &format!("{{ \"name\": \"cname\", \"type\": \"name\"{fields} }}"),
        ))
    }

    /// Allowed is EXACTLY the earlier dual role: an equality predicate with
    /// the same value as the constant. The counter-case is in the same
    /// test — if the value-equality check is dropped, this test goes
    /// red.
    #[test]
    fn predicate_with_matching_constant_is_allowed() {
        assert!(
            cname_with(
                Some("x"),
                Some("{ \"kind\": \"equals\", \"value\": \"x\" }")
            )
            .is_ok(),
            "the same value in constant and equality predicate must pass"
        );
        assert!(
            matches!(
                cname_with(
                    Some("x"),
                    Some("{ \"kind\": \"equals\", \"value\": \"y\" }")
                ),
                Err(LoadError::ConstantPredicateMismatch { .. })
            ),
            "different values must NOT pass"
        );
    }

    /// `constant` plus a non-equality predicate: exactly the cases the earlier form
    /// rejected with CompileError. The constant must not save them.
    #[test]
    fn constant_does_not_save_a_non_equality_predicate() {
        for p in [
            "{ \"kind\": \"exists\" }",
            "{ \"kind\": \"prefix\", \"value\": \"x\" }",
            "{ \"kind\": \"regex\", \"pattern\": \"x.*\" }",
            "{ \"kind\": \"numeric_range\", \"min\": 0.0, \"max\": 1.0 }",
        ] {
            match cname_with(Some("x"), Some(p)) {
                Err(LoadError::PredicateOnCreatedNode { rule, side, node }) => {
                    assert_eq!(
                        (rule.as_str(), side.as_str(), node.as_str()),
                        ("R_Class", "left", "cname")
                    );
                }
                other => panic!("{p} with a constant must be rejected, was {other:?}"),
            }
        }
    }

    /// Equality predicate WITHOUT a constant on a created node: in the
    /// creation direction the node has no value.
    #[test]
    fn equality_predicate_without_constant_is_rejected() {
        assert!(matches!(
            cname_with(None, Some("{ \"kind\": \"equals\", \"value\": \"x\" }")),
            Err(LoadError::PredicateOnCreatedNode { .. })
        ));
    }

    /// `jmodel` carries `same_as` and is therefore never created in
    /// either direction — a constant there would fall through on both
    /// sides.
    #[test]
    fn constant_on_never_created_node_is_rejected() {
        let s = MIN.replace(
            "{ \"name\": \"jmodel\", \"type\": \"Model\", \"same_as\": \"model\" }",
            "{ \"name\": \"jmodel\", \"type\": \"Model\", \"same_as\": \"model\", \"constant\": \"x\" }",
        );
        match load(&s) {
            Err(LoadError::ConstantOnMatchedNode { rule, side, node }) => {
                assert_eq!(rule, "R_Class");
                assert_eq!(side, "right");
                assert_eq!(node, "jmodel");
            }
            other => panic!("expected ConstantOnMatchedNode, was {other:?}"),
        }
    }

    /// A rule with one within-side value link per side and one
    /// cross-side join. The placeholders are inserted verbatim into
    /// the JSON lists, so a test can smuggle in an unknown name.
    fn with_value_joins(
        links_sv: &str,
        right_sv: &str,
        joins: &str,
    ) -> Result<Resolved, LoadError> {
        load(&format!(
            r#"{{"format":3,"name":"j","rules":[{{"name":"R","rank":1,
            "left":{{"anchor":"a","nodes":[
                {{"name":"a","type":"A"}},
                {{"name":"l1","type":"name"}},
                {{"name":"l2","type":"name","context":true}}],
              "links":[["a","l1"],["a","l2"]],"same_value_links":[{links_sv}]}},
            "right":{{"anchor":"b","nodes":[
                {{"name":"b","type":"B"}},
                {{"name":"r1","type":"name","context":true}}],
              "links":[["b","r1"]],"same_value_links":[{right_sv}]}},
            "corrs":[{{"type":"C","left":"a","right":"b","role":"establishes"}}],
            "joins":[{joins}]}}]}}"#
        ))
    }

    #[test]
    fn value_joins_resolve_to_positions() {
        let r = with_value_joins(r#"["l1","l2"]"#, r#"["r1","b"]"#, r#"["l2","r1"]"#)
            .expect("must validate");
        let rule = &r.rules[0];
        assert_eq!(rule.left.same_value_links, vec![(1, 2)]);
        assert_eq!(rule.right.same_value_links, vec![(1, 0)]);
        assert_eq!(rule.joins, vec![(2, 1)], "left l2=2, right r1=1");
    }

    #[test]
    fn unknown_name_in_same_value_link_reports_side() {
        match with_value_joins(r#"["l1","typo"]"#, "", "") {
            Err(LoadError::UnknownNode { rule, side, name }) => {
                assert_eq!(
                    (rule.as_str(), side.as_str(), name.as_str()),
                    ("R", "left", "typo")
                );
            }
            other => panic!("expected UnknownNode/left, was {other:?}"),
        }
        match with_value_joins("", r#"["r1","typo"]"#, "") {
            Err(LoadError::UnknownNode { rule, side, name }) => {
                assert_eq!(
                    (rule.as_str(), side.as_str(), name.as_str()),
                    ("R", "right", "typo")
                );
            }
            other => panic!("expected UnknownNode/right, was {other:?}"),
        }
    }

    /// A join has two sides: the first name belongs on the left, the
    /// second on the right. The error must name the side the name is
    /// on -- otherwise the author looks in the wrong place.
    #[test]
    fn unknown_name_in_join_reports_the_correct_side() {
        match with_value_joins("", "", r#"["typo","r1"]"#) {
            Err(LoadError::UnknownNode { rule, side, name }) => {
                assert_eq!(
                    (rule.as_str(), side.as_str(), name.as_str()),
                    ("R", "left", "typo")
                );
            }
            other => panic!("expected UnknownNode/left, was {other:?}"),
        }
        // `l2` exists -- but on the left, not the right.
        match with_value_joins("", "", r#"["l2","l2"]"#) {
            Err(LoadError::UnknownNode { rule, side, name }) => {
                assert_eq!(
                    (rule.as_str(), side.as_str(), name.as_str()),
                    ("R", "right", "l2")
                );
            }
            other => panic!("expected UnknownNode/right, was {other:?}"),
        }
    }

    #[test]
    fn forbidden_regex_syntax_fails_on_load() {
        let s = MIN.replace(
            "{ \"name\": \"cname\", \"type\": \"name\" }",
            "{ \"name\": \"cname\", \"type\": \"name\", \"predicate\": { \"kind\": \"regex\", \"pattern\": \"^a\" } }",
        );
        assert!(matches!(load(&s), Err(LoadError::Predicate { .. })));
    }

    // ── FINDING 1: rule name enters the GhostId identity ────────────

    /// Two rules of the same set with the same name must be rejected
    /// on load -- see the documentation on
    /// `LoadError::DuplicateRuleName`. In the test, the second rule is
    /// a plain copy of the first (name identical, rest irrelevant to
    /// the finding: the name alone decides the constant identity).
    #[test]
    fn duplicate_rule_name_is_rejected() {
        let mut f = RuleFile::from_json(MIN).expect("json");
        let duplicate = f.rules[0].clone();
        f.rules.push(duplicate);
        match validate(&f) {
            Err(LoadError::DuplicateRuleName { name }) => {
                assert_eq!(name, "R_Class");
            }
            other => panic!("expected DuplicateRuleName, was {other:?}"),
        }
    }

    // ── FINDING 2: mixed binding (node AND type source) ─────────────

    /// `cname` (static, node name) on the left, `jname` via
    /// `right_type` (dynamic, leaf type name) on the right -- the
    /// format assigns no meaning to this mix. Until now this only
    /// surfaced during lowering, as a string error without a location.
    #[test]
    fn mixed_binding_is_rejected_on_load() {
        let s = MIN.replace(
            "{ \"left\": \"cname\", \"right\": \"jname\", \"transform\": [] }",
            "{ \"left\": \"cname\", \"right_type\": \"name\", \"transform\": [] }",
        );
        match load(&s) {
            Err(LoadError::MixedBinding { rule, corr }) => {
                assert_eq!(rule, "R_Class");
                assert_eq!(corr, "CorrClass");
            }
            other => panic!("expected MixedBinding, was {other:?}"),
        }
    }

    /// Converse check in the other combination: dynamic left side
    /// (`left_type`), static right side (`right`).
    #[test]
    fn mixed_binding_is_also_rejected_in_reversed_combination() {
        let s = MIN.replace(
            "{ \"left\": \"cname\", \"right\": \"jname\", \"transform\": [] }",
            "{ \"left_type\": \"name\", \"right\": \"jname\", \"transform\": [] }",
        );
        assert!(matches!(load(&s), Err(LoadError::MixedBinding { .. })));
    }

    // ── FINDING 3: `is_created` (validation) held against the actual
    //    lowering ─────────────────────────────────────────────────────

    /// A rule set with a context node, a `same_as` reference, and
    /// correspondences of both roles (establishes AND references) on
    /// both sides. Every node carries a GLOBALLY unique type name, so
    /// the type set of the actually created nodes (`DirectedRule::
    /// create_nodes`) can be unambiguously traced back to side
    /// positions -- without unique types, a type-set comparison
    /// wouldn't be a reliable test at the position level.
    const CROSS: &str = r#"{"format":3,"name":"kreuz","rules":[{"name":"R","rank":1,
        "left":{"anchor":"a","nodes":[
            {"name":"a","type":"A"},
            {"name":"ctxL","type":"CtxL","context":true},
            {"name":"plainL","type":"PlainL"},
            {"name":"shared","type":"Shared"},
            {"name":"refX","type":"RefX"}],
          "links":[["a","ctxL"],["a","plainL"],["a","shared"],["a","refX"]]},
        "right":{"anchor":"b","nodes":[
            {"name":"b","type":"B"},
            {"name":"jctx","type":"JCtx","context":true},
            {"name":"plainR","type":"PlainR"},
            {"name":"rshared","type":"RShared","same_as":"shared"},
            {"name":"refY","type":"RefY"}],
          "links":[["b","jctx"],["b","plainR"],["b","rshared"],["b","refY"]]},
        "corrs":[
            {"type":"EstCorr","left":"a","right":"b","role":"establishes"},
            {"type":"RefCorr","left":"refX","right":"refY","role":"references"}]}]}"#;

    /// Holds `validate::is_created` against what `lower.rs` actually
    /// creates: for each direction, the type set of created nodes must
    /// exactly match the type set that `is_created` predicts for the
    /// respective input side. If the comparison shows a mismatch, the
    /// two paths are NOT congruent -- that would be the actual finding.
    #[test]
    fn is_created_matches_the_actual_lowering() {
        let f = RuleFile::from_json(CROSS).expect("json");
        let res = validate(&f).expect("must validate");
        let rule = &res.rules[0];

        let expected_right: BTreeSet<&str> = rule
            .right
            .nodes
            .iter()
            .enumerate()
            .filter(|&(i, _)| is_created(rule, false, i))
            .map(|(_, n)| n.typ.as_str())
            .collect();
        let expected_left: BTreeSet<&str> = rule
            .left
            .nodes
            .iter()
            .enumerate()
            .filter(|&(i, _)| is_created(rule, true, i))
            .map(|(_, n)| n.typ.as_str())
            .collect();

        // Sanity check on the fixture itself: exactly the ordinary
        // nodes (anchor + leaf) must count as created, context/
        // same_as/references-endpoint must not.
        assert_eq!(expected_right, BTreeSet::from(["B", "PlainR"]));
        assert_eq!(expected_left, BTreeSet::from(["A", "PlainL"]));

        let mut g = Graph::default();
        let [fwd, bwd] = lower_rule(&res, 0, &mut g).expect("must lower");

        // Type universe of all side nodes -- filters the corr's own
        // types (EstCorr/RefCorr) out of the created nodes, which
        // would otherwise skew the set comparison.
        let side_types: BTreeSet<&str> = rule
            .left
            .nodes
            .iter()
            .chain(rule.right.nodes.iter())
            .map(|n| n.typ.as_str())
            .collect();

        let created_forward: BTreeSet<&str> = fwd
            .create_nodes
            .iter()
            .map(|cn| cn.typ.as_str())
            .filter(|t| side_types.contains(t))
            .collect();
        let created_backward: BTreeSet<&str> = bwd
            .create_nodes
            .iter()
            .map(|cn| cn.typ.as_str())
            .filter(|t| side_types.contains(t))
            .collect();

        assert_eq!(
            created_forward, expected_right,
            "forward (right side is the input): is_created vs. the actual lowering"
        );
        assert_eq!(
            created_backward, expected_left,
            "backward (left side is the input): is_created vs. the actual lowering"
        );
    }

    // ── FINDING 4 (fix round 2, task 1): rescued duplicate-links lint ──
    //
    // Replaces the removed positional path (removed with
    // `v2_drycleaner_convert.rs`, see the commit there). Counter-check:
    // both tests below go red if `check_no_duplicate_pairs` is NOT
    // called (checked manually, see the report) -- `validate` then
    // returns `Ok` without complaint, and the `assert!(matches!`
    // fails on `Ok(_)` instead of on a named error.

    /// Two identical `(a,b)` links on the left side. Self-loop
    /// deliberately NOT part of this case (`a != b`) -- whether a
    /// single `(a, a)` is allowed is a separate question, not touched
    /// here.
    #[test]
    fn duplicate_link_is_rejected_on_load() {
        let s = r#"{"format":3,"name":"duptest","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},
                {"name":"l1","type":"L"}],
              "links":[["a","l1"],["a","l1"]]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]}}]}"#;
        match load(s) {
            Err(LoadError::DuplicateLink { rule, side, a, b }) => {
                assert_eq!(
                    (rule.as_str(), side.as_str(), a.as_str(), b.as_str()),
                    ("R", "left", "a", "l1")
                );
            }
            other => panic!("expected DuplicateLink, was {other:?}"),
        }
    }

    /// The same idea for `same_value_links`: two identical `(a,b)`
    /// value-equality constraints on the same side.
    #[test]
    fn duplicate_same_value_link_is_rejected_on_load() {
        let s = r#"{"format":3,"name":"svldup","rules":[{"name":"R","rank":1,
            "left":{"anchor":"a","nodes":[
                {"name":"a","type":"A"},
                {"name":"l1","type":"L1"},
                {"name":"l2","type":"L2"}],
              "links":[["a","l1"],["a","l2"]],
              "same_value_links":[["l1","l2"],["l1","l2"]]},
            "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]}}]}"#;
        match load(s) {
            Err(LoadError::DuplicateSameValueLink { rule, side, a, b }) => {
                assert_eq!(
                    (rule.as_str(), side.as_str(), a.as_str(), b.as_str()),
                    ("R", "left", "l1", "l2")
                );
            }
            other => panic!("expected DuplicateSameValueLink, was {other:?}"),
        }
    }
}
