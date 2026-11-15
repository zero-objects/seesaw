//! The engine: the delta-local cascade.
//!
//! Spec §1.6 implemented verbatim:
//! - add: expand anchored → create a match record, refs into the
//!   participant lists, todo insert. μ = the ref sequence itself.
//! - delete/modify: `adj[k]` directly yields the affected matches —
//!   eagerly mark dead; no lazy viability, no re-enum, no index.
//! - Duplicate check BEFORE application (ids are purely derivable).
//! - The todo list = the ONLY extra structure.
//!
//! Sequencing (memory design sketch): minimal engine first (add-only
//! cascade, NACs, rank order, duplication verdict) — retraction/
//! contradiction/fold/backtracking are ported from first-generation afterward.

pub(crate) mod anchor;
pub mod matcher;
#[cfg(all(feature = "parallel_seed", not(target_arch = "wasm32")))]
mod par;
mod reserve;

pub use anchor::AnchorIndex;
pub use reserve::Sizes;

use std::collections::BTreeSet;

use crate::ident::{GhostId, Status};

#[cfg(any(test, seesaw_checks))]
use crate::engine::matcher::find_matches_with_fixed;
use crate::engine::matcher::{find_matches, find_matches_planned, Bindings};
use crate::graph::{Graph, ValueResolver};
use crate::plan::{apply_creation_at, DirectedRule, Names, Ref, RuleDirection, RuleNames};

/// Is the node tentatively retracted? `step` reads that together with
/// the participations of the node (`recognise`); this is the form the
/// test checks compare with.
#[cfg(any(test, seesaw_checks))]
fn self_node_tt(g: &Graph, id: &GhostId) -> bool {
    g.node(id)
        .is_some_and(|n| n.status == Status::TentativeTombstone)
}

/// Match record (spec §1): a record living in the same world as nodes
/// and connections — participants via `Engine::by_element`.
///
/// The engine keeps its matches in a record of its own (the ref
/// sequence shared with its indexes) and hands none out; this type is
/// what 2.0.2 published and stays for source compatibility.
#[deprecated(note = "no API hands out or takes a `MatchRec`; the engine's \
                     match store is private")]
#[derive(Debug, Clone)]
pub struct MatchRec {
    pub rule_ix: usize,
    pub refs: Bindings,
    pub dead: bool,
    /// Provenance edge match → cascade entry: `Some(cascade-ix)` if
    /// this match WAS APPLIED (for 1:1, the `applied` guard prevents a
    /// double application), otherwise `None`. Materializes the edge so
    /// `retract_for` can follow provenance without a cascade scan.
    pub entry: Option<u32>,
}

/// A match in the engine's store.
#[derive(Debug, Clone)]
struct Stored {
    rule_ix: usize,
    /// The ref sequence, shared with the engine's indexes.
    refs: Refs,
    dead: bool,
    /// Provenance edge match → cascade entry: `Some(cascade-ix)` if
    /// this match WAS APPLIED (for 1:1, the `applied` guard prevents a
    /// double application), otherwise `None`. Materializes the edge so
    /// `retract_for` can follow provenance without a cascade scan.
    entry: Option<u32>,
}

/// Cascade entry: what was applied (ref sequence = μ = identity).
#[derive(Debug, Clone)]
pub struct Entry {
    pub rule_ix: usize,
    pub rank: u64,
    pub refs: Bindings,
    pub created: Vec<GhostId>,
    /// Created edges (connection ids). Kept separate from `created`
    /// because retraction retracts them via `set_connection_status`
    /// and they have no provenance children (not queued).
    pub created_edges: Vec<GhostId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Termination {
    Duplication,
    Convergence,
    StepLimit,
    /// V7: a candidate wanted to reuse tombstone substance —
    /// a contradiction with history.
    Contradiction,
}

/// Domain in which an incoming host delta originated.
///
/// A delta may name both domains (a concurrent/mixed edit). The engine
/// turns the complete set into one immutable route for the following
/// realisation wave; products created during that wave cannot change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaDomain {
    Source,
    Target,
}

#[derive(Debug, Clone, Copy, Default)]
struct WaveDirections {
    forward: bool,
    backward: bool,
}

impl WaveDirections {
    fn admits(self, direction: RuleDirection) -> bool {
        match direction {
            RuleDirection::Forward => self.forward,
            RuleDirection::Backward => self.backward,
            RuleDirection::Undirected => true,
        }
    }
}

/// Backtracking bound: the intrinsic selection position (rule rank,
/// ref sequence) of the undone choice — derivable from every entry, no
/// state-dependent numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionBound {
    pub rank: u64,
    pub refs: Bindings,
}

/// Todo key: (rank descending, ref sequence descending, rule index
/// ascending) — the same ordering doctrine as the first-generation engine's
/// intrinsic μ, here the ref sequence is the key.
///
/// The second component is the head of the ref sequence (the first 16
/// bytes of its first id). It orders like the sequence itself wherever
/// two heads differ, so it changes nothing about the order; it lets
/// the todo compare two keys without following both sequences to the
/// heap, which is where most comparisons of a lookup were decided.
type TodoKey = (
    std::cmp::Reverse<u64>,
    std::cmp::Reverse<u128>,
    std::cmp::Reverse<Refs>,
    usize,
);

/// The first 16 bytes of the first id of a ref sequence, big-endian;
/// 0 for an empty sequence. `head(a) < head(b)` implies `a < b`.
fn head(refs: &[GhostId]) -> u128 {
    refs.first().map_or(0, |id| {
        let mut b = [0u8; 16];
        b.copy_from_slice(&id.as_bytes()[..16]);
        u128::from_be_bytes(b)
    })
}

/// The ref sequence of a match as the engine keeps it: one allocation,
/// shared by the match store, the todo, the key index and the applied
/// set. Equality, order and hash are those of the sequence.
type Refs = std::sync::Arc<[GhostId]>;

fn todo_key(rank: u64, refs: &Refs, rule_ix: usize) -> TodoKey {
    (
        std::cmp::Reverse(rank),
        std::cmp::Reverse(head(refs)),
        std::cmp::Reverse(refs.clone()),
        rule_ix,
    )
}

/// A list of indices (into the match store or the cascade) kept per
/// element. Most elements stand in a few matches and are produced by
/// one entry, so up to [`IXS_INLINE`] indices are kept in place and
/// only longer lists on the heap. Order of insertion.
#[derive(Debug, Clone)]
enum Ixs {
    Inline { len: u8, ix: [u32; IXS_INLINE] },
    Heap(IxsLong),
}

/// Indices an [`Ixs`] holds without an allocation.
#[cfg(not(feature = "low_memory"))]
const IXS_INLINE: usize = 4;

/// The list of an [`Ixs`] on the heap. With the vector in place an
/// [`Ixs`] takes 32 bytes, an entry of a map from an id to it 64.
#[cfg(not(feature = "low_memory"))]
type IxsLong = Vec<u32>;

#[cfg(not(feature = "low_memory"))]
fn ixs_long(v: Vec<u32>) -> IxsLong {
    v
}

/// Indices an [`Ixs`] holds without an allocation, with the feature
/// `low_memory`: one less, so that the list fits 16 bytes.
#[cfg(feature = "low_memory")]
const IXS_INLINE: usize = 3;

/// The list of an [`Ixs`] on the heap, with the feature `low_memory`:
/// behind one pointer, so that an [`Ixs`] takes 16 bytes and an entry
/// of a map from an id to it 48. A long list costs a second allocation
/// and one more step to its indices.
#[cfg(feature = "low_memory")]
#[allow(clippy::box_collection)]
type IxsLong = Box<Vec<u32>>;

#[cfg(feature = "low_memory")]
fn ixs_long(v: Vec<u32>) -> IxsLong {
    Box::new(v)
}

/// Capacity of the list of an [`Ixs`] when it moves to the heap.
const IXS_FIRST_ON_HEAP: usize = 8;

impl Default for Ixs {
    fn default() -> Self {
        Self::Inline {
            len: 0,
            ix: [0; IXS_INLINE],
        }
    }
}

impl Ixs {
    fn push(&mut self, value: u32) {
        match self {
            Self::Inline { len, ix } => {
                if (*len as usize) < IXS_INLINE {
                    ix[*len as usize] = value;
                    *len += 1;
                } else {
                    let mut v = Vec::with_capacity(IXS_FIRST_ON_HEAP);
                    v.extend_from_slice(&ix[..]);
                    v.push(value);
                    *self = Self::Heap(ixs_long(v));
                }
            }
            Self::Heap(v) => v.push(value),
        }
    }

    fn as_slice(&self) -> &[u32] {
        match self {
            Self::Inline { len, ix } => &ix[..*len as usize],
            Self::Heap(v) => v,
        }
    }
}

// Three parts of the engine in files of their own. They are included,
// not modules: each implements private parts of `Engine` and reads
// the types above.
//
// The matches that rest on a connection (`Links`, `file_links`,
// `Engine::filed_under`, `Engine::removed_unfiled`,
// `Engine::settle_removed`).
include!("links.rs");

// The todo (`Todo`, `Taken`, `Carried`).
include!("todo.rs");

// What `step` knows of a pulled candidate (`Recognised`, `recognise`,
// `Applied`, `Engine::pulled_match`).
include!("applied.rs");

/// The engine.
pub struct Engine<'r> {
    /// The rules the engine runs.
    ///
    /// The matches the engine has recorded belong to these rules: what
    /// a match rests on is read from the pattern of its rule, and its
    /// place in the todo from the rule's rank. Do not set the field to
    /// other rules while the engine holds matches. Rules of the same
    /// content at another address are the same rules. For other rules
    /// take a new engine.
    pub rules: &'r [DirectedRule],
    /// Match store (records logically live in the map; physically
    /// here, referenced from `by_element` — realization of the spec
    /// map).
    matches: Vec<Stored>,
    /// adj[k] → match refs (spec §1.6: "which matches contain k?").
    by_element: crate::hash::FxHashMap<GhostId, Ixs>,
    /// What the registry of links keeps beyond `by_element`.
    links: Links,
    /// Duplicate memory: what was applied already.
    applied: Applied,
    todo: Todo,
    pub saw_contradiction: bool,
    /// (rule_ix, refs) → match store index for todo resolution.
    by_key: crate::hash::FxHashMap<(usize, Refs), u32>,
    /// Element → the cascade entries that PRODUCED it. The counterpart
    /// to `by_element`, which answers which matches CONTAIN an element.
    ///
    /// Without it, retraction cannot start from a product: a rule's
    /// match does not contain what the rule creates, so following
    /// `by_element` from a deleted product finds nothing, and the
    /// correspondence built by the same entry is unreachable. Deleting
    /// a generated element then left a correspondence attesting a
    /// translation whose result no longer exists (measured 2026-08-11).
    ///
    /// Filed here are the created NODES. The entries that produced a
    /// created connection are found from its two ends when a deleted
    /// connection asks for them ([`Engine::producers_of`]); filing
    /// every connection was more than half of the entries.
    by_product: crate::hash::FxHashMap<GhostId, Ixs>,
    /// Test check of [`Engine::producers_of`]: the created connections
    /// filed as they were before.
    #[cfg(any(test, seesaw_checks))]
    by_product_filed: crate::hash::FxHashMap<GhostId, Ixs>,
    pub cascade: Vec<Entry>,
    /// Runtime activation (tool configuration, e.g. Benchmarx
    /// decisions): None = all active. Inactive candidates stay in the
    /// todo (a configuration change reactivates them).
    pub active: Option<Vec<bool>>,
    /// Direction selected by the complete incoming delta for the
    /// current realisation wave. `None` is the deliberately un-routed
    /// initial/batch run. This is separate from `active`: tool choices,
    /// delta direction, and a backtracking ceiling are intersected.
    wave_directions: Option<WaveDirections>,
    /// TT collection of the retraction: `retract_match` notes here
    /// which nodes/edges it tentatively retracted — `consolidate`
    /// processes ONLY these (O(Δ)), instead of scanning the whole
    /// model.
    /// Elements the HOST deleted, as opposed to elements the engine
    /// retracted itself.
    ///
    /// The distinction Sandra's delta types draw: a delete acts along
    /// the correspondences, an add or a change must not. The engine
    /// cannot read it off the status — a change retracts and re-derives
    /// through the very same path, and mid-run a product looks equally
    /// dead either way. It can read it off the CALLER: `retract_for`
    /// receives an element the host already tombstoned.
    /// Ist die laufende Welle durch ein DELETE ausgeloest? Kommt von
    /// der Tuer (`element_deleted` gegen `element_changed`), nicht aus
    /// einer Statuspruefung. Eigenschaft der Welle, nicht eines
    /// Elements: dasselbe Element kann heute geloescht und morgen
    /// geaendert werden.
    deleting: bool,
    /// Was in der laufenden Welle GELOESCHT wurde. Gefuellt von
    /// `element_deleted` und von der Transitivitaet, geleert von der
    /// Konsolidierung -- die Markierung gehoert der Welle, nicht der
    /// Identitaet. Als dauerhafte Menge waere sie bei Undo oder einem
    /// Baseline-Wechsel falsch (Review 2026-08-11, Punkt 3).
    deleted: BTreeSet<GhostId>,
    pending_tt_nodes: Vec<GhostId>,
    pending_tt_edges: Vec<GhostId>,
    /// Anchor conditions of `rules` for the delta-local expansion
    /// ([`AnchorIndex`]): which (rule, position) a new node can stand at.
    anchor: AnchorSlot<'r>,
    /// Working memory of the anchor walks.
    walk: anchor::Walk,
    /// Working memory of the anchored searches.
    scratch: crate::engine::matcher::Scratch,
    /// Working memory of a step: the identities of its plan's nodes.
    ids: Vec<GhostId>,
    /// The names of the rules' plans and what they resolve to in the
    /// graph at hand ([`Names`]).
    names: Names,
}

