//! The model: everything is a node, connections are anonymous.
//!
//! Sign-off Sandra 2026-07-14 (specs/2026-07-14-datenmodell-v2.md).
//!
//! ```text
//! Node        = { id, typ, status }            Ghost leaves additionally:
//!                                              { quelle: Ref, transform }
//! Connection  = { source → target, status }    anonymous, directed, attr-less
//!                                              [OPEN, reported to Sandra:
//!                                              multiplicity — without type/attrs
//!                                              parallel edges are indistinguishable;
//!                                              realized here for now as a set (at
//!                                              most 1 per direction)]
//! Map         :  id → participations           nodes, connections — and from
//!                                              stage 4 also match records
//! ```
//!
//! Principles (all Sandra's decisions):
//! - **Values only in the original.** The overlay stores structure and
//!   provenance; raw values come from a [`ValueResolver`] (host adapter
//!   or [`ValueStore`] for standalone runs).
//! - **No value in any hash.** Identity is structural + provenance;
//!   renames can never touch identity, by construction.
//! - **No connection types.** Roles are reified nodes; every node's
//!   participation list is sorted by (counterpart type, id) — the
//!   search lives in the list, not in a global view.

use std::collections::BTreeSet;

use crate::hash::FxHashMap;
use crate::ident::{GhostId, Status};
use crate::rules::transform::{Chain, ChainId, ChainTable};

/// Interned node type. The only type information in the model —
/// assigned by the [`TypeTable`], never strings in hot structures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub u32);

/// Type name ↔ [`TypeId`] interning (insert-only, deterministic in
/// registration order).
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    by_name: FxHashMap<String, TypeId>,
    names: Vec<String>,
}

impl TypeTable {
    pub fn intern(&mut self, name: &str) -> TypeId {
        if let Some(t) = self.by_name.get(name) {
            return *t;
        }
        let t = TypeId(self.names.len() as u32);
        self.names.push(name.to_string());
        self.by_name.insert(name.to_string(), t);
        t
    }

    pub fn lookup(&self, name: &str) -> Option<TypeId> {
        self.by_name.get(name).copied()
    }

    pub fn name(&self, t: TypeId) -> &str {
        &self.names[t.0 as usize]
    }
}

/// Spec §1: `Node = { id, typ, status }` — ghost leaves ADDITIONALLY
/// carry `{ quelle, transform }` (spec §1.1). No value, no attr map,
/// nothing else.
#[derive(Clone, Debug)]
pub struct Node {
    pub id: GhostId,
    pub typ: TypeId,
    pub status: Status,
    /// Only set on derived ghost leaves: value =
    /// transform(source value), materialized only at the boundary.
    pub derived: Option<DerivedLeaf>,
    /// Rule constant (§3b.4, DECIDED Sandra 2026-07-14): the value
    /// lives in the RULE (source material); the node carries only the
    /// reference into the graph's constant table — no value in any
    /// hash, identity is disambiguated via the generating rule
    /// (see the constant case in the identity derivation).
    pub konst: Option<KonstId>,
    /// Is this node a correspondence?
    ///
    /// A property of the node, not engine bookkeeping: it survives
    /// `materialize` with the node and is what deletion follows. When a
    /// correspondence falls, the elements it connects fall with it —
    /// the correspondence is the carrier, so an attestation of a
    /// translation whose result is gone is not a legal state at rest.
    pub is_corr: bool,
}

/// Interned rule constant (reference, analogous to [`TypeId`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KonstId(pub u32);

/// Constant interning: values from the rule set (source material),
/// insert-only, deterministic in registration order.
#[derive(Debug, Clone, Default)]
pub struct KonstTable {
    by_value: FxHashMap<String, KonstId>,
    values: Vec<String>,
}

impl KonstTable {
    pub fn intern(&mut self, value: &str) -> KonstId {
        if let Some(k) = self.by_value.get(value) {
            return *k;
        }
        let k = KonstId(self.values.len() as u32);
        self.values.push(value.to_string());
        self.by_value.insert(value.to_string(), k);
        k
    }

    pub fn value(&self, k: KonstId) -> &str {
        &self.values[k.0 as usize]
    }
}

/// Value provenance of a derived leaf as it appears in a rule's
/// CREATION PLAN: the chain itself, not an id.
///
/// Deliberately content-based: a plan is lowered against one graph and
/// afterward often runs against a DIFFERENT one (tests, roundtrips, a
/// second cascade). An interning id wouldn't resolve there. Rules are
/// few — carrying the chain along costs nothing; the graph interns it
/// when the leaf is created ([`LeafTransform`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanTransform {
    Chain(Chain),
}

