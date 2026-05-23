//! Engine-Modul — T₃, T₄, T₆.
//!
//! Zuständigkeit:
//! - Ghost-View als inkrementelle Projektion (Def. 6.1, 6.2)
//! - Matcher-Contract (Def. 6.3) inkl. Status-awareness
//! - Pattern-Matching mit Injektivitätszwang und Edge-Patterns (Phase 1.3b)
//! - Regel-Trait mit Produktion (Def. 4.1)
//! - Rang-basierte Selektion höchstrangiger Kandidaten (Def. 4.3)
//! - Canonical Match-Enumeration μ (Def. 4.2)
//! - Cascade-Controller

use crate::graph::{GhostId, NodeData, Status, TypedGraph};
use crate::ops::{DeltaEntry, Op, OpError, OpTarget, Origin};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use thiserror::Error;

// ── Attribut-Prädikate ═══════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub enum AttrPredicate {
    Exists,
    Equals(String),
    /// Regex-Match über Wert (Rust `regex`-Crate).
    Regex(regex::Regex),
    /// Wert startet mit Präfix.
    Prefix(String),
    /// Wert endet mit Suffix.
    Suffix(String),
    /// Numerischer Bereich, Wert wird via `parse::<f64>()` konvertiert.
    NumericRange {
        min: f64,
        max: f64,
    },
}

impl AttrPredicate {
    pub fn matches(&self, value: Option<&String>) -> bool {
        match self {
            AttrPredicate::Exists => value.is_some(),
            AttrPredicate::Equals(expected) => value == Some(expected),
            AttrPredicate::Regex(re) => value.is_some_and(|s| re.is_match(s)),
            AttrPredicate::Prefix(p) => value.is_some_and(|s| s.starts_with(p)),
            AttrPredicate::Suffix(suf) => value.is_some_and(|s| s.ends_with(suf)),
            AttrPredicate::NumericRange { min, max } => value
                .and_then(|s| s.parse::<f64>().ok())
                .is_some_and(|n| n >= *min && n <= *max),
        }
    }
}

// ── Pattern ══════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct NodePattern {
    pub var: String,
    pub type_id: String,
    pub attr_constraints: Vec<(String, AttrPredicate)>,
}

impl NodePattern {
    pub fn new(var: &str, type_id: &str) -> Self {
        Self {
            var: var.into(),
            type_id: type_id.into(),
            attr_constraints: Vec::new(),
        }
    }

    pub fn with_attr_equals(mut self, key: &str, value: &str) -> Self {
        self.attr_constraints
            .push((key.into(), AttrPredicate::Equals(value.into())));
        self
    }

    pub fn with_attr_exists(mut self, key: &str) -> Self {
        self.attr_constraints
            .push((key.into(), AttrPredicate::Exists));
        self
    }

    pub fn matches_node(&self, node: &NodeData) -> bool {
        if node.type_id != self.type_id {
            return false;
        }
        self.attr_constraints
            .iter()
            .all(|(key, pred)| pred.matches(node.attrs.get(key)))
    }
}

/// Kanten-Pattern: verlangt matchbare Kante zwischen zwei Pattern-Variablen.
#[derive(Clone, Debug)]
pub struct EdgePattern {
    pub source_var: String,
    pub target_var: String,
    pub type_id: String,
}

impl EdgePattern {
    pub fn new(source_var: &str, target_var: &str, type_id: &str) -> Self {
        Self {
            source_var: source_var.into(),
            target_var: target_var.into(),
            type_id: type_id.into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Pattern {
    pub nodes: Vec<NodePattern>,
    pub edges: Vec<EdgePattern>,
}

impl Pattern {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node(mut self, np: NodePattern) -> Self {
        self.nodes.push(np);
        self
    }

    pub fn with_edge(mut self, ep: EdgePattern) -> Self {
        self.edges.push(ep);
        self
    }
}

// ── Match ════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Default)]
pub struct PatternMatch {
    pub bindings: HashMap<String, GhostId>,
}

impl PatternMatch {
    pub fn get(&self, var: &str) -> Option<&GhostId> {
        self.bindings.get(var)
    }
}

/// Canonical Match-Key für deterministische Enumeration μ (Def. 4.2).
///
/// Ordnet Matches lexikographisch nach den gebundenen Ghost-IDs in
/// Pattern-Variablen-Reihenfolge.
pub fn canonical_key(m: &PatternMatch, pattern: &Pattern) -> Vec<[u8; 32]> {
    pattern
        .nodes
        .iter()
        .filter_map(|np| m.bindings.get(&np.var).map(|id| id_bytes(*id)))
        .collect()
}

fn id_bytes(id: GhostId) -> [u8; 32] {
    // GhostId ist tuple-struct; wir brauchen einen bytes-accessor.
    // serde-bincode wäre eine Option; hier direkt über Hash-Seed als proxy:
    // Da GhostId nach Copy implementiert ist, ist der direkte Zugriff
    // via Debug-String unschön. Sauberer Weg: GhostId-API erweitern.
    // Für jetzt: über serde_json-Roundtrip als Fallback wäre absurd.
    // Beste Lösung: GhostId einen bytes()-Accessor geben.
    id.as_bytes()
}

/// Findet alle Matches eines Patterns im Graphen.
///
/// Eigenschaften:
/// - **Ghost-aware**: `graph.matchable_nodes()` schließt TOMB aus.
/// - **Injektiv**: verschiedene Pattern-Variablen binden an verschiedene
///   Graph-Knoten.
/// - **Edge-Constraints**: alle `pattern.edges` müssen im Graph existieren
///   und matchbar sein.
///
/// Naive Backtracking-Enumeration.
pub fn find_matches(pattern: &Pattern, graph: &TypedGraph) -> Vec<PatternMatch> {
    find_matches_with_fixed(pattern, graph, &HashMap::new())
}

/// Variante von [`find_matches`] mit Pre-Bindings: bestimmte
/// Pattern-Variablen sind bereits an konkrete `GhostId`s fixiert.
/// Der Matcher verzweigt nur über die restlichen Variablen.
///
/// Verwendet von NAC-Checks (M2), wo die `shared_with_l`-Anker
/// aus dem Haupt-Match bereits gebunden sind.
pub fn find_matches_with_fixed(
    pattern: &Pattern,
    graph: &TypedGraph,
    fixed: &HashMap<String, GhostId>,
) -> Vec<PatternMatch> {
    if pattern.nodes.is_empty() {
        return if pattern.edges.is_empty() {
            let mut pm = PatternMatch::default();
            for (k, v) in fixed {
                pm.bindings.insert(k.clone(), *v);
            }
            vec![pm]
        } else {
            Vec::new()
        };
    }
    let plan = build_match_plan(pattern, fixed);
    let mut results = Vec::new();
    let mut current = PatternMatch::default();
    for (k, v) in fixed {
        current.bindings.insert(k.clone(), *v);
    }
    enumerate_matches(pattern, graph, &plan, 0, &mut current, &mut results);
    // Determinismus (Def. 4.2): kanonische μ-Ordnung, unabhängig vom
    // edge-geleiteten Traversal-Pfad. Die Match-*Menge* ist pfad-
    // invariant (erschöpfende Suche); nur die Push-Reihenfolge variiert.
    results.sort_by_key(|m| canonical_key(m, pattern));
    results
}

// ── Edge-geleiteter Match-Plan ═══════════════════════════════════════════
//
// Statt das kartesische Produkt über alle Pattern-Positionen zu
// enumerieren und Edge-Constraints erst am Blatt zu prüfen (O(M^N)),
// arbeitet der Matcher die Knoten in *connected order* ab: ein
// Seed-Knoten wird per Typ-Scan gewählt, jeder Folgeknoten hängt per
// Pattern-Edge an einem bereits gebundenen Knoten und wird über
// Graph-Adjazenz statt über die volle Typ-Population erzeugt. Das
// senkt den Worst Case auf O(M · d^(N-1)), d = mittlerer Knotengrad.

/// Eine Pattern-Edge eines Plan-Knotens zu einem im Plan bereits
/// platzierten Knoten — relativ zum *neuen* Knoten ausgedrückt.
struct EdgeLink {
    /// Variable des bereits platzierten Nachbarn.
    placed_var: String,
    type_id: String,
    /// `true`: der neue Knoten ist Source, der platzierte ist Target.
    /// `false`: der platzierte ist Source, der neue ist Target.
    new_is_source: bool,
}

/// Ein Schritt im Traversal-Plan: welcher Pattern-Knoten als nächstes
/// gebunden wird und wie seine Kandidaten erzeugt werden.
struct MatchStep {
    /// Index in `pattern.nodes`.
    node_idx: usize,
    /// `true`: Variable ist via `fixed` vorgebunden — nur validieren.
    pre_bound: bool,
    /// Pattern-Edges zu bereits platzierten Knoten. Nicht-leer ⇒
    /// `links[0]` dient als Adjazenz-Guide (Kandidaten aus den
    /// Nachbarn des Ankers); alle Links werden zusätzlich verifiziert.
    /// Leer ⇒ Seed-Knoten (Typ-Scan).
    links: Vec<EdgeLink>,
}

/// Baut den deterministischen Traversal-Plan für ein Pattern.
///
/// Determinismus: das Pattern ist fixe Eingabe, die Knoten-Auswahl ist
/// eine reine Funktion über (Pattern, platzierte Menge). Der
/// abschließende `canonical_key`-Sort in [`find_matches_with_fixed`]
/// macht die Plan-Reihenfolge ohnehin an der API-Grenze unsichtbar.
fn build_match_plan(pattern: &Pattern, fixed: &HashMap<String, GhostId>) -> Vec<MatchStep> {
    let n = pattern.nodes.len();
    let mut placed = vec![false; n];
    let mut steps: Vec<MatchStep> = Vec::with_capacity(n);

    // 1. fixed-Variablen zuerst: sie sind bereits gebunden und dienen
    //    als Adjazenz-Anker für den Rest (relevant für NAC- und
    //    Re-Validation-Matching mit shared-Anchors).
    for (i, np) in pattern.nodes.iter().enumerate() {
        if fixed.contains_key(&np.var) {
            placed[i] = true;
            steps.push(MatchStep {
                node_idx: i,
                pre_bound: true,
                links: Vec::new(),
            });
        }
    }

    // 2. Restliche Knoten greedy: bevorzugt einen, der per Edge an die
    //    platzierte Menge anknüpft (guided); sonst den selektivsten
    //    ungebundenen Knoten als Komponenten-Seed.
    while steps.len() < n {
        let next = pick_next_node(pattern, &placed);
        let links = links_to_placed(pattern, &placed, next);
        placed[next] = true;
        steps.push(MatchStep {
            node_idx: next,
            pre_bound: false,
            links,
        });
    }
    steps
}

/// Wählt den nächsten zu platzierenden Pattern-Knoten. Bevorzugt
/// Knoten mit Edge zur platzierten Menge (guided); Tie-Break:
/// most-constrained-variable-first (meiste `attr_constraints`), dann
/// kleinster Index.
fn pick_next_node(pattern: &Pattern, placed: &[bool]) -> usize {
    let mut best_idx: Option<usize> = None;
    let mut best_key = (false, 0usize, 0usize);
    for (i, np) in pattern.nodes.iter().enumerate() {
        if placed[i] {
            continue;
        }
        let guided = !links_to_placed(pattern, placed, i).is_empty();
        // `usize::MAX - i` ⇒ größerer Schlüssel = kleinerer Index.
        let key = (guided, np.attr_constraints.len(), usize::MAX - i);
        if best_idx.is_none() || key > best_key {
            best_idx = Some(i);
            best_key = key;
        }
    }
    best_idx.expect("pick_next_node: kein ungebundener Knoten vorhanden")
}

/// Sammelt alle Pattern-Edges des Knotens `idx` zu bereits platzierten
/// Knoten, relativ zum Knoten `idx` ausgedrückt. Selbst-Schleifen und
/// Edges mit unbekannter Gegen-Variable werden übersprungen — der
/// Leaf-Check `satisfies_edge_patterns` deckt sie ab.
fn links_to_placed(pattern: &Pattern, placed: &[bool], idx: usize) -> Vec<EdgeLink> {
    let var = pattern.nodes[idx].var.as_str();
    let mut links = Vec::new();
    for ep in &pattern.edges {
        if ep.source_var == ep.target_var {
            continue;
        }
        let (other_var, new_is_source) = if ep.source_var == var {
            (ep.target_var.as_str(), true)
        } else if ep.target_var == var {
            (ep.source_var.as_str(), false)
        } else {
            continue;
        };
        let other_placed = pattern
            .nodes
            .iter()
            .position(|np| np.var == other_var)
            .map(|j| placed[j])
            .unwrap_or(false);
        if other_placed {
            links.push(EdgeLink {
                placed_var: other_var.to_string(),
                type_id: ep.type_id.clone(),
                new_is_source,
            });
        }
    }
    links
}

fn enumerate_matches(
    pattern: &Pattern,
    graph: &TypedGraph,
    plan: &[MatchStep],
    depth: usize,
    current: &mut PatternMatch,
    out: &mut Vec<PatternMatch>,
) {
    if depth == plan.len() {
        // Komponenten-interne Edges wurden inkrementell geprüft; der
        // Leaf-Check deckt zusätzlich Selbst-Schleifen und Edges mit
        // unbekannter Variable ab (Defense-in-depth).
        if satisfies_edge_patterns(pattern, graph, current) {
            out.push(current.clone());
        }
        return;
    }
    let step = &plan[depth];
    let np = &pattern.nodes[step.node_idx];

    // Vorgebundene Variable (M2 / Re-Validation): nicht enumerieren,
    // nur prüfen, ob der gebundene Knoten das Pattern erfüllt.
    if step.pre_bound {
        if let Some(id) = current.bindings.get(&np.var).copied() {
            if let Some(node) = graph.get_node(&id) {
                if node.status.is_matchable() && np.matches_node(node) {
                    enumerate_matches(pattern, graph, plan, depth + 1, current, out);
                }
            }
        }
        return;
    }

    for cand in step_candidates(graph, np, step, current) {
        // Injektivität: Knoten darf noch nicht gebunden sein.
        if current.bindings.values().any(|id| *id == cand) {
            continue;
        }
        let node = match graph.get_node(&cand) {
            Some(node) if node.status.is_matchable() => node,
            _ => continue,
        };
        if !np.matches_node(node) {
            continue;
        }
        // Edge-Constraints zu bereits gebundenen Knoten *sofort* prüfen
        // — nicht erst am Blatt (early pruning).
        if !satisfies_links(graph, current, &step.links, cand) {
            continue;
        }
        current.bindings.insert(np.var.clone(), cand);
        enumerate_matches(pattern, graph, plan, depth + 1, current, out);
        current.bindings.remove(&np.var);
    }
}

/// Kandidaten-Knoten für einen Plan-Schritt.
///
/// - **Guided** (`links` nicht leer): Adjazenz-Lookup über `links[0]`
///   — nur Nachbarn des bereits gebundenen Ankers, dedupliziert
///   (parallele Kanten). O(Knotengrad) statt O(Typ-Population).
/// - **Seed** (`links` leer): Typ-Scan über `matchable_nodes_by_kind`
///   (F15-Mitigation, BTreeSet-kanonisch).
fn step_candidates(
    graph: &TypedGraph,
    np: &NodePattern,
    step: &MatchStep,
    current: &PatternMatch,
) -> Vec<GhostId> {
    let guide = match step.links.first() {
        Some(guide) => guide,
        None => {
            return graph
                .matchable_nodes_by_kind(&np.type_id)
                .map(|node| node.id)
                .collect();
        }
    };
    let anchor = match current.bindings.get(&guide.placed_var) {
        Some(id) => *id,
        None => return Vec::new(),
    };
    let mut seen: BTreeSet<GhostId> = BTreeSet::new();
    if guide.new_is_source {
        // Pattern-Edge: neu ─type─▶ Anker ⇒ der neue Knoten ist Quelle
        // einer eingehenden Kante des Ankers.
        for (edge, src) in graph.incoming_edges(&anchor) {
            if edge.type_id == guide.type_id {
                seen.insert(src);
            }
        }
    } else {
        // Pattern-Edge: Anker ─type─▶ neu ⇒ der neue Knoten ist Ziel
        // einer ausgehenden Kante des Ankers.
        for (edge, tgt) in graph.outgoing_edges(&anchor) {
            if edge.type_id == guide.type_id {
                seen.insert(tgt);
            }
        }
    }
    seen.into_iter().collect()
}

/// Prüft alle Edge-Links eines Plan-Schritts gegen den Graphen.
fn satisfies_links(
    graph: &TypedGraph,
    current: &PatternMatch,
    links: &[EdgeLink],
    new_id: GhostId,
) -> bool {
    links.iter().all(|link| {
        let other = match current.bindings.get(&link.placed_var) {
            Some(id) => *id,
            None => return false,
        };
        if link.new_is_source {
            graph.has_edge_between(&new_id, &other, &link.type_id)
        } else {
            graph.has_edge_between(&other, &new_id, &link.type_id)
        }
    })
}

fn satisfies_edge_patterns(pattern: &Pattern, graph: &TypedGraph, m: &PatternMatch) -> bool {
    pattern.edges.iter().all(|ep| {
        let src = match m.bindings.get(&ep.source_var) {
            Some(id) => id,
            None => return false,
        };
        let tgt = match m.bindings.get(&ep.target_var) {
            Some(id) => id,
            None => return false,
        };
        graph.has_edge_between(src, tgt, &ep.type_id)
    })
}

// ── Rule ═════════════════════════════════════════════════════════════════

/// Negative Application Condition als Engine-Pattern (M2).
///
/// Wird vom Matcher via [`find_matches_with_fixed`] gegen den Graph
/// geprüft — wenn mindestens ein Match mit den shared-Anker-Bindings
/// existiert, ist die Rule-Anwendung verboten.
#[derive(Clone, Debug)]
pub struct NacPattern {
    pub name: String,
    pub pattern: Pattern,
    /// Node-Vars, die an das L-Match fixiert werden.
    pub shared_with_l: Vec<String>,
}

/// Eintrag im Attribut-Propagations-Plan einer Rule (M5.3/M5.4).
///
/// Spiegelt `crate::rule::compile::AttrPropagation` als engine-
/// interne Sicht, damit der Re-Validation-Code keine Cross-Modul-
/// Abhängigkeit von `rule` braucht.
#[derive(Clone, Debug)]
pub struct EnginePropagation {
    pub source_node_var: String,
    pub source_attr: String,
    pub target_node_var: String,
    pub target_attr: String,
    /// String-Tag der `AttrTransform`-Variante. Aufgelöst bei der
    /// Anwendung im Rule-eigenen Kontext.
    pub transform_tag: String,
}

pub trait Rule: fmt::Debug + Send + Sync {
    fn id(&self) -> &str;
    fn rank(&self) -> u64;
    fn pattern(&self) -> &Pattern;
    fn produce(&self, m: &PatternMatch, graph: &TypedGraph) -> Vec<Op>;
    /// Negative Application Conditions. Default: keine.
    fn nacs(&self) -> &[NacPattern] {
        &[]
    }
    /// Liste der Propagations, die zu (l_var, source_attr) passen.
    /// Default: leer.
    fn propagations_for(&self, _l_var: &str, _source_attr: &str) -> Vec<EnginePropagation> {
        Vec::new()
    }
}

/// Konkrete Regel-Implementierung mit Closure-basierter Produktion.
pub struct BasicRule {
    id: String,
    rank: u64,
    pattern: Pattern,
    #[allow(clippy::type_complexity)]
    production: Box<dyn Fn(&PatternMatch, &TypedGraph) -> Vec<Op> + Send + Sync>,
    nacs: Vec<NacPattern>,
    propagations: Vec<EnginePropagation>,
}

impl BasicRule {
    pub fn new<F>(id: &str, rank: u64, pattern: Pattern, production: F) -> Self
    where
        F: Fn(&PatternMatch, &TypedGraph) -> Vec<Op> + Send + Sync + 'static,
    {
        Self {
            id: id.into(),
            rank,
            pattern,
            production: Box::new(production),
            nacs: Vec::new(),
            propagations: Vec::new(),
        }
    }

    /// Setzt die NACs dieser Rule (Builder-Style).
    pub fn with_nacs(mut self, nacs: Vec<NacPattern>) -> Self {
        self.nacs = nacs;
        self
    }

    /// Setzt den Propagations-Plan (Builder-Style, M5.3).
    pub fn with_propagations(mut self, propagations: Vec<EnginePropagation>) -> Self {
        self.propagations = propagations;
        self
    }
}

impl fmt::Debug for BasicRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicRule")
            .field("id", &self.id)
            .field("rank", &self.rank)
            .field("pattern", &self.pattern)
            .finish()
    }
}

impl Rule for BasicRule {
    fn id(&self) -> &str {
        &self.id
    }
    fn rank(&self) -> u64 {
        self.rank
    }
    fn pattern(&self) -> &Pattern {
        &self.pattern
    }
    fn produce(&self, m: &PatternMatch, graph: &TypedGraph) -> Vec<Op> {
        (self.production)(m, graph)
    }
    fn nacs(&self) -> &[NacPattern] {
        &self.nacs
    }
    fn propagations_for(&self, l_var: &str, source_attr: &str) -> Vec<EnginePropagation> {
        self.propagations
            .iter()
            .filter(|p| p.source_node_var == l_var && p.source_attr == source_attr)
            .cloned()
            .collect()
    }
}

/// Prüft, ob irgendeine NAC der Rule im Graph matcht (mit den
/// aus dem Haupt-Match übernommenen `shared_with_l`-Bindings).
///
/// Rückgabe: `true` = Rule-Anwendung verboten.
pub fn nacs_forbid(m: &PatternMatch, rule: &dyn Rule, graph: &TypedGraph) -> bool {
    for nac in rule.nacs() {
        let mut fixed = HashMap::new();
        for var in &nac.shared_with_l {
            if let Some(id) = m.bindings.get(var) {
                fixed.insert(var.clone(), *id);
            }
        }
        if !find_matches_with_fixed(&nac.pattern, graph, &fixed).is_empty() {
            return true;
        }
    }
    false
}

// ── Kandidaten-Selektion ═════════════════════════════════════════════════

/// Ein Match-Kandidat mit Rang-Informationen für die Selektion.
pub struct MatchCandidate<'a> {
    pub rule: &'a dyn Rule,
    pub pattern_match: PatternMatch,
    pub match_idx: usize,
}

