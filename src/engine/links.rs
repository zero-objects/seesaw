// Part of `engine` (included by `mod.rs`): the matches that rest on a
// connection. They are not filed under it. `by_element` holds the
// nodes of a match only, and the matches that rest on a connection are
// found from the lists of its two ends when a removed connection asks
// for them.
//
// 2.0.2 filed a match under the id of the connection a→b for every
// link of its pattern (other than a value join) whose positions are
// bound to a and b, whether the graph held that connection or not. The
// id of a connection is a function of its two ends alone
// (`ident::connection`), and a graph never drops a connection, so the
// same matches are found from the ends: those that bind both, with a
// link from the position of the one to the position of the other. They
// come in the order of filing (ascending, a match once per such link).
//
// That leaves the ids the graph at hand does not know. A match can
// rest on one: its pattern reads a direction-free link against the
// direction the graph holds, or the host has replaced the graph since
// the match was recorded (a materialized copy leaves the tombstones
// out). The ends cannot be read off such an id. Those links are filed
// after all, in an index of their own (`Unheld`) that exists only once
// a call has asked for an id the graph does not know.

/// What the engine keeps about the links of its matches beyond
/// `by_element`.
struct Links {
    /// Ids `element_removed` was called with and found no list under.
    /// Such an id may be a connection, and the engine learns its ends
    /// from the graph, which that door does not get. The matches
    /// resting on it are killed by the next call that brings the graph
    /// ([`Engine::settle_removed`]), before that call does anything
    /// else. No match is recorded and none is pulled in between, so the
    /// same matches die as at the call itself.
    removed: Vec<GhostId>,
    /// The links whose connection the graph does not hold. `None`
    /// until a call asks for an id the graph does not know.
    unheld: Option<Unheld>,
    /// Test check of the derivation: the links, filed as 2.0.2 filed
    /// them.
    #[cfg(any(test, seesaw_checks))]
    filed: crate::hash::FxHashMap<GhostId, Ixs>,
}

/// The matches with a link whose connection one graph does not hold,
/// by the id that connection would have: ascending, a match once per
/// such link.
///
/// Built from the whole match store at the first question about an id
/// the graph does not know, and kept up by `record` from then on. It is
/// about one graph: a connection that graph does not hold when a match
/// is filed here may come later (then the id is known and the matches
/// are found from its ends, this entry is not read any more), but one
/// it holds stays. For another graph the index is built anew.
struct Unheld {
    /// The graph the index is about: the number of its type table
    /// (`TableUid`), which every graph and every copy has for itself.
    graph: u64,
    by_id: crate::hash::FxHashMap<GhostId, Ixs>,
}

impl Links {
    fn new() -> Self {
        Self {
            removed: Vec::new(),
            unheld: None,
            #[cfg(any(test, seesaw_checks))]
            filed: Default::default(),
        }
    }
}

/// The links of the match `ix` are not filed under their connections
/// (see the head of this file). Once the index of unheld links exists,
/// the links whose connection the graph does not hold go there. A test
/// build files all of them in the check map.
#[inline(always)]
fn file_links(
    links: &mut Links,
    g: &Graph,
    pattern: &crate::engine::matcher::Pattern,
    refs: &[GhostId],
    ix: u32,
) {
    #[cfg(any(test, seesaw_checks))]
    for l in &pattern.links {
        if l.kind == crate::engine::matcher::LinkKind::SameValue {
            continue;
        }
        if let (Some(a), Some(b)) = (refs.get(l.from), refs.get(l.to)) {
            let e = g
                .connection_id_typed(a, b, pattern.nodes[l.to].typ)
                .unwrap_or_else(|| crate::graph::preview_connection_id(a, b));
            links.filed.entry(e).or_default().push(ix);
        }
    }
    if links.unheld.is_some() {
        file_unheld(links, g, pattern, refs, ix);
    }
}

/// Files the links of the match `ix` whose connection `g` does not
/// hold in the index of unheld links. If the index is about another
/// graph it is dropped: the next question builds it for the graph it
/// comes with.
#[cold]
#[inline(never)]
fn file_unheld(
    links: &mut Links,
    g: &Graph,
    pattern: &crate::engine::matcher::Pattern,
    refs: &[GhostId],
    ix: u32,
) {
    let Some(unheld) = &mut links.unheld else {
        return;
    };
    if unheld.graph != g.types.uid.get() {
        links.unheld = None;
        return;
    }
    for l in &pattern.links {
        if l.kind == crate::engine::matcher::LinkKind::SameValue {
            continue;
        }
        if let (Some(a), Some(b)) = (refs.get(l.from), refs.get(l.to)) {
            if g.connection_id_typed(a, b, pattern.nodes[l.to].typ)
                .is_none()
            {
                unheld
                    .by_id
                    .entry(crate::graph::preview_connection_id(a, b))
                    .or_default()
                    .push(ix);
            }
        }
    }
}

