//! Rule-Compiler — Zwischen-Repräsentation zwischen `RuleSpec`
//! (deklarativ, bidirektional) und der konkreten Engine-Rule-
//! Instantiierung.
//!
//! Der Compiler macht zwei Dinge, die man NICHT mit Closure-Magie
//! vermischen sollte:
//!
//! 1. **Statische Analyse**: welche Pattern-Knoten sind Shared-Anchor
//!    (auf L und R mit selber ID und selbem Kind)? Welche
//!    CorrespondenceLinks sind Kontext (alle beteiligten Knoten sind
//!    schon gebunden) vs. zu erzeugen (mindestens eine Seite ist neu)?
//! 2. **Produktions-Plan**: welche Knoten, Kanten, Corrs müssen beim
//!    Match erzeugt werden; welche Attribut-Propagationen greifen.
//!
//! Der resultierende [`CompiledRuleSpec`] ist reine Datenstruktur, frei
//! von Graph-Topology-Entscheidungen. Die finale Abbildung auf
//! `Box<dyn Rule>` (mit konkreter Corr-Layout-Semantik) ist Aufgabe
//! eines Folge-Schritts — siehe Paper-Kapitel „Von abstrakter Rule-
//! Spec zur Engine-Operationalisierung".

use std::collections::{HashMap, HashSet};
use thiserror::Error;

use super::spec::{
    AttrMatcherSpec, AttrTransform, CorrespondenceLinkSpec, PatternSpec, RuleSpec, UnknownTransform,
};

// ══════════════════════════════════════════════════════════════════════
// CompiledRuleSpec — das Ergebnis der Kompilierung
// ══════════════════════════════════════════════════════════════════════

/// Statisch analysiertes Abbild einer [`RuleSpec`].
#[derive(Debug, Clone)]
pub struct CompiledRuleSpec {
    pub name: String,
    pub rank: i32,
    pub documentation: Option<String>,
    pub match_plan: MatchPlan,
    pub creation_plan: CreationPlan,
    pub propagation_plan: Vec<AttrPropagation>,
    /// Kompilierte NACs (M2). Werden vom Matcher nach dem
    /// Haupt-Match geprüft.
    pub nacs: Vec<CompiledNac>,
}

/// Kompilierte Negative Application Condition.
#[derive(Debug, Clone)]
pub struct CompiledNac {
    pub name: String,
    /// Alle NodePatterns im NAC — in kanonischer Reihenfolge.
    pub nodes: Vec<MatchNode>,
    /// Edge-Constraints im NAC.
    pub edges: Vec<MatchEdge>,
    /// Attribut-Constraints (falls vorhanden).
    pub constraints: Vec<MatchConstraint>,
    /// NodePattern-IDs, die an das L-Match gekoppelt sind.
    /// Werden beim NAC-Check per Var-Name aus dem Haupt-Match
    /// fixiert.
    pub shared_with_l: Vec<String>,
}

/// Match-Teil: was der Matcher finden muss, damit die Rule anwendbar ist.
#[derive(Debug, Clone, Default)]
pub struct MatchPlan {
    /// Knoten, die der Matcher findet — in kanonischer Reihenfolge
    /// (lPattern zuerst, dann rPattern, Duplikate via Shared-Anchor
    /// entfernt).
    pub nodes: Vec<MatchNode>,
    /// Kanten-Constraints zwischen gematchten Knoten.
    pub edges: Vec<MatchEdge>,
    /// Literal-Attribut-Constraints.
    pub constraints: Vec<MatchConstraint>,
    /// Correspondence-Knoten, die als Kontext (via bestehende Corr-
    /// Edges im Graph) vorausgesetzt werden.
    pub context_correspondences: Vec<CorrespondenceLinkSpec>,
}

/// Creation-Teil: was die Rule erzeugt.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreationPlan {
    /// R-Pattern-Knoten, die nicht bereits im Match gebunden sind
    /// (Shared-Anchors und Context-R-Knoten ausgenommen).
    pub nodes_to_create: Vec<MatchNode>,
    /// R-Pattern-Kanten, deren Endpunkte neu oder kontext-
    /// gebunden sind.
    pub edges_to_create: Vec<MatchEdge>,
    /// Correspondence-Links, die diese Rule neu etabliert — nicht
    /// identisch zu `context_correspondences` im MatchPlan.
    pub correspondences_to_create: Vec<CorrespondenceLinkSpec>,
    /// R-Pattern-Literal-Constraints auf **Kontext-Knoten** (Shared-
    /// Anchor oder R-only-mit-context-corr): das Attribut soll auf
    /// einem bereits existierenden Knoten geändert werden. Wird bei
    /// Rule-Anwendung als `Op::SetAttr` materialisiert.
    ///
    /// rc6 (B6): bis rc5 landeten R-Literale auf ALLEN R-Knoten in
    /// dieser Liste — auch auf R-only-Creation-Knoten. Das führte zu
    /// Oszillation, wenn zwei Rules strukturell-identische Creation-
    /// Knoten (gleicher corr-parent, gleiche attribute_bindings →
    /// gleicher GhostId-Hash) produzieren wollten, deren R-Literale
    /// aber unterschiedliche Werte für dasselbe Attribut hatten — die
    /// SetAttrs überschrieben sich gegenseitig in jeder Iteration.
    /// Pendant zum Edge-Identitäts-Verhalten (B4).
    pub attrs_to_set: Vec<AttrToSet>,
    /// R-Pattern-Literal-Constraints auf **R-only-Creation-Knoten**:
    /// das Attribut ist *Teil der Identität* des neu erzeugten
    /// Knotens und fließt in den GhostId-Hash via `collect_r_attrs`
    /// (siehe Modul [`mod@crate::rule::instantiate`]). Dadurch
    /// produzieren zwei Rules mit verschiedenen Literal-Werten *zwei
    /// verschiedene* Creation-Knoten statt sich auf demselben Knoten
    /// zu überschreiben.
    pub creation_attrs: Vec<CreationAttr>,
}

