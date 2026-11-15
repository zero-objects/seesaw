// Part of `engine` (included by `mod.rs` with `opt_a`): the links of a
// match are not filed. `by_element` holds the nodes of a match only,
// and the matches that rest on a connection are found from the lists of
// its two ends when a removed connection asks for them.
//
// Without `opt_a`, `record` files a match under the id of the
// connection a→b for every link of its pattern (other than a value
// join) whose positions are bound to a and b, whether the graph holds
// that connection or not. The id of a connection is a function of its
// two ends alone (`ident::connection`), and the graph never drops a
// connection, so the same matches are found from the ends: those that
// bind both, with a link from the position of the one to the position
// of the other. They come in the order of filing (ascending, a match
// once per such link).

/// What the registry of links keeps beyond `by_element`.
struct Links {
    /// Ids `element_removed` was called with and found no list under.
    /// Such an id may be a connection, and the engine learns its ends
    /// from the graph, which that door does not get. The matches
    /// resting on it are killed by the next call that brings the graph
    /// ([`Engine::settle_removed`]), before that call does anything
    /// else. No match is recorded and none is pulled in between, so the
    /// same matches die as at the call itself.
    removed: Vec<GhostId>,
    /// Test check of the derivation: the links, filed as without
    /// `opt_a`.
    #[cfg(any(test, seesaw_checks))]
    filed: crate::hash::FxHashMap<GhostId, Ixs>,
}

impl Links {
    fn new() -> Self {
        Self {
            removed: Vec::new(),
            #[cfg(any(test, seesaw_checks))]
            filed: Default::default(),
        }
    }
}

/// The links of the match `ix` are not filed (see the head of this
/// file). A test build files them in the check map.
#[inline(always)]
fn file_links(
    _by_element: &mut crate::hash::FxHashMap<GhostId, Ixs>,
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
    #[cfg(not(any(test, seesaw_checks)))]
    let _ = (links, g, pattern, refs, ix);
}

impl Engine<'_> {
    /// The matches filed under `id`: those that bind the node `id`, or
    /// rest on the connection `id`. A copy of the list: what the caller
    /// does with the matches writes the engine.
    fn filed_under(&self, g: &Graph, id: &GhostId) -> Ixs {
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
            // Neither a node nor a connection of the graph. A match
            // can still be filed under such an id without `opt_a`: the
            // id of a connection its pattern reads against the
            // direction the graph holds. A host has no reason to name
            // one, and the ends cannot be read off the id, so every
            // match is asked.
            None if g.node(id).is_none() => self.resting_on_unknown(id),
            // A node no match binds.
            None => Ixs::default(),
        };
        #[cfg(any(test, seesaw_checks))]
        assert_eq!(
            resting.as_slice(),
            self.links.filed.get(id).map_or(&[][..], |f| f.as_slice()),
            "the matches found from the ends of a connection differ from the filed ones"
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
    /// id the graph does not know: every match is asked, and the id of
    /// each of its links derived. Cost in the number of matches; see
    /// [`Engine::filed_under`] for when this runs.
    fn resting_on_unknown(&self, id: &GhostId) -> Ixs {
        let mut resting = Ixs::default();
        for (mix, m) in self.matches.iter().enumerate() {
            let Some(rule) = self.rules.get(m.rule_ix) else {
                continue;
            };
            for l in &rule.pattern.links {
                if l.kind == crate::engine::matcher::LinkKind::SameValue {
                    continue;
                }
                if let (Some(a), Some(b)) = (m.refs.get(l.from), m.refs.get(l.to)) {
                    if crate::graph::preview_connection_id(a, b) == *id {
                        resting.push(mix as u32);
                    }
                }
            }
        }
        resting
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
