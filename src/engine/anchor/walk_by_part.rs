// `AnchorIndex::walk` with the feature `opt_m`: a prefix-tree node with
// many groups is served by one pass over the participation list.

/// A prefix-tree node with fewer groups than this asks for one range
/// per group, as without the feature.
const PASS_FROM_GROUPS: usize = 4;

/// The pass is taken while the list has at most this many entries per
/// group. A longer list (a hub under a node with few groups) is
/// cheaper to ask for its ranges: a range costs the logarithm of the
/// list, the pass its length.
const PASS_ENTRIES_PER_GROUP: usize = 8;

impl AnchorIndex {
    /// Marks every prefix-tree node below `at` that a walk from a graph
    /// node realises without entering a node `skip` names. `parts` is
    /// the participation list of the node the walk stands at.
    ///
    /// The list is ordered by the type of the counterpart first. One
    /// pass over it meets each type once, as a run of entries, and the
    /// groups of that type are found in the node's `by_typ`. That
    /// visits the pairs (group, participation of the group's type) the
    /// ranges visit, per participation instead of per group.
    fn walk<'g>(
        &self,
        g: &'g Graph,
        v: &dyn ValueResolver,
        parts: &'g Parts,
        at: u32,
        st: &mut Walk,
        skip: &dyn Fn(&GhostId) -> bool,
    ) {
        trace!(anchor_walk_visits);
        let node = &self.nodes[at as usize];
        let groups = &node.groups;
        if groups.len() < PASS_FROM_GROUPS || parts.len() > groups.len() * PASS_ENTRIES_PER_GROUP {
            for group in groups {
                let valued = !group.equals.is_empty() || !group.other.is_empty();
                for p in parts.of_type(group.typ) {
                    self.pair(g, v, group, valued, p, st, skip);
                }
            }
            return;
        }
        debug_assert_eq!(node.by_typ.len(), groups.len(), "groups not filed by type");
        // Test check: the pass visits as many pairs as the ranges hold.
        // A pair is a group and a participation of the group's type,
        // and neither way visits one twice, so equal counts are equal
        // sets.
        #[cfg(any(test, seesaw_checks))]
        let mut pairs = 0usize;
        let mut typ: Option<TypeId> = None;
        let mut of: &[(TypeId, u32)] = &[];
        for p in parts.iter() {
            if typ != Some(p.other_typ) {
                typ = Some(p.other_typ);
                let from = node.by_typ.partition_point(|(t, _)| *t < p.other_typ);
                let len = node.by_typ[from..].partition_point(|(t, _)| *t == p.other_typ);
                of = &node.by_typ[from..from + len];
            }
            for &(_, gi) in of {
                let group = &groups[gi as usize];
                debug_assert_eq!(group.typ, p.other_typ);
                #[cfg(any(test, seesaw_checks))]
                {
                    pairs += 1;
                }
                let valued = !group.equals.is_empty() || !group.other.is_empty();
                self.pair(g, v, group, valued, p, st, skip);
            }
        }
        #[cfg(any(test, seesaw_checks))]
        assert_eq!(
            pairs,
            groups
                .iter()
                .map(|group| parts.of_type(group.typ).count())
                .sum::<usize>(),
            "the pass over the participation list visits other pairs than the ranges"
        );
    }

    /// Files the groups of every prefix-tree node by type. Called once,
    /// when the tree is complete.
    fn index_groups(&mut self) {
        for node in &mut self.nodes {
            node.by_typ = node
                .groups
                .iter()
                .enumerate()
                .map(|(gi, group)| (group.typ, gi as u32))
                .collect();
            node.by_typ.sort_unstable();
        }
    }
}
