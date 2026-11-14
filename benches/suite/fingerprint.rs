//! Fingerprint of a run: the graph and the cascade in the order it was
//! applied. Two builds of the engine that agree on it produced the same
//! result by the same sequence of steps.

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueResolver};
use seesaw_tgg::ident::GhostId;

fn var(h: &mut blake3::Hasher, bytes: &[u8]) {
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// Hash over all nodes (id, type, status, correspondence flag, value),
/// all connections (id, status) and the cascade entries in order.
pub fn of(g: &Graph, e: &Engine<'_>, v: &dyn ValueResolver) -> String {
    let mut h = blake3::Hasher::new();

    let mut nodes: Vec<_> = g.nodes().collect();
    nodes.sort_by_key(|n| n.id);
    h.update(&(nodes.len() as u64).to_le_bytes());
    let mut connections: Vec<(GhostId, String)> = Vec::new();
    for n in &nodes {
        h.update(&n.id.as_bytes());
        var(&mut h, g.types.name(n.typ).as_bytes());
        var(&mut h, format!("{:?}", n.status).as_bytes());
        h.update(&[u8::from(n.is_corr)]);
        match g.resolve_value(&n.id, v) {
            None => {
                h.update(&[0]);
            }
            Some(s) => {
                h.update(&[1]);
                var(&mut h, s.as_bytes());
            }
        }
        for p in g.parts(&n.id) {
            if p.outgoing {
                let status = g
                    .connection(&p.connection)
                    .map(|c| format!("{:?}", c.status))
                    .unwrap_or_default();
                connections.push((p.connection, status));
            }
        }
    }
    connections.sort();
    h.update(&(connections.len() as u64).to_le_bytes());
    for (id, status) in &connections {
        h.update(&id.as_bytes());
        var(&mut h, status.as_bytes());
    }

    h.update(&(e.cascade.len() as u64).to_le_bytes());
    for entry in &e.cascade {
        h.update(&(entry.rule_ix as u64).to_le_bytes());
        h.update(&entry.rank.to_le_bytes());
        for list in [&entry.refs, &entry.created, &entry.created_edges] {
            h.update(&(list.len() as u64).to_le_bytes());
            for id in list {
                h.update(&id.as_bytes());
            }
        }
    }
    h.finalize().to_hex()[..16].to_string()
}