impl Engine<'_> {
    /// The matches filed under `id`: those that bind the node `id`, or
    /// rest on the connection `id`. A copy of the list: what the caller
    /// does with the matches writes the engine.
    fn filed_under(&mut self, g: &Graph, id: &GhostId) -> Ixs {
        if let Some(binding) = self.by_element.get(id) {
            // A node: no connection is a key of `by_element`.
            #[cfg(any(test, seesaw_checks))]
            assert!(
                !self.links.filed.contains_key(id),
                "links filed under the id of a node"
            );
            return binding.clone();
        }
        let resting = match g.connection(id) {
            Some(c) => self.resting_on(&c.source, &c.target),
            // Neither a node nor a connection of the graph.
            None if g.node(id).is_none() => self.unheld_under(g, id),
            // A node no match binds.
            None => Ixs::default(),
        };
        #[cfg(any(test, seesaw_checks))]
        assert_eq!(
            resting.as_slice(),
            self.links.filed.get(id).map_or(&[][..], |f| f.as_slice()),
            "the matches found for a connection differ from the filed ones"
        );
        resting
    }

    /// The matches with a link whose positions are bound to `source`
    /// and `target`, in that direction: ascending, a match once per
    /// such link.
    fn resting_on(&self, source: &GhostId, target: &GhostId) -> Ixs {
        let mut resting = Ixs::default();
        // Such a match binds both ends.
        let (Some(at_source), Some(at_target)) =
            (self.by_element.get(source), self.by_element.get(target))
        else {
            return resting;
        };
        let shorter = if at_source.as_slice().len() <= at_target.as_slice().len() {
            at_source
        } else {
            at_target
        };
        let mut last = None;
        for &mix in shorter.as_slice() {
            // A match that binds a node at two positions stands in the
            // node's list twice, one entry after the other.
            if last == Some(mix) {
                continue;
            }
            last = Some(mix);
            let m = &self.matches[mix as usize];
            let Some(rule) = self.rules.get(m.rule_ix) else {
                continue;
            };
            for l in &rule.pattern.links {
                if l.kind != crate::engine::matcher::LinkKind::SameValue
                    && m.refs.get(l.from) == Some(source)
                    && m.refs.get(l.to) == Some(target)
                {
                    resting.push(mix);
                }
            }
        }
        resting
    }

    /// The matches with a link whose connection has the id `id`, for an
    /// id the graph does not know. Read from the index of unheld links,
    /// which is built here if there is none for this graph: one pass
    /// over the match store, once per graph.
    fn unheld_under(&mut self, g: &Graph, id: &GhostId) -> Ixs {
        let graph = g.types.uid.get();
        if self.links.unheld.as_ref().map(|u| u.graph) != Some(graph) {
            self.links.unheld = Some(self.unheld_of(g));
        }
        self.links
            .unheld
            .as_ref()
            .and_then(|u| u.by_id.get(id))
            .cloned()
            .unwrap_or_default()
    }

    /// Every link of every match whose connection `g` does not hold,
    /// by the id of that connection.
    fn unheld_of(&self, g: &Graph) -> Unheld {
        let mut by_id: crate::hash::FxHashMap<GhostId, Ixs> = Default::default();
        for (mix, m) in self.matches.iter().enumerate() {
            let Some(rule) = self.rules.get(m.rule_ix) else {
                continue;
            };
            for l in &rule.pattern.links {
                if l.kind == crate::engine::matcher::LinkKind::SameValue {
                    continue;
                }
                if let (Some(a), Some(b)) = (m.refs.get(l.from), m.refs.get(l.to)) {
                    if g.connection_id_typed(a, b, rule.pattern.nodes[l.to].typ)
                        .is_none()
                    {
                        by_id
                            .entry(crate::graph::preview_connection_id(a, b))
                            .or_default()
                            .push(mix as u32);
                    }
                }
            }
        }
        Unheld {
            graph: g.types.uid.get(),
            by_id,
        }
    }

    /// `element_removed` found no list under `id`. It may be a
    /// connection: noted for [`Engine::settle_removed`].
    fn removed_unfiled(&mut self, id: &GhostId) {
        self.links.removed.push(*id);
    }

    /// Kills the matches resting on the connections `element_removed`
    /// was called with since the graph was last at hand. Every door
    /// that takes the graph calls this first.
    fn settle_removed(&mut self, g: &Graph) {
        if self.links.removed.is_empty() {
            return;
        }
        let removed = std::mem::take(&mut self.links.removed);
        for id in &removed {
            let resting = self.filed_under(g, id);
            self.kill(resting.as_slice());
        }
    }
}