/// Where the engine's [`AnchorIndex`] comes from.
enum AnchorSlot<'r> {
    /// Not built yet ([`Engine::new`]: built on first use).
    Unbuilt,
    /// Built by the engine itself, or taken from the indices the
    /// process keeps ([`anchor::Own`]).
    Owned(anchor::Own),
    /// Handed in by the host ([`Engine::with_anchor_index`]), with the
    /// rule slice (address, length) it was accepted for.
    Shared(&'r AnchorIndex, (usize, usize)),
}

/// Address and length of a rule slice.
fn slice_of(rules: &[DirectedRule]) -> (usize, usize) {
    (rules.as_ptr() as usize, rules.len())
}

impl AnchorSlot<'_> {
    /// The index of a slot [`Engine::take_index`] has filled.
    fn index(&self) -> &AnchorIndex {
        match self {
            AnchorSlot::Owned(a) => a,
            AnchorSlot::Shared(a, _) => a,
            AnchorSlot::Unbuilt => unreachable!("taken from `take_index`"),
        }
    }
}

/// The matches of every admitted rule over the whole graph, found
/// through the anchor index: rule by rule, those of one rule in μ order,
/// handed to `each`. That is the sequence of one [`find_matches`] per
/// rule, up to repetitions: a search returns a match once per way it
/// reads a direction-free link between two nodes connected both ways,
/// and the two searches can read that link at different steps. `record`
/// drops a repetition.
///
/// Every matchable node is anchored at the places the index offers for
/// it. A match is searched from the smallest of its nodes only: the
/// search anchored at a node leaves out every smaller one, and the match
/// with a smaller node is found at that node, where the index offers its
/// position as well (the condition of the index is necessary). A pattern
/// without nodes has no place in the index and is searched directly.
#[allow(clippy::too_many_arguments)]
fn index_seed(
    index: &AnchorIndex,
    rules: &[DirectedRule],
    g: &Graph,
    resolver: &dyn ValueResolver,
    walk: &mut anchor::Walk,
    scratch: &mut crate::engine::matcher::Scratch,
    admitted: &[bool],
    mut each: impl FnMut(usize, &[GhostId]),
) {
    let mut nodes: Vec<GhostId> = g
        .iter_nodes()
        .filter(|n| n.status.is_matchable())
        .map(|n| n.id)
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    let candidates = index.candidates(
        g,
        resolver,
        &nodes,
        walk,
        |ri| admitted[ri],
        |ni, id| *id < nodes[ni],
    );
    let mut next = 0;
    // The matches of one rule, one after the other, and their starts in
    // μ order.
    let mut found: Vec<GhostId> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for (ri, rule) in rules.iter().enumerate() {
        if !admitted[ri] {
            continue;
        }
        let pattern = &rule.pattern;
        let n = pattern.nodes.len();
        if n == 0 {
            for m in find_matches(g, resolver, pattern) {
                each(ri, &m);
            }
            continue;
        }
        found.clear();
        while let Some(&(rule_ix, ni, pos)) = candidates.get(next).filter(|c| c.0 == ri) {
            next += 1;
            let at = nodes[ni];
            let smaller = |id: &GhostId| *id < at;
            trace!(anchored_searches);
            find_matches_planned(
                g,
                resolver,
                pattern,
                index.plan(rule_ix, pos, pattern),
                pos,
                at,
                Some(&smaller),
                scratch,
            );
            for m in scratch.matches(pattern) {
                trace!(anchored_found);
                found.extend_from_slice(m);
            }
        }
        order.clear();
        order.extend((0..found.len() / n).map(|i| i * n));
        order.sort_unstable_by(|&a, &b| found[a..a + n].cmp(&found[b..b + n]));
        for &at in &order {
            each(ri, &found[at..at + n]);
        }
    }
    debug_assert_eq!(next, candidates.len(), "a candidate of a rule not admitted");
}

/// Does `anchor` already carry a correspondence of type `typ` with a
/// live counterpart of type `endpoint_typ`? `ix`: the entry of the
/// rule's `corr_recognition` the two types come from.
///
/// `step` asks this with the participations of the anchor at hand
/// (`anchor_translated_in`); this is the form the test checks compare
/// with.
#[cfg(any(test, seesaw_checks))]
fn anchor_translated(
    g: &Graph,
    names: &mut RuleNames<'_>,
    ix: usize,
    typ: &str,
    endpoint_typ: &str,
    anchor: &GhostId,
) -> bool {
    names.corr_typ(g, ix, typ).is_some_and(|t| {
        let ep = names.endpoint_typ(g, ix, endpoint_typ);
        g.parts_by_other_type(anchor, t).any(|p| {
            // TT does NOT count (M5 window): something tentatively
            // retracted doesn't suppress a new derivation.
            let corr_ok = g
                .connection(&p.connection)
                .is_some_and(|c| matches!(c.status, Status::Solid | Status::Ghost))
                && g.node_alive(&p.other);
            // Check the endpoint type: the corr's counterpart (≠ the
            // anchor) must match the expected type (single-corr-type
            // rule sets).
            corr_ok
                && ep.is_some_and(|ept| {
                    g.parts(&p.other)
                        .any(|q| q.other != *anchor && q.other_typ == ept && g.node_alive(&q.other))
                })
        })
    })
}

/// rc8-#2: every anchor of the rule's establishes corrs already
/// carries a corr of its type with a MATCHABLE counterpart — the
/// element is translated, regardless of which direction/variant ⇒ a
/// duplicate.
#[cfg(any(test, seesaw_checks))]
fn is_translated(
    g: &Graph,
    rule: &DirectedRule,
    refs: &[GhostId],
    names: &mut RuleNames<'_>,
) -> bool {
    !rule.corr_recognition.is_empty()
        && rule
            .corr_recognition
            .iter()
            .enumerate()
            .all(|(ix, (typ, pos, endpoint_typ))| {
                anchor_translated(g, names, ix, typ, endpoint_typ, &refs[*pos])
            })
}

/// What the graph holds of a rule's creation plan for one match.
enum Planned {
    /// Every node and every link of the plan exists, alive: applying
    /// the rule would create nothing.
    Exists,
    /// A node of the plan is a tombstone.
    Tombstone,
    /// Something of the plan is missing or tentatively retracted.
    Missing,
}

/// Derives the identities of the nodes of `rule`'s creation plan for
/// the match `refs` into `ids` (plan-indexed; a dynamic leaf without a
/// source gets a placeholder that is never read) and reports what the
/// graph holds of the plan. Each identity is derived once, for the
/// duplicate check, for the tombstone check and for the application.
///
/// The duplicate check asks whether everything the plan would create
/// already exists, matchable (ids are purely derivable). After a
/// tombstone `ids` is incomplete.
fn plan_of(
    g: &Graph,
    rule: &DirectedRule,
    refs: &[GhostId],
    ids: &mut Vec<GhostId>,
    names: &mut RuleNames<'_>,
) -> Planned {
    ids.clear();
    let mut all_alive = true;
    for cn in &rule.create_nodes {
        let parent = match cn.parent {
            Ref::Matched(p) => refs[p],
            Ref::New(ix) => ids[ix],
        };
        let id = if cn.corr_full_match {
            crate::graph::preview_corr_id(&parent, &cn.typ, refs)
        } else if let Some((anchor, ref attr, ref t)) = cn.derived_dyn {
            match names.child_leaf(g, ids.len(), &refs[anchor], attr) {
                // apply-if-present: without a source nothing would be
                // created.
                None => {
                    ids.push(parent); // placeholder (never referenced)
                    continue;
                }
                Some(src) => crate::graph::preview_derived_id(&parent, &cn.typ, &src, t),
            }
        } else {
            match (&cn.konst, &cn.derived) {
                (Some(_), _) => {
                    crate::graph::preview_konst_id(&parent, &cn.typ, &rule.name, ids.len() as u32)
                }
                (None, Some((leaf, t))) => {
                    crate::graph::preview_derived_id(&parent, &cn.typ, &refs[*leaf], t)
                }
                (None, None) => crate::graph::preview_ghost_id(&parent, &cn.typ),
            }
        };
        match g.node(&id).map(|n| n.status) {
            Some(Status::Tombstone) => return Planned::Tombstone,
            Some(Status::Solid | Status::Ghost) => {}
            _ => all_alive = false,
        }
        ids.push(id);
    }
    if !all_alive {
        return Planned::Missing;
    }
    let at = |r: Ref| match r {
        Ref::Matched(p) => refs[p],
        Ref::New(ix) => ids[ix],
    };
    if rule
        .create_links
        .iter()
        .all(|&(a, b)| g.connected_alive(&at(a), &at(b)))
    {
        Planned::Exists
    } else {
        Planned::Missing
    }
}

/// Up to this many new nodes, "stands earlier among the new nodes" is
/// answered by a scan of the slice; above it by a map.
const LINEAR_NEW: usize = 8;

/// Does `rule` take part under the configured mask and wave route?
fn rule_admitted(
    active: Option<&Vec<bool>>,
    directions: Option<WaveDirections>,
    rule: &DirectedRule,
    rule_ix: usize,
) -> bool {
    active.is_none_or(|configured| configured[rule_ix])
        && directions.is_none_or(|d| d.admits(rule.direction))
}

impl<'r> Engine<'r> {
    pub fn new(rules: &'r [DirectedRule]) -> Self {
        Self {
            rules,
            matches: Vec::new(),
            by_element: Default::default(),
            links: Links::new(),
            applied: Applied::new(),
            todo: Todo::new(),
            by_key: Default::default(),
            by_product: Default::default(),
            #[cfg(any(test, seesaw_checks))]
            by_product_filed: Default::default(),
            saw_contradiction: false,
            cascade: Vec::new(),
            active: None,
            wave_directions: None,
            deleting: false,
            deleted: BTreeSet::new(),
            pending_tt_nodes: Vec::new(),
            pending_tt_edges: Vec::new(),
            anchor: AnchorSlot::Unbuilt,
            walk: anchor::Walk::default(),
            scratch: Default::default(),
            ids: Vec::new(),
            names: Default::default(),
        }
    }

    /// Like [`Engine::new`], with an [`AnchorIndex`] the host built once
    /// for `rules` (several engines over the same rules share it). An
    /// index built for other rules is not used; the engine then builds
    /// its own. Whether the index belongs to `rules` is decided here by
    /// the patterns ([`AnchorIndex::fits`]), so an index built from an
    /// equal copy of the rules is taken, and one built from other rules
    /// at the same address is not.
    pub fn with_anchor_index(rules: &'r [DirectedRule], index: &'r AnchorIndex) -> Self {
        let mut e = Self::new(rules);
        if index.fits(rules) {
            e.anchor = AnchorSlot::Shared(index, slice_of(rules));
        }
        e
    }

    pub fn step(&mut self, g: &mut Graph, resolver: &dyn ValueResolver) -> Option<bool> {
        self.step_with_limit(g, resolver, None)
    }

    /// Admit one complete external delta before invoking its Add,
    /// Change, and Delete doors. The domain union is fixed for the
    /// complete retraction/re-derivation/consolidation wave.
    pub fn admit_delta(&mut self, domains: &[DeltaDomain]) {
        let mut directions = WaveDirections::default();
        for domain in domains {
            match domain {
                DeltaDomain::Source => directions.forward = true,
                DeltaDomain::Target => directions.backward = true,
            }
        }
        self.wave_directions = Some(directions);
    }

    /// Select the un-routed initial/batch semantics explicitly after a
    /// routed wave. A fresh engine starts in this state already.
    pub fn admit_initial(&mut self) {
        self.wave_directions = None;
    }

    fn admit_delta_types(&mut self, delta_types: &[String]) {
        let mut directions = WaveDirections::default();
        for rule in self.rules {
            if rule
                .input_types
                .iter()
                .any(|t| delta_types.iter().any(|d| d == t))
            {
                match rule.direction {
                    RuleDirection::Forward => directions.forward = true,
                    RuleDirection::Backward => directions.backward = true,
                    RuleDirection::Undirected => {}
                }
            }
        }
        self.wave_directions = Some(directions);
    }

    fn rule_active(&self, rule_ix: usize) -> bool {
        rule_admitted(
            self.active.as_ref(),
            self.wave_directions,
            &self.rules[rule_ix],
            rule_ix,
        )
    }

    fn record(&mut self, g: &Graph, rule_ix: usize, refs: &[GhostId]) {
        trace!(record_calls);
        let refs: Refs = refs.into();
        let slot = match self.by_key.entry((rule_ix, refs.clone())) {
            std::collections::hash_map::Entry::Occupied(_) => {
                trace!(record_known);
                return;
            }
            std::collections::hash_map::Entry::Vacant(slot) => slot,
        };
        let ix = self.matches.len() as u32;
        for id in refs.iter() {
            self.by_element.entry(*id).or_default().push(ix);
        }
        file_links(&mut self.links, g, &self.rules[rule_ix].pattern, &refs, ix);
        self.todo
            .insert(self.rules[rule_ix].rank, &refs, rule_ix, ix);
        slot.insert(ix);
        self.matches.push(Stored {
            rule_ix,
            refs,
            dead: false,
            entry: None,
        });
    }

