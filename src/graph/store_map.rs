// Where the participants of a graph live: one map from participant
// id to slot, the nodes' insertion order and the type index beside
// it. This is the store without the feature `low_memory`; with it the
// module `store_dense` takes its place behind the same interface.

/// Slot of one participant in the global map.
///
/// A participant is a node (with its participations) or a connection,
/// never both: the identities of the two come from different
/// derivations. The slot holds the one it is, not room for both.
#[derive(Clone, Debug, Default)]
enum Slot {
    /// Created by `entry`, about to be filled.
    #[default]
    Empty,
    Node {
        node: Node,
        /// Participations, sorted (deterministic order, O(log) access
        /// by counterpart type via range).
        parts: Parts,
    },
    Connection(Connection),
}

impl Slot {
    #[inline(always)]
    fn node(&self) -> Option<&Node> {
        match self {
            Self::Node { node, .. } => Some(node),
            _ => None,
        }
    }

    #[inline(always)]
    fn node_mut(&mut self) -> Option<&mut Node> {
        match self {
            Self::Node { node, .. } => Some(node),
            _ => None,
        }
    }

    #[inline(always)]
    fn connection(&self) -> Option<&Connection> {
        match self {
            Self::Connection(c) => Some(c),
            _ => None,
        }
    }

    #[inline(always)]
    fn connection_mut(&mut self) -> Option<&mut Connection> {
        match self {
            Self::Connection(c) => Some(c),
            _ => None,
        }
    }

    #[inline(always)]
    fn parts(&self) -> Option<&Parts> {
        match self {
            Self::Node { parts, .. } => Some(parts),
            _ => None,
        }
    }

    #[inline(always)]
    fn parts_mut(&mut self) -> Option<&mut Parts> {
        match self {
            Self::Node { parts, .. } => Some(parts),
            _ => None,
        }
    }
}

/// The participants of a graph. Insert-only.
#[derive(Debug, Clone, Default)]
struct Store {
    map: FxHashMap<GhostId, Slot>,
    /// Insertion order of the nodes (deterministic iteration for
    /// dumps/fingerprints).
    node_order: Vec<GhostId>,
    /// Type index for seed scans (realization, §3b.2): ids per type in
    /// insertion order, insert-only; status is filtered on read like
    /// everywhere else.
    by_type: FxHashMap<TypeId, Vec<GhostId>>,
}

impl Store {
    #[inline(always)]
    fn node(&self, id: &GhostId) -> Option<&Node> {
        self.map.get(id).and_then(Slot::node)
    }

    #[inline(always)]
    fn node_mut(&mut self, id: &GhostId) -> Option<&mut Node> {
        self.map.get_mut(id).and_then(Slot::node_mut)
    }

    /// A node with its participation list, in one access.
    #[inline(always)]
    fn node_parts(&self, id: &GhostId) -> Option<(&Node, &Parts)> {
        match self.map.get(id)? {
            Slot::Node { node, parts } => Some((node, parts)),
            _ => None,
        }
    }

    #[inline(always)]
    fn parts(&self, id: &GhostId) -> Option<&Parts> {
        self.map.get(id).and_then(Slot::parts)
    }

    #[inline(always)]
    fn parts_mut(&mut self, id: &GhostId) -> Option<&mut Parts> {
        self.map.get_mut(id).and_then(Slot::parts_mut)
    }

    #[inline(always)]
    fn connection(&self, id: &GhostId) -> Option<&Connection> {
        self.map.get(id).and_then(Slot::connection)
    }

    #[inline(always)]
    fn connection_mut(&mut self, id: &GhostId) -> Option<&mut Connection> {
        self.map.get_mut(id).and_then(Slot::connection_mut)
    }

    /// Two different nodes with their participation lists, for writing,
    /// in one access each. `None`: one of them is not a node.
    #[inline(always)]
    fn two_nodes_mut(&mut self, a: &GhostId, b: &GhostId) -> Option<[(&mut Node, &mut Parts); 2]> {
        let [a, b] = self.map.get_disjoint_mut([a, b]);
        let (
            Some(Slot::Node {
                node: a,
                parts: a_parts,
            }),
            Some(Slot::Node {
                node: b,
                parts: b_parts,
            }),
        ) = (a, b)
        else {
            return None;
        };
        Some([(a, a_parts), (b, b_parts)])
    }

    /// Writes a node into its slot. One that exists stays; a
    /// tentatively retracted one takes the status of `node` (M5:
    /// resurrection).
    fn insert_node(&mut self, node: Node) {
        use std::collections::hash_map::Entry;
        // A new node is written into its slot once (no default slot
        // first).
        match self.map.entry(node.id) {
            Entry::Vacant(v) => {
                self.node_order.push(node.id);
                self.by_type.entry(node.typ).or_default().push(node.id);
                v.insert(Slot::Node {
                    node,
                    parts: Parts::default(),
                });
            }
            Entry::Occupied(mut o) => match o.get_mut() {
                Slot::Node { node: existing, .. } => {
                    // M5: resurrection.
                    if existing.status == Status::TentativeTombstone {
                        existing.status = node.status;
                    }
                }
                slot @ Slot::Empty => {
                    self.node_order.push(node.id);
                    self.by_type.entry(node.typ).or_default().push(node.id);
                    *slot = Slot::Node {
                        node,
                        parts: Parts::default(),
                    };
                }
                Slot::Connection(_) => {
                    unreachable!("a node identity is not a connection identity")
                }
            },
        }
    }

    /// Writes the connection `id` into its slot. `true`: it is new.
    /// `false`: it existed; a tentatively retracted one is revived
    /// (M5).
    fn insert_connection(
        &mut self,
        id: GhostId,
        source: GhostId,
        target: GhostId,
        status: Status,
    ) -> bool {
        let connection = Connection {
            id,
            source,
            target,
            status,
        };
        match self.map.entry(id) {
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(Slot::Connection(connection));
            }
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let slot = o.get_mut();
                if let Some(c) = slot.connection_mut() {
                    if c.status == Status::TentativeTombstone {
                        c.status = status;
                    }
                    return false;
                }
                debug_assert!(matches!(slot, Slot::Empty));
                *slot = Slot::Connection(connection);
            }
        }
        true
    }

    /// Every node, in the order of the map (no particular order).
    fn nodes(&self) -> impl Iterator<Item = &Node> + '_ {
        self.map.values().filter_map(Slot::node)
    }

    /// Every connection, in the order of the map (no particular order).
    fn connections(&self) -> impl Iterator<Item = &Connection> + '_ {
        self.map.values().filter_map(Slot::connection)
    }

    /// Nodes in insertion order.
    fn nodes_in_order(&self) -> impl Iterator<Item = &Node> + '_ {
        self.node_order
            .iter()
            .filter_map(|id| self.map.get(id).and_then(Slot::node))
    }

    fn node_count(&self) -> usize {
        self.node_order.len()
    }

    /// The nodes of a type in insertion order, whatever their status.
    fn of_type(&self, typ: TypeId) -> impl Iterator<Item = &Node> + '_ {
        self.by_type
            .get(&typ)
            .into_iter()
            .flatten()
            .filter_map(move |id| self.node(id))
    }

    fn type_count(&self, typ: TypeId) -> usize {
        self.by_type.get(&typ).map_or(0, Vec::len)
    }

    /// Room for `nodes` further nodes and `connections` further
    /// connections.
    fn reserve(&mut self, nodes: usize, connections: usize) {
        self.map.reserve(nodes + connections);
        self.node_order.reserve(nodes);
    }
}
