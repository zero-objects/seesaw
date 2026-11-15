//! Capacity hints of a host.
//!
//! A host that has run a rule set before knows how large graph and
//! engine get per node of its input model. The factors here were read
//! off one run of each workload without hints: `SEESAW_SIZES=1` makes
//! a cell print what its stores reached, per 1000 nodes of the input.
//!
//! The hints are given where the graph and the engine are created, as
//! a host would give them: before the model is read in and before the
//! first delta. That is before the measured section.

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::Graph;

/// What a run adds per 1000 nodes of the input model.
pub struct Factors {
    /// Nodes the cascade creates.
    pub nodes: usize,
    /// Connections the cascade creates.
    pub connections: usize,
    /// Matches recorded.
    pub matches: usize,
    /// Nodes that stand in a match.
    pub elements: usize,
    /// Applied steps.
    pub steps: usize,
}

/// Families to Persons, forward (read off `f2p/add/f20000`). Per
/// family the model has 13 nodes and the cascade applies four steps,
/// each of which creates four nodes and five connections; twelve
/// matches are recorded over 37 nodes and connections.
pub const F2P: Factors = Factors {
    nodes: 1231,
    connections: 1539,
    matches: 924,
    elements: 2847,
    steps: 308,
};

/// Language pack, forward (read off `langpack/add/k700/n20000`; the
/// sizes `k700/n2000` and `k50/n2000` lie within 1 % of it).
pub const LANGPACK: Factors = Factors {
    nodes: 1375,
    connections: 1875,
    matches: 1000,
    elements: 4286,
    steps: 500,
};

/// Language pack, backward (read off `langpack/backward/k700/n20000`).
pub const LANGPACK_BACKWARD: Factors = Factors {
    nodes: 1571,
    connections: 2142,
    matches: 571,
    elements: 3621,
    steps: 571,
};

/// Room in `g` for an input model of `input_nodes` nodes and
/// `input_connections` connections and for what the cascade adds.
pub fn graph(g: &mut Graph, input_nodes: usize, input_connections: usize, f: &Factors) {
    g.reserve(
        input_nodes + input_nodes * f.nodes / 1000,
        input_connections + input_nodes * f.connections / 1000,
    );
}

/// Room in `e` for a run over an input model of `input_nodes` nodes.
pub fn engine(e: &mut Engine<'_>, input_nodes: usize, f: &Factors) {
    e.reserve(seesaw_tgg::engine::Sizes {
        matches: input_nodes * f.matches / 1000,
        elements: input_nodes * f.elements / 1000,
        products: input_nodes * f.nodes / 1000,
        steps: input_nodes * f.steps / 1000,
    });
}

/// With `SEESAW_SIZES=1`: what a cell's stores reached per 1000 nodes
/// of the input, in the shape of [`Factors`].
pub fn report(cell: &str, g: &Graph, e: &Engine<'_>, input_nodes: usize) {
    if std::env::var_os("SEESAW_SIZES").is_none() {
        return;
    }
    let per = |n: usize| (n * 1000).div_ceil(input_nodes);
    let sizes = e.sizes();
    let connections: usize = e.cascade.iter().map(|c| c.created_edges.len()).sum();
    eprintln!(
        "sizes {cell}: input_nodes={input_nodes} nodes: {}, connections: {}, matches: {}, \
         elements: {}, steps: {}, products: {} (absolute: {sizes:?}, nodes={})",
        per(g.node_count() - input_nodes),
        per(connections),
        per(sizes.matches),
        per(sizes.elements),
        per(sizes.steps),
        per(sizes.products),
        g.node_count(),
    );
}
