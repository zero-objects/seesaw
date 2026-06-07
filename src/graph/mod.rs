//! Graph module — T₂, T₅.
//!
//! Responsibilities:
//! - Typed attributed graphs (L, R, D)
//! - Status annotation (SOLID, GHOST, TOMB) — see Def. 2.4
//! - Parent-rooted Ghost-ID via blake3 — see Def. 5.3

use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::hash::FxHashMap;

/// Status of an element in the ghost graph.
///
/// See Def. 2.4 in the paper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    /// Element from the baseline L_0/R_0, untouched.
    Solid,
    /// Virtually added, not yet materialized.
    Ghost,
    /// **M5.5**: Tentative tombstone during a cascade
    /// invalidation phase. Resolves in the consolidation phase
    /// either back to `Solid` (resurrection via identical
    /// Ghost-ID of a new rule application) or to `Tombstone`
    /// (final invalidation).
    ///
    /// Remains matchable (see `Status::is_matchable`) so that a
    /// new rule application can reclaim the identity.
    TentativeTombstone,
    /// Virtually deleted; invisible to pattern matching,
    /// visible to conflict detection and V₁₂ induction.
    Tombstone,
}

impl Status {
    /// Elements visible to pattern matching.
    ///
    /// Solid + Ghost as usual. **TentativeTombstone** is deliberately
    /// matchable (M5.5): during the consolidation phase a new rule
    /// application may reclaim the element via the same Ghost-ID
    /// (resurrection).
    pub fn is_matchable(&self) -> bool {
        matches!(
            self,
            Status::Solid | Status::Ghost | Status::TentativeTombstone
        )
    }
}

/// Parent-rooted Ghost-ID (blake3 hash).
///
/// See Def. 5.3 in the paper. 32-byte blake3 hash over the cascade
/// history of an element. blake3 is cryptographically collision-resistant
/// (so the structural identity contract of Def. 5.3 holds — distinct
/// content never aliases) while being SIMD-accelerated, unlike the prior
/// SHA-256 software fallback (rc10 perf).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GhostId([u8; 32]);

impl GhostId {
    /// Builds a baseline ID for a SOLID element from a stable name.
    ///
    /// Used for root elements in L_0/R_0 — the recursion anchor
    /// for Def. 5.3.
    pub fn from_baseline(name: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"SOLID\0");
        hasher.update(name.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Builds a Ghost-ID for a GHOST element, per Def. 5.3:
    ///
    /// ```text
    /// id(e) = H(id(parent(e)) || edgedata(e) || σ(e))
    /// ```
    pub fn from_parent(
        parent: &GhostId,
        edge_type: &str,
        own_type: &str,
        attrs: &BTreeMap<String, String>,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"GHOST\0");
        hasher.update(&parent.0);
        hasher.update(b"\0");
        hasher.update(edge_type.as_bytes());
        hasher.update(b"\0");
        hasher.update(own_type.as_bytes());
        hasher.update(b"\0");
        for (k, v) in attrs {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"\0");
        }
        Self(*hasher.finalize().as_bytes())
    }

    /// Builds a Ghost-ID for an edge from endpoints and type information.
    pub fn for_edge(
        source: &GhostId,
        target: &GhostId,
        edge_type: &str,
        attrs: &BTreeMap<String, String>,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"EDGE\0");
        hasher.update(&source.0);
        hasher.update(&target.0);
        hasher.update(b"\0");
        hasher.update(edge_type.as_bytes());
        hasher.update(b"\0");
        for (k, v) in attrs {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"\0");
        }
        Self(*hasher.finalize().as_bytes())
    }

    /// Short hexadecimal form (8 characters) for UI and logging.
    pub fn short(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }

    /// Full 64-character hex form (32 bytes). Passed across the
    /// JNI boundary so the pilot side can return a cascade-generated
    /// identity losslessly (rc7 return-path bridge).
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// Parses the 64-character hex form back; returns `None` on invalid
    /// length or non-hex characters.
    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            let byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
            bytes[i] = byte;
        }
        Some(Self(bytes))
    }

    /// Raw 32-byte hash.
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// ID derived from an opaque external identifier (e.g.\ EMF URI fragment
    /// or JDT handle identifier). Used at the integration boundary,
    /// where the external world carries its own identities that we
    /// cannot structurally re-derive.
    pub fn from_opaque(opaque: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"OPAQUE\0");
        hasher.update(opaque.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }
}