/// SetAttr-Intent aus einer Rule: setzt ein Attribut auf einem im
/// Match gebundenen (Kontext-) Knoten auf einen Literal-Wert. Wird im
/// Modul [`mod@crate::rule::instantiate`] zu `Op::SetAttr`
/// materialisiert.
#[derive(Debug, Clone, PartialEq)]
pub struct AttrToSet {
    /// NodePattern-ID — muss im MatchPlan gebunden sein.
    pub node_var: String,
    /// Attribut-Name.
    pub attr_name: String,
    /// Ziel-Wert.
    pub value: String,
}

/// Identitäts-Attribut für einen R-only-Creation-Knoten: aus einem
/// R-Pattern-Literal-Constraint abgeleitet, fließt in die Ghost-ID
/// des neu erzeugten Knotens ein. Verhindert die rc5-Oszillation
/// zwischen Rules, die strukturell-identische Creation-Knoten mit
/// verschiedenen Literal-Werten erzeugen würden.
#[derive(Debug, Clone, PartialEq)]
pub struct CreationAttr {
    /// NodePattern-ID des Creation-Knotens — muss in
    /// `creation_plan.nodes_to_create` sein.
    pub node_var: String,
    /// Attribut-Name.
    pub attr_name: String,
    /// Literal-Wert.
    pub value: String,
}

/// Propagations-Plan: welche Attribute wohin mit welcher Transformation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrPropagation {
    pub source_node_var: String,
    pub source_attr: String,
    pub target_node_var: String,
    pub target_attr: String,
    pub transform: AttrTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchNode {
    pub var: String,
    pub kind: String,
    /// Welche Seite hat diesen Knoten ins Pattern gebracht.
    pub origin: NodeOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeOrigin {
    /// Nur im L-Pattern.
    LOnly,
    /// Nur im R-Pattern.
    ROnly,
    /// In beiden mit identischer ID+Kind — shared anchor.
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEdge {
    pub source_var: String,
    pub target_var: String,
    pub kind: String,
    pub side: EdgeSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSide {
    L,
    R,
}

#[derive(Debug, Clone)]
pub struct MatchConstraint {
    pub node_var: String,
    pub attr_name: String,
    pub predicate: crate::engine::AttrPredicate,
}

/// Kompiliert einen `AttrMatcherSpec` zu einem engine-nativen
/// `AttrPredicate`. Regex-Syntax-Fehler werden als
/// `CompileError::InvalidRegex` hochgereicht.
pub fn compile_matcher(
    spec: &super::spec::AttrMatcherSpec,
) -> Result<crate::engine::AttrPredicate, CompileError> {
    use super::spec::AttrMatcherSpec;
    Ok(match spec {
        AttrMatcherSpec::Literal { value } => crate::engine::AttrPredicate::Equals(value.clone()),
        AttrMatcherSpec::Regex { pattern } => {
            let re = regex::Regex::new(pattern).map_err(|e| CompileError::InvalidRegex {
                pattern: pattern.clone(),
                reason: e.to_string(),
            })?;
            crate::engine::AttrPredicate::Regex(re)
        }
        AttrMatcherSpec::Prefix { prefix } => crate::engine::AttrPredicate::Prefix(prefix.clone()),
        AttrMatcherSpec::Suffix { suffix } => crate::engine::AttrPredicate::Suffix(suffix.clone()),
        AttrMatcherSpec::NumericRange { min, max } => crate::engine::AttrPredicate::NumericRange {
            min: *min,
            max: *max,
        },
    })
}

// ══════════════════════════════════════════════════════════════════════
// Fehler
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Rule '{rule}' hat weder l_pattern noch r_pattern")]
    EmptyRule { rule: String },

    #[error(
        "Rule '{rule}': Shared-Anchor '{var}' hat inkonsistenten Kind \
             ('{l_kind}' auf L, '{r_kind}' auf R)"
    )]
    SharedAnchorKindMismatch {
        rule: String,
        var: String,
        l_kind: String,
        r_kind: String,
    },

    #[error("Rule '{rule}': Kante referenziert unbekannten NodePattern '{var}'")]
    DanglingEdge { rule: String, var: String },

    #[error(
        "Rule '{rule}': CorrespondenceLink referenziert unbekannten \
             NodePattern (l_node_id='{l}', r_node_id='{r}')"
    )]
    DanglingCorrespondence { rule: String, l: String, r: String },

    #[error("Rule '{rule}': unbekannte Transformation '{tag}'")]
    InvalidTransform { rule: String, tag: String },

    #[error("Ungültiger Regex '{pattern}': {reason}")]
    InvalidRegex { pattern: String, reason: String },

    #[error("Rule '{rule}' NAC '{nac}': shared_with_l-Node '{var}' existiert nicht im l_pattern")]
    NacSharedAnchorUnknown {
        rule: String,
        nac: String,
        var: String,
    },

    #[error(
        "Rule '{rule}': R-Pattern-Knoten '{node}' Attribut '{attr}' hat \
         einen nicht-Literal-Matcher (Regex/Prefix/Suffix/NumericRange). \
         R-Constraints werden in attrs_to_set uebersetzt; das geht nur \
         mit Literal-Werten, weil 'Wert auf Regex setzen' semantisch \
         undefiniert ist."
    )]
    NonLiteralRAttrUnsupported {
        rule: String,
        node: String,
        attr: String,
    },
}

