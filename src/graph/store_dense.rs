// Where the participants of a graph live, with the feature
// `low_memory`:
// the map holds one number per participant id, and the nodes and the
// connections stand in two vectors in insertion order. A map entry is
// 36 bytes instead of the 152 to 160 of a slot that holds the
// participant itself, so the table is a quarter of the size and moves
// a quarter of the bytes when it grows.
//
// The interface is that of `store_map`, and so is everything a caller
// can observe: the order of the two iterations "in no particular
// order" is the order of the map's keys, which depends on the ids and
// on the sequence they were inserted in, not on what the map holds
// for them.

/// Bit of a number in the index that marks a connection. Without it
/// the number is the place of a node in `nodes`, with it (masked out)
/// the place of a connection in `connections`.
const CONNECTION: u32 = 1 << 31;

/// A node with its participations.
#[derive(Clone, Debug)]
struct Entry {
    node: Node,
    /// Participations, sorted (deterministic order, O(log) access by
    /// counterpart type via range).
    parts: Parts,
}

/// The participants of a graph. Insert-only.
#[derive(Debug, Clone, Default)]
struct Store {
    /// Participant id → its number (see [`CONNECTION`]).
    index: FxHashMap<GhostId, u32>,
    /// The nodes in insertion order (deterministic iteration for
    /// dumps/fingerprints).
    nodes: Vec<Entry>,
    /// The connections in insertion order.
    connections: Vec<Connection>,
    /// Type index for seed scans (realization, §3b.2): per type id the
    /// numbers of its nodes in insertion order, insert-only; status is
    /// filtered on read like everywhere else.
    by_type: Vec<Vec<u32>>,
}

impl Store {
    /// The place of the node `id` in `nodes`.
    #[inline]
    fn node_at(&self, id: &GhostId) -> Option<usize> {
        match self.index.get(id) {
            Some(&n) if n & CONNECTION == 0 => Some(n as usize),
            _ => None,
        }
    }

    /// The place of the connection `id` in `connections`.
    #[inline]
    fn connection_at(&self, id: &GhostId) -> Option<usize> {
        match self.index.get(id) {
            Some(&n) if n & CONNECTION != 0 => Some((n & !CONNECTION) as usize),
            _ => None,
        }
    }

    #[inline]
    fn node(&self, id: &GhostId) -> Option<&Node> {
        self.node_at(id).map(|at| &self.nodes[at].node)
    }

    #[inline]
    fn node_mut(&mut self, id: &GhostId) -> Option<&mut Node> {
        self.node_at(id).map(|at| &mut self.nodes[at].node)
    }

    /// A node with its participation list, in one access.
    #[inline]
    fn node_parts(&self, id: &GhostId) -> Option<(&Node, &Parts)> {
        self.node_at(id).map(|at| {
            let e = &self.nodes[at];
            (&e.node, &e.parts)
        })
    }

    #[inline]
    fn parts(&self, id: &GhostId) -> Option<&Parts> {
        self.node_at(id).map(|at| &self.nodes[at].parts)
    }

    #[inline]
    fn parts_mut(&mut self, id: &GhostId) -> Option<&mut Parts> {
        self.node_at(id).map(|at| &mut self.nodes[at].parts)
    }

    #[inline]
    fn connection(&self, id: &GhostId) -> Option<&Connection> {
        self.connection_at(id).map(|at| &self.connections[at])
    }

    #[inline]
    fn connection_mut(&mut self, id: &GhostId) -> Option<&mut Connection> {
        self.connection_at(id).map(|at| &mut self.connections[at])
    }

    /// Two different nodes with their participation lists, for writing,
    /// in one access each. `None`: one of them is not a node.
    #[inline]
    fn two_nodes_mut(&mut self, a: &GhostId, b: &GhostId) -> Option<[(&mut Node, &mut Parts); 2]> {
        let (a, b) = (self.node_at(a)?, self.node_at(b)?);
        let [a, b] = self.nodes.get_disjoint_mut([a, b]).ok()?;
        Some([(&mut a.node, &mut a.parts), (&mut b.node, &mut b.parts)])
    }