impl PlanTransform {
    /// Canonical bytes for identity derivation. The chain enters in
    /// full (including arguments), no longer just a tag byte.
    fn ident_bytes(&self) -> Vec<u8> {
        match self {
            Self::Chain(c) => {
                let mut out = vec![0u8];
                out.extend_from_slice(&c.ident_bytes());
                out
            }
        }
    }
}

/// Value provenance of a derived leaf as it appears in the GRAPH: the
/// chain interned in [`Graph::chains`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafTransform {
    Chain(ChainId),
}

impl LeafTransform {
    /// Applies forward. `None` = chain not applicable (strict).
    fn apply(self, value: &str, chains: &ChainTable) -> Option<String> {
        match self {
            Self::Chain(c) => chains.chain(c).apply(value),
        }
    }
}

/// Spec §1.1: `{ quelle: Ref, transform }` of a ghost leaf. Field kept
/// as `quelle` (not translated): read cross-crate by
/// `seesaw-bench-f2p/tests/benchmarx_ledger.rs`, outside this crate's
/// `src/` and therefore out of scope for this pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedLeaf {
    pub quelle: GhostId,
    pub transform: LeafTransform,
}

/// Anonymous directed connection (spec §1). Realized as a set — at
/// most one per (source, target); the multiplicity question is an OPEN
/// spec point reported to Sandra (see module header).
#[derive(Clone, Debug)]
pub struct Connection {
    pub id: GhostId,
    pub source: GhostId,
    pub target: GhostId,
    pub status: Status,
}

/// One participation in a node's list: sorted by (counterpart type,
/// counterpart id, direction) — Sandra's "search at most across the
/// set of adjacencies", realized as an order WITHIN the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Participation {
    pub other_typ: TypeId,
    pub other: GhostId,
    /// `true`: this node is the source side (creation provenance
    /// points away from here).
    pub outgoing: bool,
    pub connection: GhostId,
}

/// Slot of one participant in the global map.
#[derive(Clone, Debug, Default)]
struct Slot {
    node: Option<Node>,
    connection: Option<Connection>,
    /// Participations, sorted (BTreeSet = deterministic order,
    /// O(log) access by counterpart type via range).
    parts: BTreeSet<Participation>,
}

/// Raw-value access — the ONLY place where values exist. Implemented
/// by the host adapter (EMF/JDT) or by the [`ValueStore`] for
/// standalone/bench runs. Derived leaves are resolved via
/// [`Graph::resolve_value`] (ref chain + transformations).
pub trait ValueResolver {
    fn baseline_value(&self, id: &GhostId) -> Option<String>;
}

/// Standalone original: values of the baseline leaves (e.g. from the
/// seed generator). Does NOT belong to the overlay.
#[derive(Debug, Clone, Default)]
pub struct ValueStore {
    values: FxHashMap<GhostId, String>,
}

impl ValueStore {
    pub fn insert(&mut self, id: GhostId, value: impl Into<String>) {
        self.values.insert(id, value.into());
    }
}

impl ValueResolver for ValueStore {
    fn baseline_value(&self, id: &GhostId) -> Option<String> {
        self.values.get(id).cloned()
    }
}

/// Identity derivation: purely structural plus provenance. Every kind
/// of node hashes under its own domain prefix.
///
/// ## Unambiguous encoding
///
/// Every field of VARIABLE length is preceded by its length as a
/// little-endian `u32`. Fixed-width fields (32-byte ids, the plan
/// index) are hashed raw, because the domain tag at the front already
/// fixes the structure.
///
/// Without the length prefixes the encoding is not injective, and not
/// as a hash-collision argument but before hashing even starts: with
/// two adjacent variable fields, `("ab", "c")` and `("a", "bc")`
/// produce the same input bytes. Two distinct nodes would then share
/// an identity and silently collapse into one. Found in the review of
/// 2026-08-10, fixed for every derivation at once, so the rule is one
/// sentence rather than a case distinction.
mod ident {
    use super::*;

    /// A variable-length field: length first, then content.
    fn var(h: &mut blake3::Hasher, bytes: &[u8]) {
        h.update(&(bytes.len() as u32).to_le_bytes());
        h.update(bytes);
    }

    pub fn baseline(external: &str) -> GhostId {
        let mut h = blake3::Hasher::new();
        h.update(b"V2B\0");
        var(&mut h, external.as_bytes());
        GhostId::from_raw(*h.finalize().as_bytes())
    }