impl CompileError {
    fn from_transform(rule: &str, err: UnknownTransform) -> Self {
        CompileError::InvalidTransform {
            rule: rule.to_string(),
            tag: err.0,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Compiler
// ══════════════════════════════════════════════════════════════════════

/// Kompiliert eine einzelne [`RuleSpec`] in einen [`CompiledRuleSpec`].
pub fn compile(spec: &RuleSpec) -> Result<CompiledRuleSpec, CompileError> {
    // ── 1. Patterns holen (leere ok, aber nicht beide leer) ──────────
    let empty = PatternSpec::default();
    let l_pat = spec.l_pattern.as_ref().unwrap_or(&empty);
    let r_pat = spec.r_pattern.as_ref().unwrap_or(&empty);
    if l_pat.nodes.is_empty() && r_pat.nodes.is_empty() {
        return Err(CompileError::EmptyRule {
            rule: spec.name.clone(),
        });
    }

    // ── 2. NodePatterns indizieren, Shared-Anchors ermitteln ─────────
    let mut l_by_id: HashMap<&str, &str> = HashMap::new();
    for n in &l_pat.nodes {
        l_by_id.insert(&n.id, &n.kind);
    }
    let mut r_by_id: HashMap<&str, &str> = HashMap::new();
    for n in &r_pat.nodes {
        r_by_id.insert(&n.id, &n.kind);
    }

    let mut match_nodes: Vec<MatchNode> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // L-Pattern-Knoten: LOnly oder Shared (wenn auch in R)
    for n in &l_pat.nodes {
        let origin = match r_by_id.get(n.id.as_str()) {
            None => NodeOrigin::LOnly,
            Some(r_kind) => {
                if *r_kind != n.kind.as_str() {
                    return Err(CompileError::SharedAnchorKindMismatch {
                        rule: spec.name.clone(),
                        var: n.id.clone(),
                        l_kind: n.kind.clone(),
                        r_kind: r_kind.to_string(),
                    });
                }
                NodeOrigin::Shared
            }
        };
        if seen.insert(n.id.clone()) {
            match_nodes.push(MatchNode {
                var: n.id.clone(),
                kind: n.kind.clone(),
                origin,
            });
        }
    }
    // R-Pattern-Knoten, die nicht schon in L waren → entweder
    // Match-Teil (wenn via Context-Corr an einen bestehenden Graph-
    // Knoten gebunden) oder Creation-Teil (wenn sie durch eine neue
    // Corr mit dieser Rule erst entstehen).
    //
    // Kontext-Corr = CorrespondenceLink ohne attribute_bindings.
    let r_only_in_context: HashSet<&str> = spec
        .correspondence_links
        .iter()
        .filter(|cl| cl.attribute_bindings.is_empty())
        .map(|cl| cl.r_node_id.as_str())
        .collect();

    let mut nodes_to_create: Vec<MatchNode> = Vec::new();
    for n in &r_pat.nodes {
        if l_by_id.contains_key(n.id.as_str()) {
            continue; // bereits als Shared im Match
        }
        let is_context_r = r_only_in_context.contains(n.id.as_str());
        let mn = MatchNode {
            var: n.id.clone(),
            kind: n.kind.clone(),
            origin: NodeOrigin::ROnly,
        };
        if is_context_r {
            // Context-R: muss im Graph existieren — ins Match.
            if seen.insert(n.id.clone()) {
                match_nodes.push(mn);
            }
        } else {
            // Creation-R: gehört nicht ins Match-Pattern, sonst würde
            // die Rule nie greifen (Matcher findet den Knoten ja
            // nicht vor der Rule-Anwendung).
            nodes_to_create.push(mn);
        }
    }

    // ── 3. Kanten: Referenz-Integrität checken ───────────────────────
    // Alle bekannten Vars = Match-Vars + zu-erzeugende Vars.
    let mut all_known: HashSet<String> = match_nodes.iter().map(|n| n.var.clone()).collect();
    for n in &nodes_to_create {
        all_known.insert(n.var.clone());
    }

    let _match_var: HashSet<String> = match_nodes.iter().map(|n| n.var.clone()).collect();

    let mut match_edges: Vec<MatchEdge> = Vec::new();
    let mut edges_to_create: Vec<MatchEdge> = Vec::new();
    for e in &l_pat.edges {
        check_node_ref(spec, &all_known, &e.source_node_id)?;
        check_node_ref(spec, &all_known, &e.target_node_id)?;
        match_edges.push(MatchEdge {
            source_var: e.source_node_id.clone(),
            target_var: e.target_node_id.clone(),
            kind: e.kind.clone(),
            side: EdgeSide::L,
        });
    }
    for e in &r_pat.edges {
        check_node_ref(spec, &all_known, &e.source_node_id)?;
        check_node_ref(spec, &all_known, &e.target_node_id)?;
        let edge = MatchEdge {
            source_var: e.source_node_id.clone(),
            target_var: e.target_node_id.clone(),
            kind: e.kind.clone(),
            side: EdgeSide::R,
        };
        // R-Edge-Klassifikation:
        // Kommt die Kante identisch im L-Pattern vor, ist sie bereits
        // als L-Match-Edge gezählt → überspringen. Jede andere R-Edge
        // wird erzeugt (edges_to_create) — auch eine zwischen zwei
        // Kontext-Knoten. Der op-granulare Duplicate-Check (is_duplicate
        // mit .all()) macht eine zweite, rein wiederholende Anwendung
        // sauber idempotent, statt die Regel zu blockieren — damit ist
        // die frühere R-Context-Edge-Sonderbehandlung (cf1c6c4) hinfällig.
        let also_in_l = l_pat.edges.iter().any(|le| {
            le.source_node_id == e.source_node_id
                && le.target_node_id == e.target_node_id
                && le.kind == e.kind
        });
        if also_in_l {
            continue;
        }
        edges_to_create.push(edge);
    }

    // ── 4. Correspondence-Links: Kontext vs. Neu ─────────────────────
    let mut context_corrs: Vec<CorrespondenceLinkSpec> = Vec::new();
    let mut corrs_to_create: Vec<CorrespondenceLinkSpec> = Vec::new();
    for cl in &spec.correspondence_links {
        let known_l = all_known.contains(&cl.l_node_id);
        let known_r = all_known.contains(&cl.r_node_id);
        if !known_l || !known_r {
            return Err(CompileError::DanglingCorrespondence {
                rule: spec.name.clone(),
                l: cl.l_node_id.clone(),
                r: cl.r_node_id.clone(),
            });
        }
        // Ein Corr ist Kontext, wenn er **keine** AttrBindings hat —
        // dann ist seine einzige Rolle, einen bereits bestehenden
        // Corr-Knoten im Match zu referenzieren. Corrs mit
        // AttrBindings etablieren bijektive Synchronisation und
        // werden von dieser Rule materialisiert.
        if cl.attribute_bindings.is_empty() {
            context_corrs.push(cl.clone());
        } else {
            corrs_to_create.push(cl.clone());
        }
    }

    // ── 5. Propagationen aus AttrBindings ableiten ───────────────────
    // Für jeden neu etablierten Corr-Link wird pro AttrBinding eine
    // Propagation L→R (und die Umkehrrichtung) geplant. Die konkrete
    // Richtung (wann welche) entscheidet die Engine zur Match-Zeit.
    let mut propagations: Vec<AttrPropagation> = Vec::new();
    for cl in &corrs_to_create {
        for b in &cl.attribute_bindings {
            let transform = AttrTransform::parse(b.transformation.as_deref())
                .map_err(|e| CompileError::from_transform(&spec.name, e))?;
            propagations.push(AttrPropagation {
                source_node_var: cl.l_node_id.clone(),
                source_attr: b.l_attr_name.clone(),
                target_node_var: cl.r_node_id.clone(),
                target_attr: b.r_attr_name.clone(),
                transform,
            });
        }
    }

    // ── 6. Attribut-Constraints klassifizieren ──────────────────────
    // L-Constraints sind immer Match-Constraints (wir matchen den
    // Ausgangs-Zustand).
    let mut constraints: Vec<MatchConstraint> = Vec::new();
    for n in &l_pat.nodes {
        for c in &n.constraints {
            let predicate = compile_matcher(&c.matcher)?;
            constraints.push(MatchConstraint {
                node_var: n.id.clone(),
                attr_name: c.name.clone(),
                predicate,
            });
        }
    }
    // R-Constraints werden klassifiziert:
    //   1. Identisch zu einem L-Constraint am selben Knoten/Attribut
    //      → redundant, weglassen (L-Constraint deckt den Match ab).
    //   2. Auf einem **R-only-Creation-Knoten** (steht in
    //      `nodes_to_create`) → `creation_attrs`. Das Literal ist
    //      Identitäts-Attribut und fließt in den Ghost-Hash, NICHT
    //      als SetAttr-Op. Verhindert die rc5-Oszillation zwischen
    //      Rules, die strukturell-identische Creation-Knoten mit
    //      verschiedenen Literal-Werten erzeugen würden.
    //   3. Auf einem **Kontext-Knoten** (Shared-Anchor oder R-only-
    //      mit-context-corr) → `attrs_to_set` (B5/rc4-Semantik:
    //      `Op::SetAttr` mutiert das Attribut auf dem bestehenden
    //      Knoten).
    //   4. Nicht-Literal-Matcher (Regex/Prefix/Suffix/NumericRange)
    //      → CompileError, weil „Wert auf Regex setzen" semantisch
    //      undefiniert ist.
    let creation_var_set: HashSet<&str> =
        nodes_to_create.iter().map(|mn| mn.var.as_str()).collect();
    let mut attrs_to_set: Vec<AttrToSet> = Vec::new();
    let mut creation_attrs: Vec<CreationAttr> = Vec::new();
    for n in &r_pat.nodes {
        let l_node = l_pat.nodes.iter().find(|ln| ln.id == n.id);
        let is_creation = creation_var_set.contains(n.id.as_str());
        for c in &n.constraints {
            let l_same_attr =
                l_node.and_then(|ln| ln.constraints.iter().find(|lc| lc.name == c.name));
            if let Some(lc) = l_same_attr {
                if lc.matcher == c.matcher {
                    continue; // identisch → L deckt es ab
                }
            }
            match &c.matcher {
                AttrMatcherSpec::Literal { value } => {
                    if is_creation {
                        creation_attrs.push(CreationAttr {
                            node_var: n.id.clone(),
                            attr_name: c.name.clone(),
                            value: value.clone(),
                        });
                    } else {
                        attrs_to_set.push(AttrToSet {
                            node_var: n.id.clone(),
                            attr_name: c.name.clone(),
                            value: value.clone(),
                        });
                    }
                }
                _ => {
                    return Err(CompileError::NonLiteralRAttrUnsupported {
                        rule: spec.name.clone(),
                        node: n.id.clone(),
                        attr: c.name.clone(),
                    });
                }
            }
        }
    }

    Ok(CompiledRuleSpec {
        name: spec.name.clone(),
        rank: spec.rank,
        documentation: spec.documentation.clone(),
        match_plan: MatchPlan {
            nodes: match_nodes,
            edges: match_edges,
            constraints,
            context_correspondences: context_corrs,
        },
        creation_plan: CreationPlan {
            nodes_to_create,
            edges_to_create,
            correspondences_to_create: corrs_to_create,
            attrs_to_set,
            creation_attrs,
        },
        propagation_plan: propagations,
        nacs: compile_nacs(spec, l_pat)?,
    })
}

/// Kompiliert alle NACs einer Rule.
fn compile_nacs(spec: &RuleSpec, l_pat: &PatternSpec) -> Result<Vec<CompiledNac>, CompileError> {
    let l_vars: HashSet<&str> = l_pat.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut compiled = Vec::with_capacity(spec.nacs.len());
    for nac in &spec.nacs {
        // shared_with_l validieren
        for var in &nac.shared_with_l {
            if !l_vars.contains(var.as_str()) {
                return Err(CompileError::NacSharedAnchorUnknown {
                    rule: spec.name.clone(),
                    nac: nac.name.clone(),
                    var: var.clone(),
                });
            }
        }
        // Nodes
        let mut nodes = Vec::with_capacity(nac.nodes.len());
        let mut constraints = Vec::new();
        for n in &nac.nodes {
            nodes.push(MatchNode {
                var: n.id.clone(),
                kind: n.kind.clone(),
                origin: NodeOrigin::LOnly,
            });
            for c in &n.constraints {
                constraints.push(MatchConstraint {
                    node_var: n.id.clone(),
                    attr_name: c.name.clone(),
                    predicate: compile_matcher(&c.matcher)?,
                });
            }
        }
        // Edges
        let edges = nac
            .edges
            .iter()
            .map(|e| MatchEdge {
                source_var: e.source_node_id.clone(),
                target_var: e.target_node_id.clone(),
                kind: e.kind.clone(),
                side: EdgeSide::L,
            })
            .collect();
        compiled.push(CompiledNac {
            name: nac.name.clone(),
            nodes,
            edges,
            constraints,
            shared_with_l: nac.shared_with_l.clone(),
        });
    }
    Ok(compiled)
}

fn check_node_ref(spec: &RuleSpec, known: &HashSet<String>, var: &str) -> Result<(), CompileError> {
    if known.contains(var) {
        Ok(())
    } else {
        Err(CompileError::DanglingEdge {
            rule: spec.name.clone(),
            var: var.to_string(),
        })
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::spec::parse_ruleset;

    const DEMO_FIXTURE: &str = include_str!("../../tests/fixtures/demo-ruleset.json");

    fn demo_rule(name: &str) -> RuleSpec {
        let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
        rs.rules.into_iter().find(|r| r.name == name).unwrap()
    }

    // ── Fehlerfälle ──────────────────────────────────────────────────

    #[test]
    fn leere_rule_ist_fehler() {
        let r = RuleSpec {
            name: "empty".into(),
            rank: 0,
            documentation: None,
            l_pattern: None,
            r_pattern: None,
            correspondence_links: vec![],
            nacs: vec![],
        };
        let err = compile(&r).unwrap_err();
        matches!(err, CompileError::EmptyRule { .. });
    }

    #[test]
    fn dangling_edge_schlaegt_fehl() {
        let json = r#"{"rules":[{
            "name":"bad","rank":1,
            "l_pattern":{"nodes":[{"id":"a","kind":"Foo","constraints":[]}],
                         "edges":[{"kind":"x","source_node_id":"a","target_node_id":"unknown"}]},
            "r_pattern":{"nodes":[],"edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        match err {
            CompileError::DanglingEdge { var, .. } => assert_eq!(var, "unknown"),
            _ => panic!("falsche Fehlerart"),
        }
    }

    #[test]
    fn shared_anchor_kind_mismatch_schlaegt_fehl() {
        let json = r#"{"rules":[{
            "name":"bad","rank":1,
            "l_pattern":{"nodes":[{"id":"m","kind":"Model","constraints":[]}],"edges":[]},
            "r_pattern":{"nodes":[{"id":"m","kind":"SomethingElse","constraints":[]}],"edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        match err {
            CompileError::SharedAnchorKindMismatch {
                var,
                l_kind,
                r_kind,
                ..
            } => {
                assert_eq!(var, "m");
                assert_eq!(l_kind, "Model");
                assert_eq!(r_kind, "SomethingElse");
            }
            _ => panic!("falsche Fehlerart"),
        }
    }

    #[test]
    fn dangling_correspondence_schlaegt_fehl() {
        let json = r#"{"rules":[{
            "name":"bad","rank":1,
            "l_pattern":{"nodes":[{"id":"a","kind":"Foo","constraints":[]}],"edges":[]},
            "r_pattern":{"nodes":[],"edges":[]},
            "correspondence_links":[{"l_node_id":"a","r_node_id":"nope",
                "attribute_bindings":[]}]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let err = compile(&rs.rules[0]).unwrap_err();
        matches!(err, CompileError::DanglingCorrespondence { .. });
    }

    // ── Erfolgsfälle auf Demo-Fixture ────────────────────────────────

    #[test]
    fn rclass_wird_kompiliert() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        assert_eq!(cr.name, "R_Class");
        assert_eq!(cr.rank, 40);
    }

    #[test]
    fn rclass_erkennt_model_als_shared_anchor() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        // Shared Anchor (Model): ist im Match-Pattern
        let m = cr.match_plan.nodes.iter().find(|n| n.var == "m").unwrap();
        assert_eq!(m.kind, "Model");
        assert_eq!(m.origin, NodeOrigin::Shared);

        // LOnly (Class): ist im Match-Pattern
        let c = cr.match_plan.nodes.iter().find(|n| n.var == "c").unwrap();
        assert_eq!(c.origin, NodeOrigin::LOnly);

        // R-Only ohne Context-Corr: NICHT im Match-Pattern, dafür in
        // creation_plan.nodes_to_create. Sonst würde die Rule nie
        // greifen — jc existiert erst, nachdem die Rule gefeuert hat.
        assert!(
            cr.match_plan.nodes.iter().all(|n| n.var != "jc"),
            "jc darf nicht im Match-Pattern sein (R-only, keine Context-Corr)"
        );
        let jc_create = cr
            .creation_plan
            .nodes_to_create
            .iter()
            .find(|n| n.var == "jc")
            .expect("jc muss in creation_plan.nodes_to_create sein");
        assert_eq!(jc_create.kind, "JavaClass");
        assert_eq!(jc_create.origin, NodeOrigin::ROnly);
    }

    #[test]
    fn rclass_plant_jc_als_zu_erzeugen_mit_namen_propagation() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        // jc muss in nodes_to_create stehen
        let jc = cr
            .creation_plan
            .nodes_to_create
            .iter()
            .find(|n| n.var == "jc");
        assert!(jc.is_some(), "jc fehlt in nodes_to_create");
        // Propagation: c.name → jc.name identity
        let p = cr
            .propagation_plan
            .iter()
            .find(|p| p.source_attr == "name" && p.target_attr == "name")
            .expect("name-Propagation fehlt");
        assert_eq!(p.source_node_var, "c");
        assert_eq!(p.target_node_var, "jc");
        assert_eq!(p.transform, AttrTransform::Identity);
    }

    #[test]
    fn rclass_corr_mit_bindings_wird_als_zu_erzeugen_klassifiziert() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        assert_eq!(cr.match_plan.context_correspondences.len(), 0);
        assert_eq!(cr.creation_plan.correspondences_to_create.len(), 1);
        assert_eq!(
            cr.creation_plan.correspondences_to_create[0]
                .kind
                .as_deref(),
            Some("CorrClass")
        );
    }