    /// Initial enumeration: all rules in full (seeds) — with stage 8
    /// (add stream) this special path mostly disappears.
    ///
    /// The matches are recorded rule by rule, those of one rule in μ
    /// order. They are found either by one search per rule over the
    /// whole graph or through the [`AnchorIndex`], whichever looks at
    /// fewer nodes; both find the same matches and record them in the
    /// same order.
    pub fn seed(&mut self, g: &Graph, resolver: &dyn ValueResolver) {
        self.settle_removed(g);
        #[cfg(any(test, seesaw_checks))]
        self.check_seed(g, resolver);
        if self.seed_through_index(g) {
            self.seed_by_index(g, resolver);
        } else {
            self.seed_by_scan(g, resolver);
        }
    }

    /// One search per admitted rule over the whole graph. The search
    /// starts at the nodes of the type of the rule's first position.
    fn seed_by_scan(&mut self, g: &Graph, resolver: &dyn ValueResolver) {
        for ri in 0..self.rules.len() {
            if self.rule_active(ri) {
                self.seed_rule(g, resolver, ri);
            }
        }
    }

    fn seed_rule(&mut self, g: &Graph, resolver: &dyn ValueResolver, ri: usize) {
        trace!(seed_searches);
        for m in find_matches(g, resolver, &self.rules[ri].pattern) {
            self.record(g, ri, &m);
        }
    }

    /// Does the seed go through the [`AnchorIndex`]?
    ///
    /// The scan looks at the nodes of the type of the first position
    /// once per rule; the index walks every node once for all rules. In
    /// a rule set whose rules share one structural type the scan costs
    /// rules × nodes, the walk nodes. In a rule set with a type of its
    /// own per rule the scan looks at fewer nodes than the graph has,
    /// and the walk costs more per node than the scan. The index is
    /// taken when the scan would start at more than `FACTOR` times the
    /// nodes of the graph (measured: 560 times in the suite's language
    /// pack with 700 kinds, 0.3 times in Families to Persons).
    fn seed_through_index(&self, g: &Graph) -> bool {
        const FACTOR: u64 = 4;
        // In 64 bits: rules times nodes can pass a 32-bit `usize`.
        let starts: u64 = self
            .rules
            .iter()
            .enumerate()
            .filter(|(ri, _)| self.rule_active(*ri))
            .filter_map(|(_, rule)| rule.pattern.nodes.first())
            .map(|pn| g.type_count(pn.typ) as u64)
            .sum();
        starts > FACTOR * g.node_count() as u64
    }

    /// The seed through the [`AnchorIndex`]: every matchable node of the
    /// graph is anchored like an added one ([`index_seed`]).
    fn seed_by_index(&mut self, g: &Graph, resolver: &dyn ValueResolver) {
        let rules = self.rules;
        let admitted: Vec<bool> = (0..rules.len()).map(|ri| self.rule_active(ri)).collect();
        let anchor = self.take_index();
        let mut walk = std::mem::take(&mut self.walk);
        let mut scratch = std::mem::take(&mut self.scratch);
        index_seed(
            anchor.index(),
            rules,
            g,
            resolver,
            &mut walk,
            &mut scratch,
            &admitted,
            |ri, m| self.record(g, ri, m),
        );
        self.walk = walk;
        self.scratch = scratch;
        self.anchor = anchor;
    }

    /// Test check of the seed: the index finds the matches of the scan,
    /// in its order.
    #[cfg(any(test, seesaw_checks))]
    fn check_seed(&mut self, g: &Graph, resolver: &dyn ValueResolver) {
        let rules = self.rules;
        let admitted: Vec<bool> = (0..rules.len()).map(|ri| self.rule_active(ri)).collect();
        let mut scanned: Vec<(usize, Vec<GhostId>)> = Vec::new();
        for (ri, rule) in rules.iter().enumerate() {
            if admitted[ri] {
                scanned.extend(
                    find_matches(g, resolver, &rule.pattern)
                        .into_iter()
                        .map(|m| (ri, m)),
                );
            }
        }
        let anchor = self.take_index();
        let mut walk = std::mem::take(&mut self.walk);
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut indexed: Vec<(usize, Vec<GhostId>)> = Vec::new();
        index_seed(
            anchor.index(),
            rules,
            g,
            resolver,
            &mut walk,
            &mut scratch,
            &admitted,
            |ri, m| indexed.push((ri, m.to_vec())),
        );
        self.walk = walk;
        self.scratch = scratch;
        self.anchor = anchor;
        // Repetitions of one match stand next to each other (μ order)
        // and are dropped by `record`.
        indexed.dedup();
        scanned.dedup();
        assert_eq!(
            indexed, scanned,
            "the seed through the index differs from the scan"
        );
    }