    /// Writes a node into its place. One that exists stays; a
    /// tentatively retracted one takes the status of `node` (M5:
    /// resurrection).
    fn insert_node(&mut self, node: Node) {
        use std::collections::hash_map::Entry as Slot;
        match self.index.entry(node.id) {
            Slot::Vacant(v) => {
                let at = self.nodes.len();
                assert!(at < CONNECTION as usize, "more than 2^31 nodes");
                v.insert(at as u32);
                let typ = node.typ.0 as usize;
                if self.by_type.len() <= typ {
                    self.by_type.resize_with(typ + 1, Vec::new);
                }
                self.by_type[typ].push(at as u32);
                self.nodes.push(Entry {
                    node,
                    parts: Parts::default(),
                });
            }
            Slot::Occupied(o) => {
                let n = *o.get();
                assert!(
                    n & CONNECTION == 0,
                    "a node identity is not a connection identity"
                );
                // M5: resurrection.
                let existing = &mut self.nodes[n as usize].node;
                if existing.status == Status::TentativeTombstone {
                    existing.status = node.status;
                }
            }
        }
    }

    /// Writes the connection `id` into its place. `true`: it is new.
    /// `false`: it existed; a tentatively retracted one is revived
    /// (M5).
    fn insert_connection(
        &mut self,
        id: GhostId,
        source: GhostId,
        target: GhostId,
        status: Status,
    ) -> bool {
        use std::collections::hash_map::Entry as Slot;
        match self.index.entry(id) {
            Slot::Vacant(v) => {
                let at = self.connections.len();
                assert!(at < CONNECTION as usize, "more than 2^31 connections");
                v.insert(at as u32 | CONNECTION);
                self.connections.push(Connection {
                    id,
                    source,
                    target,
                    status,
                });
                true
            }
            Slot::Occupied(o) => {
                let n = *o.get();
                assert!(
                    n & CONNECTION != 0,
                    "a connection identity is not a node identity"
                );
                let c = &mut self.connections[(n & !CONNECTION) as usize];
                if c.status == Status::TentativeTombstone {
                    c.status = status;
                }
                false
            }
        }
    }

    /// Every node, in the order of the map (no particular order).
    fn nodes(&self) -> impl Iterator<Item = &Node> + '_ {
        self.index
            .values()
            .filter(|n| **n & CONNECTION == 0)
            .map(|n| &self.nodes[*n as usize].node)
    }

    /// Every connection, in the order of the map (no particular order).
    fn connections(&self) -> impl Iterator<Item = &Connection> + '_ {
        self.index
            .values()
            .filter(|n| **n & CONNECTION != 0)
            .map(|n| &self.connections[(*n & !CONNECTION) as usize])
    }

    /// Nodes in insertion order.
    fn nodes_in_order(&self) -> impl Iterator<Item = &Node> + '_ {
        self.nodes.iter().map(|e| &e.node)
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The nodes of a type in insertion order, whatever their status.
    fn of_type(&self, typ: TypeId) -> impl Iterator<Item = &Node> + '_ {
        self.by_type
            .get(typ.0 as usize)
            .into_iter()
            .flatten()
            .map(move |at| &self.nodes[*at as usize].node)
    }

    fn type_count(&self, typ: TypeId) -> usize {
        self.by_type.get(typ.0 as usize).map_or(0, Vec::len)
    }

    /// Room for `nodes` further nodes and `connections` further
    /// connections.
    fn reserve(&mut self, nodes: usize, connections: usize) {
        self.index.reserve(nodes + connections);
        self.nodes.reserve(nodes);
        self.connections.reserve(connections);
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    /// The sizes the memory estimate of the store rests on.
    #[test]
    fn sizes_of_the_store() {
        assert_eq!(std::mem::size_of::<(GhostId, u32)>(), 36);
        assert_eq!(std::mem::size_of::<Entry>(), 120);
        assert_eq!(std::mem::size_of::<Connection>(), 97);
    }
}
