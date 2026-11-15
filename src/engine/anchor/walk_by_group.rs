// `AnchorIndex::walk` without the feature `opt_m`: one participation
// range per group.

impl AnchorIndex {
    /// Marks every prefix-tree node below `at` that a walk from a graph
    /// node realises without entering a node `skip` names. `parts` is
    /// the participation list of the node the walk stands at.
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
        for group in &self.nodes[at as usize].groups {
            let valued = !group.equals.is_empty() || !group.other.is_empty();
            for p in parts.of_type(group.typ) {
                self.pair(g, v, group, valued, p, st, skip);
            }
        }
    }

    /// Nothing to prepare for this form of the walk.
    #[inline(always)]
    fn index_groups(&mut self) {}
}
