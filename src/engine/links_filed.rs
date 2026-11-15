// Part of `engine` (included by `mod.rs` without `opt_a`): the links of
// a match are filed in `by_element` under the id of their connection,
// next to the nodes of the match.

/// What the registry of links keeps beyond `by_element`: nothing.
struct Links;

impl Links {
    fn new() -> Self {
        Links
    }
}

/// Files the match `ix` under the connections its pattern requires.
///
/// The match's LINKS belong in the registry too, not only its nodes. A
/// match rests on the edges its pattern requires, so removing one has
/// to kill it exactly as removing a node does.
///
/// Until now `by_element` held nodes only, and a pure edge delta (an
/// owner or super change) killed no match at all. That is why
/// `link_removed` exists as an over-approximation, and why the
/// ecore2sql ledger cannot express a super change as the edge delta it
/// is and replaces the whole node instead — which leaves a leaf owned
/// by two carriers for the duration of the move. Sandra ruled that
/// state out on 2026-08-12: a delta changes or deletes, and a temporary
/// multi-ownership makes no technical sense.
#[inline(always)]
fn file_links(
    by_element: &mut crate::hash::FxHashMap<GhostId, Ixs>,
    _links: &mut Links,
    g: &Graph,
    pattern: &crate::engine::matcher::Pattern,
    refs: &[GhostId],
    ix: u32,
) {
    for l in &pattern.links {
        // SameValue is a join over two leaf values, not an edge —
        // there is no connection in the graph to index.
        if l.kind == crate::engine::matcher::LinkKind::SameValue {
            continue;
        }
        if let (Some(a), Some(b)) = (refs.get(l.from), refs.get(l.to)) {
            // The id of the connection a→b: read from the graph
            // where the connection exists, derived otherwise (a
            // direction-free link the graph holds the other way
            // round). Both are the same id.
            // The type of `b` is that of its position.
            let e = g
                .connection_id_typed(a, b, pattern.nodes[l.to].typ)
                .unwrap_or_else(|| crate::graph::preview_connection_id(a, b));
            by_element.entry(e).or_default().push(ix);
        }
    }
}

impl Engine<'_> {
    /// The matches filed under `id`: those that bind the node `id`, or
    /// rest on the connection `id`. A copy of the list (in place up to
    /// [`IXS_INLINE`] entries): what the caller does with the matches
    /// writes the engine.
    #[inline(always)]
    fn filed_under(&self, _g: &Graph, id: &GhostId) -> Ixs {
        self.by_element.get(id).cloned().unwrap_or_default()
    }

    /// `element_removed` found no list under `id`: no match binds it or
    /// rests on it.
    #[inline(always)]
    fn removed_unfiled(&mut self, _id: &GhostId) {}

    /// `element_removed` leaves nothing open.
    #[inline(always)]
    fn settle_removed(&mut self, _g: &Graph) {}
}