impl fmt::Debug for GhostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GhostId({})", self.short())
    }
}

/// Typed node data inside the TypedGraph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeData {
    pub id: GhostId,
    pub type_id: String,
    pub attrs: BTreeMap<String, String>,
    pub status: Status,
}

/// Typed edge data inside the TypedGraph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeData {
    pub id: GhostId,
    pub type_id: String,
    pub attrs: BTreeMap<String, String>,
    pub status: Status,
}

/// Typed attributed graph with status.
///
/// Realizes the ghost projection φ_L (or φ_R) or the
/// correspondence graph D, including all ghost/tombstone annotations.
#[derive(Clone, Debug, Default)]
pub struct TypedGraph {
    inner: Graph<NodeData, EdgeData>,
    node_index: FxHashMap<GhostId, NodeIndex>,
    edge_index: FxHashMap<GhostId, EdgeIndex>,
    /// Match index (F15 mitigation): kind → set of node IDs of that
    /// type. Enables `O(matching_kind_count)` lookup instead of
    /// `O(graph_size)` for each pattern position. BTreeSet for
    /// deterministic ordering (canonical μ).
    ///
    /// Status filtering (Solid/Ghost/TentativeTombstone vs. Tombstone)
    /// happens at lookup time; the index tracks only the type, not the
    /// status. This keeps index updates insert-only — no update
    /// is needed on status changes.
    kind_index: BTreeMap<String, BTreeSet<GhostId>>,
    /// Content→id memo (rc10 perf): GhostIds are content-addressed — the
    /// same content always yields the same id. The cascade re-matches
    /// everything every step and checks the same ops per candidate
    /// (`is_duplicate`, `apply`), so without a memo the identical hash is
    /// recomputed 2–4×/candidate and across all steps. The memo hashes
    /// **once per content** and reads it afterwards. A pure function cache
    /// — content→id is constant, so it never needs invalidation; GhostIds
    /// stay byte-identical. Interior-mutable because the hot callers
    /// (`is_duplicate`, `produce`) hold `&TypedGraph`.
    id_memo: std::cell::RefCell<IdMemo>,
}

/// Memo for content-addressed GhostIds (see `TypedGraph::id_memo`).
#[derive(Debug, Default, Clone)]
struct IdMemo {
    nodes: FxHashMap<(GhostId, String, String, BTreeMap<String, String>), GhostId>,
    edges: FxHashMap<(GhostId, GhostId, String, BTreeMap<String, String>), GhostId>,
    /// Instrumentation: avoided hash computations (hits) vs. misses.
    hits: u64,
    misses: u64,
}