    pub fn ghost(parent: &GhostId, typ_name: &str) -> GhostId {
        let mut h = blake3::Hasher::new();
        h.update(b"V2G\0");
        h.update(&parent.as_bytes());
        var(&mut h, typ_name.as_bytes());
        GhostId::from_raw(*h.finalize().as_bytes())
    }

    /// Derived leaf: value = transform(source), materialized only at
    /// the boundary. The chain enters via `ident_bytes`, which is
    /// itself length-prefixed per primitive.
    pub fn derived(
        parent: &GhostId,
        typ_name: &str,
        source: &GhostId,
        t: &PlanTransform,
    ) -> GhostId {
        let mut h = blake3::Hasher::new();
        h.update(b"V2D\0");
        h.update(&parent.as_bytes());
        var(&mut h, typ_name.as_bytes());
        h.update(&source.as_bytes());
        var(&mut h, &t.ident_bytes());
        GhostId::from_raw(*h.finalize().as_bytes())
    }

    pub fn connection(source: &GhostId, target: &GhostId) -> GhostId {
        let mut h = blake3::Hasher::new();
        h.update(b"V2C\0");
        h.update(&source.as_bytes());
        h.update(&target.as_bytes());
        GhostId::from_raw(*h.finalize().as_bytes())
    }

    /// Corr node: identity = anchor + type + MATCH digest (spec §1.4
    /// "same rule + same match ⇒ same id") — refs are structure, not
    /// values; multiple matches at the same anchor don't collide,
    /// reapplication is idempotent.
    ///
    /// The ref list carries its COUNT, not a byte length: without it a
    /// longer type name and one ref fewer would hash the same.
    pub fn corr(anchor: &GhostId, typ_name: &str, refs: &[GhostId]) -> GhostId {
        let mut h = blake3::Hasher::new();
        h.update(b"V2R\0");
        h.update(&anchor.as_bytes());
        var(&mut h, typ_name.as_bytes());
        h.update(&(refs.len() as u32).to_le_bytes());
        for r in refs {
            h.update(&r.as_bytes());
        }
        GhostId::from_raw(*h.finalize().as_bytes())
    }

    /// Rule-constant leaf: identity via the GENERATING RULE (name +
    /// plan position), NOT via the value — no value in any hash; two
    /// variant rules produce structurally distinct leaves at the same
    /// anchor.
    pub fn konst(parent: &GhostId, typ_name: &str, rule_name: &str, plan_ix: u32) -> GhostId {
        let mut h = blake3::Hasher::new();
        h.update(b"V2K\0");
        h.update(&parent.as_bytes());
        var(&mut h, typ_name.as_bytes());
        var(&mut h, rule_name.as_bytes());
        h.update(&plan_ix.to_le_bytes());
        GhostId::from_raw(*h.finalize().as_bytes())
    }
}

/// Pure id preview (spec §1.4) — identical to the derivation in the
/// add_*constructors, without inserting. Basis of duplicate detection
/// BEFORE application (engine, spec §1.6).
/// Baseline id of an external name, without touching a graph.
pub fn preview_baseline_id(external: &str) -> GhostId {
    ident::baseline(external)
}

/// Connection id of two endpoints, without touching a graph.
pub fn preview_connection_id(source: &GhostId, target: &GhostId) -> GhostId {
    ident::connection(source, target)
}

pub fn preview_ghost_id(parent: &GhostId, typ_name: &str) -> GhostId {
    ident::ghost(parent, typ_name)
}

pub fn preview_derived_id(
    parent: &GhostId,
    typ_name: &str,
    source: &GhostId,
    transform: &PlanTransform,
) -> GhostId {
    ident::derived(parent, typ_name, source, transform)
}

pub fn preview_konst_id(
    parent: &GhostId,
    typ_name: &str,
    rule_name: &str,
    plan_ix: u32,
) -> GhostId {
    ident::konst(parent, typ_name, rule_name, plan_ix)
}

pub fn preview_corr_id(anchor: &GhostId, typ_name: &str, refs: &[GhostId]) -> GhostId {
    ident::corr(anchor, typ_name, refs)
}

impl Graph {
    /// Corr node (match-digest identity, spec §1.4).
    pub fn add_corr(&mut self, anchor: &GhostId, typ_name: &str, refs: &[GhostId]) -> GhostId {
        let id = ident::corr(anchor, typ_name, refs);
        let typ = self.types.intern(typ_name);
        self.insert_node(Node {
            id,
            typ,
            status: Status::Ghost,
            derived: None,
            konst: None,
            is_corr: true,
        });
        id
    }
}