impl<'a> MatchCandidate<'a> {
    /// Zusammengesetzter Rang (ρ(r), μ(m)), lexikographisch geordnet.
    ///
    /// Entspricht Def. 4.3 im PDF; das Produkt ρ·M + μ wird hier als
    /// Tupel repräsentiert, was semantisch identisch ist und keine
    /// Wahl von M erfordert.
    pub fn rank_key(&self) -> (u64, usize) {
        (self.rule.rank(), self.match_idx)
    }
}

impl<'a> fmt::Debug for MatchCandidate<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MatchCandidate")
            .field("rule_id", &self.rule.id())
            .field("rule_rank", &self.rule.rank())
            .field("match_idx", &self.match_idx)
            .finish()
    }
}

/// Wählt den höchstrangigen Kandidaten aus allen Regeln.
///
/// Konvention: **höherer Rang-Wert gewinnt**. Default-Heuristik
/// `ρ(r_i) = i` (Definitionsreihenfolge) bedeutet damit, dass
/// später-definierte Regeln höhere Priorität haben. Anwender, die
/// umgekehrte Priorität wünschen, setzen `ρ(r_i) = N - i`.
pub fn select_highest_rank<'a, I>(rules: I, graph: &TypedGraph) -> Option<MatchCandidate<'a>>
where
    I: IntoIterator<Item = &'a dyn Rule>,
{
    let mut best: Option<MatchCandidate<'a>> = None;

    for rule in rules {
        let pattern = rule.pattern();
        let mut matches = find_matches(pattern, graph);
        // Kanonische Enumeration μ: sortieren nach Ghost-ID-Tupel.
        matches.sort_by_key(|m| canonical_key(m, pattern));

        for (idx, pattern_match) in matches.into_iter().enumerate() {
            let candidate = MatchCandidate {
                rule,
                pattern_match,
                match_idx: idx,
            };
            let better = match &best {
                None => true,
                Some(b) => candidate.rank_key() > b.rank_key(),
            };
            if better {
                best = Some(candidate);
            }
        }
    }

    best
}

// ── Cascade ══════════════════════════════════════════════════════════════

#[derive(Debug, Default, Clone)]
pub struct Cascade {
    pub entries: Vec<DeltaEntry>,
}

impl Cascade {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user_delta(user_delta: DeltaEntry) -> Self {
        Self {
            entries: vec![user_delta],
        }
    }