impl TypedGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Content-addressed node id, memoized (instead of re-hashing via
    /// `GhostId::from_parent`). Byte-identical to the direct computation.
    pub fn node_id(
        &self,
        parent: &GhostId,
        edge_type: &str,
        type_id: &str,
        attrs: &BTreeMap<String, String>,
    ) -> GhostId {
        let mut memo = self.id_memo.borrow_mut();
        if let Some(id) = memo
            .nodes
            .get(&(
                *parent,
                edge_type.to_string(),
                type_id.to_string(),
                attrs.clone(),
            ))
            .copied()
        {
            memo.hits += 1;
            return id;
        }
        memo.misses += 1;
        let id = GhostId::from_parent(parent, edge_type, type_id, attrs);
        memo.nodes.insert(
            (*parent, edge_type.into(), type_id.into(), attrs.clone()),
            id,
        );
        id
    }

    /// Content-addressed edge id, memoized (instead of `GhostId::for_edge`).
    pub fn edge_id(
        &self,
        source: &GhostId,
        target: &GhostId,
        edge_type: &str,
        attrs: &BTreeMap<String, String>,
    ) -> GhostId {
        let mut memo = self.id_memo.borrow_mut();
        if let Some(id) = memo
            .edges
            .get(&(*source, *target, edge_type.to_string(), attrs.clone()))
            .copied()
        {
            memo.hits += 1;
            return id;
        }
        memo.misses += 1;
        let id = GhostId::for_edge(source, target, edge_type, attrs);
        memo.edges
            .insert((*source, *target, edge_type.into(), attrs.clone()), id);
        id
    }

    /// Memo statistics (avoided hash computations, misses) — for profiling.
    pub fn id_memo_stats(&self) -> (u64, u64) {
        let memo = self.id_memo.borrow();
        (memo.hits, memo.misses)
    }

    /// Adds a SOLID baseline node.
    pub fn add_baseline_node(
        &mut self,
        type_id: &str,
        name: &str,
        attrs: BTreeMap<String, String>,
    ) -> GhostId {
        let id = GhostId::from_baseline(name);
        self.insert_node(NodeData {
            id,
            type_id: type_id.into(),
            attrs,
            status: Status::Solid,
        });
        id
    }

    /// Adds a GHOST node with a parent reference.
    pub fn add_ghost_node(
        &mut self,
        parent: GhostId,
        edge_type: &str,
        type_id: &str,
        attrs: BTreeMap<String, String>,
    ) -> GhostId {
        let id = self.node_id(&parent, edge_type, type_id, &attrs);
        self.insert_node(NodeData {
            id,
            type_id: type_id.into(),
            attrs,
            status: Status::Ghost,
        });
        id
    }

    /// Adds a SOLID node with a parent reference (for baseline
    /// elements that structurally hang off a parent node but
    /// already belong to the initial baseline — not ghosts).
    pub fn add_solid_child_node(
        &mut self,
        parent: GhostId,
        edge_type: &str,
        type_id: &str,
        attrs: BTreeMap<String, String>,
    ) -> GhostId {
        let id = self.node_id(&parent, edge_type, type_id, &attrs);
        self.insert_node(NodeData {
            id,
            type_id: type_id.into(),
            attrs,
            status: Status::Solid,
        });
        id
    }

    fn insert_node(&mut self, node: NodeData) {
        if let Some(idx) = self.node_index.get(&node.id) {
            // M5: resurrection — when a TentativeTombstone node
            // with an identical ID is reclaimed, restore its
            // status to the new status (Solid/Ghost).
            if let Some(existing) = self.inner.node_weight_mut(*idx) {
                if existing.status == Status::TentativeTombstone {
                    existing.status = node.status;
                }
            }
            // Index is already current (node exists with
            // identical type via Ghost-ID hash).
            return;
        }
        let id = node.id;
        let kind = node.type_id.clone();
        let idx = self.inner.add_node(node);
        self.node_index.insert(id, idx);
        self.kind_index.entry(kind).or_default().insert(id);
    }

    /// Adds an edge. Returns the edge's Ghost-ID, or `None` if
    /// either endpoint does not exist.
    pub fn add_edge(
        &mut self,
        source: GhostId,
        target: GhostId,
        edge_type: &str,
        attrs: BTreeMap<String, String>,
        status: Status,
    ) -> Option<GhostId> {
        let source_idx = *self.node_index.get(&source)?;
        let target_idx = *self.node_index.get(&target)?;
        let id = self.edge_id(&source, &target, edge_type, &attrs);
        if let Some(existing_idx) = self.edge_index.get(&id) {
            // M5: edge resurrection — if TentativeTombstone, reset
            // to the requested status.
            if let Some(existing) = self.inner.edge_weight_mut(*existing_idx) {
                if existing.status == Status::TentativeTombstone {
                    existing.status = status;
                }
            }
            return Some(id);
        }
        let edge_data = EdgeData {
            id,
            type_id: edge_type.into(),
            attrs,
            status,
        };
        let idx = self.inner.add_edge(source_idx, target_idx, edge_data);
        self.edge_index.insert(id, idx);
        Some(id)
    }

    /// Sets the status of a node (tombstone marker).
    pub fn set_node_status(&mut self, id: &GhostId, status: Status) -> bool {
        if let Some(idx) = self.node_index.get(id) {
            self.inner[*idx].status = status;
            true
        } else {
            false
        }
    }

    /// Sets the status of an edge (tombstone marker).
    pub fn set_edge_status(&mut self, id: &GhostId, status: Status) -> bool {
        if let Some(idx) = self.edge_index.get(id) {
            self.inner[*idx].status = status;
            true
        } else {
            false
        }
    }

    /// Sets an attribute on a node (phase 1: overwriting).
    ///
    /// Phase 3 (planned): shadow stack with κ-tagging for step-back semantics.
    pub fn set_node_attr(&mut self, id: &GhostId, key: &str, value: &str) -> bool {
        if let Some(idx) = self.node_index.get(id) {
            self.inner[*idx]
                .attrs
                .insert(key.to_string(), value.to_string());
            true
        } else {
            false
        }
    }

    /// Reads a node by ID.
    pub fn get_node(&self, id: &GhostId) -> Option<&NodeData> {
        self.node_index.get(id).map(|idx| &self.inner[*idx])
    }

    /// Reads an edge by ID.
    pub fn get_edge(&self, id: &GhostId) -> Option<&EdgeData> {
        self.edge_index.get(id).map(|idx| &self.inner[*idx])
    }

    /// Returns the endpoint IDs of an edge (source, target).
    pub fn edge_endpoints(&self, id: &GhostId) -> Option<(GhostId, GhostId)> {
        let idx = *self.edge_index.get(id)?;
        let (src_idx, tgt_idx) = self.inner.edge_endpoints(idx)?;
        let src_data = self.inner.node_weight(src_idx)?;
        let tgt_data = self.inner.node_weight(tgt_idx)?;
        Some((src_data.id, tgt_data.id))
    }

    /// Node count (including TOMB).
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Edge count (including TOMB).
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Iterates matchable nodes of a given type (F15
    /// mitigation: indexed lookup instead of full iteration).
    /// Returns only nodes with `status.is_matchable()`.
    pub fn matchable_nodes_by_kind<'a>(
        &'a self,
        kind: &str,
    ) -> impl Iterator<Item = &'a NodeData> + 'a {
        self.kind_index
            .get(kind)
            .into_iter()
            .flat_map(move |set| set.iter())
            .filter_map(move |id| self.node_index.get(id).copied())
            .filter_map(move |idx| self.inner.node_weight(idx))
            .filter(|n| n.status.is_matchable())
    }

    /// Iterates all matchable nodes (SOLID + GHOST).
    pub fn matchable_nodes(&self) -> impl Iterator<Item = &NodeData> {
        self.inner
            .node_weights()
            .filter(|n| n.status.is_matchable())
    }

    /// Checks whether a matchable edge of the given type exists
    /// between `source` and `target`.
    pub fn has_edge_between(&self, source: &GhostId, target: &GhostId, edge_type: &str) -> bool {
        let src_idx = match self.node_index.get(source) {
            Some(i) => *i,
            None => return false,
        };
        let tgt_idx = match self.node_index.get(target) {
            Some(i) => *i,
            None => return false,
        };
        self.inner.edges(src_idx).any(|e| {
            e.target() == tgt_idx
                && e.weight().status.is_matchable()
                && e.weight().type_id == edge_type
        })
    }

    /// rc7 (S): does ANY matchable edge exist between `a` and `b`
    /// (any direction, any kind)? Used for symmetric
    /// correspondence-membership matching (see
    /// `EdgePattern::membership`). A correspondence node wires only
    /// its two endpoints, so "any edge" here correctly identifies
    /// correspondence membership without needing to know corrL/corrR.
    pub fn has_any_edge_either_dir(&self, a: &GhostId, b: &GhostId) -> bool {
        let a_idx = match self.node_index.get(a) {
            Some(i) => *i,
            None => return false,
        };
        let b_idx = match self.node_index.get(b) {
            Some(i) => *i,
            None => return false,
        };
        self.inner
            .edges(a_idx)
            .any(|e| e.target() == b_idx && e.weight().status.is_matchable())
            || self
                .inner
                .edges(b_idx)
                .any(|e| e.target() == a_idx && e.weight().status.is_matchable())
    }

    /// All matchable outgoing edges of a node, paired with the target node ID.
    pub fn outgoing_edges(&self, source: &GhostId) -> Vec<(&EdgeData, GhostId)> {
        let src_idx = match self.node_index.get(source) {
            Some(i) => *i,
            None => return Vec::new(),
        };
        self.inner
            .edges(src_idx)
            .filter(|e| e.weight().status.is_matchable())
            .map(|e| (e.weight(), self.inner[e.target()].id))
            .collect()
    }

    /// All matchable incoming edges of a node, paired with the source node ID.
    pub fn incoming_edges(&self, target: &GhostId) -> Vec<(&EdgeData, GhostId)> {
        let tgt_idx = match self.node_index.get(target) {
            Some(i) => *i,
            None => return Vec::new(),
        };
        self.inner
            .edges_directed(tgt_idx, petgraph::Direction::Incoming)
            .filter(|e| e.weight().status.is_matchable())
            .map(|e| (e.weight(), self.inner[e.source()].id))
            .collect()
    }

    /// All matchable incident edges of a node (outgoing and incoming).
    pub fn incident_edges(&self, node: &GhostId) -> Vec<(&EdgeData, GhostId)> {
        let mut out = self.outgoing_edges(node);
        out.extend(self.incoming_edges(node));
        out
    }

    /// Iterates all nodes regardless of status.
    pub fn iter_nodes(&self) -> impl Iterator<Item = &NodeData> {
        self.inner.node_weights()
    }

    /// Iterates all edges with source and target IDs.
    pub fn iter_edges(&self) -> Vec<(GhostId, GhostId, &EdgeData)> {
        self.inner
            .edge_references()
            .map(|e| {
                (
                    self.inner[e.source()].id,
                    self.inner[e.target()].id,
                    e.weight(),
                )
            })
            .collect()
    }

    /// Inserts a node from a precomputed `NodeData`.
    /// Skipped if the ID already exists.
    ///
    /// Must maintain the `kind_index` in addition to the `node_index` —
    /// `matchable_nodes_by_kind` (pattern-matcher hot path) iterates
    /// only over it. Without this entry, nodes would be invisible
    /// to any rule that matches by LHS `kind` (regression from
    /// the May commit 8401f0e/F15 match indexing — `insert_node` was
    /// extended correctly, `insert_node_data` was overlooked).
    pub fn insert_node_data(&mut self, data: NodeData) {
        if !self.node_index.contains_key(&data.id) {
            let id = data.id;
            let kind = data.type_id.clone();
            let idx = self.inner.add_node(data);
            self.node_index.insert(id, idx);
            self.kind_index.entry(kind).or_default().insert(id);
        }
    }

    /// Inserts an edge from a precomputed `EdgeData`. Requires
    /// that `source` and `target` already exist.
    pub fn insert_edge_data(&mut self, source: GhostId, target: GhostId, data: EdgeData) -> bool {
        let src_idx = match self.node_index.get(&source) {
            Some(i) => *i,
            None => return false,
        };
        let tgt_idx = match self.node_index.get(&target) {
            Some(i) => *i,
            None => return false,
        };
        let id = data.id;
        let idx = self.inner.add_edge(src_idx, tgt_idx, data);
        self.edge_index.insert(id, idx);
        true
    }

    /// Materializes the graph (Def. 5.1): builds a new
    /// `TypedGraph` in which all TOMB elements are removed and all
    /// remaining GHOST elements carry SOLID status.
    ///
    /// Emits edges whose endpoints survive in the materialized node
    /// set; edges with dropped endpoints fall away with them.
    pub fn materialize(&self) -> TypedGraph {
        let mut new_g = TypedGraph::new();

        // Copy nodes (excluding TOMB); status → SOLID.
        for node in self.inner.node_weights() {
            if node.status == Status::Tombstone {
                continue;
            }
            new_g.insert_node_data(NodeData {
                id: node.id,
                type_id: node.type_id.clone(),
                attrs: node.attrs.clone(),
                status: Status::Solid,
            });
        }

        // Copy edges (excluding TOMB) — only when both endpoints exist.
        for edge_ref in self.inner.edge_references() {
            let data = edge_ref.weight();
            if data.status == Status::Tombstone {
                continue;
            }
            let src_id = self.inner[edge_ref.source()].id;
            let tgt_id = self.inner[edge_ref.target()].id;
            if new_g.node_index.contains_key(&src_id) && new_g.node_index.contains_key(&tgt_id) {
                new_g.insert_edge_data(
                    src_id,
                    tgt_id,
                    EdgeData {
                        id: data.id,
                        type_id: data.type_id.clone(),
                        attrs: data.attrs.clone(),
                        status: Status::Solid,
                    },
                );
            }
        }

        new_g
    }
}