/// The overlay: ONE map from participant id to slot, one type
/// table. Insert-only (deletion is tombstoning), status is filtered on
/// read.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    map: FxHashMap<GhostId, Slot>,
    pub types: TypeTable,
    /// Insertion order of the nodes (deterministic iteration for
    /// dumps/fingerprints).
    node_order: Vec<GhostId>,
    /// Type index for seed scans (realization, §3b.2): ids per type in
    /// insertion order, insert-only; status is filtered on read like
    /// everywhere else.
    by_type: FxHashMap<TypeId, Vec<GhostId>>,
    /// Rule constants (source-material table, §3b.4).
    pub konsts: KonstTable,
    /// Transformation chains of derived leaves,
    /// interned like types and constants. A directed rule's creation
    /// plan carries only the [`ChainId`] into here.
    pub chains: ChainTable,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Creation ─────────────────────────────────────────────────

    /// Baseline node: identity from the outside (host handle, seed
    /// name).
    pub fn add_baseline(&mut self, external: &str, typ_name: &str) -> GhostId {
        let id = ident::baseline(external);
        let typ = self.types.intern(typ_name);
        self.insert_node(Node {
            id,
            typ,
            status: Status::Solid,
            derived: None,
            konst: None,
            is_corr: false,
        });
        id
    }

    /// Ghost node (created, not a leaf): structural identity.
    /// M5 resurrection: if the id already exists as a
    /// TentativeTombstone, the status is reset.
    pub fn add_ghost(&mut self, parent: &GhostId, typ_name: &str) -> GhostId {
        let id = ident::ghost(parent, typ_name);
        let typ = self.types.intern(typ_name);
        self.insert_node(Node {
            id,
            typ,
            status: Status::Ghost,
            derived: None,
            konst: None,
            is_corr: false,
        });
        id
    }

    /// Converts a plan transformation into the graph representation:
    /// the chain moves into this graph's table, the leaf carries only
    /// the id from here on.
    pub fn intern_plan(&mut self, t: &PlanTransform) -> LeafTransform {
        match t {
            PlanTransform::Chain(c) => LeafTransform::Chain(self.chains.intern(&c.0)),
        }
    }

    /// Derived leaf: value = transform(source), provenance only.
    pub fn add_derived_leaf(
        &mut self,
        parent: &GhostId,
        typ_name: &str,
        source: GhostId,
        transform: &PlanTransform,
    ) -> GhostId {
        let id = ident::derived(parent, typ_name, &source, transform);
        let transform = self.intern_plan(transform);
        let typ = self.types.intern(typ_name);
        self.insert_node(Node {
            id,
            typ,
            status: Status::Ghost,
            derived: Some(DerivedLeaf {
                quelle: source,
                transform,
            }),
            konst: None,
            is_corr: false,
        });
        id
    }

    /// Rule-constant leaf (§3b.4): the value lives in the rule set,
    /// the node carries the interned reference.
    pub fn add_konst_leaf(
        &mut self,
        parent: &GhostId,
        typ_name: &str,
        rule_name: &str,
        plan_ix: u32,
        value: &str,
    ) -> GhostId {
        let id = ident::konst(parent, typ_name, rule_name, plan_ix);
        let typ = self.types.intern(typ_name);
        let konst = self.konsts.intern(value);
        self.insert_node(Node {
            id,
            typ,
            status: Status::Ghost,
            derived: None,
            konst: Some(konst),
            is_corr: false,
        });
        id
    }

    /// Mark an existing node as a correspondence. Separate from
    /// creation because a correspondence is created through the same
    /// paths as any other node (in the canonical rule format it is a
    /// node with two endpoints, not a special kind) — the mark must not
    /// touch identity.
    pub fn mark_corr(&mut self, id: &GhostId) {
        if let Some(n) = self.map.get_mut(id).and_then(|s| s.node.as_mut()) {
            n.is_corr = true;
        }
    }

    fn insert_node(&mut self, node: Node) {
        let slot = self.map.entry(node.id).or_default();
        match &mut slot.node {
            Some(existing) => {
                // M5: resurrection.
                if existing.status == Status::TentativeTombstone {
                    existing.status = node.status;
                }
            }
            None => {
                self.node_order.push(node.id);
                self.by_type.entry(node.typ).or_default().push(node.id);
                slot.node = Some(node);
            }
        }
    }

    /// Boundary materialization of ONE leaf: the derivation is cut off
    /// (derived = None) — the caller has already carried the current
    /// value into its original (store/host). From here on the leaf is
    /// value-independent (state-based adapter).
    pub fn freeze_derived(&mut self, id: &GhostId) {
        if let Some(n) = self.map.get_mut(id).and_then(|s| s.node.as_mut()) {
            n.derived = None;
        }
    }

    /// Matchable nodes of a type (type index, insertion order).
    pub fn nodes_of_type(&self, typ: TypeId) -> impl Iterator<Item = &Node> + '_ {
        self.by_type
            .get(&typ)
            .into_iter()
            .flatten()
            .filter_map(move |id| self.node(id))
            .filter(|n| n.status.is_matchable())
    }

    /// Connects two nodes (set: idempotent per direction). Direction =
    /// creation provenance (source creates/owns target). TOMB endpoints
    /// are rejected; a TentativeTombstone connection is revived (M5).
    pub fn connect(&mut self, source: GhostId, target: GhostId, status: Status) -> Option<GhostId> {
        self.connect_reporting(source, target, status)
            .map(|(id, _)| id)
    }

    /// Like `connect`, additionally reports whether the edge was NEWLY
    /// created (`true`) or already existed (`false`). Saves the
    /// retraction bookkeeping a separate `has_connection` lookup — the
    /// edge hash `ident::connection` is computed only ONCE.
    pub fn connect_reporting(
        &mut self,
        source: GhostId,
        target: GhostId,
        status: Status,
    ) -> Option<(GhostId, bool)> {
        let (styp, ttyp) = {
            let s = self.node(&source)?;
            let t = self.node(&target)?;
            if s.status == Status::Tombstone || t.status == Status::Tombstone {
                return None;
            }
            (s.typ, t.typ)
        };
        // Reclaim through USE (M5): whoever derives a live connection
        // to a tentatively retracted node re-legitimizes it
        // (consolidation only clears the unreclaimed) — e.g. a family
        // survives the deletion of its creator as long as one member
        // still uses it.
        if status != Status::Tombstone && status != Status::TentativeTombstone {
            for id in [source, target] {
                if let Some(n) = self.map.get_mut(&id).and_then(|s| s.node.as_mut()) {
                    if n.status == Status::TentativeTombstone {
                        n.status = Status::Ghost;
                    }
                }
            }
        }
        let id = ident::connection(&source, &target);
        if let Some(slot) = self.map.get(&id) {
            if slot.connection.is_some() {
                if let Some(c) = self.map.get_mut(&id).and_then(|s| s.connection.as_mut()) {
                    if c.status == Status::TentativeTombstone {
                        c.status = status;
                    }
                }
                return Some((id, false));
            }
        }
        self.map.entry(id).or_default().connection = Some(Connection {
            id,
            source,
            target,
            status,
        });
        self.map
            .entry(source)
            .or_default()
            .parts
            .insert(Participation {
                other_typ: ttyp,
                other: target,
                outgoing: true,
                connection: id,
            });
        self.map
            .entry(target)
            .or_default()
            .parts
            .insert(Participation {
                other_typ: styp,
                other: source,
                outgoing: false,
                connection: id,
            });
        Some((id, true))
    }

    // ── Status ───────────────────────────────────────────────────

    pub fn set_node_status(&mut self, id: &GhostId, status: Status) -> bool {
        match self.map.get_mut(id).and_then(|s| s.node.as_mut()) {
            Some(n) => {
                n.status = status;
                true
            }
            None => false,
        }
    }

    pub fn set_connection_status(&mut self, id: &GhostId, status: Status) -> bool {
        match self.map.get_mut(id).and_then(|s| s.connection.as_mut()) {
            Some(c) => {
                c.status = status;
                true
            }
            None => false,
        }
    }

    // ── Reading ──────────────────────────────────────────────────

    pub fn node(&self, id: &GhostId) -> Option<&Node> {
        self.map.get(id).and_then(|s| s.node.as_ref())
    }

    /// Every node in the graph, in no particular order. Callers that
    /// need a stable sequence sort by id.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> + '_ {
        self.map.values().filter_map(|s| s.node.as_ref())
    }

    pub fn connection(&self, id: &GhostId) -> Option<&Connection> {
        self.map.get(id).and_then(|s| s.connection.as_ref())
    }

    /// A node's participations whose counterpart has type `typ` (range
    /// over the sorted list — O(log + hits)).
    pub fn parts_by_other_type<'a>(
        &'a self,
        id: &GhostId,
        typ: TypeId,
    ) -> impl Iterator<Item = &'a Participation> + 'a {
        self.map.get(id).into_iter().flat_map(move |s| {
            s.parts.range(
                Participation {
                    other_typ: typ,
                    other: GhostId::from_raw([0; 32]),
                    outgoing: false,
                    connection: GhostId::from_raw([0; 32]),
                }..=Participation {
                    other_typ: typ,
                    other: GhostId::from_raw([0xFF; 32]),
                    outgoing: true,
                    connection: GhostId::from_raw([0xFF; 32]),
                },
            )
        })
    }

    /// All participations (sorted) — for walks/diagnostics.
    pub fn parts<'a>(&'a self, id: &GhostId) -> impl Iterator<Item = &'a Participation> + 'a {
        self.map.get(id).into_iter().flat_map(|s| s.parts.iter())
    }

    /// Does a matchable connection source→target exist?
    pub fn connected(&self, source: &GhostId, target: &GhostId) -> bool {
        let id = ident::connection(source, target);
        self.connection(&id)
            .is_some_and(|c| c.status.is_matchable())
    }

    /// Exists NON-tentatively (Solid|Ghost)? For duplicate suppression:
    /// TT does NOT count as present — the reclaim runs via apply/connect
    /// (M5 resurrection), consolidate clears the unreclaimed.
    pub fn connected_alive(&self, source: &GhostId, target: &GhostId) -> bool {
        let id = ident::connection(source, target);
        self.connection(&id)
            .is_some_and(|c| matches!(c.status, Status::Solid | Status::Ghost))
    }

    /// Node present NON-tentatively (Solid|Ghost)?
    pub fn node_alive(&self, id: &GhostId) -> bool {
        self.node(id)
            .is_some_and(|n| matches!(n.status, Status::Solid | Status::Ghost))
    }

    /// Nodes in insertion order.
    pub fn iter_nodes(&self) -> impl Iterator<Item = &Node> {
        self.node_order
            .iter()
            .filter_map(|id| self.map.get(id).and_then(|s| s.node.as_ref()))
    }

    pub fn node_count(&self) -> usize {
        self.node_order.len()
    }

    /// Materialization (fold end stage, def. 5.1): a new
    /// graph without tombstones, all ghosts become solid; connections
    /// only if both endpoints survive. Values stay in the original
    /// (derived leaves keep source+transform — the export resolves via
    /// the resolver).
    pub fn materialize(&self) -> Graph {
        let mut out = Graph::new();
        out.types = self.types.clone();
        out.konsts = self.konsts.clone();
        // Carry the chains along: the surviving leaves carry ChainIds
        // that are only resolvable in this table.
        out.chains = self.chains.clone();
        for n in self.iter_nodes() {
            if n.status == Status::Tombstone {
                continue;
            }
            let mut node = n.clone();
            node.status = Status::Solid;
            let id = node.id;
            out.node_order.push(id);
            out.by_type.entry(node.typ).or_default().push(id);
            out.map.entry(id).or_default().node = Some(node);
        }
        for slot in self.map.values() {
            if let Some(c) = &slot.connection {
                if c.status == Status::Tombstone {
                    continue;
                }
                if out.node(&c.source).is_some() && out.node(&c.target).is_some() {
                    let _ = out.connect(c.source, c.target, Status::Solid);
                }
            }
        }
        out
    }

    /// Value resolution: follow the ref chain to the baseline leaf,
    /// then apply transformations forward. Values only exist here,
    /// transiently — never in the overlay.
    pub fn resolve_value(&self, id: &GhostId, resolver: &dyn ValueResolver) -> Option<String> {
        let mut chain: Vec<LeafTransform> = Vec::new();
        let mut cur = *id;
        loop {
            let node = self.node(&cur)?;
            if let Some(k) = node.konst {
                let mut v = self.konsts.value(k).to_string();
                for t in chain.iter().rev() {
                    v = t.apply(&v, &self.chains)?;
                }
                return Some(v);
            }
            match node.derived {
                None => {
                    let mut v = resolver.baseline_value(&cur)?;
                    for t in chain.iter().rev() {
                        v = t.apply(&v, &self.chains)?;
                    }
                    return Some(v);
                }
                Some(DerivedLeaf { quelle, transform }) => {
                    chain.push(transform);
                    cur = quelle;
                }
            }
        }
    }
}

