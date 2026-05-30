//! Graph-Modul — T₂, T₅.
//!
//! Zuständigkeit:
//! - Typisierte attributierte Graphen (L, R, D)
//! - Status-Annotation (SOLID, GHOST, TOMB) — siehe Def. 2.4
//! - Parent-rooted Ghost-ID via SHA-256 — siehe Def. 5.3

use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Graph;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

/// Status eines Elements im Ghost-Graphen.
///
/// Siehe Def. 2.4 im PDF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    /// Element aus der Baseline L_0/R_0, unberührt.
    Solid,
    /// Virtuell hinzugefügt, noch nicht materialisiert.
    Ghost,
    /// **M5.5**: Vorläufiger Tombstone während einer Cascade-
    /// Invalidations-Phase. Wird in der Konsolidierungs-Phase
    /// entweder zu `Solid` zurück (Resurrection durch identische
    /// Ghost-ID einer neuen Rule-Anwendung) oder zu `Tombstone`
    /// (endgültige Invalidation).
    ///
    /// Bleibt matchbar (siehe `Status::is_matchable`), damit eine
    /// neue Rule-Anwendung die Identität wiederbeanspruchen kann.
    TentativeTombstone,
    /// Virtuell gelöscht; für Pattern-Matching unsichtbar,
    /// für Konfliktdetektion und V₁₂-Induktion sichtbar.
    Tombstone,
}

impl Status {
    /// Elemente, die für Pattern-Matching sichtbar sind.
    ///
    /// Solid + Ghost klassisch. **TentativeTombstone** ist bewusst
    /// matchbar (M5.5): während der Konsolidierungs-Phase soll eine
    /// neue Rule-Anwendung das Element via gleicher Ghost-ID
    /// beanspruchen können (Resurrection).
    pub fn is_matchable(&self) -> bool {
        matches!(
            self,
            Status::Solid | Status::Ghost | Status::TentativeTombstone
        )
    }
}

/// Parent-rooted Ghost-ID (SHA-256 Hash).
///
/// Siehe Def. 5.3 im PDF. 32-Byte SHA-256-Hash der Kaskaden-Historie
/// eines Elements.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GhostId([u8; 32]);

impl GhostId {
    /// Erzeugt eine Baseline-ID für ein SOLID-Element aus einem stabilen Namen.
    ///
    /// Benutzt für Wurzel-Elemente in L_0/R_0 — die Rekursionsverankerung
    /// für Def. 5.3.
    pub fn from_baseline(name: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"SOLID\0");
        hasher.update(name.as_bytes());
        Self(hasher.finalize().into())
    }

    /// Erzeugt eine Ghost-ID für ein GHOST-Element, gemäß Def. 5.3:
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
        let mut hasher = Sha256::new();
        hasher.update(b"GHOST\0");
        hasher.update(parent.0);
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
        Self(hasher.finalize().into())
    }

    /// Erzeugt eine Ghost-ID für eine Kante aus Endpunkten und Typinformation.
    pub fn for_edge(
        source: &GhostId,
        target: &GhostId,
        edge_type: &str,
        attrs: &BTreeMap<String, String>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"EDGE\0");
        hasher.update(source.0);
        hasher.update(target.0);
        hasher.update(b"\0");
        hasher.update(edge_type.as_bytes());
        hasher.update(b"\0");
        for (k, v) in attrs {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"\0");
        }
        Self(hasher.finalize().into())
    }

    /// Kurze hexadezimale Darstellung (8 Zeichen) für UI und Logging.
    pub fn short(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }

    /// Volle 64-stellige Hex-Darstellung (32 Bytes). Wird über die
    /// JNI-Grenze gereicht, damit die Pilot-Seite eine cascade-erzeugte
    /// Identität verlustfrei zurückgeben kann (rc7 Rückweg-Bridge).
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// Parst die 64-stellige Hex-Form zurück; `None` bei ungültiger
    /// Länge oder Nicht-Hex-Zeichen.
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

    /// Raw 32-Byte-Hash.
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// ID aus einem opaken externen Identifier (z.\,B.\ EMF-URI-Fragment
    /// oder JDT-Handle-Identifier). Wird an der Integrations-Grenze
    /// benutzt, wo die externe Welt eigene Identitäten trägt, die wir
    /// nicht strukturell re-ableiten können.
    pub fn from_opaque(opaque: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"OPAQUE\0");
        hasher.update(opaque.as_bytes());
        Self(hasher.finalize().into())
    }
}

impl fmt::Debug for GhostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GhostId({})", self.short())
    }
}

/// Typisierte Knoten-Daten im TypedGraph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeData {
    pub id: GhostId,
    pub type_id: String,
    pub attrs: BTreeMap<String, String>,
    pub status: Status,
}

/// Typisierte Kanten-Daten im TypedGraph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeData {
    pub id: GhostId,
    pub type_id: String,
    pub attrs: BTreeMap<String, String>,
    pub status: Status,
}