// ══ Tests ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn baseline_id_is_deterministic() {
        let a = GhostId::from_baseline("Person");
        let b = GhostId::from_baseline("Person");
        assert_eq!(a, b);
    }

    /// rc7 return-path bridge: for the JNI hand-off to the pilot side
    /// (which needs the full ID to re-register it), `hex()` must yield
    /// the full 64-character form, and `from_hex` must parse it back
    /// losslessly.
    #[test]
    fn ghost_id_hex_round_trip() {
        let a = GhostId::from_baseline("Person");
        let s = a.hex();
        assert_eq!(s.len(), 64, "64 hex chars = 32 bytes");
        let b = GhostId::from_hex(&s).expect("gültige Hex-Form parst zurück");
        assert_eq!(a, b);
    }

    #[test]
    fn ghost_id_from_hex_rejects_invalid() {
        assert!(GhostId::from_hex("").is_none());
        assert!(GhostId::from_hex("zzz").is_none());
        assert!(
            GhostId::from_hex(&"a".repeat(63)).is_none(),
            "ungerade Länge"
        );
        assert!(GhostId::from_hex(&"a".repeat(66)).is_none(), "zu lang");
    }

    #[test]
    fn baseline_id_distinguishes_names() {
        let a = GhostId::from_baseline("Person");
        let b = GhostId::from_baseline("Car");
        assert_ne!(a, b);
    }

    #[test]
    fn ghost_id_parent_rooted() {
        let parent = GhostId::from_baseline("Person");
        let child_a = GhostId::from_parent(
            &parent,
            "hasAttribute",
            "Attribute",
            &attrs(&[("name", "age")]),
        );
        let child_b = GhostId::from_parent(
            &parent,
            "hasAttribute",
            "Attribute",
            &attrs(&[("name", "age")]),
        );
        assert_eq!(child_a, child_b, "Gleiche Eingabe → gleicher Hash");
    }

    #[test]
    fn ghost_id_differs_on_attr_change() {
        let parent = GhostId::from_baseline("Person");
        let a = GhostId::from_parent(
            &parent,
            "hasAttribute",
            "Attribute",
            &attrs(&[("name", "age")]),
        );
        let b = GhostId::from_parent(
            &parent,
            "hasAttribute",
            "Attribute",
            &attrs(&[("name", "email")]),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn ghost_id_differs_on_parent_change() {
        let p1 = GhostId::from_baseline("Person");
        let p2 = GhostId::from_baseline("Car");
        let attrs = attrs(&[("name", "id")]);
        let a = GhostId::from_parent(&p1, "hasAttribute", "Attribute", &attrs);
        let b = GhostId::from_parent(&p2, "hasAttribute", "Attribute", &attrs);
        assert_ne!(a, b);
    }

    /// Integration test: mini UML with Person, Car, attributes, association.
    #[test]
    fn mini_uml_example() {
        let mut g = TypedGraph::new();

        // Classes
        let person = g.add_baseline_node("Class", "Person", attrs(&[("name", "Person")]));
        let car = g.add_baseline_node("Class", "Car", attrs(&[("name", "Car")]));

        // Attributes
        let person_name = g.add_ghost_node(
            person,
            "hasAttribute",
            "Attribute",
            attrs(&[("name", "name"), ("type", "String")]),
        );
        let car_model = g.add_ghost_node(
            car,
            "hasAttribute",
            "Attribute",
            attrs(&[("name", "model"), ("type", "String")]),
        );

        // Edges
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

        // Association Person ──owns──▶ Car
        g.add_edge(
            person,
            car,
            "owns",
            attrs(&[("multiplicity", "1..*")]),
            Status::Ghost,
        )
        .unwrap();

        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 3);
        assert_eq!(
            g.matchable_nodes().count(),
            4,
            "alle SOLID + GHOST sind matchbar"
        );

        // Tombstone one attribute
        assert!(g.set_node_status(&person_name, Status::Tombstone));
        assert_eq!(g.matchable_nodes().count(), 3, "TOMB nicht mehr matchbar");
        assert_eq!(g.node_count(), 4, "TOMB bleibt physisch erhalten");
    }

    #[test]
    fn node_lookup_and_typing() {
        let mut g = TypedGraph::new();
        let id = g.add_baseline_node("Class", "Foo", attrs(&[("pkg", "org.example")]));
        let node = g.get_node(&id).unwrap();
        assert_eq!(node.type_id, "Class");
        assert_eq!(node.attrs.get("pkg"), Some(&"org.example".to_string()));
        assert_eq!(node.status, Status::Solid);
    }

    #[test]
    fn ghost_id_short_format() {
        let id = GhostId::from_baseline("Test");
        let short = id.short();
        assert_eq!(short.len(), 8, "8 Hex-Zeichen");
        assert!(short.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