impl Graph {
    /// First matchable child leaf of an anchor with the given type
    /// (deterministic: participation order) — source of dynamic
    /// bindings (apply-if-present).
    pub fn child_leaf_of_type(&self, anchor: &GhostId, typ_name: &str) -> Option<GhostId> {
        let typ = self.types.lookup(typ_name)?;
        self.parts_by_other_type(anchor, typ)
            .filter(|p| p.outgoing)
            .map(|p| p.other)
            .find(|id| self.node(id).is_some_and(|n| n.status.is_matchable()))
    }
}

// ══ Tests (stage 1) ═════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use crate::rules::transform::{Chain, Prim};

    /// Die Kette, die frueher `getter_name` hiess.
    fn getter_chain() -> PlanTransform {
        PlanTransform::Chain(Chain(vec![Prim::Capitalize, Prim::Prefix("get".into())]))
    }

    use super::*;

    fn leaf_setup() -> (Graph, ValueStore, GhostId, GhostId) {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        let class = g.add_baseline("uml:/Person", "Class");
        let name = g.add_baseline("uml:/Person/name", "name");
        vs.insert(name, "person");
        g.connect(class, name, Status::Solid);
        (g, vs, class, name)
    }

    #[test]
    fn identity_survives_a_rename_without_any_value() {
        // Core of Sandra's decision: a rename (value change in the
        // original) changes NO identity in the overlay.
        let (g, mut vs, _class, name) = leaf_setup();
        let derived = {
            let mut g2 = g.clone();
            g2.add_derived_leaf(&name, "name", name, &getter_chain())
        };
        vs.insert(name, "personRenamed");
        let mut g3 = g.clone();
        let derived_after = g3.add_derived_leaf(&name, "name", name, &getter_chain());
        assert_eq!(derived, derived_after, "identity never hangs off the value");
    }

    #[test]
    fn value_resolves_through_the_transform_chain() {
        let (mut g, vs, _class, name) = leaf_setup();
        let getter = g.add_derived_leaf(&name, "getterName", name, &getter_chain());
        assert_eq!(
            g.resolve_value(&getter, &vs).as_deref(),
            Some("getPerson"),
            "value = transform(original), stored nowhere"
        );
        // Chain: capitalize(getter(name))
        let cap = g.add_derived_leaf(
            &getter,
            "x",
            getter,
            &PlanTransform::Chain(Chain(vec![Prim::Capitalize])),
        );
        assert_eq!(g.resolve_value(&cap, &vs).as_deref(), Some("GetPerson"));
    }

    /// Identity contract after the chain switch: [`ident::derived`]
    /// now takes in the CHAIN (including its arguments), no longer a
    /// tag byte of a closed set. The encoding must stay unambiguous
    /// throughout — chain, comparator table and forward/backward must
    /// not collide.
    #[test]
    fn derived_identity_hangs_off_the_chain() {
        let (_g, _vs, _class, name) = leaf_setup();
        let id = |t: PlanTransform| preview_derived_id(&name, "x", &name, &t);
        let chain = |p: &[Prim]| PlanTransform::Chain(Chain(p.to_vec()));

        // The chain's argument enters.
        assert_ne!(
            id(chain(&[Prim::Prefix("get".into())])),
            id(chain(&[Prim::Prefix("set".into())])),
        );
        // Order enters.
        assert_ne!(
            id(chain(&[Prim::Capitalize, Prim::Prefix("get".into())])),
            id(chain(&[Prim::Prefix("get".into()), Prim::Capitalize])),
        );
        // Semantically equal spellings collapse — identity hangs off
        // the chain's normal form, not its notation.
        assert_eq!(id(chain(&[])), id(chain(&[Prim::Identity])));
        assert_eq!(id(chain(&[])), id(chain(&[Prim::Prefix("".into())])));
        assert_eq!(
            id(chain(&[Prim::Prefix("a".into()), Prim::Prefix("b".into())])),
            id(chain(&[Prim::Prefix("ba".into())])),
        );
        // Die frueher benannten Vokabeln sind gewoehnliche Ketten.
        assert_eq!(
            getter_chain(),
            chain(&[Prim::Capitalize, Prim::Prefix("get".into())]),
        );
        assert_eq!(PlanTransform::Chain(Chain(Vec::new())), chain(&[]));
    }

    #[test]
    fn connections_are_a_set_and_idempotent() {
        let (mut g, _vs, class, name) = leaf_setup();
        let c1 = g.connect(class, name, Status::Solid).unwrap();
        let c2 = g.connect(class, name, Status::Solid).unwrap();
        assert_eq!(c1, c2, "exactly one connection per (source,target)");
        assert_eq!(g.parts(&class).count(), 1);
        // The reverse direction is a DIFFERENT connection (provenance!).
        let back = g.connect(name, class, Status::Solid).unwrap();
        assert_ne!(c1, back);
    }

    #[test]
    fn participations_by_counterpart_type() {
        let mut g = Graph::new();
        let hub = g.add_baseline("hub", "Reg");
        for i in 0..100 {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            g.connect(hub, f, Status::Solid);
        }
        let corr = g.add_baseline("c0", "Corr");
        g.connect(corr, hub, Status::Solid);
        let corr_t = g.types.lookup("Corr").unwrap();
        let hits: Vec<_> = g.parts_by_other_type(&hub, corr_t).collect();
        assert_eq!(hits.len(), 1, "hub search by counterpart type: O(log+1)");
        assert_eq!(hits[0].other, corr);
        assert!(!hits[0].outgoing, "corr → hub: hub is the target");
    }

    #[test]
    fn tombstone_and_resurrection() {
        let (mut g, _vs, class, name) = leaf_setup();
        let c = g.connect(class, name, Status::Solid).unwrap();
        g.set_connection_status(&c, Status::TentativeTombstone);
        // Reconnecting revives it again (M5).
        let c2 = g.connect(class, name, Status::Ghost).unwrap();
        assert_eq!(c, c2);
        assert_eq!(g.connection(&c).unwrap().status, Status::Ghost);
        // A TOMB endpoint is rejected.
        g.set_node_status(&name, Status::Tombstone);
        assert!(g.connect(class, name, Status::Solid).is_none());
    }

    #[test]
    fn types_intern_deterministically() {
        let mut g = Graph::new();
        let a1 = g.types.intern("Class");
        let a2 = g.types.intern("Class");
        let b = g.types.intern("Member");
        assert_eq!(a1, a2);
        assert_ne!(a1, b);
        assert_eq!(g.types.name(a1), "Class");
    }
}