    #[test]
    fn rattr_trennt_kontext_von_neuer_corr() {
        let cr = compile(&demo_rule("R_Attr")).unwrap();
        // Der erste Corr (CorrClass ohne Bindings) ist Kontext
        assert_eq!(cr.match_plan.context_correspondences.len(), 1);
        assert_eq!(
            cr.match_plan.context_correspondences[0].kind.as_deref(),
            Some("CorrClass")
        );
        // Der zweite (CorrAttr mit 2 Bindings) ist neu
        assert_eq!(cr.creation_plan.correspondences_to_create.len(), 1);
        assert_eq!(
            cr.creation_plan.correspondences_to_create[0]
                .kind
                .as_deref(),
            Some("CorrAttr")
        );
        assert_eq!(cr.propagation_plan.len(), 2); // name + type
    }

    #[test]
    fn rgetter_propagation_mit_getter_name() {
        let cr = compile(&demo_rule("R_Getter")).unwrap();
        let gn = cr
            .propagation_plan
            .iter()
            .find(|p| p.transform == AttrTransform::GetterName)
            .expect("GetterName-Propagation fehlt");
        assert_eq!(gn.source_attr, "name");
        assert_eq!(gn.target_attr, "name");
    }

    #[test]
    fn edges_werden_korrekt_auf_l_oder_r_seite_zugeordnet() {
        let cr = compile(&demo_rule("R_Class")).unwrap();
        // Die classes-Edge ist L-Seite, match-Pattern.
        // Die javaClasses-Edge ist R-Seite; ihr Endpunkt jc ist ROnly,
        // also muss sie in edges_to_create stehen.
        assert!(cr
            .match_plan
            .edges
            .iter()
            .any(|e| e.kind == "classes" && e.side == EdgeSide::L));
        assert!(cr
            .creation_plan
            .edges_to_create
            .iter()
            .any(|e| e.kind == "javaClasses" && e.side == EdgeSide::R));
    }