    /// The engine's [`AnchorIndex`], built if it is missing or was built
    /// for other rules. It leaves the engine for the time of an
    /// expansion, so that the searches can read its plans while `record`
    /// writes the match store; the caller puts it back.
    fn take_index(&mut self) -> AnchorSlot<'r> {
        let rules = self.rules;
        let usable = match &self.anchor {
            AnchorSlot::Unbuilt => false,
            AnchorSlot::Owned(a) => a.accepts(rules),
            // Accepted for the slice the engine was created with; the
            // field `rules` is public and may have been set since.
            AnchorSlot::Shared(_, accepted) => *accepted == slice_of(rules),
        };
        if !usable {
            self.anchor = AnchorSlot::Owned(anchor::Own::for_rules(rules));
        }
        std::mem::replace(&mut self.anchor, AnchorSlot::Unbuilt)
    }

    /// Compatibility route for hosts that report touched type names.
    /// The types select a DIRECTION; every rule lowered for that
    /// direction remains active for the wave, including follow-up rules
    /// enabled by newly created correspondence context.
    pub fn seed_routed(&mut self, g: &Graph, resolver: &dyn ValueResolver, delta_types: &[String]) {
        self.admit_delta_types(delta_types);
        self.seed(g, resolver);
    }

    /// Add stream (spec §1.6): anchor externally added elements DELTA-
    /// LOCALLY — without a full `seed`. Symmetric to `element_removed`;
    /// call `step` afterward until saturation. The candidates for the
    /// neighboring positions come via the edge index
    /// (`parts_by_other_type`), not via a type scan of the model — the
    /// cost hangs off the delta's local neighborhood, not the model
    /// size.
    pub fn elements_added(
        &mut self,
        g: &Graph,
        resolver: &dyn ValueResolver,
        new_nodes: &[GhostId],
    ) {
        self.settle_removed(g);
        self.expand_at(g, resolver, new_nodes);
    }

    /// Delta-local expansion: anchor new elements (spec §1.6 add).
    ///
    /// Anchors only at the (rule, node, position) triples whose
    /// neighbourhood realises the paths the pattern requires from there
    /// ([`AnchorIndex`]); the triples come in the order of the loop over
    /// rules, nodes and positions, so the recorded matches and their
    /// order are those of anchoring at every position of the same type.
    ///
    /// Every match found is recorded, also one whose anchor is already
    /// translated: `step` decides that when it
    /// pulls the match, on the graph as it is then, and the match store
    /// keeps the duplicate so that a retraction of one of its elements
    /// makes it a candidate again.
    fn expand_at(&mut self, g: &Graph, resolver: &dyn ValueResolver, new_nodes: &[GhostId]) {
        if new_nodes.is_empty() {
            return;
        }
        let rules = self.rules;
        let anchor = self.take_index();
        let index = anchor.index();
        // A match that binds several of the new nodes would be found
        // once from each of them. It is searched from the first of them
        // only: the search anchored at `new_nodes[ni]` leaves out every
        // node that stands earlier in `new_nodes`. A match with such a
        // node has it at a position where the index offers it as a
        // candidate too (the condition is necessary), and that triple
        // sorts before this one, so the match is recorded there, at the
        // place in the order where it was recorded first before. The
        // index leaves the same nodes out of its walks, so it offers no
        // place whose search would only find such matches, and none for
        // a node that stands in `new_nodes` a second time.
        let first: crate::hash::FxHashMap<GhostId, usize> = if new_nodes.len() > LINEAR_NEW {
            let mut first = crate::hash::FxHashMap::default();
            for (i, id) in new_nodes.iter().enumerate() {
                first.entry(*id).or_insert(i);
            }
            first
        } else {
            crate::hash::FxHashMap::default()
        };
        let earlier = |ni: usize, id: &GhostId| {
            if new_nodes.len() > LINEAR_NEW {
                first.get(id).is_some_and(|&i| i < ni)
            } else {
                new_nodes[..ni].contains(id)
            }
        };
        let candidates = {
            let (active, directions) = (self.active.as_ref(), self.wave_directions);
            index.candidates(
                g,
                resolver,
                new_nodes,
                &mut self.walk,
                |ri| rule_admitted(active, directions, &rules[ri], ri),
                earlier,
            )
        };
        #[cfg(any(test, seesaw_checks))]
        self.check_candidates(g, resolver, new_nodes, &candidates);
        let mut scratch = std::mem::take(&mut self.scratch);
        for (ri, ni, pos) in candidates {
            let earlier = |id: &GhostId| earlier(ni, id);
            debug_assert!(!earlier(&new_nodes[ni]), "a triple for a repeated node");
            let pattern = &rules[ri].pattern;
            let plan = index.plan(ri, pos, pattern);
            trace!(anchored_searches);
            find_matches_planned(
                g,
                resolver,
                pattern,
                plan,
                pos,
                new_nodes[ni],
                Some(&earlier),
                &mut scratch,
            );
            for m in scratch.matches(pattern) {
                trace!(anchored_found);
                self.record(g, ri, m);
            }
        }
        self.scratch = scratch;
        self.anchor = anchor;
        #[cfg(any(test, seesaw_checks))]
        self.check_recorded(g, resolver, new_nodes);
    }

    /// Test check of the expansion as a whole: every match the
    /// unfiltered loop finds (every active rule, every new node, every
    /// position of its type, no node left out) is in the match store
    /// afterwards.
    #[cfg(any(test, seesaw_checks))]
    fn check_recorded(&self, g: &Graph, resolver: &dyn ValueResolver, new_nodes: &[GhostId]) {
        for (ri, rule) in self.rules.iter().enumerate() {
            if !self.rule_active(ri) {
                continue;
            }
            for id in new_nodes {
                let Some(node) = g.node(id) else { continue };
                for (pos, pn) in rule.pattern.nodes.iter().enumerate() {
                    if pn.typ != node.typ {
                        continue;
                    }
                    let mut fixed = vec![None; rule.pattern.nodes.len()];
                    fixed[pos] = Some(*id);
                    for m in find_matches_with_fixed(g, resolver, &rule.pattern, &fixed) {
                        assert!(
                            self.by_key.contains_key(&(ri, Refs::from(m))),
                            "expansion drops a match: rule {} position {pos}",
                            rule.name
                        );
                    }
                }
            }
        }
    }

    /// Test check of [`anchor::AnchorIndex`]: every (rule, node,
    /// position) triple at which the search that leaves out the earlier
    /// new nodes finds a match is a candidate.
    #[cfg(any(test, seesaw_checks))]
    fn check_candidates(
        &self,
        g: &Graph,
        resolver: &dyn ValueResolver,
        new_nodes: &[GhostId],
        candidates: &[(usize, usize, usize)],
    ) {
        for (ri, rule) in self.rules.iter().enumerate() {
            if !self.rule_active(ri) {
                continue;
            }
            for (ni, id) in new_nodes.iter().enumerate() {
                let Some(node) = g.node(id) else { continue };
                for (pos, pn) in rule.pattern.nodes.iter().enumerate() {
                    if pn.typ != node.typ {
                        continue;
                    }
                    let earlier = |other: &GhostId| new_nodes[..ni].contains(other);
                    if earlier(id) {
                        continue;
                    }
                    let mut fixed = vec![None; rule.pattern.nodes.len()];
                    fixed[pos] = Some(*id);
                    let found = crate::engine::matcher::find_matches_without(
                        g,
                        resolver,
                        &rule.pattern,
                        &fixed,
                        Some(&earlier),
                    );
                    if !found.is_empty() {
                        assert!(
                            candidates.binary_search(&(ri, ni, pos)).is_ok(),
                            "anchor index drops a match: rule {} position {pos}",
                            rule.name
                        );
                    }
                }
            }
        }
    }

    /// Retraction (M5.3/M5.5): an element has dropped out —
    /// applied entries whose match contained it lose their
    /// justification. Their CREATED elements are tentatively
    /// tombstoned (a provenance walk: entries' created lists,
    /// recursively over entries anchored on created elements).
    /// Consolidation (`consolidate`) decides TT → tombstone or
    /// resurrection (if a new derivation reclaimed the same structural
    /// identity).
    /// **Der Host hat ein Element GELOESCHT.** Eine der drei Tueren des
    /// Interfaces zwischen Host und Engine (add / delete / update).
    ///
    /// Der Delta-Typ kommt vom AUFRUF, nicht aus einer Statuspruefung.
    /// Bis 2026-08-13 gab es nur `retract_for`, und die Engine las am
    /// Status ab, was gemeint war: tot beim Eintreffen hiess Delete,
    /// lebendig hiess Update. Das war eine Absprache zwischen Host und
    /// Engine, keine Schnittstelle -- wer die Reihenfolge vertauschte,
    /// bekam stillschweigend die andere Semantik.
    ///
    /// Der Aufrufer hat den Tombstone bereits gesetzt.
    pub fn element_deleted(&mut self, g: &mut Graph, removed: &GhostId) {
        self.settle_removed(g);
        self.deleted.insert(*removed);
        self.retract_wave(g, removed, true);
    }

    /// **Der Host hat ein Element GEAENDERT.** Die dritte Tuer.
    ///
    /// Zurueckziehen und neu ableiten, aber ohne dass etwas entlang der
    /// Korrespondenzen faellt: ein Change, der seine Realisierung
    /// verletzt, zieht sie samt Korrespondenz zurueck, macht daraus
    /// aber kein Delete der Gegenseite.
    pub fn element_changed(&mut self, g: &mut Graph, changed: &GhostId) {
        self.settle_removed(g);
        self.retract_wave(g, changed, false);
    }

    fn retract_wave(&mut self, g: &mut Graph, id: &GhostId, deleting: bool) {
        let outer = std::mem::replace(&mut self.deleting, deleting);
        let mut queue: Vec<GhostId> = vec![*id];
        self.drain_retraction(g, &mut queue);
        self.deleting = outer;
    }

    #[deprecated(note = "use element_deleted or element_changed — the delta type \
                         belongs to the call, not to a status check")]
    pub fn retract_for(&mut self, g: &mut Graph, removed: &GhostId) {
        self.settle_removed(g);
        let deleting = !g.node(removed).is_some_and(|n| n.status.is_matchable());
        if deleting {
            self.deleted.insert(*removed);
        }
        self.retract_wave(g, removed, deleting);
    }

    /// The cascade entries that produced the element `id`, ascending.
    ///
    /// A node is filed in `by_product`. A connection is not: its
    /// producer names it among its created edges, and it created or
    /// matched one of the connection's ends (a link of a creation plan
    /// joins matched and created nodes, and the connections of a
    /// dynamic attribute correspondence start at the correspondence the
    /// entry created). The producers of the ends and the entries of the
    /// matches that bind an end are therefore all the candidates.
    fn producers_of(&self, g: &Graph, id: &GhostId) -> Ixs {
        match g.connection(id) {
            None => self.by_product.get(id).cloned().unwrap_or_default(),
            Some(c) => {
                let mut eixs: Vec<u32> = Vec::new();
                for end in [&c.source, &c.target] {
                    if let Some(produced_by) = self.by_product.get(end) {
                        eixs.extend_from_slice(produced_by.as_slice());
                    }
                    if let Some(bound_in) = self.by_element.get(end) {
                        eixs.extend(
                            bound_in
                                .as_slice()
                                .iter()
                                .filter_map(|&mix| self.matches[mix as usize].entry),
                        );
                    }
                }
                eixs.sort_unstable();
                eixs.dedup();
                let mut producers = Ixs::default();
                for eix in eixs {
                    if self.cascade[eix as usize].created_edges.contains(id) {
                        producers.push(eix);
                    }
                }
                #[cfg(any(test, seesaw_checks))]
                {
                    let mut filed = self
                        .by_product_filed
                        .get(id)
                        .map(|ixs| ixs.as_slice().to_vec())
                        .unwrap_or_default();
                    // An entry that names a connection twice was filed
                    // twice.
                    filed.dedup();
                    assert_eq!(
                        producers.as_slice(),
                        filed.as_slice(),
                        "the producers found from the ends differ from the filed ones"
                    );
                }
                producers
            }
        }
    }

    /// A single match loses its justification: invalidate it (dead +
    /// todo), forget the record (applied/by_key, reclaim ability) and
    /// — if it was applied — follow the provenance edge to its
    /// products (TT + queue). Shared core of `retract_for` and
    /// `link_removed`.
    /// Retract by ENTRY: find the match that produced it and retract
    /// that. Reuses `retract_match` so the reclaim bookkeeping
    /// (`applied`/`by_key`) stays in one place.
    fn retract_entry(&mut self, g: &mut Graph, eix: u32, queue: &mut Vec<GhostId>) {
        let key = {
            let e = &self.cascade[eix as usize];
            (e.rule_ix, Refs::from(e.refs.as_slice()))
        };
        if let Some(&mix) = self.by_key.get(&key) {
            self.retract_match(g, mix, queue);
        }
    }

    fn retract_match(&mut self, g: &mut Graph, mix: u32, queue: &mut Vec<GhostId>) {
        let (rule_ix, refs, entry, was_dead) = {
            let m = &self.matches[mix as usize];
            (m.rule_ix, m.refs.clone(), m.entry, m.dead)
        };
        // Invalidate the candidate (like `element_removed`): dead + out
        // of the todo. Idempotent if `element_removed` already ran.
        if !was_dead {
            self.matches[mix as usize].dead = true;
            self.todo
                .remove(self.rules[rule_ix].rank, &refs, rule_ix, mix);
        }
        // Reclaim ability: FORGET the record (applied/by_key), so an
        // identical re-derivation can APPLY again and thereby
        // resurrect (M5 reclaim via insert_node/connect).
        let key = (rule_ix, refs);
        self.applied.forget(&key);
        let current = self.by_key.remove(&key);
        self.applied.forget_match(current);
        // If the match WAS APPLIED (provenance edge set), follow it:
        // tentatively tombstone its products and continue via the
        // queue. The next by_element lookup finds the children for
        // each `created` entry.
        if let Some(eix) = entry {
            let eix = eix as usize;
            // By index: the loops write the graph, the pending lists and
            // the queue, not the cascade.
            for i in 0..self.cascade[eix].created.len() {
                let c = self.cascade[eix].created[i];
                // A product of THIS entry, so its provenance is proven
                // by the loop itself — the status does not decide.
                //
                // Until 2026-08-10 the condition read `== Ghost`, which
                // ended retraction at a materialization: a folded
                // product is `Solid` and was left standing, so the
                // delta produced no tombstone at all. Provenance is the
                // criterion, not the lifecycle state; `add_baseline`
                // nodes appear in no `created` and stay untouched.
                //
                // TENTATIVE, not final: the consolidation at the end of
                // the run decides. Re-derived within this run means the
                // element is reclaimed, otherwise it resolves to
                // `Tombstone`. At rest only `Ghost` and `Solid` remain.
                if g.retract_node(&c) {
                    self.pending_tt_nodes.push(c);
                }
                queue.push(c);
            }
            // Tentatively retract the CREATED EDGES: reclaim via
            // `connect` lifts TT again (same endpoints re-derived),
            // otherwise `consolidate` finalizes to tombstone. Edges
            // have no provenance children ⇒ not queued.
            for i in 0..self.cascade[eix].created_edges.len() {
                let e = self.cascade[eix].created_edges[i];
                // Same reasoning as for the nodes above: provenance
                // decides, not the lifecycle state. A materialized
                // edge is `Solid` and was left standing until
                // 2026-08-10, so a retracted node kept its connections.
                if g.retract_connection(&e) {
                    self.pending_tt_edges.push(e);
                }
            }
        }
    }

    /// Provenance walk over the queue (core of `retract_for`).
    fn drain_retraction(&mut self, g: &mut Graph, queue: &mut Vec<GhostId>) {
        // Only filled and asked, never walked.
        let mut seen: std::collections::HashSet<GhostId, crate::hash::FxBuildHasher> =
            Default::default();
        while let Some(id) = queue.pop() {
            if !seen.insert(id) {
                continue;
            }
            // Affected matches DIRECTLY via the by_element index (spec
            // §1.6) — no scan over applied/by_key/cascade. Every match
            // whose refs contain `id` shows up here, because `record`
            // maintains the index for every ref element. This covers
            // both applied and suppressed duplicate matches (rename:
            // attach dropped out, create must become applicable again).
            // Two ways an element can justify further retraction.
            //
            // It PARTICIPATES in matches: those lose their ground.
            // A copy of the list (in place up to four entries): the
            // retraction below writes the engine.
            let mixes: Ixs = self.filed_under(g, &id);
            for &mix in mixes.as_slice() {
                self.retract_match(g, mix, queue);
            }
            // It was PRODUCED by entries: those lose their result, and
            // with them everything else they built — the correspondence
            // above all. Without this direction a deleted product left
            // its correspondence standing, because a rule's match does
            // not contain what the rule creates (measured 2026-08-11).
            let eixs = self.producers_of(g, &id);
            if !eixs.as_slice().is_empty() {
                // Die Ursache folgt dem PFAD, nicht dem Zeitraum. Durch
                // eine Delete-Welle laufen auch Elemente, die selbst
                // nicht geloescht wurden -- Erzeugnisse, die fallen,
                // weil ihr Match den Boden verlor. Deren Eintraege
                // tragen keine Loeschung weiter. Die Tuer sagt, ob
                // ueberhaupt geloescht wurde; dieses Element sagt, ob
                // die Loeschung HIER anliegt.
                let hier = !g.node(&id).is_some_and(|n| n.status.is_matchable())
                    && g.connection(&id).is_none();
                let hier_gilt = self.deleting && hier;
                let outer = std::mem::replace(&mut self.deleting, hier_gilt);
                for &eix in eixs.as_slice() {
                    self.retract_entry(g, eix, queue);
                }
                self.deleting = outer;
            }
        }
    }

    /// Consolidation (M5.5): TentativeTombstone → Tombstone, unless a
    /// new derivation has reclaimed the element (a reclaim resets the
    /// status in `insert_node`/`connect`).
    pub fn consolidate(&mut self, g: &mut Graph) {
        self.settle_removed(g);
        // Processes ONLY the TT collected by retraction — no full
        // model scan. An element that was RECLAIMED in the meantime is
        // still in the list, but is no longer TT (connect set it to
        // Ghost) ⇒ the status check skips it.
        let mut fallen: Vec<GhostId> = Vec::new();
        for id in self.pending_tt_nodes.drain(..) {
            if g.settle_node(&id) == Some(true) {
                fallen.push(id);
            }
        }
        for id in self.pending_tt_edges.drain(..) {
            g.settle_connection(&id);
        }
        self.follow_fallen_corrs(g, &mut fallen);
        // Die Markierung gehoert der Welle, die gerade endete.
        self.deleted.clear();
    }

    /// **Deletion along the correspondences** (Sandra 2026-08-11).
    ///
    /// A correspondence that survived consolidation as a tombstone
    /// attests a translation that no longer holds, so the elements it
    /// connects fall with it. That carries a deletion to the other
    /// side: deleting the generated Java class deletes the UML class.
    ///
    /// Why HERE and not in `drain_retraction`: retraction is also the
    /// engine's internal means for a change (retract, re-derive,
    /// reclaim). Mid-retraction a fallen correspondence says nothing —
    /// it may be reclaimed in the same run. Only consolidation decides
    /// whether it really fell, which is the same distinction Sandra's
    /// constraint draws. A change that still carries its realisation
    /// acts along the same correspondence. A change that VIOLATES its
    /// realisation retracts it, correspondence included -- but it does
    /// not turn that into a delete of the far side. Only a delete does
    /// that, and only through a correspondence that consolidation left
    /// fallen.
    ///
    /// Transitivity comes for free: each fallen endpoint is retracted
    /// like any other removal, which can fell further correspondences,
    /// so the loop runs to a fixed point.
    fn follow_fallen_corrs(&mut self, g: &mut Graph, fallen: &mut Vec<GhostId>) {
        let mut seen: std::collections::HashSet<GhostId, crate::hash::FxBuildHasher> =
            Default::default();
        while let Some(corr) = fallen.pop() {
            if !seen.insert(corr) || !g.node(&corr).is_some_and(|n| n.is_corr) {
                continue;
            }
            let ends: Vec<GhostId> = g.parts(&corr).map(|p| p.other).collect();
            // A fallen correspondence carries a deletion only when an
            // end was DELETED, not merely retracted. A product that
            // vanished because its rule stopped applying (a re-typing
            // drops the Cellar, a removed link drops the NextConstrRel)
            // is the result of a change. A change may well retract a
            // correspondence, but it never deletes the far side. Measured on case02, where exactly those
            // two took a live House and a live NextRel with them.
            if !ends.iter().any(|e| self.deleted.contains(e)) {
                continue;
            }
            for end in &ends {
                let end = *end;
                if !g.node(&end).is_some_and(|n| n.status.is_matchable()) {
                    continue;
                }
                g.set_node_status(&end, Status::Tombstone);
                // Transitivity: an element felled along a correspondence
                // is deleted, so it carries the next one in turn.
                self.deleted.insert(end);
                self.element_removed(&end);
                // Transitivitaet: ein entlang einer Korrespondenz
                // mitgerissenes Element IST geloescht, seine eigene
                // Welle traegt die Ursache weiter.
                let outer = std::mem::replace(&mut self.deleting, true);
                let mut queue: Vec<GhostId> = vec![end];
                self.drain_retraction(g, &mut queue);
                self.deleting = outer;
                // The endpoint's own retraction may have felled further
                // correspondences; resolve them in the same fixed point.
                for id in self.pending_tt_nodes.drain(..) {
                    if g.settle_node(&id) == Some(true) {
                        fallen.push(id);
                    }
                }
                for id in self.pending_tt_edges.drain(..) {
                    g.settle_connection(&id);
                }
            }
        }
    }

    /// Edge removal (spec §1.6, delete on a connection): matches whose
    /// refs contain BOTH endpoints lose their justification — the match
    /// memory only indexes nodes (`by_element`), it doesn't know a used
    /// link; without this call, a tombstoned edge silently strips the
    /// justification from a match (case 19 finding: UpdateRef didn't
    /// re-derive dstName because the rc8-#2 recognition saw the
    /// surviving corr). Deliberately an OVER-approximation: even a
    /// match that uses both nodes without the removed edge gets
    /// retracted — the TT/resurrection semantics heal that, because the
    /// identical re-derivation reclaims on the next sync (id-stable).
    /// Symmetric to `element_removed`/`retract_for`; call
    /// `seed`/`step` until saturation and `consolidate` afterward.
    /// Ein Link ist HINZUGEKOMMEN (Gegenstueck zu [`Self::link_removed`]).
    ///
    /// Ein `connect` auf zwei BESTEHENDE Knoten hat bisher nichts
    /// geweckt: die Anker-Expansion laeuft ueber neue KNOTEN, und
    /// beide Enden waren ja schon da. Ein reines Add-Kanten-Delta
    /// (Owner-Wechsel, Generalisierung) blieb damit unsichtbar,
    /// weshalb der ecore2sql-Ledger auch dafuer den ganzen Knoten
    /// ersetzt statt die Kante zu ziehen -- mit dem Zwischenzustand
    /// zweier Traeger, den Sandra am 2026-08-12 ausgeschlossen hat.
    ///
    /// Beide Enden neu zu expandieren ist die gleiche
    /// Ueber-Approximation wie bei `link_removed`: es kann Kandidaten
    /// wecken, die den neuen Link gar nicht brauchen. Die
    /// Duplikat-Unterdrueckung faengt sie ab, weil Identitaeten
    /// ableitbar sind.
    pub fn link_added(
        &mut self,
        g: &Graph,
        resolver: &dyn ValueResolver,
        a: &GhostId,
        b: &GhostId,
    ) {
        self.settle_removed(g);
        self.expand_at(g, resolver, &[*a, *b]);
    }

    pub fn link_removed(&mut self, g: &mut Graph, a: &GhostId, b: &GhostId) {
        self.settle_removed(g);
        let Some(in_a) = self.by_element.get(a) else {
            return;
        };
        let Some(in_b) = self.by_element.get(b) else {
            return;
        };
        let set_b: BTreeSet<u32> = in_b.as_slice().iter().copied().collect();
        let mixes: Vec<u32> = in_a
            .as_slice()
            .iter()
            .copied()
            .filter(|m| set_b.contains(m))
            .collect();
        let mut queue: Vec<GhostId> = Vec::new();
        for mix in mixes {
            self.retract_match(g, mix, &mut queue);
        }
        self.drain_retraction(g, &mut queue);
    }

    /// delete/modify (spec §1.6): EAGERLY kill the element's matches.
    pub fn element_removed(&mut self, id: &GhostId) {
        let Some(refs) = self.by_element.get(id).cloned() else {
            self.removed_unfiled(id);
            return;
        };
        self.kill(refs.as_slice());
    }

    /// Kills matches eagerly: dead and out of the todo.
    fn kill(&mut self, mixes: &[u32]) {
        for &mix in mixes {
            let m = &mut self.matches[mix as usize];
            if !m.dead {
                m.dead = true;
                self.todo
                    .remove(self.rules[m.rule_ix].rank, &m.refs, m.rule_ix, mix);
            }
        }
    }

    /// One step: pull the highest-ranked live candidate.
    /// `ceiling`: only candidates strictly BELOW the bound (def. 3.10,
    /// position-local — the caller manages the position).
    pub fn step_with_limit(
        &mut self,
        g: &mut Graph,
        resolver: &dyn ValueResolver,
        ceiling: Option<&SelectionBound>,
    ) -> Option<bool> {
        // `true` in Some = applied; None = no admitted todo below the bound.
        self.settle_removed(g);
        loop {
            let filtered = self.active.is_some() || self.wave_directions.is_some();
            let taken = match ceiling {
                None if !filtered => {
                    trace!(todo_scanned);
                    self.todo.pop_first()
                }
                None => {
                    let (active, directions, rules) =
                        (self.active.as_ref(), self.wave_directions, self.rules);
                    self.todo.take_first(|rule_ix| {
                        trace!(todo_scanned);
                        rule_admitted(active, directions, &rules[rule_ix], rule_ix)
                    })
                }
                Some(b) => {
                    // Strictly below the bound: from the position of
                    // the bound on, but not at that position itself.
                    let (active, directions, rules) =
                        (self.active.as_ref(), self.wave_directions, self.rules);
                    self.todo.take_from(
                        b.rank,
                        &Refs::from(b.refs.as_slice()),
                        |at_bound, rule_ix| {
                            trace!(todo_scanned);
                            !at_bound
                                && (!filtered
                                    || rule_admitted(active, directions, &rules[rule_ix], rule_ix))
                        },
                    )
                }
            };
            let Taken {
                rule_ix,
                refs,
                carried,
            } = taken?;
            let rule = &self.rules[rule_ix];
            if self.applied.contains(rule_ix, &refs, carried) {
                continue;
            }
            let mut names = self.names.for_step(g, self.rules, rule_ix);
            match recognise(g, rule, &refs, &mut names) {
                // TT ANCHORS don't fire: the resurrection window keeps
                // TT matchable for re-derivations (context reclaim) —
                // but as an ESTABLISHES anchor, something retracted
                // doesn't work (otherwise reclaim zombies). Context
                // refs may be TT (reclaim through use). A forgotten
                // match is found again by the next seed, if
                // resurrected.
                Recognised::Tentative => {
                    self.by_key.remove(&(rule_ix, refs));
                    continue;
                }
                // rc8-#2: the anchor already carries a corr of
                // this type with a MATCHABLE counterpart — the element
                // is translated, regardless of which direction/variant
                // ⇒ a duplicate.
                Recognised::Translated => {
                    // Duplicate: nothing new — note it for the verdict.
                    self.applied.insert_owned(rule_ix, refs, carried);
                    return Some(false);
                }
                Recognised::Open => {}
            }
            let mut ids = std::mem::take(&mut self.ids);
            match plan_of(g, rule, &refs, &mut ids, &mut names) {
                Planned::Exists => {
                    // Duplicate as well: everything the plan would
                    // create is there.
                    self.ids = ids;
                    self.applied.insert_owned(rule_ix, refs, carried);
                    return Some(false);
                }
                Planned::Tombstone => {
                    // V7 guard: reusing tombstone substance is a
                    // contradiction with history (not TT — that's the
                    // legitimate resurrection zone).
                    self.ids = ids;
                    self.saw_contradiction = true;
                    continue;
                }
                Planned::Missing => {}
            }
            trace!(steps_applied);
            let (created, created_edges) =
                apply_creation_at(g, rule, &refs, Some(&ids), &mut names);
            self.ids = ids;
            let eix = self.cascade.len() as u32;
            // Provenance, product side: every node this entry built
            // points back at it. Its edges are found from their ends
            // (`producers_of`).
            for id in &created {
                self.by_product.entry(*id).or_default().push(eix);
            }
            #[cfg(any(test, seesaw_checks))]
            for id in &created_edges {
                self.by_product_filed.entry(*id).or_default().push(eix);
            }
            // The entry keeps `created`; the expansion below borrows
            // it back for its duration.
            self.cascade.push(Entry {
                rule_ix,
                rank: rule.rank,
                refs: refs.to_vec(),
                created: Vec::new(),
                created_edges,
            });
            self.applied.insert(rule_ix, &refs, carried);
            // Materialize the provenance edge: the applied match points
            // at its cascade entry (so `retract_for` needs no scan).
            if let Some(mix) = self.pulled_match(rule_ix, &refs, carried) {
                self.matches[mix as usize].entry = Some(eix);
            }
            self.expand_at(g, resolver, &created);
            self.cascade[eix as usize].created = created;
            return Some(true);
        }
    }

    /// Saturation verdict (first-generation parity): applied or recognized-as-
    /// duplicate matches are "dead" candidates ⇒ Duplication; only an
    /// empty history converges.
    fn verdict(&self) -> Termination {
        if self.saw_contradiction {
            Termination::Contradiction
        } else if self.applied.is_empty() {
            Termination::Convergence
        } else {
            Termination::Duplication
        }
    }

    /// Cascade until saturation.
    pub fn run(
        &mut self,
        g: &mut Graph,
        resolver: &dyn ValueResolver,
        max_steps: usize,
    ) -> Termination {
        self.seed(g, resolver);
        for _ in 0..max_steps {
            if self.step(g, resolver).is_none() {
                return self.verdict();
            }
        }
        Termination::StepLimit
    }
}