/// Typisierter attributierter Graph mit Status.
///
/// Realisiert die Ghost-Projektion φ_L (bzw. φ_R) oder den
/// Korrespondenzgraphen D inkl. aller Ghost/Tombstone-Annotationen.
#[derive(Clone, Debug, Default)]
pub struct TypedGraph {
    inner: Graph<NodeData, EdgeData>,
    node_index: HashMap<GhostId, NodeIndex>,
    edge_index: HashMap<GhostId, EdgeIndex>,
    /// Match-Index (F15-Mitigation): kind → Set der Knoten-IDs dieses
    /// Typs. Erlaubt `O(matching_kind_count)` Lookup statt
    /// `O(graph_size)` für jede Pattern-Position. BTreeSet für
    /// deterministische Reihenfolge (canonical μ).
    ///
    /// Status-Filter (Solid/Ghost/TentativeTombstone vs. Tombstone)
    /// erfolgt beim Lookup; der Index trackt nur den Typ, nicht den
    /// Status. Das hält Index-Updates auf Insert-only — kein Update
    /// bei Status-Änderungen nötig.
    kind_index: BTreeMap<String, BTreeSet<GhostId>>,
}

impl TypedGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fügt einen SOLID-Baseline-Knoten hinzu.
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

    /// Fügt einen GHOST-Knoten mit Parent-Referenz hinzu.
    pub fn add_ghost_node(
        &mut self,
        parent: GhostId,
        edge_type: &str,
        type_id: &str,
        attrs: BTreeMap<String, String>,
    ) -> GhostId {
        let id = GhostId::from_parent(&parent, edge_type, type_id, &attrs);
        self.insert_node(NodeData {
            id,
            type_id: type_id.into(),
            attrs,
            status: Status::Ghost,
        });
        id
    }

    /// Fügt einen SOLID-Knoten mit Parent-Referenz hinzu (für Baseline-
    /// Elemente, die strukturell an einem Elternknoten hängen, aber
    /// bereits Teil der initialen Baseline sind — keine Ghosts).
    pub fn add_solid_child_node(
        &mut self,
        parent: GhostId,
        edge_type: &str,
        type_id: &str,
        attrs: BTreeMap<String, String>,
    ) -> GhostId {
        let id = GhostId::from_parent(&parent, edge_type, type_id, &attrs);
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
            // M5: Resurrection — wenn ein TentativeTombstone-Knoten
            // mit identischer ID neu beansprucht wird, setzen wir
            // den Status auf den neuen Status (Solid/Ghost) zurück.
            if let Some(existing) = self.inner.node_weight_mut(*idx) {
                if existing.status == Status::TentativeTombstone {
                    existing.status = node.status;
                }
            }
            // Index ist bereits aktuell (Knoten existiert mit
            // identischem Typ via Ghost-ID-Hash).
            return;
        }
        let id = node.id;
        let kind = node.type_id.clone();
        let idx = self.inner.add_node(node);
        self.node_index.insert(id, idx);
        self.kind_index.entry(kind).or_default().insert(id);
    }

    /// Fügt eine Kante hinzu. Rückgabe: Ghost-ID der Kante oder `None`, wenn
    /// einer der Endpunkte nicht existiert.
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
        let id = GhostId::for_edge(&source, &target, edge_type, &attrs);
        if let Some(existing_idx) = self.edge_index.get(&id) {
            // M5: Resurrection für Edge — falls TentativeTombstone, neu
            // setzen mit gewünschtem Status.
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

    /// Setzt den Status eines Knotens (Tombstone-Marker).
    pub fn set_node_status(&mut self, id: &GhostId, status: Status) -> bool {
        if let Some(idx) = self.node_index.get(id) {
            self.inner[*idx].status = status;
            true
        } else {
            false
        }
    }

    /// Setzt den Status einer Kante (Tombstone-Marker).
    pub fn set_edge_status(&mut self, id: &GhostId, status: Status) -> bool {
        if let Some(idx) = self.edge_index.get(id) {
            self.inner[*idx].status = status;
            true
        } else {
            false
        }
    }

    /// Setzt ein Attribut eines Knotens (Phase 1: überschreibend).
    ///
    /// Phase 3 (geplant): Shadow-Stack mit κ-Tagging für Step-Zurück-Semantik.
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

    /// Liest einen Knoten nach ID.
    pub fn get_node(&self, id: &GhostId) -> Option<&NodeData> {
        self.node_index.get(id).map(|idx| &self.inner[*idx])
    }

    /// Liest eine Kante nach ID.
    pub fn get_edge(&self, id: &GhostId) -> Option<&EdgeData> {
        self.edge_index.get(id).map(|idx| &self.inner[*idx])
    }

    /// Liefert die Endpunkt-IDs einer Kante (Source, Target).
    pub fn edge_endpoints(&self, id: &GhostId) -> Option<(GhostId, GhostId)> {
        let idx = *self.edge_index.get(id)?;
        let (src_idx, tgt_idx) = self.inner.edge_endpoints(idx)?;
        let src_data = self.inner.node_weight(src_idx)?;
        let tgt_data = self.inner.node_weight(tgt_idx)?;
        Some((src_data.id, tgt_data.id))
    }

    /// Anzahl der Knoten (inkl. TOMB).
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Anzahl der Kanten (inkl. TOMB).
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Iteriert über matchbare Knoten eines bestimmten Typs (F15-
    /// Mitigation: indizierter Lookup statt vollständiger Iteration).
    /// Gibt nur Knoten mit `status.is_matchable()` zurück.
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

    /// Iteriert über alle matchbaren Knoten (SOLID + GHOST).
    pub fn matchable_nodes(&self) -> impl Iterator<Item = &NodeData> {
        self.inner
            .node_weights()
            .filter(|n| n.status.is_matchable())
    }

    /// Prüft, ob zwischen `source` und `target` eine matchbare Kante
    /// mit dem angegebenen Typ existiert.
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

    /// rc7 (S): existiert IRGENDEINE matchbare Kante zwischen `a` und `b`
    /// (beliebige Richtung, beliebige Art)? Für das symmetrische
    /// Korrespondenz-Mitgliedschafts-Matching (siehe
    /// `EdgePattern::membership`). Ein Korrespondenz-Knoten verkabelt nur
    /// seine zwei Endpunkte, daher identifiziert „irgendeine Kante" hier
    /// korrekt die Corr-Mitgliedschaft, ohne corrL/corrR zu kennen.
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

    /// Alle matchbaren ausgehenden Kanten eines Knotens mit Zielknoten-ID.
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

    /// Alle matchbaren eingehenden Kanten eines Knotens mit Quellknoten-ID.
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

    /// Alle matchbaren inzidenten Kanten eines Knotens (aus- und eingehend).
    pub fn incident_edges(&self, node: &GhostId) -> Vec<(&EdgeData, GhostId)> {
        let mut out = self.outgoing_edges(node);
        out.extend(self.incoming_edges(node));
        out
    }

    /// Iteriert über alle Knoten, unabhängig vom Status.
    pub fn iter_nodes(&self) -> impl Iterator<Item = &NodeData> {
        self.inner.node_weights()
    }

    /// Iteriert über alle Kanten mit Quell- und Ziel-IDs.
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

    /// Fügt einen Knoten mit bereits berechneter `NodeData` ein.
    /// Ignoriert, falls die ID bereits existiert.
    ///
    /// Muss zusätzlich zum `node_index` auch den `kind_index` pflegen —
    /// `matchable_nodes_by_kind` (Pattern-Matcher-Hot-Path) iteriert
    /// ausschließlich darüber. Ohne diese Eintragung blieben Nodes
    /// unsichtbar für jede Rule, die per LHS-`kind` matched (Regression
    /// von Mai-Commit 8401f0e/F15-Match-Indexing — `insert_node` wurde
    /// korrekt erweitert, `insert_node_data` übersehen).
    pub fn insert_node_data(&mut self, data: NodeData) {
        if !self.node_index.contains_key(&data.id) {
            let id = data.id;
            let kind = data.type_id.clone();
            let idx = self.inner.add_node(data);
            self.node_index.insert(id, idx);
            self.kind_index.entry(kind).or_default().insert(id);
        }
    }

    /// Fügt eine Kante mit bereits berechneter `EdgeData` ein. Setzt
    /// voraus, dass `source` und `target` existieren.
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

    /// Materialisiert den Graphen (Def. 5.1): erzeugt einen neuen
    /// `TypedGraph`, in dem alle TOMB-Elemente entfernt sind und alle
    /// verbleibenden GHOST-Elemente SOLID-Status tragen.
    ///
    /// Gibt Kanten aus, deren Endpunkte in der materialisierten Knoten-
    /// Menge existieren; Endpunkte, die entfielen, fallen mit der Kante.
    pub fn materialize(&self) -> TypedGraph {
        let mut new_g = TypedGraph::new();

        // Knoten kopieren (ohne TOMB), status → SOLID.
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

        // Kanten kopieren (ohne TOMB) — nur wenn beide Endpunkte existieren.
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

    /// rc7 Rückweg-Bridge: für die JNI-Übergabe an die Pilot-Seite (die
    /// die volle ID braucht, um sie wieder einzutragen) muss `hex()` die
    /// 64-stellige Volldarstellung liefern, und `from_hex` muss sie
    /// verlustfrei zurückparsen.
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

    /// Integrations-Test: Mini-UML mit Person, Car, Attributen, Assoziation.
    #[test]
    fn mini_uml_example() {
        let mut g = TypedGraph::new();

        // Klassen
        let person = g.add_baseline_node("Class", "Person", attrs(&[("name", "Person")]));
        let car = g.add_baseline_node("Class", "Car", attrs(&[("name", "Car")]));

        // Attribute
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

        // Kanten
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

        // Assoziation Person ──owns──▶ Car
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

        // Tombstone ein Attribut
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