    #[test]
    fn alle_vier_demo_rules_kompilieren_fehlerfrei() {
        let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
        for r in &rs.rules {
            let cr = compile(r).unwrap_or_else(|_| panic!("rule {} muss kompilieren", r.name));
            assert!(
                !cr.match_plan.nodes.is_empty(),
                "Rule {} hat leeren MatchPlan",
                r.name
            );
        }
    }

    #[test]
    fn literal_constraints_landen_im_match_plan() {
        let json = r#"{"rules":[{
            "name":"WithConstraints","rank":1,
            "l_pattern":{"nodes":[{"id":"c","kind":"Class",
                "constraints":[{"name":"isInterface","matcher":{"type":"literal","value":"false"}}]}],
                "edges":[]},
            "r_pattern":{"nodes":[],"edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();
        assert_eq!(cr.match_plan.constraints.len(), 1);
        assert_eq!(cr.match_plan.constraints[0].attr_name, "isInterface");
        assert!(matches!(
            &cr.match_plan.constraints[0].predicate,
            crate::engine::AttrPredicate::Equals(v) if v == "false"
        ));
    }

    #[test]
    fn r_kante_zwischen_kontext_knoten_wird_erzeugt() {
        // L matcht zwei Knoten a, b ohne Kante; R fügt eine Kante a→b
        // zwischen denselben (Kontext-)Knoten hinzu. Diese Kante muss
        // erzeugt werden — nicht stillschweigend zur Match-Bedingung
        // umklassifiziert.
        let json = r#"{"rules":[{
            "name":"CtxEdge","rank":1,
            "l_pattern":{"nodes":[
                {"id":"a","kind":"Class"},
                {"id":"b","kind":"Class"}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"a","kind":"Class"},
                {"id":"b","kind":"Class"}],
                "edges":[{"kind":"link","source_node_id":"a","target_node_id":"b"}]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();
        assert!(
            cr.creation_plan
                .edges_to_create
                .iter()
                .any(|e| e.kind == "link" && e.side == EdgeSide::R),
            "Context-Context-R-Kante muss in edges_to_create stehen, \
             nicht als Match-Constraint"
        );
    }

    #[test]
    fn r_literal_unterschiedlich_zu_l_klassifiziert_als_attrs_to_set() {
        // Pendant zum Edge-Bug (B4) auf der Attribut-Seite: L matcht
        // Job mit image="old"; R sagt image="new" am selben (Kontext-)
        // Knoten. Das R-Literal darf NICHT als Match-Constraint landen
        // — sonst sucht der Matcher gleichzeitig nach "old" und "new"
        // und findet nie etwas. Es gehört in
        // creation_plan.attrs_to_set als SetAttr-Intent.
        let json = r#"{"rules":[{
            "name":"CtxAttr","rank":1,
            "l_pattern":{"nodes":[
                {"id":"j","kind":"Job",
                 "constraints":[{"name":"image","matcher":{"type":"literal","value":"old"}}]}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"j","kind":"Job",
                 "constraints":[{"name":"image","matcher":{"type":"literal","value":"new"}}]}],
                "edges":[]},
            "correspondence_links":[]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();

        // (1) R-Literal "new" darf nicht als Match-Constraint auftauchen
        let r_literal_in_match = cr.match_plan.constraints.iter().any(|c| {
            c.node_var == "j"
                && c.attr_name == "image"
                && matches!(
                    &c.predicate,
                    crate::engine::AttrPredicate::Equals(v) if v == "new"
                )
        });
        assert!(
            !r_literal_in_match,
            "R-Literal 'new' darf nicht als Match-Constraint behandelt \
             werden — die Regel würde sonst nie matchen (sucht \
             gleichzeitig nach 'old' und 'new')"
        );

        // (2) R-Literal muss als attrs_to_set im creation_plan stehen
        assert_eq!(
            cr.creation_plan.attrs_to_set.len(),
            1,
            "R-Literal auf Kontext-Knoten muss als attrs_to_set \
             materialisiert werden"
        );
        let ats = &cr.creation_plan.attrs_to_set[0];
        assert_eq!(ats.node_var, "j");
        assert_eq!(ats.attr_name, "image");
        assert_eq!(ats.value, "new");

        // (3) L-Literal "old" bleibt als Match-Constraint — die Regel
        //     soll nur Knoten mit image="old" matchen und dann auf "new"
        //     setzen.
        let l_literal_in_match = cr.match_plan.constraints.iter().any(|c| {
            c.node_var == "j"
                && c.attr_name == "image"
                && matches!(
                    &c.predicate,
                    crate::engine::AttrPredicate::Equals(v) if v == "old"
                )
        });
        assert!(
            l_literal_in_match,
            "L-Literal 'old' muss als Match-Constraint erhalten bleiben"
        );
    }

