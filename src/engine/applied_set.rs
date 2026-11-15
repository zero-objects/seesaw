// Part of `engine` (included by `mod.rs` without `opt_e`): what `step`
// knows of a pulled candidate is looked up by its key, the rule and the
// ref sequence.

/// What the graph says about the anchors of the correspondences a
/// pulled candidate would establish.
enum Recognised {
    /// An anchor is tentatively retracted.
    Tentative,
    /// Every anchor carries its correspondence already
    /// ([`is_translated`]).
    Translated,
    /// Neither.
    Open,
}

#[inline(always)]
fn recognise(
    g: &Graph,
    rule: &DirectedRule,
    refs: &[GhostId],
    names: &mut RuleNames<'_>,
) -> Recognised {
    if rule
        .corr_recognition
        .iter()
        .any(|(_, pos, _)| self_node_tt(g, &refs[*pos]))
    {
        Recognised::Tentative
    } else if is_translated(g, rule, refs, names) {
        Recognised::Translated
    } else {
        Recognised::Open
    }
}

/// Duplicate memory: ref sequences already applied per rule.
struct Applied(std::collections::HashSet<(usize, Refs), crate::hash::FxBuildHasher>);

impl Applied {
    fn new() -> Self {
        Self(Default::default())
    }

    /// Makes room for `more` entries.
    #[cfg(feature = "opt_b")]
    fn reserve(&mut self, more: usize) {
        self.0.reserve(more);
    }

    /// Is the candidate (`rule_ix`, `refs`) applied already?
    #[inline(always)]
    fn contains(&self, rule_ix: usize, refs: &Refs, _carried: Carried) -> bool {
        self.0.contains(&(rule_ix, refs.clone()))
    }

    /// Notes the pulled candidate as applied.
    #[inline(always)]
    fn insert(&mut self, rule_ix: usize, refs: &Refs, _carried: Carried) {
        self.0.insert((rule_ix, refs.clone()));
    }

    /// [`Applied::insert`] for a ref sequence the caller is done with.
    #[inline(always)]
    fn insert_owned(&mut self, rule_ix: usize, refs: Refs, _carried: Carried) {
        self.0.insert((rule_ix, refs));
    }

    /// Forgets that `key` was applied (first half; the second is
    /// [`Applied::forget_match`]).
    #[inline(always)]
    fn forget(&mut self, key: &(usize, Refs)) {
        self.0.remove(key);
    }

    /// Second half of forgetting a key: `current` is the match that
    /// was filed under it.
    #[inline(always)]
    fn forget_match(&mut self, _current: Option<u32>) {}

    /// Is nothing applied?
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Engine<'_> {
    /// The match of the pulled candidate (`rule_ix`, `refs`): the one
    /// filed under its key.
    #[inline(always)]
    fn pulled_match(&self, rule_ix: usize, refs: &Refs, _carried: Carried) -> Option<u32> {
        self.by_key.get(&(rule_ix, refs.clone())).copied()
    }
}