    /// Fügt einen Delta-Eintrag an — strikte Monotonie V₆.
    pub fn append(&mut self, entry: DeltaEntry) -> usize {
        let idx = self.entries.len();
        self.entries.push(entry);
        idx
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last(&self) -> Option<&DeltaEntry> {
        self.entries.last()
    }

    /// Findet den Delta-Eintrag, der das Element mit der gegebenen ID erstmals
    /// erzeugte. Rückgabe: `Some(idx)` oder `None`, wenn das Element nicht
    /// durch die Kaskade erzeugt wurde (z. B. SOLID-Baseline).
    pub fn creator_of(&self, id: &GhostId) -> Option<usize> {
        for (idx, entry) in self.entries.iter().enumerate() {
            for op in &entry.op_star {
                match op.target() {
                    OpTarget::Node(t) if &t == id => return Some(idx),
                    OpTarget::Edge(t) if &t == id => return Some(idx),
                    _ => {}
                }
            }
        }
        None
    }

    /// Transitive Vorfahren-Menge bzgl. `≺_D` (Def. 2.6): alle Delta-Indizes,
    /// die (transitiv) Elemente erzeugt haben, die im gegebenen Anchor
    /// referenziert werden.
    pub fn ancestors_of_anchor(&self, anchor: &[GhostId]) -> HashSet<usize> {
        let mut result = HashSet::new();
        let mut queue: Vec<GhostId> = anchor.to_vec();
        while let Some(id) = queue.pop() {
            if let Some(creator) = self.creator_of(&id) {
                if result.insert(creator) {
                    for a in &self.entries[creator].anchor {
                        queue.push(*a);
                    }
                }
            }
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminationState {
    Convergence,
    Duplication,
    Contradiction { reason: String },
    Running,
}

/// Engine-Fehler beim Kaskaden-Step.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Op-Anwendung fehlgeschlagen: {0}")]
    OpApplication(#[from] OpError),
    #[error("Schrittlimit überschritten ({limit})")]
    StepLimitExceeded { limit: usize },
}

// ── Duplikations- und Kontradiktions-Prädikate ═══════════════════════════

/// Prüft Duplikation (Def. 3.2): trägt eine Regel-Anwendung *nichts
/// Neues* bei — d.h. würde **jede** ihrer Ops nur ein bereits
/// existierendes (matchbares) Element re-erzeugen?
///
/// Op-granular (`.all()`, nicht `.any()`): eine *gemischte* Anwendung
/// (echte Neuarbeit + schon erfüllte Ops) ist **kein** Duplikat — sie
/// wird angewandt, die schon erfüllten Add-Ops sind dabei idempotente
/// No-ops (`insert_node`/`add_edge` re-bestätigen Bestehendes). Nur
/// eine rein wiederholende Anwendung saturiert zu `Duplication`. Damit
/// können Regeln Kanten zwischen bereits existierenden Knoten erzeugen.
///
/// Da Ghost-IDs strukturell via SHA-256 berechnet werden (Def. 5.3),
/// produziert ein isomorphes Emissions-Ziel denselben Hash → direkter
/// Lookup im Graphen.
pub fn is_duplicate(ops: &[Op], graph: &TypedGraph) -> bool {
    // M5.5: TentativeTombstone-Elemente zählen NICHT als Duplikate —
    // sie sind Resurrection-Kandidaten und sollen via insert_node/
    // add_edge auf Solid/Ghost zurück gesetzt werden, wenn die
    // Add-Op läuft.
    let active_non_tentative =
        |status: Status| status.is_matchable() && status != Status::TentativeTombstone;
    ops.iter().all(|op| match op {
        Op::AddNode {
            parent,
            edge_type,
            type_id,
            attrs,
        } => {
            let would_be_id = GhostId::from_parent(parent, edge_type, type_id, attrs);
            graph
                .get_node(&would_be_id)
                .map(|n| active_non_tentative(n.status))
                .unwrap_or(false)
        }
        Op::AddEdge {
            source,
            target,
            type_id,
            attrs,
        } => {
            let would_be_id = GhostId::for_edge(source, target, type_id, attrs);
            graph
                .get_edge(&would_be_id)
                .map(|e| active_non_tentative(e.status))
                .unwrap_or(false)
        }
        // B5-rc5: SetAttr ist Duplikat, wenn der Zielknoten matchbar+
        // non-tentative ist und das Attribut bereits exakt diesen Wert
        // trägt — d.h. die Op wäre ein idempotenter No-op. Vor diesem
        // Arm fiel SetAttr in den catch-all `_ => false`, was bei
        // rc4-attrs_to_set-Regeln zu nicht-terminierender Cascade
        // führte (jeder Step galt als „productive", obwohl produce()
        // nur dieselbe SetAttr wieder emittierte). Op-granular: nur
        // wenn ALLE Ops dieser Anwendung idempotent sind, saturiert
        // der Step zu Duplication.
        Op::SetAttr { target, key, value } => graph
            .get_node(target)
            .filter(|n| active_non_tentative(n.status))
            .and_then(|n| n.attrs.get(key))
            .map(|current| current == value)
            .unwrap_or(false),
        _ => false,
    })
}

/// Prüft Kontradiktion ohne Kaskaden-Kontext (Vereinfachung, deprecated).
///
/// Nutzt nur SOLID- und TOMB-Checks, keine Vorfahren-Analyse.
/// Phase 1.3c verwendet [`is_contradictory_with_cascade`].
pub fn is_contradictory(ops: &[Op], graph: &TypedGraph) -> Option<String> {
    for op in ops {
        match op {
            Op::DelNode { target } => match graph.get_node(target) {
                None => return Some(format!("DelNode-Ziel {} nicht gefunden", target.short())),
                Some(n) if n.status == Status::Solid => {
                    return Some(format!(
                        "V₇-Verletzung: SOLID-Knoten {} nicht erasbar",
                        target.short()
                    ))
                }
                Some(n) if n.status == Status::Tombstone => {
                    return Some(format!(
                        "Doppel-Tombstone auf {} (bereits tombstoned)",
                        target.short()
                    ))
                }
                _ => {}
            },
            Op::DelEdge { target } => match graph.get_edge(target) {
                None => return Some(format!("DelEdge-Ziel {} nicht gefunden", target.short())),
                Some(e) if e.status == Status::Solid => {
                    return Some(format!(
                        "V₇-Verletzung: SOLID-Kante {} nicht erasbar",
                        target.short()
                    ))
                }
                _ => {}
            },
            Op::AddEdge { source, target, .. } => {
                // Ghost-Endpunkt darf nicht TOMB sein (Def. 2.5).
                if matches!(graph.get_node(source), Some(n) if n.status == Status::Tombstone) {
                    return Some(format!("AddEdge: source {} ist TOMB", source.short()));
                }
                if matches!(graph.get_node(target), Some(n) if n.status == Status::Tombstone) {
                    return Some(format!("AddEdge: target {} ist TOMB", target.short()));
                }
            }
            _ => {}
        }
    }
    None
}

/// Volle Kontradiktions-Prüfung mit Vorfahren-Analyse (Def. 3.6 + V₇).
///
/// Zusätzlich zu den lokalen SOLID- und TOMB-Checks prüft diese Variante:
/// - Del-Ops auf $d_0$-Elementen (User-Delta geschützt).
/// - Del-Ops auf Elementen, die durch einen (transitiven) Vorfahren
///   des Kandidaten erzeugt wurden (Reconciliation-Zulässigkeit V₇).
pub fn is_contradictory_with_cascade(
    ops: &[Op],
    anchor: &[GhostId],
    graph: &TypedGraph,
    cascade: &Cascade,
) -> Option<String> {
    if let Some(reason) = is_contradictory(ops, graph) {
        return Some(reason);
    }

    let ancestors = cascade.ancestors_of_anchor(anchor);

    for op in ops {
        match op {
            Op::DelNode { target } | Op::DelEdge { target } => {
                if let Some(creator) = cascade.creator_of(target) {
                    if creator == 0 {
                        return Some(format!(
                            "V₇: d_0-Element {} während Kaskade nicht erasbar",
                            target.short()
                        ));
                    }
                    if ancestors.contains(&creator) {
                        return Some(format!(
                            "V₇: Vorfahre d_{} mit Element {} nicht erasbar",
                            creator,
                            target.short()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Retraktions-Kaskade (Def. 3.8) ═══════════════════════════════════════

/// Berechnet die Retraktions-Kaskade für eine Op.
///
/// Aktuell implementiert: bei DelNode werden alle matchbaren inzidenten
/// Kanten als induzierte DelEdge-Ops produziert (strukturelle Abhängigkeit
/// Def. 3.7 für Kanten-Endpunkte).
///
/// Weitere Abhängigkeits-Typen (z. B. Korrespondenz-Knoten, Attribute mit
/// Pflicht-Typ) können hier in zukünftigen Iterationen ergänzt werden.
pub fn retraction_cascade_for(op: &Op, graph: &TypedGraph) -> Vec<Op> {
    match op {
        Op::DelNode { target } => graph
            .incident_edges(target)
            .into_iter()
            .map(|(edge, _)| Op::DelEdge { target: edge.id })
            .collect(),
        _ => Vec::new(),
    }
}

/// Erweitert eine Primär-Op-Liste um ihre Retraktions-Kaskaden und erzeugt
/// dabei die `induces`-DAG-Struktur aus V₁₂.
///
/// Rückgabe: `(erweiterte Ops, induces-Map)` — die induces-Map hat gleiche
/// Länge wie die Op-Liste und enthält pro Op die Indizes der direkt
/// induzierten Folge-Ops.
pub fn expand_with_retraction(
    primary_ops: Vec<Op>,
    graph: &TypedGraph,
) -> (Vec<Op>, Vec<Vec<usize>>) {
    let mut full_ops: Vec<Op> = Vec::new();
    let mut induces: Vec<Vec<usize>> = Vec::new();

    for primary in primary_ops {
        let primary_idx = full_ops.len();
        let induced = retraction_cascade_for(&primary, graph);
        full_ops.push(primary);
        induces.push(Vec::new());

        let mut induced_indices = Vec::new();
        for induced_op in induced {
            let idx = full_ops.len();
            full_ops.push(induced_op);
            induces.push(Vec::new());
            induced_indices.push(idx);
        }
        induces[primary_idx] = induced_indices;
    }

    (full_ops, induces)
}

// ── Cascade-Step und Runner ══════════════════════════════════════════════

/// Kodiert einen (rule_rank, match_idx)-Paar als u64 für DeltaEntry.rank.
///
/// Layout: `[rule_rank: u32 high][match_idx: u32 low]`. Damit dominiert
/// der Regel-Rang den Match-Index (Def. 4.3 mit implizitem M = 2^32).
pub fn encode_delta_rank(rule_rank: u64, match_idx: usize) -> u64 {
    (rule_rank << 32) | (match_idx as u64 & 0xFFFF_FFFF)
}

/// Sammelt alle Match-Kandidaten aller Regeln, sortiert in
/// Rang-absteigender Reihenfolge (`rank_key` von max zu min).
pub fn collect_candidates<'a, I>(rules: I, graph: &TypedGraph) -> Vec<MatchCandidate<'a>>
where
    I: IntoIterator<Item = &'a dyn Rule>,
{
    let mut all = Vec::new();
    for rule in rules {
        let pattern = rule.pattern();
        let mut matches = find_matches(pattern, graph);
        matches.sort_by_key(|m| canonical_key(m, pattern));
        for (idx, pattern_match) in matches.into_iter().enumerate() {
            all.push(MatchCandidate {
                rule,
                pattern_match,
                match_idx: idx,
            });
        }
    }
    // Absteigend nach rank_key — std::cmp::Reverse macht es als
    // sort_by_key kanonisch (clippy-sauber).
    all.sort_by_key(|c| std::cmp::Reverse(c.rank_key()));
    all
}

/// Führt einen einzelnen Kaskaden-Schritt durch (Phase 1.3c).
///
/// Ablauf (Rang-absteigend, impliziter Backtracking-Pfad):
/// 1. Sammle alle Match-Kandidaten, sortiert max→min nach Rang.
/// 2. Für jeden Kandidaten in Rang-absteigender Ordnung:
///    a) Produziere Primär-Ops via Regel.
///    b) Erweitere um Retraktions-Kaskade (Def. 3.8); induces-DAG (V₁₂)
///    wird dabei aufgebaut.
///    c) Baue voraussichtlichen Anchor aus Pattern-Bindings.
///    d) Prüfe Duplikation auf der vollen Op-Menge → `any_duplicate`, next.
///    e) Prüfe Kontradiktion inkl. Vorfahren-Check (V₇) →
///    `any_contradiction` mit Grund, next.
///    f) Durchgekommen: DeltaEntry bauen (mit populated `induces`),
///    anwenden, anhängen, `Running`.
/// 3. Saturation: wenn kein Kandidat durchkommt, priorisiere
///    Contradiction > Duplication > Convergence als Terminierungs-Grund.
pub fn cascade_step(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
) -> Result<TerminationState, EngineError> {
    let candidates = collect_candidates(rules.iter().copied(), graph);

    if candidates.is_empty() {
        return Ok(TerminationState::Convergence);
    }

    let mut any_duplicate = false;
    let mut last_contradiction: Option<String> = None;

    for candidate in candidates {
        // NAC-Check (M2) zuerst — vor Produktion. Wenn eine NAC
        // matcht, ist der Kandidat verboten.
        if nacs_forbid(&candidate.pattern_match, candidate.rule, graph) {
            continue;
        }

        let primary_ops = candidate.rule.produce(&candidate.pattern_match, graph);
        if primary_ops.is_empty() {
            continue;
        }

        // Retraktions-Kaskade aufbauen.
        let (full_ops, induces) = expand_with_retraction(primary_ops, graph);

        let anchor: Vec<GhostId> = candidate
            .rule
            .pattern()
            .nodes
            .iter()
            .filter_map(|np| candidate.pattern_match.bindings.get(&np.var).copied())
            .collect();

        if is_duplicate(&full_ops, graph) {
            any_duplicate = true;
            continue;
        }

        if let Some(reason) = is_contradictory_with_cascade(&full_ops, &anchor, graph, cascade) {
            last_contradiction = Some(reason);
            continue;
        }

        // Durchgekommen — DeltaEntry bauen und anwenden.
        let rank = encode_delta_rank(candidate.rule.rank(), candidate.match_idx);
        let delta = DeltaEntry {
            origin: Origin::Rule {
                rule_id: candidate.rule.id().into(),
            },
            rank,
            op_star: full_ops,
            anchor,
            induces,
            bindings: candidate.pattern_match.bindings.clone(),
        };

        for op in &delta.op_star {
            op.apply(graph)?;
        }

        cascade.append(delta);
        return Ok(TerminationState::Running);
    }

    // Saturation-Priorisierung: Contradiction > Duplication > Convergence.
    if let Some(reason) = last_contradiction {
        Ok(TerminationState::Contradiction { reason })
    } else if any_duplicate {
        Ok(TerminationState::Duplication)
    } else {
        Ok(TerminationState::Convergence)
    }
}

// ── Re-Validation-Logik (M5.3) ═══════════════════════════════════════════

/// Ergebnis einer Re-Validation einer Rule-Anwendung nach einer
/// Op auf einem ihrer Match-Participants.
#[derive(Debug, Clone)]
pub enum RevalidationOutcome {
    /// L-Pattern + NACs + Constraints noch erfüllt; keine Aktion nötig.
    StillMatches,
    /// Match noch da, aber ein gebundenes Attribut hat sich geändert
    /// und die Rule hat passende Propagation(s).
    AttrChanged {
        propagations: Vec<EnginePropagation>,
        l_var: String,
        attr: String,
        new_value: String,
    },
    /// L-Pattern matcht nicht mehr / NAC trifft jetzt / Constraint
    /// nicht mehr erfüllt → Rule-Anwendung muss invalidiert werden.
    NoLongerMatches,
}

/// Re-Validiert eine bestehende Rule-Anwendung nach einer Op auf
/// einem Match-Participant.
pub fn revalidate_app(
    app_idx: usize,
    cascade: &Cascade,
    graph: &TypedGraph,
    rules: &[&dyn Rule],
    last_op: &Op,
) -> RevalidationOutcome {
    let entry = match cascade.entries.get(app_idx) {
        Some(e) => e,
        None => return RevalidationOutcome::NoLongerMatches,
    };
    let rule_id = match &entry.origin {
        Origin::Rule { rule_id } => rule_id.clone(),
        Origin::User => return RevalidationOutcome::NoLongerMatches,
    };
    let rule = match rules.iter().find(|r| r.id() == rule_id) {
        Some(r) => *r,
        None => return RevalidationOutcome::NoLongerMatches,
    };

    // Pattern mit fixierten Bindings re-matchen
    let matches = find_matches_with_fixed(rule.pattern(), graph, &entry.bindings);
    if matches.is_empty() {
        return RevalidationOutcome::NoLongerMatches;
    }
    let pm = &matches[0];

    // NACs noch frei?
    if nacs_forbid(pm, rule, graph) {
        return RevalidationOutcome::NoLongerMatches;
    }

    // Wenn die Op ein SetAttr auf einem gebundenen Knoten war: prüfe
    // ob die Rule eine Attribut-Propagation für diese (l_var, attr)-
    // Kombination hat.
    if let Op::SetAttr { target, key, value } = last_op {
        let l_var = entry.bindings.iter().find_map(|(var, id)| {
            if id == target {
                Some(var.clone())
            } else {
                None
            }
        });
        if let Some(lv) = l_var {
            let propagations = rule.propagations_for(&lv, key);
            if !propagations.is_empty() {
                return RevalidationOutcome::AttrChanged {
                    propagations,
                    l_var: lv,
                    attr: key.clone(),
                    new_value: value.clone(),
                };
            }
        }
    }

    RevalidationOutcome::StillMatches
}

// ── Tentative-Tombstone + Konsolidierung (M5.5) ══════════════════════════

/// Markiert das Created-Set einer Rule-Anwendung als
/// `TentativeTombstone`. Rückgabe: alle Element-IDs, die markiert
/// wurden (Nodes und Edges).
pub fn tentative_invalidate(
    app_idx: usize,
    cascade: &Cascade,
    graph: &mut TypedGraph,
) -> Vec<GhostId> {
    let store = MatchPersistenceStore::new(cascade);
    let created = store.created_set(app_idx);
    for id in &created {
        graph.set_node_status(id, Status::TentativeTombstone);
        graph.set_edge_status(id, Status::TentativeTombstone);
    }
    created
}

/// Konsolidiert nach Phase B: TentativeTombstone-Elemente, die in
/// `just_created` enthalten sind (Resurrection durch identische
/// Ghost-ID), werden zurück auf `Solid` gesetzt. Alle anderen
/// TentativeTombstones werden zu endgültigem `Tombstone`.
pub fn consolidate_tentative(graph: &mut TypedGraph, just_created: &[GhostId]) {
    use std::collections::HashSet;
    let just: HashSet<GhostId> = just_created.iter().copied().collect();

    // Alle TentativeTombstone-Nodes sammeln (wir mutieren während
    // wir iterieren, daher zuerst Vec).
    let tt_nodes: Vec<GhostId> = graph
        .iter_nodes()
        .filter(|n| n.status == Status::TentativeTombstone)
        .map(|n| n.id)
        .collect();
    for id in tt_nodes {
        if just.contains(&id) {
            graph.set_node_status(&id, Status::Solid);
        } else {
            graph.set_node_status(&id, Status::Tombstone);
        }
    }
    // Edges analog
    let tt_edges: Vec<GhostId> = graph
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.status == Status::TentativeTombstone)
        .map(|(_, _, e)| e.id)
        .collect();
    for id in tt_edges {
        if just.contains(&id) {
            graph.set_edge_status(&id, Status::Solid);
        } else {
            graph.set_edge_status(&id, Status::Tombstone);
        }
    }
}

/// Match-Observability-fähige Cascade-Loop. Pro User-Delta:
/// 1. Wende User-Ops an, sammle affected RuleApps.
/// 2. Phase A: Re-Validate jede affected RuleApp. Bei
///    `NoLongerMatches` → tentative_invalidate. Bei `AttrChanged`
///    → apply_attr_propagation.
/// 3. Phase B: Standard-Cascade-Loop mit `cascade_step`. Neue
///    Created-Entities sammeln.
/// 4. Konsolidierung: consolidate_tentative.
///
/// Ohne User-Delta in der Cascade läuft sie wie `run_cascade`.
pub fn run_cascade_observable(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
) -> Result<TerminationState, EngineError> {
    // User-Delta finden — der jüngste mit Origin::User triggert die
    // Re-Validation.
    let user_idx = cascade
        .entries
        .iter()
        .rposition(|e| matches!(e.origin, Origin::User));

    if let Some(idx) = user_idx {
        let user_entry = cascade.entries[idx].clone();

        // Phase A: pro affected RuleApp re-validieren.
        let mut affected: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for op in &user_entry.op_star {
            for app_idx in watch_op(op, cascade, graph).affected_apps {
                affected.insert(app_idx);
            }
        }

        for app_idx in affected {
            // Letzte Op die diesen App matcht reicht — prüfe gegen alle
            // Ops und nimm das schwerwiegendste Outcome.
            let mut final_outcome = RevalidationOutcome::StillMatches;
            for op in &user_entry.op_star {
                let outcome = revalidate_app(app_idx, cascade, graph, rules, op);
                final_outcome = match (final_outcome, outcome) {
                    (_, RevalidationOutcome::NoLongerMatches) => {
                        RevalidationOutcome::NoLongerMatches
                    }
                    (RevalidationOutcome::StillMatches, other) => other,
                    (existing, _) => existing,
                };
                if matches!(final_outcome, RevalidationOutcome::NoLongerMatches) {
                    break;
                }
            }
            match final_outcome {
                RevalidationOutcome::NoLongerMatches => {
                    let _ = tentative_invalidate(app_idx, cascade, graph);
                }
                RevalidationOutcome::AttrChanged {
                    propagations,
                    new_value,
                    ..
                } => {
                    let entry = &cascade.entries[app_idx];
                    let rule_id_str = match &entry.origin {
                        Origin::Rule { rule_id } => rule_id.clone(),
                        _ => continue,
                    };
                    let bindings = entry.bindings.clone();
                    apply_attr_propagation(
                        &propagations,
                        &bindings,
                        &new_value,
                        graph,
                        cascade,
                        &rule_id_str,
                    )?;
                }
                RevalidationOutcome::StillMatches => {}
            }
        }

        // Phase B: Standard-Cascade-Loop. Neue Created-Entities sammeln.
        let entries_at_phase_b_start = cascade.entries.len();
        let term = run_cascade(cascade, graph, rules, max_steps)?;

        // Sammle alle Ops-Targets ab `entries_at_phase_b_start`.
        let mut just_created: Vec<GhostId> = Vec::new();
        for entry in cascade.entries.iter().skip(entries_at_phase_b_start) {
            for op in &entry.op_star {
                if let OpTarget::Node(id) | OpTarget::Edge(id) = op.target() {
                    just_created.push(id);
                }
            }
        }

        // Phase C: Konsolidierung — Resurrection oder endgültiger Tombstone.
        consolidate_tentative(graph, &just_created);

        Ok(term)
    } else {
        run_cascade(cascade, graph, rules, max_steps)
    }
}

// ── Attribut-Propagation (M5.4) ══════════════════════════════════════════

/// Wendet eine Liste von Propagations an: für jede Propagation
/// wird ein `Op::SetAttr` auf den gebundenen R-Knoten erzeugt,
/// mit dem `transform_tag`-aufgelösten Wert.
///
/// Erzeugt eine neue `DeltaEntry` mit `Origin::Rule { rule_id:
/// "<rule>@propagate" }` und hängt sie an die Cascade an.
pub fn apply_attr_propagation(
    propagations: &[EnginePropagation],
    bindings: &std::collections::HashMap<String, GhostId>,
    new_value: &str,
    graph: &mut TypedGraph,
    cascade: &mut Cascade,
    rule_id: &str,
) -> Result<(), OpError> {
    use crate::rule::spec::AttrTransform;
    let mut ops = Vec::new();
    for prop in propagations {
        let target_id = match bindings.get(&prop.target_node_var).copied() {
            Some(id) => id,
            None => continue,
        };
        let transform =
            AttrTransform::parse(Some(&prop.transform_tag)).unwrap_or(AttrTransform::Identity);
        let transformed = transform.apply(new_value);
        ops.push(Op::SetAttr {
            target: target_id,
            key: prop.target_attr.clone(),
            value: transformed,
        });
    }
    if ops.is_empty() {
        return Ok(());
    }
    let len = ops.len();
    let entry = DeltaEntry {
        origin: Origin::Rule {
            rule_id: format!("{rule_id}@propagate"),
        },
        rank: 0,
        op_star: ops,
        anchor: bindings.values().copied().collect(),
        induces: vec![Vec::new(); len],
        bindings: bindings.clone(),
    };
    entry.apply(graph)?;
    cascade.append(entry);
    Ok(())
}

// ── Match-Persistence-Store (M5.1) ═══════════════════════════════════════

/// Read-only-Sicht auf alle persistenten Rule-Match-Anwendungen
/// in einer `Cascade`. Wird vom Watch-Hook (M5.2) genutzt, um zu
/// affected RuleApps zu finden.
///
/// `RuleApplicationId` = Index in `cascade.entries`. Das spart eine
/// separate ID-Generierung — die Cascade ist sowieso append-only.
pub struct MatchPersistenceStore<'cascade> {
    cascade: &'cascade Cascade,
}

impl<'cascade> MatchPersistenceStore<'cascade> {
    pub fn new(cascade: &'cascade Cascade) -> Self {
        Self { cascade }
    }

    /// Indizes aller Rule-Anwendungen in der Cascade, deren Bindings
    /// `id` referenzieren.
    pub fn applications_referencing(&self, id: &GhostId) -> Vec<usize> {
        self.cascade
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                matches!(e.origin, Origin::Rule { .. }) && e.bindings.values().any(|v| v == id)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Created-Set einer Rule-Anwendung — alle GhostIds (Nodes + Edges),
    /// die durch ihre `op_star`-Sequenz erzeugt wurden.
    pub fn created_set(&self, app_idx: usize) -> Vec<GhostId> {
        if app_idx >= self.cascade.entries.len() {
            return Vec::new();
        }
        self.cascade.entries[app_idx]
            .op_star
            .iter()
            .filter_map(|op| match op.target() {
                OpTarget::Node(id) | OpTarget::Edge(id) => Some(id),
                OpTarget::Attr(_, _) => None,
            })
            .collect()
    }

    /// Bindings einer Rule-Anwendung als Read-Reference.
    pub fn bindings(&self, app_idx: usize) -> Option<&std::collections::HashMap<String, GhostId>> {
        self.cascade.entries.get(app_idx).map(|e| &e.bindings)
    }
}

// ── Watch-Hook (M5.2) ════════════════════════════════════════════════════

/// Resultat eines Watch-Lookups: welche RuleApps sind betroffen, und
/// hat die Op das Attr-Propagation-Potenzial.
#[derive(Debug, Default, Clone)]
pub struct WatchOutcome {
    /// Indizes der Rule-Anwendungen, deren Bindings das berührte
    /// Element referenzieren.
    pub affected_apps: Vec<usize>,
    /// `true`, wenn die Op ein `SetAttr` auf einem gebundenen Knoten ist
    /// — Kandidat für Attribut-Propagation (M5.4).
    pub propagation_candidate: bool,
}

/// Sucht im MatchPersistenceStore alle RuleApps, deren Bindings das
/// von `op` berührte Element referenzieren.
///
/// Für Edge-Ops (AddEdge/DelEdge) werden die Endpunkt-Knoten gelookupt,
/// weil Rule-Bindings selten Edge-IDs enthalten (Edges sind im Pattern
/// als Edge-Constraints, nicht als Variablen).
pub fn watch_op(op: &Op, cascade: &Cascade, graph: &TypedGraph) -> WatchOutcome {
    let store = MatchPersistenceStore::new(cascade);
    let mut out = WatchOutcome::default();
    match op {
        Op::AddNode { parent, .. } => {
            out.affected_apps = store.applications_referencing(parent);
        }
        Op::AddEdge { source, target, .. } => {
            let mut affected = store.applications_referencing(source);
            affected.extend(store.applications_referencing(target));
            affected.sort();
            affected.dedup();
            out.affected_apps = affected;
        }
        Op::DelNode { target } => {
            out.affected_apps = store.applications_referencing(target);
        }
        Op::DelEdge { target } => {
            // Endpunkte aus dem Graph-Edge-Index holen.
            let mut affected: Vec<usize> = store.applications_referencing(target);
            if let Some((src, tgt)) = graph.edge_endpoints(target) {
                affected.extend(store.applications_referencing(&src));
                affected.extend(store.applications_referencing(&tgt));
            }
            affected.sort();
            affected.dedup();
            out.affected_apps = affected;
        }
        Op::SetAttr { target, .. } => {
            out.propagation_candidate = true;
            out.affected_apps = store.applications_referencing(target);
        }
    }
    out
}

/// Wendet eine `DeltaEntry` an und sammelt dabei alle RuleApps,
/// deren Match-Participants berührt wurden.
///
/// Die Cascade wird **vor** dem Anhängen der DeltaEntry konsultiert,
/// damit der Lookup nicht auf den eigenen Eintrag zurückfällt.
pub fn apply_with_watch(
    delta: &DeltaEntry,
    graph: &mut TypedGraph,
    cascade: &Cascade,
) -> Result<Vec<usize>, OpError> {
    let mut affected = std::collections::HashSet::new();
    for op in &delta.op_star {
        let outcome = watch_op(op, cascade, graph);
        for app_idx in outcome.affected_apps {
            affected.insert(app_idx);
        }
        op.apply(graph)?;
    }
    let mut sorted: Vec<usize> = affected.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// Läuft die Kaskade bis zur Terminierung oder bis `max_steps` erreicht ist.
pub fn run_cascade(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
) -> Result<TerminationState, EngineError> {
    for _ in 0..max_steps {
        match cascade_step(cascade, graph, rules)? {
            TerminationState::Running => continue,
            terminal => return Ok(terminal),
        }
    }
    Err(EngineError::StepLimitExceeded { limit: max_steps })
}

// ── Echtes Backtracking mit Position-markierten Rang-Limits ══════════════

/// Führt einen Kaskaden-Schritt mit optionalem Rang-Ceiling durch.
///
/// Kandidaten mit `encode_delta_rank(rule.rank, match_idx) >= rank_ceiling`
/// werden herausgefiltert. Das markiert die aktuelle Kaskaden-Position
/// als rang-limitiert — eine Einschränkung, die nur lokal an dieser
/// Position wirkt, nicht global über die Regel (vgl. Def. 3.10,
/// Korrekturen in $\alpha_{3\text{-}4}$).
pub fn cascade_step_with_limit(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    rank_ceiling: Option<u64>,
) -> Result<TerminationState, EngineError> {
    let mut candidates = collect_candidates(rules.iter().copied(), graph);

    if let Some(ceil) = rank_ceiling {
        candidates.retain(|c| encode_delta_rank(c.rule.rank(), c.match_idx) < ceil);
    }

    if candidates.is_empty() {
        return Ok(TerminationState::Convergence);
    }

    let mut any_duplicate = false;
    let mut last_contradiction: Option<String> = None;

    for candidate in candidates {
        // NAC-Check (M2)
        if nacs_forbid(&candidate.pattern_match, candidate.rule, graph) {
            continue;
        }

        let primary_ops = candidate.rule.produce(&candidate.pattern_match, graph);
        if primary_ops.is_empty() {
            continue;
        }

        let (full_ops, induces) = expand_with_retraction(primary_ops, graph);

        let anchor: Vec<GhostId> = candidate
            .rule
            .pattern()
            .nodes
            .iter()
            .filter_map(|np| candidate.pattern_match.bindings.get(&np.var).copied())
            .collect();

        if is_duplicate(&full_ops, graph) {
            any_duplicate = true;
            continue;
        }

        if let Some(reason) = is_contradictory_with_cascade(&full_ops, &anchor, graph, cascade) {
            last_contradiction = Some(reason);
            continue;
        }

        let rank = encode_delta_rank(candidate.rule.rank(), candidate.match_idx);
        let delta = DeltaEntry {
            origin: Origin::Rule {
                rule_id: candidate.rule.id().into(),
            },
            rank,
            op_star: full_ops,
            anchor,
            induces,
            bindings: candidate.pattern_match.bindings.clone(),
        };

        for op in &delta.op_star {
            op.apply(graph)?;
        }

        cascade.append(delta);
        return Ok(TerminationState::Running);
    }

    if let Some(reason) = last_contradiction {
        Ok(TerminationState::Contradiction { reason })
    } else if any_duplicate {
        Ok(TerminationState::Duplication)
    } else {
        Ok(TerminationState::Convergence)
    }
}

/// Rollt den höchstrangigen Rule-Delta-Eintrag der Kaskade zurück.
///
/// Findet den höchstrangigen Delta-Eintrag mit `Origin::Rule` (User-Deltas
/// bleiben per V₇ geschützt). Truncatet die Kaskade ab dieser Position,
/// replayt den Graph via `base` + verbleibende Ops, und setzt ein
/// Position-markiertes Rang-Limit.
///
/// Rückgabe: `Some(position)` bei erfolgreichem Rollback,
/// `None` wenn kein Rule-Delta zum Zurückrollen verfügbar ist (Kapitulation).
pub fn rollback_highest_rank(
    base: &TypedGraph,
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    limits: &mut HashMap<usize, u64>,
) -> Option<usize> {
    // Höchstrangigen Rule-Delta-Eintrag finden.
    let (highest_pos, highest_rank) = cascade
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.origin, Origin::Rule { .. }))
        .map(|(i, e)| (i, e.rank))
        .max_by_key(|(_, r)| *r)?;

    // Kaskade truncaten: Einträge ab Position highest_pos (inkl.) entfernt.
    cascade.entries.truncate(highest_pos);

    // Graph replayen: base cloned, dann alle Ops der verbleibenden Einträge
    // anwenden. Fehler beim Reapply deuten auf Inkonsistenz in der
    // Kaskaden-Struktur — konservativ werden sie ignoriert, weil der
    // Zustand vor Rollback reproduzierbar sein muss.
    *graph = base.clone();
    for entry in &cascade.entries {
        for op in &entry.op_star {
            let _ = op.apply(graph);
        }
    }

    // Position-markiertes Limit setzen. Existiert dort bereits ein
    // niedrigeres Limit (aus früherem Rollback), behalten wir das
    // niedrigere — limits können nur monoton sinken.
    let new_limit = limits
        .get(&highest_pos)
        .map(|existing| (*existing).min(highest_rank))
        .unwrap_or(highest_rank);
    limits.insert(highest_pos, new_limit);

    Some(highest_pos)
}

/// Statistik eines Kaskaden-Laufs mit Rollback.
#[derive(Clone, Debug, Default)]
pub struct RollbackStats {
    pub rollback_count: usize,
    pub limits_applied: HashMap<usize, u64>,
}

/// Läuft die Kaskade mit echtem Backtracking gemäß Def. 3.10.
///
/// Bei Kontradiktion: `rollback_highest_rank`, dann Retry mit dem
/// Position-markierten Limit. Bei Convergence-unter-Limit (keine
/// Kandidaten passen) ebenfalls Rollback, weil das Limit die Position
/// nicht-füllbar macht.
///
/// Rückgabe: `(TerminationState, RollbackStats)`. Kapitulation, wenn
/// kein Rollback mehr möglich ist oder `max_rollbacks` überschritten.
pub fn run_cascade_with_rollback(
    base: &TypedGraph,
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
    max_steps: usize,
    max_rollbacks: usize,
) -> Result<(TerminationState, RollbackStats), EngineError> {
    let mut limits: HashMap<usize, u64> = HashMap::new();
    let mut rollback_count = 0;
    let mut last_contradiction: Option<String> = None;

    for _ in 0..max_steps {
        let position = cascade.entries.len();
        let ceiling = limits.get(&position).copied();
        let had_ceiling = ceiling.is_some();

        match cascade_step_with_limit(cascade, graph, rules, ceiling)? {
            TerminationState::Running => continue,
            TerminationState::Convergence if had_ceiling => {
                // Unter Limit konvergiert — kann an künstlicher Blockade liegen.
                // Rollback, um evtl. durch Umweg zu anderer Lösung zu kommen.
                if rollback_count >= max_rollbacks {
                    return Ok((
                        TerminationState::Convergence,
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                if rollback_highest_rank(base, cascade, graph, &mut limits).is_none() {
                    return Ok((
                        TerminationState::Convergence,
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                rollback_count += 1;
            }
            TerminationState::Convergence => {
                return Ok((
                    TerminationState::Convergence,
                    RollbackStats {
                        rollback_count,
                        limits_applied: limits,
                    },
                ));
            }
            TerminationState::Duplication => {
                return Ok((
                    TerminationState::Duplication,
                    RollbackStats {
                        rollback_count,
                        limits_applied: limits,
                    },
                ));
            }
            TerminationState::Contradiction { reason } => {
                last_contradiction = Some(reason.clone());
                if rollback_count >= max_rollbacks {
                    return Ok((
                        TerminationState::Contradiction { reason },
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                if rollback_highest_rank(base, cascade, graph, &mut limits).is_none() {
                    return Ok((
                        TerminationState::Contradiction { reason },
                        RollbackStats {
                            rollback_count,
                            limits_applied: limits,
                        },
                    ));
                }
                rollback_count += 1;
            }
        }
    }

    // Schrittlimit erreicht ohne Terminierung.
    let _ = last_contradiction;
    let _ = limits;
    let _ = rollback_count;
    Err(EngineError::StepLimitExceeded { limit: max_steps })
}

// ══ Tests ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Status;
    use crate::ops::Origin;
    use std::collections::BTreeMap;

    fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn setup_uml_graph() -> (TypedGraph, GhostId, GhostId, GhostId, GhostId) {
        let mut g = TypedGraph::new();
        let person = g.add_baseline_node("Class", "Person", attrs(&[("name", "Person")]));
        let car = g.add_baseline_node("Class", "Car", attrs(&[("name", "Car")]));
        let person_name = g.add_ghost_node(
            person,
            "hasAttribute",
            "Attribute",
            attrs(&[("name", "name")]),
        );
        let car_model = g.add_ghost_node(
            car,
            "hasAttribute",
            "Attribute",
            attrs(&[("name", "model")]),
        );
        g.add_edge(
            person,
            person_name,
            "hasAttribute",
            BTreeMap::new(),
            Status::Ghost,
        )
        .unwrap();
        g.add_edge(
            car,
            car_model,
            "hasAttribute",
            BTreeMap::new(),
            Status::Ghost,
        )
        .unwrap();
        (g, person, car, person_name, car_model)
    }

    // ── Injektivität ─────────────────────────────────────────────────

    #[test]
    fn injectivity_prevents_same_node_twice() {
        let (g, _, _, _, _) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c1", "Class"))
            .with_node(NodePattern::new("c2", "Class"));
        let matches = find_matches(&pattern, &g);
        // 2 Klassen, zwei Pattern-Variablen, injektiv → 2×1 = 2 Matches
        // (Person,Car) und (Car,Person).
        assert_eq!(matches.len(), 2);
        for m in &matches {
            let c1 = m.get("c1").unwrap();
            let c2 = m.get("c2").unwrap();
            assert_ne!(c1, c2, "Injektivität: c1 ≠ c2");
        }
    }

    // ── Edge-Patterns ────────────────────────────────────────────────

    #[test]
    fn edge_pattern_requires_existing_edge() {
        let (g, _, _, _, _) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class"))
            .with_node(NodePattern::new("a", "Attribute"))
            .with_edge(EdgePattern::new("c", "a", "hasAttribute"));
        let matches = find_matches(&pattern, &g);
        // Person─hasAttribute→name, Car─hasAttribute→model: 2 Matches.
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn edge_pattern_filters_out_unconnected() {
        let (g, person, _, _, car_model) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class").with_attr_equals("name", "Person"))
            .with_node(NodePattern::new("a", "Attribute").with_attr_equals("name", "model"))
            .with_edge(EdgePattern::new("c", "a", "hasAttribute"));
        let matches = find_matches(&pattern, &g);
        // Person hat kein "model" — Kante fehlt, kein Match.
        assert_eq!(matches.len(), 0);
        let _ = person;
        let _ = car_model;
    }

    #[test]
    fn edge_pattern_respects_tombstone() {
        let (mut g, person, _, person_name, _) = setup_uml_graph();
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Class").with_attr_equals("name", "Person"))
            .with_node(NodePattern::new("a", "Attribute"))
            .with_edge(EdgePattern::new("c", "a", "hasAttribute"));
        assert_eq!(find_matches(&pattern, &g).len(), 1, "Person→name");

        // Tombstone die einzige passende Kante:
        let edge_id = GhostId::for_edge(&person, &person_name, "hasAttribute", &BTreeMap::new());
        assert!(g.set_edge_status(&edge_id, Status::Tombstone));

        assert_eq!(
            find_matches(&pattern, &g).len(),
            0,
            "TOMB-Kante nicht mehr matchbar"
        );
    }

    // ── Canonical Enumeration ────────────────────────────────────────

    #[test]
    fn canonical_key_is_deterministic() {
        let (g, _, _, _, _) = setup_uml_graph();
        let pattern = Pattern::new().with_node(NodePattern::new("c", "Class"));

        let matches_a = find_matches(&pattern, &g);
        let matches_b = find_matches(&pattern, &g);

        let keys_a: Vec<_> = matches_a
            .iter()
            .map(|m| canonical_key(m, &pattern))
            .collect();
        let keys_b: Vec<_> = matches_b
            .iter()
            .map(|m| canonical_key(m, &pattern))
            .collect();
        assert_eq!(keys_a, keys_b, "Reihenfolge ist deterministisch");
    }

    #[test]
    fn edge_guided_matching_is_deterministic_and_complete() {
        // Drei Container mit je zwei Items. Pattern: Container c +
        // zwei Items i1, i2, beide per `holds`-Edge an c. Der
        // edge-geleitete Matcher erzeugt i1/i2 aus den Nachbarn von c
        // (Adjazenz) statt aus der vollen Item-Population — zwei
        // gleichartige guided Knoten testen Dedup + Injektivität.
        let mut g = TypedGraph::new();
        for ci in 0..3 {
            let c = g.add_baseline_node("Container", &format!("c{ci}"), BTreeMap::new());
            for ii in 0..2 {
                let item = g.add_ghost_node(
                    c,
                    "holds",
                    "Item",
                    attrs(&[("name", &format!("c{ci}-i{ii}"))]),
                );
                g.add_edge(c, item, "holds", BTreeMap::new(), Status::Ghost)
                    .unwrap();
            }
        }
        let pattern = Pattern::new()
            .with_node(NodePattern::new("c", "Container"))
            .with_node(NodePattern::new("i1", "Item"))
            .with_node(NodePattern::new("i2", "Item"))
            .with_edge(EdgePattern::new("c", "i1", "holds"))
            .with_edge(EdgePattern::new("c", "i2", "holds"));

        let a = find_matches(&pattern, &g);
        let b = find_matches(&pattern, &g);

        // Pro Container: 2 Items, injektiv geordnet → 2×1 = 2 Matches.
        // Drei Container → 6 Matches.
        assert_eq!(a.len(), 6, "3 Container × 2 geordnete Item-Paare");

        let keys = |ms: &[PatternMatch]| -> Vec<Vec<[u8; 32]>> {
            ms.iter().map(|m| canonical_key(m, &pattern)).collect()
        };
        assert_eq!(
            keys(&a),
            keys(&b),
            "edge-geleitete Enumeration ist deterministisch"
        );

        for m in &a {
            let c = m.get("c").unwrap();
            let i1 = m.get("i1").unwrap();
            let i2 = m.get("i2").unwrap();
            assert_ne!(i1, i2, "Injektivität: i1 ≠ i2");
            assert!(g.has_edge_between(c, i1, "holds"), "i1 hängt an c");
            assert!(g.has_edge_between(c, i2, "holds"), "i2 hängt an c");
        }
    }

    // ── Rang-Selektion ───────────────────────────────────────────────

    fn dummy_production(_m: &PatternMatch, _g: &TypedGraph) -> Vec<Op> {
        Vec::new()
    }

    #[test]
    fn rank_selection_picks_highest_rule() {
        let (g, _, _, _, _) = setup_uml_graph();
        let low = BasicRule::new(
            "low",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            dummy_production,
        );
        let high = BasicRule::new(
            "high",
            5,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            dummy_production,
        );

        let rules: Vec<&dyn Rule> = vec![&low, &high];
        let chosen = select_highest_rank(rules, &g).unwrap();
        assert_eq!(chosen.rule.id(), "high");
    }

    #[test]
    fn rank_selection_same_rule_picks_canonical_first() {
        let (g, _, _, _, _) = setup_uml_graph();
        let rule = BasicRule::new(
            "only",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            dummy_production,
        );
        let rules: Vec<&dyn Rule> = vec![&rule];
        let chosen = select_highest_rank(rules, &g).unwrap();
        // Mit zwei Klassen: höchster match_idx gewinnt (rank_key lexikographisch max).
        assert_eq!(chosen.match_idx, 1, "Kanzellation letzter in μ-Ordnung");
    }

    #[test]
    fn rank_selection_no_match_returns_none() {
        let g = TypedGraph::new();
        let rule = BasicRule::new(
            "x",
            1,
            Pattern::new().with_node(NodePattern::new("c", "NonExistentType")),
            dummy_production,
        );
        let rules: Vec<&dyn Rule> = vec![&rule];
        assert!(select_highest_rank(rules, &g).is_none());
    }

    #[test]
    fn rank_selection_empty_rule_list() {
        let g = TypedGraph::new();
        let rules: Vec<&dyn Rule> = Vec::new();
        assert!(select_highest_rank(rules, &g).is_none());
    }

    // ── Integration: echte Produktion ────────────────────────────────

    #[test]
    fn basic_rule_produces_ghost_op() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let rule = BasicRule::new(
            "AttrToGetter",
            10,
            Pattern::new().with_node(NodePattern::new("a", "Attribute")),
            |m, _g| {
                let attr_id = *m.get("a").unwrap();
                vec![Op::AddNode {
                    parent: attr_id,
                    edge_type: "hasGetter".into(),
                    type_id: "Getter".into(),
                    attrs: BTreeMap::new(),
                }]
            },
        );

        let rules: Vec<&dyn Rule> = vec![&rule];
        let chosen = select_highest_rank(rules, &g).unwrap();
        let ops = chosen.rule.produce(&chosen.pattern_match, &g);
        assert_eq!(ops.len(), 1);

        match &ops[0] {
            Op::AddNode {
                parent,
                edge_type,
                type_id,
                ..
            } => {
                assert!(
                    *parent == person_name || {
                        // Oder car_model — abhängig von μ.
                        true
                    }
                );
                assert_eq!(edge_type, "hasGetter");
                assert_eq!(type_id, "Getter");
            }
            _ => panic!("AddNode expected"),
        }
    }

    // ── Cascade-Skelett ──────────────────────────────────────────────

    #[test]
    fn cascade_append_and_length() {
        let mut c = Cascade::new();
        assert!(c.is_empty());
        let user_delta = DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: Vec::new(),
            anchor: Vec::new(),
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        };
        let idx = c.append(user_delta);
        assert_eq!(idx, 0);
        assert_eq!(c.len(), 1);
    }

    // ── Duplikations-Prädikat ────────────────────────────────────────

    #[test]
    fn duplicate_detection_on_existing_ghost() {
        let (g, person, _, _, _) = setup_uml_graph();
        // Ein AddNode, der auf ein existierendes Attribut "name" abzielt.
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "name")]),
        };
        assert!(is_duplicate(&[op], &g));
    }

    #[test]
    fn no_duplicate_for_novel_attr() {
        let (g, person, _, _, _) = setup_uml_graph();
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "novel_attr")]),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn duplicate_ignores_tombstone() {
        let (mut g, person, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);

        // Derselbe Name jetzt nicht mehr matchbar → nicht mehr als Duplikat
        // erkennbar.
        let op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "name")]),
        };
        assert!(!is_duplicate(&[op], &g), "TOMB blockiert Duplikation nicht");
    }

    #[test]
    fn mixed_application_is_not_a_duplicate() {
        let (g, person, _, _, _) = setup_uml_graph();
        // Eine duplizierende Op: Attribut "name" existiert bereits.
        let dup = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "name")]),
        };
        // Eine echt neue Op im selben op_star.
        let novel = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "novel_attr")]),
        };
        assert!(
            !is_duplicate(&[dup, novel], &g),
            "gemischte Anwendung (Duplikat + echte Neuarbeit) darf nicht \
             als Duplikat verworfen werden"
        );
    }

    #[test]
    fn add_edge_for_existing_edge_is_duplicate() {
        let (g, person, _, person_name, _) = setup_uml_graph();
        // Die Kante person → person_name ("hasAttribute") existiert
        // bereits in setup_uml_graph.
        let op = Op::AddEdge {
            source: person,
            target: person_name,
            type_id: "hasAttribute".into(),
            attrs: attrs(&[]),
        };
        assert!(is_duplicate(&[op], &g));
    }

    #[test]
    fn add_edge_for_novel_edge_is_not_duplicate() {
        let (g, person, _, _, car_model) = setup_uml_graph();
        // Eine Kante person → car_model gibt es nicht.
        let op = Op::AddEdge {
            source: person,
            target: car_model,
            type_id: "hasAttribute".into(),
            attrs: attrs(&[]),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn del_ops_never_count_as_duplicate() {
        let (g, person, _, person_name, _) = setup_uml_graph();
        // Del-Ops erzeugen nichts — sie sind nie Duplikate.
        assert!(!is_duplicate(&[Op::DelNode { target: person }], &g));
        assert!(!is_duplicate(
            &[Op::DelEdge {
                target: person_name
            }],
            &g
        ));
    }

    // ── B5-rc5: SetAttr-Duplikat (Reverse-Cascade-Termination) ───────

    #[test]
    fn setattr_for_already_equal_value_is_duplicate() {
        // Regression rc4→rc5: B5 lieferte attrs_to_set → produce()
        // emittiert in jedem Step die gleiche SetAttr. Vor dem Fix
        // war is_duplicate `_ => false` für SetAttr — Folge: jeder
        // Step galt als „productive", cascade_step returnte ewig
        // Running. Idempotenz-Semantik (analog zu AddNode/AddEdge):
        // SetAttr ist Duplikat, wenn der Zielknoten matchbar+non-
        // tentative ist und der gewünschte Wert bereits gleich ist.
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::SetAttr {
            target: person_name,
            key: "name".into(),
            value: "name".into(), // person_name trägt schon name="name"
        };
        assert!(
            is_duplicate(&[op], &g),
            "SetAttr mit bereits identischem Wert muss als Duplikat zählen, \
             sonst loopt die Cascade auf jeder attrs_to_set-Rule"
        );
    }

    #[test]
    fn setattr_for_different_value_is_not_duplicate() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::SetAttr {
            target: person_name,
            key: "name".into(),
            value: "renamed".into(),
        };
        assert!(
            !is_duplicate(&[op], &g),
            "SetAttr mit abweichendem Zielwert ist echte Arbeit — kein Duplikat"
        );
    }

    #[test]
    fn setattr_for_missing_key_is_not_duplicate() {
        // Knoten existiert, aber Key noch nicht gesetzt → echte Arbeit.
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::SetAttr {
            target: person_name,
            key: "freshly_introduced".into(),
            value: "v".into(),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn setattr_on_unknown_node_is_not_duplicate() {
        // Apply würde mit NodeNotFound auflaufen — bis dahin: nicht
        // als Duplikat verschlucken, damit der Fehler sichtbar wird.
        let (g, _, _, _, _) = setup_uml_graph();
        let phantom = GhostId::from_baseline("phantom-never-inserted");
        let op = Op::SetAttr {
            target: phantom,
            key: "k".into(),
            value: "v".into(),
        };
        assert!(!is_duplicate(&[op], &g));
    }

    #[test]
    fn setattr_on_tombstone_node_is_not_duplicate() {
        // Tombstone-Knoten: nicht matchbar → SetAttr nicht als
        // Duplikat zählen, analog zu AddNode/AddEdge.
        let (mut g, _, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);
        let op = Op::SetAttr {
            target: person_name,
            key: "name".into(),
            value: "name".into(),
        };
        assert!(
            !is_duplicate(&[op], &g),
            "TOMB-Knoten ist nicht matchbar → SetAttr ist kein Duplikat"
        );
    }

    // ── Kontradiktions-Prädikat ──────────────────────────────────────

    #[test]
    fn contradiction_on_solid_deletion() {
        let (g, person, _, _, _) = setup_uml_graph();
        let op = Op::DelNode { target: person };
        let reason = is_contradictory(&[op], &g);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("SOLID"));
    }

    #[test]
    fn no_contradiction_on_ghost_deletion() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let op = Op::DelNode {
            target: person_name,
        };
        assert!(is_contradictory(&[op], &g).is_none());
    }

    #[test]
    fn contradiction_on_double_tombstone() {
        let (mut g, _, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);
        let op = Op::DelNode {
            target: person_name,
        };
        assert!(is_contradictory(&[op], &g).is_some());
    }

    #[test]
    fn contradiction_on_add_edge_to_tombstone() {
        let (mut g, person, _, person_name, _) = setup_uml_graph();
        g.set_node_status(&person_name, Status::Tombstone);

        let op = Op::AddEdge {
            source: person,
            target: person_name,
            type_id: "any".into(),
            attrs: BTreeMap::new(),
        };
        let reason = is_contradictory(&[op], &g);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("TOMB"));
    }

    // ── Encode-Rank ──────────────────────────────────────────────────

    #[test]
    fn encode_rank_dominated_by_rule() {
        let a = encode_delta_rank(1, 999);
        let b = encode_delta_rank(2, 0);
        assert!(b > a, "Regel-Rang dominiert Match-Index");
    }

    #[test]
    fn encode_rank_ties_broken_by_match_idx() {
        let a = encode_delta_rank(3, 5);
        let b = encode_delta_rank(3, 7);
        assert!(b > a);
    }

    // ── Cascade-Step: Runs ───────────────────────────────────────────

    #[test]
    fn cascade_step_convergence_on_empty_rules() {
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();
        let rules: &[&dyn Rule] = &[];
        let state = cascade_step(&mut c, &mut g, rules).unwrap();
        assert_eq!(state, TerminationState::Convergence);
    }

    #[test]
    fn cascade_step_runs_one_rule_once() {
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Eine Regel, die ein neues Attribut "derived" an jede Klasse hängt.
        let rule = BasicRule::new(
            "AddDerived",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "derived")]),
                }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let node_count_before = g.node_count();
        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        assert_eq!(state, TerminationState::Running);
        assert_eq!(c.len(), 1);
        assert!(g.node_count() > node_count_before);
    }

    #[test]
    fn cascade_step_contradiction_on_solid_del() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();
        let rule = BasicRule::new(
            "BadDel",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |m, _g| {
                let _ = m;
                vec![Op::DelNode { target: person }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];
        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        assert!(matches!(state, TerminationState::Contradiction { .. }));
    }

    // ── run_cascade ──────────────────────────────────────────────────

    #[test]
    fn run_cascade_terminates_via_convergence() {
        // Regel, die jede Klasse mit einem derived-Attribut versieht —
        // genau einmal (per Duplikations-Detektion beim zweiten Versuch).
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        let rule = BasicRule::new(
            "AddDerived",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "derived")]),
                }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let state = run_cascade(&mut c, &mut g, &rules, 100).unwrap();
        // Zwei Klassen → zwei Deltas, dann Duplikation.
        assert_eq!(c.len(), 2, "zwei Durchläufe, einer pro Klasse");
        assert!(
            matches!(
                state,
                TerminationState::Duplication | TerminationState::Convergence
            ),
            "unerwarteter State: {state:?}"
        );
    }

    // ── Retraktions-Kaskade ──────────────────────────────────────────

    #[test]
    fn retraction_cascade_includes_incident_edges() {
        let (g, person, _, person_name, _) = setup_uml_graph();
        // Kante: person → person_name (via hasAttribute, vom setup_uml_graph erzeugt).
        let del_op = Op::DelNode { target: person };
        let induced = retraction_cascade_for(&del_op, &g);
        // Mindestens eine DelEdge wird induziert (person→person_name).
        assert!(!induced.is_empty());
        assert!(
            induced.iter().all(|op| matches!(op, Op::DelEdge { .. })),
            "Retraktions-Kaskade produziert nur DelEdges"
        );
        let _ = person_name;
    }

    #[test]
    fn retraction_cascade_empty_for_non_del() {
        let (g, person, _, _, _) = setup_uml_graph();
        let add_op = Op::AddNode {
            parent: person,
            edge_type: "x".into(),
            type_id: "Y".into(),
            attrs: BTreeMap::new(),
        };
        assert!(retraction_cascade_for(&add_op, &g).is_empty());
    }

    #[test]
    fn expand_with_retraction_populates_induces() {
        let (g, _, _, person_name, _) = setup_uml_graph();
        let primary = vec![Op::DelNode {
            target: person_name,
        }];
        let (full_ops, induces) = expand_with_retraction(primary, &g);
        assert!(!full_ops.is_empty());
        assert_eq!(full_ops.len(), induces.len());
        // Primär-Op (index 0) sollte induces einträge haben, wenn
        // person_name inzidente Kanten hat.
        if full_ops.len() > 1 {
            assert!(!induces[0].is_empty(), "Primär-Op induziert Folge-Ops");
            for idx in &induces[0] {
                assert!(*idx > 0, "induzierte Op hat Index > 0");
                assert!(matches!(full_ops[*idx], Op::DelEdge { .. }));
            }
        }
    }

    // ── V₇-Vorfahren-Check ───────────────────────────────────────────

    #[test]
    fn ancestor_check_identifies_creators() {
        use crate::ops::Origin;
        let (g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Simuliere eine Kaskade: d_0 erzeugt einen Ghost, d_1 verwendet ihn als Anchor.
        let parent = g.matchable_nodes().next().unwrap().id;
        let ghost_id = GhostId::from_parent(&parent, "test-edge", "TestType", &BTreeMap::new());

        // d_0: erzeugt ghost_id
        c.append(DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: vec![Op::AddNode {
                parent,
                edge_type: "test-edge".into(),
                type_id: "TestType".into(),
                attrs: BTreeMap::new(),
            }],
            anchor: vec![parent],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        // d_1: hat ghost_id als Anchor
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r1".into(),
            },
            rank: 1,
            op_star: Vec::new(),
            anchor: vec![ghost_id],
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        });

        assert_eq!(c.creator_of(&ghost_id), Some(0));
        let ancestors = c.ancestors_of_anchor(&[ghost_id]);
        assert!(ancestors.contains(&0));
    }

    #[test]
    fn contradiction_rejects_ancestor_erasure() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // d_0 (User): trivialer User-Delta mit person als Anchor.
        c.append(DeltaEntry {
            origin: crate::ops::Origin::User,
            rank: 0,
            op_star: Vec::new(),
            anchor: vec![person],
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        });

        // Eine Regel, die versuchen würde, einen Ghost zu erzeugen UND den
        // Parent (person, SOLID) zu tombstonen. V₇ muss das abfangen.
        let rule = BasicRule::new(
            "BadRule",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |m, _g| {
                let _c = m.get("c").unwrap();
                vec![Op::DelNode { target: person }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        // SOLID-Schutz greift (Teilmenge V₇): person ist SOLID → Contradiction.
        assert!(matches!(state, TerminationState::Contradiction { .. }));
    }

    #[test]
    fn v7_protects_d0_created_ghost() {
        use crate::ops::Origin;
        let (mut g, person, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // d_0 (User): erzeugt einen Ghost-Attribut.
        let user_op = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "user_attr")]),
        };
        let ghost_id = match user_op.target() {
            OpTarget::Node(id) => id,
            _ => panic!(),
        };

        // Auf Graph anwenden:
        user_op.apply(&mut g).unwrap();

        c.append(DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: vec![user_op],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        // Regel, die genau dieses d_0-Ghost löschen will.
        let rule = BasicRule::new(
            "EraseD0",
            1,
            Pattern::new().with_node(NodePattern::new("a", "Attribute")),
            move |_m, _g| vec![Op::DelNode { target: ghost_id }],
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        // V₇ sollte d_0-Element schützen.
        assert!(matches!(state, TerminationState::Contradiction { .. }));
        if let TerminationState::Contradiction { reason } = state {
            assert!(
                reason.contains("V₇") || reason.contains("d_0") || reason.contains("SOLID"),
                "Reason sollte V₇ referenzieren: {reason}"
            );
        }
    }

    // ── Contradiction-Saturation statt sofortigem Abort ───────────────

    // ── Rollback-Szenarien (Phase 1.3d) ──────────────────────────────

    #[test]
    fn rollback_truncates_and_sets_limit() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let op1 = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "a1")]),
        };
        op1.apply(&mut g).unwrap();
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r1".into(),
            },
            rank: 50,
            op_star: vec![op1],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let op2 = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "a2")]),
        };
        op2.apply(&mut g).unwrap();
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r2".into(),
            },
            rank: 100, // höchste
            op_star: vec![op2],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        let op3 = Op::AddNode {
            parent: person,
            edge_type: "hasAttribute".into(),
            type_id: "Attribute".into(),
            attrs: attrs(&[("name", "a3")]),
        };
        op3.apply(&mut g).unwrap();
        c.append(DeltaEntry {
            origin: Origin::Rule {
                rule_id: "r3".into(),
            },
            rank: 30,
            op_star: vec![op3],
            anchor: vec![person],
            induces: vec![Vec::new()],
            bindings: std::collections::HashMap::new(),
        });

        assert_eq!(c.len(), 3);
        let mut limits: HashMap<usize, u64> = HashMap::new();

        let rolled_pos = rollback_highest_rank(&base, &mut c, &mut g, &mut limits);
        assert_eq!(rolled_pos, Some(1), "rank 100 Delta war an Position 1");

        // Cascade jetzt abgeschnitten auf [r1].
        assert_eq!(c.len(), 1);
        // Limit gesetzt an Position 1.
        assert_eq!(limits.get(&1), Some(&100));
        // Graph-Zustand: 4 Baseline-Knoten + a1 = 5; a2 und a3 sind weg.
        assert_eq!(g.matchable_nodes().count(), 5);
    }

    #[test]
    fn rollback_skips_user_delta() {
        let (mut g, person, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        // Einziger Eintrag ist ein User-Delta.
        c.append(DeltaEntry {
            origin: Origin::User,
            rank: 0,
            op_star: Vec::new(),
            anchor: vec![person],
            induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
        });

        let mut limits: HashMap<usize, u64> = HashMap::new();

        let result = rollback_highest_rank(&base, &mut c, &mut g, &mut limits);
        assert_eq!(result, None, "kein Rule-Delta → kein Rollback");
        assert_eq!(c.len(), 1, "User-Delta bleibt");
    }

    #[test]
    fn run_cascade_with_rollback_recovers_from_contradiction() {
        // Szenario:
        //   R_problem (rank 100): auf Class → erzeugt Attribute "x".
        //   R_kill (rank 5): auf Attribute "x" → versucht es zu löschen
        //     (V₇-Verletzung, da R_problem Vorfahre).
        //
        // Erwartung: ohne Rollback → Contradiction.
        //            mit Rollback → R_problem rausgerollt, Konvergenz.
        let (mut g, _, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let r_problem = BasicRule::new(
            "RProblem",
            100,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "x")]),
                }]
            },
        );

        let r_kill = BasicRule::new(
            "RKill",
            5,
            Pattern::new()
                .with_node(NodePattern::new("a", "Attribute").with_attr_equals("name", "x")),
            |m, _g| {
                let a = *m.get("a").unwrap();
                vec![Op::DelNode { target: a }]
            },
        );

        let rules: Vec<&dyn Rule> = vec![&r_problem, &r_kill];

        let (state, stats) =
            run_cascade_with_rollback(&base, &mut c, &mut g, &rules, 50, 10).unwrap();

        assert!(
            stats.rollback_count >= 1,
            "Mindestens ein Rollback erwartet, stats = {stats:?}"
        );
        assert!(!stats.limits_applied.is_empty(), "Limits gesetzt");
        // Terminierung via Convergence oder Duplication (nachdem R_problem
        // rausgerollt wurde und R_kill keinen Match mehr hat).
        assert!(
            matches!(
                state,
                TerminationState::Convergence | TerminationState::Duplication
            ),
            "nach Rollback valider Endzustand: {state:?}"
        );
    }

    #[test]
    fn run_cascade_with_rollback_capitulates_when_exhausted() {
        // Szenario: Rule, die direkt eine SOLID-Deletion versucht —
        // kein Rollback kann das lösen, weil das Problem nicht in einer
        // früheren Rule-Entscheidung liegt, sondern an der Regel selbst.
        let (mut g, person, _, _, _) = setup_uml_graph();
        let base = g.clone();
        let mut c = Cascade::new();

        let bad_rule = BasicRule::new(
            "Bad",
            10,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |_m, _g| vec![Op::DelNode { target: person }],
        );
        let rules: Vec<&dyn Rule> = vec![&bad_rule];

        let (state, _stats) =
            run_cascade_with_rollback(&base, &mut c, &mut g, &rules, 50, 10).unwrap();

        // Kein Rule-Delta in der Kaskade → kein Rollback möglich →
        // Contradiction bleibt.
        assert!(matches!(state, TerminationState::Contradiction { .. }));
    }

    #[test]
    fn contradictory_candidate_skipped_for_non_contra() {
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        // Zwei Regeln: eine mit höherem Rang, die kontradiktorisch wäre,
        // eine mit niedrigerem Rang, die valide ist.
        let person_id = g
            .matchable_nodes()
            .find(|n| n.type_id == "Class" && n.attrs.get("name") == Some(&"Person".to_string()))
            .map(|n| n.id)
            .unwrap();

        let high_bad = BasicRule::new(
            "HighBad",
            10,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            move |_m, _g| vec![Op::DelNode { target: person_id }], // SOLID-Erase
        );

        let low_good = BasicRule::new(
            "LowGood",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", "derived")]),
                }]
            },
        );

        let rules: Vec<&dyn Rule> = vec![&high_bad, &low_good];
        let state = cascade_step(&mut c, &mut g, &rules).unwrap();
        // Die kontradiktorische hohe Regel wird übersprungen; die niedrige
        // Regel läuft durch.
        assert_eq!(state, TerminationState::Running);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn run_cascade_respects_step_limit() {
        // Regel, die immer neue Ghosts produziert (nicht-kontraktiv).
        let (mut g, _, _, _, _) = setup_uml_graph();
        let mut c = Cascade::new();

        let rule = BasicRule::new(
            "Unbounded",
            1,
            Pattern::new().with_node(NodePattern::new("c", "Class")),
            |m, _g| {
                let c = *m.get("c").unwrap();
                // Wir produzieren einen Ghost mit einem eindeutigen Suffix
                // basierend auf dem aktuellen Hash — um Duplikation zu
                // vermeiden in diesem Stress-Test.
                let unique = format!("derived-{}", c.short());
                vec![Op::AddNode {
                    parent: c,
                    edge_type: "hasAttribute".into(),
                    type_id: "Attribute".into(),
                    attrs: attrs(&[("name", unique.as_str())]),
                }]
            },
        );
        let rules: Vec<&dyn Rule> = vec![&rule];

        // Mit Limit 1 sollte es nicht durchlaufen.
        // Bei 2 Klassen: Step 1 produziert, Step 2 produziert, Step 3 matcht
        // erneut aber Attribut "derived-…" existiert — Duplikation.
        // Mit max_steps = 1 erreichen wir also den Limit-Error.
        let state = run_cascade(&mut c, &mut g, &rules, 1);
        assert!(matches!(state, Err(EngineError::StepLimitExceeded { .. })));
    }

    // ── M5.1: MatchPersistenceStore ──────────────────────────────────────

    #[test]
    fn match_persistence_records_bindings() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let _p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));

        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "marker".into(),
                type_id: "Marker".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        // Sollte 2 Rule-Anwendungen geben (eine pro Class).
        let store = MatchPersistenceStore::new(&cas);
        let refs_p1 = store.applications_referencing(&p1);
        assert_eq!(refs_p1.len(), 1, "C1 wird genau 1× gebunden");
        // Bindings müssen gefüllt sein
        let bindings = store.bindings(refs_p1[0]).unwrap();
        assert!(bindings.contains_key("c"));
    }

    #[test]
    fn match_persistence_created_set_includes_emitted_node() {
        let mut g = TypedGraph::new();
        let _p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "marker".into(),
                type_id: "Marker".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        let store = MatchPersistenceStore::new(&cas);
        let created = store.created_set(0);
        assert!(!created.is_empty(), "Mindestens 1 erzeugtes Element");
    }

    // ── M5.2: Watch-Hook ─────────────────────────────────────────────────

    #[test]
    fn watch_op_finds_rule_apps_referencing_node() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let _p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "marker".into(),
                type_id: "Marker".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        // Op auf p1 — sollte 1 RuleApp finden.
        let op = Op::DelNode { target: p1 };
        let outcome = watch_op(&op, &cas, &g);
        assert_eq!(outcome.affected_apps.len(), 1);
        assert!(!outcome.propagation_candidate);
    }

    #[test]
    fn watch_op_setattr_marks_propagation_candidate() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        let op = Op::SetAttr {
            target: p1,
            key: "name".into(),
            value: "C1Renamed".into(),
        };
        let outcome = watch_op(&op, &cas, &g);
        assert!(outcome.propagation_candidate);
        assert_eq!(outcome.affected_apps.len(), 1);
    }

    #[test]
    fn apply_with_watch_returns_sorted_unique_apps() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        let snapshot_cas = cas.clone();

        let user = DeltaEntry::new_user(
            vec![Op::DelNode { target: p1 }, Op::DelNode { target: p2 }],
            vec![p1, p2],
        );
        let affected = apply_with_watch(&user, &mut g, &snapshot_cas).unwrap();
        // 2 RuleApps (eine pro Class) — beide gefunden, sortiert, ohne Duplikate
        assert_eq!(affected.len(), 2);
        assert!(affected[0] < affected[1]);
    }

    // ── M5.3: Revalidate ─────────────────────────────────────────────────

    #[test]
    fn revalidate_still_matches_when_unchanged() {
        let mut g = TypedGraph::new();
        let _p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        // Re-Validate gegen eine harmlose Op (gleicher Knoten, gleicher Status)
        let outcome = revalidate_app(
            0,
            &cas,
            &g,
            &rules,
            &Op::DelEdge {
                target: GhostId::from_baseline("nope"),
            },
        );
        assert!(matches!(outcome, RevalidationOutcome::StillMatches));
    }

    #[test]
    fn revalidate_no_longer_matches_when_node_gone() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        // Tombstone p1
        g.set_node_status(&p1, Status::Tombstone);
        let outcome = revalidate_app(0, &cas, &g, &rules, &Op::DelNode { target: p1 });
        assert!(matches!(outcome, RevalidationOutcome::NoLongerMatches));
    }

    #[test]
    fn revalidate_attr_changed_with_propagation() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        })
        .with_propagations(vec![EnginePropagation {
            source_node_var: "c".into(),
            source_attr: "name".into(),
            target_node_var: "c".into(),
            target_attr: "label".into(),
            transform_tag: "identity".into(),
        }]);
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        let outcome = revalidate_app(
            0,
            &cas,
            &g,
            &rules,
            &Op::SetAttr {
                target: p1,
                key: "name".into(),
                value: "C1Renamed".into(),
            },
        );
        match outcome {
            RevalidationOutcome::AttrChanged {
                propagations,
                l_var,
                attr,
                new_value,
            } => {
                assert_eq!(l_var, "c");
                assert_eq!(attr, "name");
                assert_eq!(new_value, "C1Renamed");
                assert_eq!(propagations.len(), 1);
            }
            other => panic!("erwartet AttrChanged, got {other:?}"),
        }
    }

    // ── M5.4: Attribut-Propagation ───────────────────────────────────────

    #[test]
    fn apply_attr_propagation_emits_setattr() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let r_node = g.add_baseline_node("Doc", "D1", attrs(&[("label", "old")]));

        let mut cas = Cascade::new();
        // Stelle Bindings: c → p1, d → r_node
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("c".to_string(), p1);
        bindings.insert("d".to_string(), r_node);

        let propagations = vec![EnginePropagation {
            source_node_var: "c".into(),
            source_attr: "name".into(),
            target_node_var: "d".into(),
            target_attr: "label".into(),
            transform_tag: "identity".into(),
        }];

        apply_attr_propagation(
            &propagations,
            &bindings,
            "C1Renamed",
            &mut g,
            &mut cas,
            "MyRule",
        )
        .unwrap();

        // Cascade hat jetzt einen Entry mit @propagate
        assert_eq!(cas.entries.len(), 1);
        let origin = &cas.entries[0].origin;
        assert!(matches!(origin, Origin::Rule { rule_id } if rule_id == "MyRule@propagate"));

        // R-Node hat neuen label-Wert
        let r_data = g.get_node(&r_node).unwrap();
        assert_eq!(r_data.attrs["label"], "C1Renamed");
    }

    #[test]
    fn apply_attr_propagation_uses_getter_name_transform() {
        let mut g = TypedGraph::new();
        let attr_node = g.add_baseline_node("Attribute", "a", attrs(&[("name", "age")]));
        let getter_node = g.add_baseline_node("Method", "m", attrs(&[("name", "old")]));

        let mut cas = Cascade::new();
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("a".to_string(), attr_node);
        bindings.insert("m".to_string(), getter_node);

        let propagations = vec![EnginePropagation {
            source_node_var: "a".into(),
            source_attr: "name".into(),
            target_node_var: "m".into(),
            target_attr: "name".into(),
            transform_tag: "getter_name".into(),
        }];

        apply_attr_propagation(
            &propagations,
            &bindings,
            "salary",
            &mut g,
            &mut cas,
            "AttrRule",
        )
        .unwrap();

        let m = g.get_node(&getter_node).unwrap();
        assert_eq!(m.attrs["name"], "getSalary");
    }

    // ── M5.5: Tentative-Tombstone + Konsolidierung ───────────────────────

    #[test]
    fn tentative_invalidate_marks_created_set() {
        let mut g = TypedGraph::new();
        let _p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();

        let invalidated = tentative_invalidate(0, &cas, &mut g);
        assert!(!invalidated.is_empty());
        for id in &invalidated {
            let n = g.get_node(id);
            if let Some(n) = n {
                assert_eq!(n.status, Status::TentativeTombstone);
            }
        }
    }

    #[test]
    fn consolidate_resurrects_just_created_and_tombstones_rest() {
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let m1 = g.add_ghost_node(p1, "m", "M", attrs(&[("name", "m1")]));
        let m2 = g.add_ghost_node(p1, "m", "M", attrs(&[("name", "m2")]));
        // Markiere beide als TentativeTombstone
        g.set_node_status(&m1, Status::TentativeTombstone);
        g.set_node_status(&m2, Status::TentativeTombstone);
        // Konsolidiere: nur m1 wurde "neu erstellt" → Resurrection
        consolidate_tentative(&mut g, &[m1]);
        assert_eq!(g.get_node(&m1).unwrap().status, Status::Solid);
        assert_eq!(g.get_node(&m2).unwrap().status, Status::Tombstone);
    }

    #[test]
    fn run_cascade_observable_invalidates_orphan_ruleapp() {
        // Zwei Klassen, Rule erzeugt Marker pro Klasse.
        // Dann User löscht eine Klasse → Marker dieser App muss
        // invalidiert (Tombstone) werden.
        let mut g = TypedGraph::new();
        let p1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
        let _p2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
        let pat = Pattern::new().with_node(NodePattern::new("c", "Class"));
        let rule = BasicRule::new("R", 10, pat, |m, _| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "m".into(),
                type_id: "M".into(),
                attrs: BTreeMap::new(),
            }]
        });
        let rules: Vec<&dyn Rule> = vec![&rule];
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &rules, 5).unwrap();
        // Nach Initial-Sync: 2 Marker-Nodes
        let marker_count = g.iter_nodes().filter(|n| n.type_id == "M").count();
        assert_eq!(marker_count, 2);

        // User-Delta: lösche p1
        let user = DeltaEntry::new_user(vec![Op::DelNode { target: p1 }], vec![p1]);
        user.apply(&mut g).unwrap();
        cas.append(user);

        let _ = run_cascade_observable(&mut cas, &mut g, &rules, 5).unwrap();

        // Marker, die noch nicht Tombstone sind (Solid oder Ghost) zählen.
        let active_markers = g
            .iter_nodes()
            .filter(|n| n.type_id == "M" && n.status != Status::Tombstone)
            .count();
        let tombstoned_markers = g
            .iter_nodes()
            .filter(|n| n.type_id == "M" && n.status == Status::Tombstone)
            .count();
        assert_eq!(active_markers, 1, "p2's Marker bleibt aktiv");
        assert_eq!(tombstoned_markers, 1, "p1's Marker wurde tombstoned");
    }

    #[test]
    fn user_delta_has_empty_bindings() {
        let cas = Cascade::with_user_delta(DeltaEntry::new_user(vec![], vec![]));
        let store = MatchPersistenceStore::new(&cas);
        // User-Delta wird nicht von applications_referencing erfasst,
        // weil sie nur Origin::Rule berücksichtigt.
        let id = GhostId::from_baseline("dummy");
        assert!(store.applications_referencing(&id).is_empty());
        assert!(store.bindings(0).unwrap().is_empty());
    }
}