    #[test]
    fn r_literal_auf_creation_knoten_landet_in_creation_attrs_nicht_attrs_to_set() {
        // rc6/B6 Klassifikations-Regression: L hat nur Source, R hat
        // Target (R-only-Creation) mit Literal `label=x`. Vor rc6
        // landete `label=x` in `attrs_to_set` und führte beim
        // Aufeinandertreffen zweier Rules mit demselben strukturellen
        // R-Output aber verschiedenen Literal-Werten zu einer
        // Oszillations-Schleife (siehe
        // tests/repro_rc5_bug2_attrs_to_set_collision.rs). Jetzt
        // landet es in `creation_attrs` und fließt in den GhostId
        // des neu erzeugten Knotens.
        let json = r#"{"rules":[{
            "name":"CreationLit","rank":1,
            "l_pattern":{"nodes":[
                {"id":"L","kind":"Source","constraints":[]}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"R","kind":"Target",
                 "constraints":[{"name":"label","matcher":{"type":"literal","value":"x"}}]}],
                "edges":[]},
            "correspondence_links":[{
                "l_node_id":"L","r_node_id":"R","kind":"tgg:refines",
                "attribute_bindings":[]
            }]
        }]}"#;
        let rs = parse_ruleset(json).unwrap();
        let cr = compile(&rs.rules[0]).unwrap();

        // R-Knoten ist R-only-Creation (nicht in r_only_in_context,
        // weil der Corr `attribute_bindings: []` hat — Context-Corr —
        // wait: context-corrs befördern R-Vars in den Match. Wenn
        // bindings leer sind, ist R kontext, nicht creation. Daher
        // muss der Corr Bindings haben, damit R als Creation
        // klassifiziert wird. Wir prüfen das durch einen separaten
        // Spec mit non-empty bindings.
        // → siehe nächster Test: hier ist R *kontext* (corr ohne
        // bindings).
        eprintln!(
            "(context-corr variant) attrs_to_set={:?} creation_attrs={:?}",
            cr.creation_plan.attrs_to_set, cr.creation_plan.creation_attrs
        );
        assert_eq!(
            cr.creation_plan.nodes_to_create.len(),
            0,
            "R via context-corr (no bindings) gehört in match, nicht in nodes_to_create"
        );
        assert_eq!(
            cr.creation_plan.attrs_to_set.len(),
            1,
            "R-Knoten ist hier Kontext (context-corr) → Literal in attrs_to_set"
        );
        assert_eq!(cr.creation_plan.creation_attrs.len(), 0);

        // Zweite Variante: corr MIT bindings → R ist R-only-Creation
        // → Literal landet in creation_attrs.
        let json2 = r#"{"rules":[{
            "name":"CreationLit2","rank":1,
            "l_pattern":{"nodes":[
                {"id":"L","kind":"Source",
                 "constraints":[{"name":"tag","matcher":{"type":"literal","value":"foo"}}]}],
                "edges":[]},
            "r_pattern":{"nodes":[
                {"id":"R","kind":"Target",
                 "constraints":[{"name":"label","matcher":{"type":"literal","value":"x"}}]}],
                "edges":[]},
            "correspondence_links":[{
                "l_node_id":"L","r_node_id":"R","kind":"tgg:refines",
                "attribute_bindings":[{
                    "l_attr_name":"tag","r_attr_name":"tag","transformation":"identity"
                }]
            }]
        }]}"#;
        let rs2 = parse_ruleset(json2).unwrap();
        let cr2 = compile(&rs2.rules[0]).unwrap();
        assert_eq!(
            cr2.creation_plan.nodes_to_create.len(),
            1,
            "R muss Creation sein"
        );
        assert_eq!(cr2.creation_plan.nodes_to_create[0].var, "R");
        assert_eq!(
            cr2.creation_plan.attrs_to_set.len(),
            0,
            "Creation-Knoten-Literal darf NICHT in attrs_to_set landen — \
             das war der rc5-Oszillations-Bug",
        );
        assert_eq!(
            cr2.creation_plan.creation_attrs.len(),
            1,
            "Creation-Knoten-Literal muss in creation_attrs landen",
        );
        let ca = &cr2.creation_plan.creation_attrs[0];
        assert_eq!(ca.node_var, "R");
        assert_eq!(ca.attr_name, "label");
        assert_eq!(ca.value, "x");
    }
}