// ══ Tests (stage 4, minimal engine) ═════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::ValueStore;
    use crate::graph::*;
    use crate::rules::format::RuleFile;

    /// Vorwaerts- und Rueckwaertsplan der Familienregel, ueber den
    /// einen Ladeweg des Crates.
    fn father_rule(g: &mut Graph) -> Vec<crate::plan::DirectedRule> {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "engine_tests",
            "rules": [{
                "name": "Father_2_Male",
                "rank": 850,
                "left": {
                    "anchor": "fam",
                    "nodes": [
                        {"name": "fam", "type": "Family"},
                        {"name": "father", "type": "Father"},
                        {"name": "member", "type": "Member"},
                        {"name": "first", "type": "firstName"}
                    ],
                    "links": [["fam", "father"], ["father", "member"],
                              ["member", "first"]]
                },
                "right": {
                    "anchor": "male",
                    "nodes": [
                        {"name": "male", "type": "Male"},
                        {"name": "name", "type": "name"}
                    ],
                    "links": [["male", "name"]]
                },
                "corrs": [
                    {"type": "PersonCorr", "left": "member", "right": "male",
                     "role": "establishes",
                     "bindings": [{"left": "first", "right": "name"}]}
                ]
            }]
        }))
        .expect("Regeldatei parst");
        crate::rules::load_file(&file, g).expect("Regeldatei laedt")
    }

    /// Die Familienregel mit anderem Namen, Rang und Corr-Typ.
    fn variant_rule(name: &str, rank: u64, corr: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "rank": rank,
            "left": {
                "anchor": "fam",
                "nodes": [
                    {"name": "fam", "type": "Family"},
                    {"name": "father", "type": "Father"},
                    {"name": "member", "type": "Member"},
                    {"name": "first", "type": "firstName"}
                ],
                "links": [["fam", "father"], ["father", "member"],
                          ["member", "first"]]
            },
            "right": {
                "anchor": "male",
                "nodes": [
                    {"name": "male", "type": "Male"},
                    {"name": "name", "type": "name"}
                ],
                "links": [["male", "name"]]
            },
            "corrs": [
                {"type": corr, "left": "member", "right": "male",
                 "role": "establishes",
                 "bindings": [{"left": "first", "right": "name"}]}
            ]
        })
    }

    fn seed(n: usize) -> (Graph, ValueStore) {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        for i in 0..n {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            let r = g.add_baseline(&format!("f{i}/father"), "Father");
            let m = g.add_baseline(&format!("f{i}/father/m"), "Member");
            let leaf = g.add_baseline(&format!("f{i}/father/m/fn"), "firstName");
            g.connect(f, r, Status::Solid);
            g.connect(r, m, Status::Solid);
            g.connect(m, leaf, Status::Solid);
            vs.insert(leaf, format!("John{i}"));
        }
        (g, vs)
    }

    #[test]
    fn cascade_saturates_deterministically() {
        let (mut g, vs) = seed(5);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        let t = e.run(&mut g, &vs, 10_000);
        assert_eq!(t, Termination::Duplication);
        assert_eq!(e.cascade.len(), 5);
        // Determinism: the second run is identical.
        let (mut g2, vs2) = seed(5);
        let mut e2 = Engine::new(&rules);
        let t2 = e2.run(&mut g2, &vs2, 10_000);
        assert_eq!(t2, Termination::Duplication);
        // Bidirectional (rc8-#2 ported): fwd+bwd together saturate
        // without ping-pong — the recognition stops the opposite
        // direction.
        let (mut g3, vs3) = seed(5);
        let mut l3 = father_rule(&mut g3);
        let f3 = l3.remove(0);
        let b3 = l3.remove(0);
        let both = vec![f3, b3];
        let mut e3 = Engine::new(&both);
        let t3 = e3.run(&mut g3, &vs3, 10_000);
        assert_eq!(t3, Termination::Duplication);
        assert_eq!(
            e3.cascade.len(),
            5,
            "no ping-pong: only 5 forward applications"
        );
        let a: Vec<_> = e
            .cascade
            .iter()
            .map(|x| (x.rule_ix, x.refs.clone()))
            .collect();
        let b: Vec<_> = e2
            .cascade
            .iter()
            .map(|x| (x.rule_ix, x.refs.clone()))
            .collect();
        assert_eq!(a, b, "μ order ⇒ identical cascade");
    }

    #[test]
    fn admitted_source_delta_holds_forward_direction_for_the_wave() {
        let (mut g, vs) = seed(2);
        let rules = father_rule(&mut g);
        let mut e = Engine::new(&rules);

        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert!(
            e.matches
                .iter()
                .all(|m| e.rules[m.rule_ix].direction == RuleDirection::Forward),
            "the initial source delta must not even record inverse candidates"
        );
        assert_eq!(e.run_remaining(&mut g, &vs), Termination::Duplication);
        assert_eq!(e.cascade.len(), 2);
        assert!(e
            .cascade
            .iter()
            .all(|entry| e.rules[entry.rule_ix].direction == RuleDirection::Forward));
    }

    #[test]
    fn explicit_delta_domain_disambiguates_equal_anchor_type_names() {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "same_metamodel",
            "rules": [{
                "name": "A_2_A", "rank": 10,
                "left": {"anchor": "l", "nodes": [{"name": "l", "type": "A"}]},
                "right": {"anchor": "r", "nodes": [{"name": "r", "type": "A"}]},
                "corrs": [{"type": "AA", "left": "l", "right": "r",
                           "role": "establishes"}]
            }]
        }))
        .expect("rule file parses");
        let mut g = Graph::new();
        g.add_baseline("source/a", "A");
        let rules = crate::rules::load_file(&file, &mut g).expect("rule lowers");
        assert_eq!(rules[0].input_types, rules[1].input_types);
        let mut e = Engine::new(&rules);

        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &ValueStore::default());
        assert!(e
            .matches
            .iter()
            .all(|m| e.rules[m.rule_ix].direction == RuleDirection::Forward));
    }

    #[test]
    fn forward_context_created_mid_wave_enables_forward_follow_up() {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "directional_follow_up",
            "rules": [
                {
                    "name": "Root", "rank": 100,
                    "left": {"anchor": "a", "nodes": [
                        {"name": "a", "type": "A"}
                    ]},
                    "right": {"anchor": "b", "nodes": [
                        {"name": "b", "type": "B"}
                    ]},
                    "corrs": [{"type": "AB", "left": "a", "right": "b",
                               "role": "establishes"}]
                },
                {
                    "name": "Child", "rank": 90,
                    "left": {"anchor": "a", "nodes": [
                        {"name": "a", "type": "A"},
                        {"name": "x", "type": "X"}
                    ], "links": [["a", "x"]]},
                    "right": {"anchor": "b", "nodes": [
                        {"name": "b", "type": "B"},
                        {"name": "y", "type": "Y"}
                    ], "links": [["b", "y"]]},
                    "corrs": [
                        {"type": "AB", "left": "a", "right": "b",
                         "role": "references"},
                        {"type": "XY", "left": "x", "right": "y",
                         "role": "establishes"}
                    ]
                }
            ]
        }))
        .expect("rule file parses");
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let x = g.add_baseline("a/x", "X");
        g.connect(a, x, Status::Solid);
        let rules = crate::rules::load_file(&file, &mut g).expect("rules lower");
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);

        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.run_remaining(&mut g, &vs), Termination::Duplication);

        let applied: Vec<&str> = e
            .cascade
            .iter()
            .map(|entry| e.rules[entry.rule_ix].name.as_str())
            .collect();
        assert_eq!(applied, vec!["Root→", "Child→"]);
        assert_eq!(
            g.nodes_of_type(g.types.lookup("Y").expect("Y type"))
                .filter(|n| n.status.is_matchable())
                .count(),
            1,
            "the second forward rule is enabled only by Root's new AB context"
        );
    }

    #[test]
    fn attribute_change_uses_admitted_domain_not_leaf_type() {
        let (mut g, mut vs) = seed(1);
        let rules = father_rule(&mut g);
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        assert_eq!(e.run(&mut g, &vs, 100), Termination::Duplication);
        e.consolidate(&mut g);

        let source_leaf = g
            .iter_nodes()
            .find(|n| g.types.name(n.typ) == "firstName")
            .expect("source name")
            .id;
        vs.insert(source_leaf, "Renamed");
        e.admit_delta(&[DeltaDomain::Source]);
        e.element_changed(&mut g, &source_leaf);
        e.seed(&g, &vs);
        while e.step(&mut g, &vs).is_some() {}
        e.consolidate(&mut g);

        let target_name = g
            .nodes_of_type(g.types.lookup("name").expect("target name type"))
            .find(|n| n.status.is_matchable())
            .expect("live target name")
            .id;
        assert_eq!(
            g.resolve_value(&target_name, &vs).as_deref(),
            Some("Renamed")
        );
    }

    #[test]
    fn attribute_leaf_add_enters_through_the_add_door() {
        let mut g = Graph::new();
        let family = g.add_baseline("f", "Family");
        let father = g.add_baseline("f/father", "Father");
        let member = g.add_baseline("f/father/member", "Member");
        g.connect(family, father, Status::Solid);
        g.connect(father, member, Status::Solid);
        let rules = father_rule(&mut g);
        let mut vs = ValueStore::default();
        let mut e = Engine::new(&rules);

        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert!(e.step(&mut g, &vs).is_none(), "ohne Blatt kein Match");

        let leaf = g.add_baseline("f/father/member/first", "firstName");
        g.connect(member, leaf, Status::Solid);
        vs.insert(leaf, "John");
        e.admit_delta(&[DeltaDomain::Source]);
        e.elements_added(&g, &vs, &[leaf]);
        while e.step(&mut g, &vs).is_some() {}

        let target_name = g
            .nodes_of_type(g.types.lookup("name").expect("target name type"))
            .find(|n| n.status.is_matchable())
            .expect("attribute add created the target leaf")
            .id;
        assert_eq!(g.resolve_value(&target_name, &vs).as_deref(), Some("John"));
        assert!(e
            .cascade
            .iter()
            .all(|entry| e.rules[entry.rule_ix].direction == RuleDirection::Forward));
    }

    #[test]
    fn attribute_leaf_delete_enters_through_the_delete_door() {
        let (mut g, vs) = seed(1);
        let rules = father_rule(&mut g);
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        assert_eq!(e.run(&mut g, &vs, 100), Termination::Duplication);
        e.consolidate(&mut g);

        let source_name = g
            .nodes_of_type(g.types.lookup("firstName").expect("source name type"))
            .find(|n| n.status.is_matchable())
            .expect("source name")
            .id;
        let target_name = g
            .nodes_of_type(g.types.lookup("name").expect("target name type"))
            .find(|n| n.status.is_matchable())
            .expect("target name")
            .id;
        g.set_node_status(&source_name, Status::Tombstone);
        e.admit_delta(&[DeltaDomain::Source]);
        e.element_deleted(&mut g, &source_name);
        e.consolidate(&mut g);

        assert_eq!(
            g.node(&target_name).map(|n| n.status),
            Some(Status::Tombstone),
            "the attribute correspondence carries the delete to the target leaf"
        );
        assert_eq!(
            g.nodes_of_type(g.types.lookup("Member").expect("source carrier type"))
                .filter(|n| n.status.is_matchable())
                .count(),
            1,
            "the externally supplied source carrier survives"
        );
        assert_eq!(
            g.nodes_of_type(g.types.lookup("Male").expect("target carrier type"))
                .filter(|n| n.status.is_matchable())
                .count(),
            0,
            "firstName is a match prerequisite, so its deletion invalidates the complete \
             Father_2_Male realisation"
        );
    }

    fn follow_up_rules(g: &mut Graph) -> Vec<DirectedRule> {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "link_delta",
            "rules": [
                {
                    "name": "Root", "rank": 100,
                    "left": {"anchor": "a", "nodes": [{"name": "a", "type": "A"}]},
                    "right": {"anchor": "b", "nodes": [{"name": "b", "type": "B"}]},
                    "corrs": [{"type": "AB", "left": "a", "right": "b",
                               "role": "establishes"}]
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
        }))
        .expect("rule file parses");
        crate::rules::load_file(&file, g).expect("rules lower")
    }

    #[test]
    fn link_add_enters_through_the_add_door() {
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let x = g.add_baseline("x", "X");
        let rules = follow_up_rules(&mut g);
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        assert_eq!(e.run(&mut g, &vs, 100), Termination::Duplication);
        assert_eq!(e.cascade.len(), 1, "only Root applies before the link add");

        g.connect(a, x, Status::Solid);
        e.admit_delta(&[DeltaDomain::Source]);
        e.link_added(&g, &vs, &a, &x);
        while e.step(&mut g, &vs).is_some() {}

        assert_eq!(
            e.cascade
                .iter()
                .map(|entry| e.rules[entry.rule_ix].name.as_str())
                .collect::<Vec<_>>(),
            vec!["Root→", "Child→"]
        );
    }

    #[test]
    fn link_delete_enters_through_the_delete_door() {
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let x = g.add_baseline("x", "X");
        let link = g.connect(a, x, Status::Solid).expect("link");
        let rules = follow_up_rules(&mut g);
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        assert_eq!(e.run(&mut g, &vs, 100), Termination::Duplication);
        let y = g
            .nodes_of_type(g.types.lookup("Y").expect("Y type"))
            .find(|n| n.status.is_matchable())
            .expect("Child product")
            .id;

        g.set_connection_status(&link, Status::Tombstone);
        e.admit_delta(&[DeltaDomain::Source]);
        e.link_removed(&mut g, &a, &x);
        e.seed(&g, &vs);
        while e.step(&mut g, &vs).is_some() {}
        e.consolidate(&mut g);

        assert_eq!(g.node(&y).map(|n| n.status), Some(Status::Tombstone));
        for id in [a, x] {
            assert!(g.node(&id).is_some_and(|n| n.status.is_matchable()));
        }
    }

    #[test]
    fn delete_kills_eagerly_via_by_element() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        assert_eq!(e.todo.len(), 2);
        // Tombstone f0's Member → its match dies IMMEDIATELY, locally.
        let member = g
            .iter_nodes()
            .find(|n| g.types.name(n.typ) == "Member")
            .unwrap()
            .id;
        g.set_node_status(&member, Status::Tombstone);
        e.element_removed(&member);
        assert_eq!(e.todo.len(), 1, "eager, no re-enum, no lazy");
        let t = e.run_remaining(&mut g, &vs);
        assert_eq!(
            t,
            Termination::Duplication,
            "one application = dead candidates"
        );
        assert_eq!(e.cascade.len(), 1, "only f1 gets translated");
    }

    /// A removed connection kills the matches that rest on it, and
    /// only those. The door `element_removed` does not get the graph;
    /// what it cannot settle itself is settled by the next door that
    /// does, here the step.
    #[test]
    fn a_removed_connection_kills_the_matches_resting_on_it() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        assert_eq!(e.todo.len(), 2);
        let member = g
            .iter_nodes()
            .find(|n| g.types.name(n.typ) == "Member")
            .unwrap()
            .id;
        let father = g
            .parts(&member)
            .find(|p| !p.outgoing)
            .expect("the member hangs on a father")
            .other;
        let link = g.connection_id(&father, &member).expect("father → member");
        // The connection the other way round is no element of the
        // graph and no link of the pattern: nothing rests on it.
        e.element_removed(&crate::graph::preview_connection_id(&member, &father));
        g.set_connection_status(&link, Status::Tombstone);
        e.element_removed(&link);
        let _ = e.run_remaining(&mut g, &vs);
        assert_eq!(e.cascade.len(), 1, "only the other family gets translated");
    }

    /// Two rules: `Root` relates an `A` to a `B`, `Child` relates an
    /// `X` under the `A` to a `Y` under the `B` and reads the
    /// correspondence of `Root` as context. The pattern of `Child`
    /// names the link from that correspondence to the `A`; the graph
    /// holds the connection the other way round.
    fn context_link_rules() -> RuleFile {
        serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "context_link",
            "rules": [
                {
                    "name": "Root", "rank": 100,
                    "left": {"anchor": "a", "nodes": [
                        {"name": "a", "type": "A"}
                    ]},
                    "right": {"anchor": "b", "nodes": [
                        {"name": "b", "type": "B"}
                    ]},
                    "corrs": [{"type": "AB", "left": "a", "right": "b",
                               "role": "establishes"}]
                },
                {
                    "name": "Child", "rank": 90,
                    "left": {"anchor": "a", "nodes": [
                        {"name": "a", "type": "A"},
                        {"name": "x", "type": "X"}
                    ], "links": [["a", "x"]]},
                    "right": {"anchor": "b", "nodes": [
                        {"name": "b", "type": "B"},
                        {"name": "y", "type": "Y"}
                    ], "links": [["b", "y"]]},
                    "corrs": [
                        {"type": "AB", "left": "a", "right": "b",
                         "role": "references"},
                        {"type": "XY", "left": "x", "right": "y",
                         "role": "establishes"}
                    ]
                }
            ]
        }))
        .expect("rule file parses")
    }

    /// A match is filed under the connection of each of its links in
    /// the direction the pattern names, also where the graph holds the
    /// connection the other way round (a link to a context
    /// correspondence is direction-free). Removing an element with that
    /// id, which the graph does not know, kills the match.
    #[test]
    fn a_match_rests_on_the_named_direction_of_a_context_link() {
        let file = context_link_rules();
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let x = g.add_baseline("a/x", "X");
        g.connect(a, x, Status::Solid);
        let rules = crate::rules::load_file(&file, &mut g).expect("rules lower");
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        // Root applies and creates the correspondence; the match of
        // Child is found from it and waits in the todo.
        assert_eq!(e.step(&mut g, &vs), Some(true));
        assert_eq!(e.todo.len(), 1, "the candidate of Child");
        let corr = g.iter_nodes().find(|n| n.is_corr).expect("AB").id;
        // The graph holds a → corr. The pattern of Child names the
        // link from the correspondence to its anchor.
        assert!(g.connection_id(&a, &corr).is_some());
        let named = crate::graph::preview_connection_id(&corr, &a);
        assert!(g.connection(&named).is_none() && g.node(&named).is_none());
        e.element_removed(&named);
        assert_eq!(e.step(&mut g, &vs), None, "the candidate of Child is dead");
        assert_eq!(e.cascade.len(), 1);
    }

    /// An id the graph does not know is answered from the index of
    /// unheld links. The index is built at the first such question and
    /// kept up by `record`; for another graph it is built anew. An id
    /// noted twice kills once.
    #[test]
    fn an_unknown_id_is_answered_from_the_unheld_links() {
        let file = context_link_rules();
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let mut xs = Vec::new();
        for i in 0..3 {
            let x = g.add_baseline(&format!("a/x{i}"), "X");
            g.connect(a, x, Status::Solid);
            xs.push(x);
        }
        let rules = crate::rules::load_file(&file, &mut g).expect("rules lower");
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.step(&mut g, &vs), Some(true), "Root");
        assert_eq!(e.todo.len(), 3, "the candidates of Child");
        let corr = g.iter_nodes().find(|n| n.is_corr).expect("AB").id;
        assert!(e.links.unheld.is_none(), "no question yet, no index");
        // An id nothing rests on: the question builds the index and
        // kills nothing.
        let nothing = crate::graph::preview_connection_id(&xs[0], &xs[1]);
        e.element_removed(&nothing);
        assert_eq!(e.step(&mut g, &vs), Some(true), "one Child");
        assert_eq!(
            e.links.unheld.as_ref().map(|u| u.graph),
            Some(g.uid.get()),
            "an index for this graph"
        );
        assert_eq!(e.todo.len(), 2);
        // A match recorded from here on is filed in the index.
        let x = g.add_baseline("a/x3", "X");
        g.connect(a, x, Status::Solid);
        e.elements_added(&g, &vs, &[x]);
        assert_eq!(e.todo.len(), 3);
        let named = crate::graph::preview_connection_id(&corr, &a);
        let filed = |e: &Engine<'_>| {
            e.links
                .unheld
                .as_ref()
                .and_then(|u| u.by_id.get(&named))
                .map_or(0, |ixs| ixs.as_slice().len())
        };
        assert_eq!(filed(&e), 4, "every match of Child rests on the named link");
        // The host goes on with a copy: the index is about the other
        // graph and is dropped, the next question builds one for the
        // copy.
        let mut copy = g.clone();
        let x = copy.add_baseline("a/x4", "X");
        copy.connect(a, x, Status::Solid);
        e.elements_added(&copy, &vs, &[x]);
        assert!(
            e.links.unheld.is_none(),
            "the index was about the other graph"
        );
        assert_eq!(e.todo.len(), 4);
        e.element_removed(&named);
        e.element_removed(&named);
        assert_eq!(
            e.step(&mut copy, &vs),
            None,
            "the candidates of Child are dead"
        );
        assert_eq!(
            e.links.unheld.as_ref().map(|u| u.graph),
            Some(copy.uid.get()),
            "an index for the copy"
        );
        assert_eq!(filed(&e), 5);
        assert_eq!(e.cascade.len(), 2);
    }

    /// A graph the host has replaced by its materialized copy: the
    /// copy leaves the tombstones out, and a later retraction asks for
    /// them by id.
    #[test]
    fn an_id_the_materialized_copy_left_out_is_answered() {
        let file = context_link_rules();
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let mut xs = Vec::new();
        for i in 0..3 {
            let x = g.add_baseline(&format!("a/x{i}"), "X");
            g.connect(a, x, Status::Solid);
            xs.push(x);
        }
        let rules = crate::rules::load_file(&file, &mut g).expect("rules lower");
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        while e.step(&mut g, &vs).is_some() {}
        assert_eq!(e.cascade.len(), 4, "Root and three Child");
        // x0 goes: what its Child created is retracted and becomes a
        // tombstone.
        g.set_node_status(&xs[0], Status::Tombstone);
        e.element_deleted(&mut g, &xs[0]);
        e.consolidate(&mut g);
        let gone: Vec<GhostId> = e.cascade[1..]
            .iter()
            .flat_map(|c| c.created.iter().copied())
            .filter(|id| g.node(id).is_some_and(|n| n.status == Status::Tombstone))
            .collect();
        assert!(!gone.is_empty(), "the products of one Child are tombstones");
        let mut copy = g.materialize();
        assert!(gone.iter().all(|id| copy.node(id).is_none()));
        // `a` goes. The retraction comes by the dead match of x0 again
        // and asks for its products, which the copy does not hold.
        copy.set_node_status(&a, Status::Tombstone);
        e.element_deleted(&mut copy, &a);
        e.consolidate(&mut copy);
        assert_eq!(
            e.links.unheld.as_ref().map(|u| u.graph),
            Some(copy.uid.get()),
            "an index for the copy"
        );
    }

    /// The graph of the tests above: `a` with `n` children of type `X`,
    /// the rules of [`context_link_rules`] loaded into it.
    fn context_link_graph(n: usize) -> (Graph, GhostId, Vec<GhostId>, Vec<DirectedRule>) {
        let file = context_link_rules();
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let mut xs = Vec::new();
        for i in 0..n {
            let x = g.add_baseline(&format!("a/x{i}"), "X");
            g.connect(a, x, Status::Solid);
            xs.push(x);
        }
        let rules = crate::rules::load_file(&file, &mut g).expect("rules lower");
        (g, a, xs, rules)
    }

    /// How many matches the index of unheld links holds under `id`.
    fn in_unheld(e: &Engine<'_>, id: &GhostId) -> usize {
        e.links
            .unheld
            .as_ref()
            .and_then(|u| u.by_id.get(id))
            .map_or(0, |ixs| ixs.as_slice().len())
    }

    /// The index is about the graph, not about its type table. `types`
    /// is a public field: a host that moves it into another graph
    /// carries the number of the table along. The index built for the
    /// first graph must not answer for the second.
    #[test]
    fn the_index_is_about_the_graph_not_its_type_table() {
        let (mut g, a, xs, rules) = context_link_graph(2);
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.step(&mut g, &vs), Some(true), "Root");
        assert_eq!(e.todo.len(), 2, "the candidates of Child");
        // The first question builds the index for g. g holds a → x0, so
        // that link is not in it.
        let nothing = crate::graph::preview_connection_id(&xs[0], &xs[1]);
        assert!(e.filed_under(&g, &nothing).as_slice().is_empty());
        let held = g.connection_id(&a, &xs[0]).expect("a → x0");
        assert_eq!(in_unheld(&e, &held), 0);
        // Another graph with the type table of g: the same baseline
        // nodes, without a → x0.
        let mut h = Graph::new();
        h.types = std::mem::take(&mut g.types);
        assert_eq!(h.add_baseline("a", "A"), a);
        assert_eq!(h.add_baseline("a/x0", "X"), xs[0]);
        assert_eq!(h.add_baseline("a/x1", "X"), xs[1]);
        h.connect(a, xs[1], Status::Solid);
        assert!(h.connection(&held).is_none() && h.node(&held).is_none());
        // As in 2.0.2: the match of Child over x0 rests on a → x0.
        let resting = e.filed_under(&h, &held);
        assert_eq!(resting.as_slice().len(), 1, "the match of Child over x0");
        assert_eq!(
            e.links.unheld.as_ref().map(|u| u.graph),
            Some(h.uid.get()),
            "an index for h"
        );
    }

    /// A match recorded after the index was built is filed in it, and a
    /// question about the id is answered from it.
    #[test]
    fn a_match_recorded_after_the_index_is_answered_from_it() {
        let (mut g, a, xs, rules) = context_link_graph(2);
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.step(&mut g, &vs), Some(true), "Root");
        let corr = g.iter_nodes().find(|n| n.is_corr).expect("AB").id;
        let named = crate::graph::preview_connection_id(&corr, &a);
        let nothing = crate::graph::preview_connection_id(&xs[0], &xs[1]);
        assert!(e.filed_under(&g, &nothing).as_slice().is_empty());
        assert_eq!(in_unheld(&e, &named), 2);
        let x = g.add_baseline("a/x2", "X");
        g.connect(a, x, Status::Solid);
        e.elements_added(&g, &vs, &[x]);
        assert_eq!(e.todo.len(), 3, "the candidates of Child");
        assert_eq!(in_unheld(&e, &named), 3, "the new match is filed by record");
        let resting = e.filed_under(&g, &named);
        assert_eq!(resting.as_slice().len(), 3, "every match of Child");
        e.element_removed(&named);
        assert_eq!(
            e.step(&mut g, &vs),
            None,
            "the candidates of Child are dead"
        );
        assert_eq!(e.cascade.len(), 1, "Root only");
    }

    /// The connection of a link filed in the index comes later, in the
    /// direction of the link. The id is known now: the matches are
    /// found from its ends, and the answer is the same.
    #[test]
    fn a_connection_that_comes_later_gives_the_same_answer() {
        let (mut g, a, xs, rules) = context_link_graph(2);
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.step(&mut g, &vs), Some(true), "Root");
        let corr = g.iter_nodes().find(|n| n.is_corr).expect("AB").id;
        let named = crate::graph::preview_connection_id(&corr, &a);
        let nothing = crate::graph::preview_connection_id(&xs[0], &xs[1]);
        assert!(e.filed_under(&g, &nothing).as_slice().is_empty());
        assert_eq!(in_unheld(&e, &named), 2);
        g.connect(corr, a, Status::Solid);
        assert!(g.connection(&named).is_some(), "corr → a is held now");
        let resting = e.filed_under(&g, &named);
        assert_eq!(resting.as_slice().len(), 2, "both matches of Child");
        e.element_removed(&named);
        assert_eq!(
            e.step(&mut g, &vs),
            None,
            "the candidates of Child are dead"
        );
        assert_eq!(e.cascade.len(), 1, "Root only");
    }

    /// The materialized copy leaves out a connection that was a
    /// tombstone. The match resting on it is found from the index for
    /// the copy and dies, the others are applied.
    #[test]
    fn a_connection_the_copy_left_out_kills_the_match_resting_on_it() {
        let (mut g, a, xs, rules) = context_link_graph(3);
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.step(&mut g, &vs), Some(true), "Root");
        assert_eq!(e.todo.len(), 3, "the candidates of Child");
        let link = g.connection_id(&a, &xs[0]).expect("a → x0");
        g.set_connection_status(&link, Status::Tombstone);
        let mut copy = g.materialize();
        assert!(copy.connection(&link).is_none() && copy.node(&link).is_none());
        e.element_removed(&link);
        let _ = e.run_remaining(&mut copy, &vs);
        assert_eq!(
            e.links.unheld.as_ref().map(|u| u.graph),
            Some(copy.uid.get()),
            "an index for the copy"
        );
        assert_eq!(e.cascade.len(), 3, "Root and the Child of x1 and x2");
        let corrs_at = |x: &GhostId| {
            copy.parts(x)
                .filter(|p| copy.node(&p.other).is_some_and(|n| n.is_corr))
                .count()
        };
        assert_eq!(corrs_at(&xs[0]), 0, "the Child of x0 is dead");
        assert_eq!(corrs_at(&xs[1]), 1);
        assert_eq!(corrs_at(&xs[2]), 1);
    }

    /// A pattern that has a link twice (a host can push one into a
    /// loaded rule): the match rests on the connection twice, found
    /// from its ends as from the index.
    #[test]
    fn a_link_twice_in_a_pattern_files_the_match_twice() {
        let (mut g, a, xs, mut rules) = context_link_graph(2);
        for r in &mut rules {
            let directed: Vec<crate::engine::matcher::Link> = r
                .pattern
                .links
                .iter()
                .filter(|l| l.kind == crate::engine::matcher::LinkKind::Directed)
                .copied()
                .collect();
            r.pattern.links.extend(directed);
        }
        let vs = ValueStore::default();
        let mut e = Engine::new(&rules);
        e.admit_delta(&[DeltaDomain::Source]);
        e.seed(&g, &vs);
        assert_eq!(e.step(&mut g, &vs), Some(true), "Root");
        assert_eq!(e.todo.len(), 2, "the candidates of Child");
        let link = g.connection_id(&a, &xs[0]).expect("a → x0");
        let from_ends = e.filed_under(&g, &link);
        assert_eq!(from_ends.as_slice().len(), 2, "one match, twice");
        assert_eq!(from_ends.as_slice()[0], from_ends.as_slice()[1]);
        g.set_connection_status(&link, Status::Tombstone);
        let copy = g.materialize();
        assert!(copy.connection(&link).is_none() && copy.node(&link).is_none());
        let from_index = e.filed_under(&copy, &link);
        assert_eq!(from_index.as_slice(), from_ends.as_slice());
    }

    impl<'r> Engine<'r> {
        /// Test helper: run without a fresh seed.
        fn run_remaining(&mut self, g: &mut Graph, resolver: &dyn ValueResolver) -> Termination {
            loop {
                if self.step(g, resolver).is_none() {
                    return self.verdict();
                }
            }
        }
    }

    #[test]
    fn retraction_walk_and_consolidation() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        let t = e.run(&mut g, &vs, 100);
        assert_eq!(t, Termination::Duplication);
        assert_eq!(e.cascade.len(), 2);
        let created_f0 = e.cascade[0].created.clone();
        // f0's Member drops out → retraction walk + consolidation.
        let member = e.cascade[0].refs[2];
        g.set_node_status(&member, Status::Tombstone);
        e.element_deleted(&mut g, &member);
        e.consolidate(&mut g);
        for c in &created_f0 {
            assert_eq!(
                g.node(c).unwrap().status,
                Status::Tombstone,
                "f0's product chain (corr, Male, name) is tombstoned"
            );
        }
        // f1's products live.
        for c in &e.cascade[1].created {
            assert_eq!(g.node(c).unwrap().status, Status::Ghost);
        }
    }

    #[test]
    fn resurrection_on_a_new_derivation() {
        let (mut g, vs) = seed(1);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd.clone()];
        let mut e = Engine::new(&rules);
        let _ = e.run(&mut g, &vs, 100);
        let created = e.cascade[0].created.clone();
        let member = e.cascade[0].refs[2];
        // Retraction WITHOUT a real drop-out (Member lives) — then a
        // reapplication reclaims the same identities BEFORE
        // consolidation: resurrection instead of tombstone.
        e.element_changed(&mut g, &member);
        for c in &created {
            assert_eq!(g.node(c).unwrap().status, Status::TentativeTombstone);
        }
        let refs = e.cascade[0].refs.clone();
        let _ = crate::plan::apply_creation(&mut g, &fwd, &refs);
        e.consolidate(&mut g);
        for c in &created {
            assert_eq!(
                g.node(c).unwrap().status,
                Status::Ghost,
                "M5.5: an identical derivation reclaims the identity"
            );
        }
    }

    #[test]
    fn fold_materializes_without_tombstones() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        let _ = e.run(&mut g, &vs, 100);
        let member = e.cascade[0].refs[2];
        g.set_node_status(&member, Status::Tombstone);
        e.element_deleted(&mut g, &member);
        e.consolidate(&mut g);
        let folded = g.materialize();
        assert!(folded.node(&member).is_none(), "the tombstone drops out");
        for c in &e.cascade[0].created {
            assert!(folded.node(c).is_none());
        }
        for c in &e.cascade[1].created {
            assert_eq!(folded.node(c).unwrap().status, Status::Solid, "Ghost→Solid");
        }
        // A derived leaf keeps its provenance — the value stays resolvable.
        let name_leaf = e.cascade[1].created[2];
        assert!(folded.resolve_value(&name_leaf, &vs).is_some());
    }

    #[test]
    fn v7_contradiction_on_tombstone_reuse() {
        let (mut g, vs) = seed(1);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        // Tombstone the product identity up front: derive the corr id.
        let m = crate::engine::matcher::find_matches(&g, &vs, &rules[0].pattern);
        let corr_id = crate::graph::preview_ghost_id(&m[0][2], "PersonCorr");
        let anchor = m[0][2];
        let _ = g.add_ghost(&anchor, "PersonCorr");
        g.set_node_status(&corr_id, Status::Tombstone);
        let mut e = Engine::new(&rules);
        let t = e.run(&mut g, &vs, 100);
        assert_eq!(t, Termination::Contradiction, "V7: tombstone substance");
        assert_eq!(e.cascade.len(), 0);
    }

    #[test]
    fn backtracking_bound_filters_strictly() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        // Without a bound: the highest-ranked (largest ref sequence) first.
        let first = e.step(&mut g, &vs);
        assert_eq!(first, Some(true));
        let applied_refs = e.cascade[0].refs.clone();
        // A fresh engine with the bound = the first choice ⇒ it's
        // skipped, the second one comes up.
        let (mut g2, vs2) = seed(2);
        let mut e2 = Engine::new(&rules);
        e2.seed(&g2, &vs2);
        let bound = SelectionBound {
            rank: rules[0].rank,
            refs: applied_refs.clone(),
        };
        let s = e2.step_with_limit(&mut g2, &vs2, Some(&bound));
        assert_eq!(s, Some(true));
        assert_ne!(e2.cascade[0].refs, applied_refs, "strictly below the bound");
    }

    #[test]
    #[ignore = "manual perf measurement: engine, F2P shape"]
    fn bench_v2_f2p_scaling_smoke() {
        for n in [1_000usize, 10_000, 50_000] {
            let (mut g, vs) = seed(n);
            let mut lowered = father_rule(&mut g);
            let fwd = lowered.remove(0);
            let bwd = lowered.remove(0);
            let rules = vec![fwd, bwd];
            let mut e = Engine::new(&rules);
            let t0 = std::time::Instant::now();
            let t = e.run(&mut g, &vs, 10_000_000);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(t, Termination::Duplication);
            assert_eq!(e.cascade.len(), n);
            eprintln!(
                "V2-SMOKE n={n:<7} steps={} ms={ms:>9.1} us_per_family={:.1}",
                e.cascade.len(),
                ms * 1000.0 / n as f64
            );
        }
    }

    #[test]
    #[ignore = "manual perf measurement: un-routed versus source-directed wave"]
    fn bench_v2_f2p_directional_wave() {
        for n in [1_000usize, 10_000, 50_000] {
            for (label, routed) in [("initial", false), ("source", true)] {
                let (mut g, vs) = seed(n);
                let mut lowered = father_rule(&mut g);
                let fwd = lowered.remove(0);
                let bwd = lowered.remove(0);
                let rules = vec![fwd, bwd];
                let mut e = Engine::new(&rules);
                if routed {
                    e.admit_delta(&[DeltaDomain::Source]);
                }

                let t0 = std::time::Instant::now();
                let t = e.run(&mut g, &vs, 10_000_000);
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(t, Termination::Duplication);
                assert_eq!(e.cascade.len(), n);

                let forward = e
                    .matches
                    .iter()
                    .filter(|m| e.rules[m.rule_ix].direction == RuleDirection::Forward)
                    .count();
                let backward = e
                    .matches
                    .iter()
                    .filter(|m| e.rules[m.rule_ix].direction == RuleDirection::Backward)
                    .count();
                eprintln!(
                    "V2-DIRECTION n={n:<7} route={label:<7} matches={}/{forward}/{backward} steps={} ms={ms:>9.1}",
                    e.matches.len(),
                    e.cascade.len()
                );
            }
        }
    }

    /// The runtime mask must INTERSECT with the rank ceiling, not
    /// replace it.
    ///
    /// Two rules over the same match space. With a ceiling at the
    /// high-ranked one and both rules active, only the low-ranked one
    /// may fire. Until 2026-08-10 the mask branch searched the whole
    /// queue and handed back the high-ranked candidate, so the ceiling
    /// was silently void whenever `active` was set — exactly the
    /// combination backtracking needs.
    #[test]
    fn mask_and_ceiling_intersect() {
        let (mut g, vs) = seed(1);
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "mask_and_ceiling",
            "rules": [
                variant_rule("Low", 10, "CorrLow"),
                variant_rule("High", 900, "CorrHigh"),
            ]
        }))
        .expect("Regeldatei parst");
        let lowered = crate::rules::load_file(&file, &mut g).expect("laedt");
        let rules: Vec<DirectedRule> = lowered.into_iter().step_by(2).collect();

        let mut e = Engine::new(&rules);
        e.active = Some(vec![true, true]);
        e.seed(&g, &vs);

        // Schranke beim hochrangigen Kandidaten: nur der niedrige darf
        // gewaehlt werden.
        let bound = {
            let high = e
                .todo
                .keys()
                .into_iter()
                .find(|k| k.3 == 1)
                .expect("der hochrangige steht in der Queue");
            SelectionBound {
                rank: (bound_rank(&high)),
                refs: (bound_refs(&high)),
            }
        };
        assert!(e.step_with_limit(&mut g, &vs, Some(&bound)).is_some());
        assert_eq!(
            e.cascade.len(),
            1,
            "genau eine Anwendung unterhalb der Schranke"
        );
        assert_eq!(
            e.rules[e.cascade[0].rule_ix].name, "Low\u{2192}",
            "die Schranke muss den hochrangigen ausschliessen, auch mit gesetzter Maske"
        );
    }

    fn bound_rank(k: &TodoKey) -> u64 {
        k.0 .0
    }

    fn bound_refs(k: &TodoKey) -> Bindings {
        k.2 .0.to_vec()
    }

    #[test]
    fn rank_order_decides() {
        // Two rules, same match space, different ranks — the higher-
        // ranked one applies first.
        let (mut g, vs) = seed(1);
        // Two rules over the same match space, different ranks and
        // different corr types so both actually create.
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "rank_order",
            "rules": [
                variant_rule("Low", 10, "CorrLow"),
                variant_rule("High", 900, "CorrHigh"),
            ]
        }))
        .expect("Regeldatei parst");
        let mut lowered = crate::rules::load_file(&file, &mut g).expect("laedt");
        let hf = lowered.remove(2);
        let lf = lowered.remove(0);
        let rules = vec![lf, hf];
        let mut e = Engine::new(&rules);
        let _ = e.run(&mut g, &vs, 100);
        assert_eq!(e.cascade[0].rank, 900, "the highest rank first");
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    /// Hosts keep rules and index in statics and run engines on several
    /// threads. Losing `Send` or `Sync` (a `Cell` in the index, say) is
    /// to fail here and not in the host's build.
    #[test]
    fn engine_types_are_send_and_sync() {
        fn ok<T: Send + Sync>() {}
        ok::<AnchorIndex>();
        ok::<Engine<'static>>();
        ok::<crate::graph::Graph>();
        ok::<crate::graph::ValueStore>();
        ok::<DirectedRule>();
    }

    /// The per-element list keeps the order of insertion in place and
    /// on the heap, and changes over after [`IXS_INLINE`] entries.
    #[test]
    fn ixs_keeps_the_order_of_insertion_across_the_switch() {
        for n in 0..=12u32 {
            let mut ixs = Ixs::default();
            let mut model = Vec::new();
            for i in 0..n {
                let value = i * 7 % 13;
                ixs.push(value);
                model.push(value);
            }
            assert_eq!(ixs.as_slice(), model.as_slice(), "{n} entries");
            assert_eq!(
                matches!(ixs, Ixs::Inline { .. }),
                n as usize <= IXS_INLINE,
                "{n} entries"
            );
        }
    }

    /// Sizes of the records the engine keeps per element and per match
    /// (64-bit targets). A change here is a change of the engine's
    /// memory per node and of what a growing map moves.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn sizes_of_the_hot_records() {
        use std::mem::size_of;
        let sizes = [
            ("Ixs", size_of::<Ixs>()),
            ("(GhostId, Ixs)", size_of::<(GhostId, Ixs)>()),
            ("TodoKey", size_of::<TodoKey>()),
            ("Stored", size_of::<Stored>()),
            ("Entry", size_of::<Entry>()),
        ];
        // `low_memory` keeps one index less in place and the long list
        // behind one pointer.
        let (ixs, filed) = if cfg!(feature = "low_memory") {
            (16, 48)
        } else {
            (32, 64)
        };
        assert_eq!(
            sizes,
            [
                ("Ixs", ixs),
                ("(GhostId, Ixs)", filed),
                ("TodoKey", 48),
                ("Stored", 40),
                ("Entry", 88),
            ]
        );
    }

    fn rules_of(kind: &str, g: &mut Graph) -> Vec<DirectedRule> {
        let file = serde_json::json!({
            "format": 3, "name": "contract",
            "rules": [{
                "name": "R", "rank": 1,
                "left": {"anchor": "a", "nodes": [
                    {"name": "a", "type": "A"},
                    {"name": "k", "type": "kind", "context": true,
                     "predicate": {"kind": "equals", "value": kind}}
                ], "links": [["a", "k"]]},
                "right": {"anchor": "b", "nodes": [{"name": "b", "type": "B"}]},
                "corrs": [{"type": "AB", "left": "a", "right": "b", "role": "establishes"}]
            }]
        });
        crate::rules::load(&file.to_string(), g).expect("rules load")
    }

    /// An index is taken for the rules whose patterns it was built
    /// from, wherever they stand, and not for other rules that stand
    /// where its rules stood.
    #[test]
    fn a_shared_index_is_matched_to_the_rules_by_their_patterns() {
        let mut g = Graph::new();
        let a = g.add_baseline("a", "A");
        let k = g.add_baseline("k", "kind");
        g.connect(a, k, Status::Solid);
        let mut vs = crate::graph::ValueStore::default();
        vs.insert(k, "two");

        let mut rules = rules_of("one", &mut g);
        let index_one = AnchorIndex::build(&rules);
        assert!(index_one.built_for(&rules) && index_one.fits(&rules));
        // An equal copy at another address.
        let copy = rules.clone();
        assert!(!index_one.built_for(&copy) && index_one.fits(&copy));
        assert!(!AnchorIndex::default().fits(&rules));
        // A value that differs in trailing NUL characters only.
        assert!(!index_one.fits(&rules_of("one\u{0}", &mut g)));

        // Other rules in the same buffer: same address, same length.
        let other = rules_of("two", &mut g);
        rules.clear();
        rules.extend(other);
        assert!(
            index_one.built_for(&rules),
            "the address alone does not tell them apart"
        );
        assert!(!index_one.fits(&rules));

        // The engine does not use the index of the other rules: the
        // rule for "two" applies.
        let mut e = Engine::with_anchor_index(&rules, &index_one);
        e.elements_added(&g, &vs, &[a, k]);
        while e.step(&mut g, &vs).is_some() {}
        assert_eq!(e.cascade.len(), 1);
    }
}

#[cfg(test)]
mod todo_key_tests {
    use super::{head, GhostId};
    use proptest::prelude::*;

    proptest! {
        /// The head orders like the sequence wherever two heads differ.
        #[test]
        fn head_is_consistent_with_the_sequence_order(
            a in prop::collection::vec(any::<[u8; 32]>(), 0..3),
            b in prop::collection::vec(any::<[u8; 32]>(), 0..3),
            share in 0usize..33,
        ) {
            let a: Vec<GhostId> = a.into_iter().map(GhostId::from_raw).collect();
            let mut b: Vec<[u8; 32]> = b;
            // Also sequences whose first ids share a prefix.
            if let (Some(x), Some(y)) = (a.first(), b.first_mut()) {
                y[..share.min(32)].copy_from_slice(&x.as_bytes()[..share.min(32)]);
            }
            let b: Vec<GhostId> = b.into_iter().map(GhostId::from_raw).collect();
            if head(&a) != head(&b) {
                prop_assert_eq!(head(&a).cmp(&head(&b)), a.cmp(&b));
            }
        }
    }
}