#[cfg(test)]
mod ident_encoding {
    use super::*;
    use crate::rules::transform::Prim;

    /// The boundary between two variable fields must not be shiftable.
    ///
    /// Before the length prefixes, `konst(p, "ab", "c", 0)` and
    /// `konst(p, "a", "bc", 0)` produced the same bytes and therefore
    /// the same identity — two distinct nodes silently became one.
    /// This test walks the boundary across every split of a fixed
    /// string, so a future field added without a prefix fails here
    /// rather than in a user's model.
    #[test]
    fn konst_boundary_is_not_shiftable() {
        let parent = ident::baseline("p");
        let joined = "abcde";
        let ids: Vec<GhostId> = (0..=joined.len())
            .map(|i| ident::konst(&parent, &joined[..i], &joined[i..], 0))
            .collect();
        for (i, a) in ids.iter().enumerate() {
            for (j, b) in ids.iter().enumerate().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "split at {i} and at {j} share an identity — the field \
                     boundary is shiftable"
                );
            }
        }
    }

    /// Same for the type/source boundary of a derived leaf.
    #[test]
    fn derived_type_boundary_is_not_shiftable() {
        let parent = ident::baseline("p");
        let src = ident::baseline("s");
        let empty = PlanTransform::Chain(Chain::default());
        let cap = PlanTransform::Chain(Chain(vec![Prim::Capitalize]));
        assert_ne!(
            ident::derived(&parent, "ab", &src, &empty),
            ident::derived(&parent, "a", &src, &empty),
            "type name length must be part of the identity"
        );
        assert_ne!(
            ident::derived(&parent, "a", &src, &empty),
            ident::derived(&parent, "a", &src, &cap),
            "the chain must be part of the identity"
        );
    }

    /// A corr's ref list carries its count, so a longer type name with
    /// one ref fewer cannot hash the same.
    #[test]
    fn corr_ref_count_is_part_of_the_identity() {
        let anchor = ident::baseline("a");
        let r1 = ident::baseline("r1");
        let r2 = ident::baseline("r2");
        assert_ne!(
            ident::corr(&anchor, "C", &[r1, r2]),
            ident::corr(&anchor, "C", &[r1]),
            "the number of refs must be part of the identity"
        );
        assert_ne!(
            ident::corr(&anchor, "CC", &[r1]),
            ident::corr(&anchor, "C", &[r1]),
            "the type name must be distinguishable from what follows"
        );
    }

    /// Every derivation lives under its own domain tag, so the same
    /// inputs cannot mean two things.
    #[test]
    fn domains_do_not_overlap() {
        let p = ident::baseline("x");
        let ids = [
            ident::baseline("x"),
            ident::ghost(&p, "x"),
            ident::connection(&p, &p),
            ident::corr(&p, "x", &[]),
            ident::konst(&p, "x", "", 0),
        ];
        for (i, a) in ids.iter().enumerate() {
            for (j, b) in ids.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "derivations {i} and {j} share an identity");
            }
        }
    }
}
