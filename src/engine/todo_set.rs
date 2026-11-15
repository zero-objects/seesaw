// Part of `engine` (included by `mod.rs` without `opt_d` and without
// `opt_e`): the todo is an ordered set of keys.

/// What a candidate carries besides its key: nothing. The match of a
/// pulled candidate is looked up by its key.
#[derive(Clone, Copy)]
struct Carried;

/// A candidate taken from the todo.
struct Taken {
    rule_ix: usize,
    refs: Refs,
    carried: Carried,
}

/// The todo: the candidates not yet pulled, in selection order
/// ([`TodoKey`]).
struct Todo(BTreeSet<TodoKey>);

impl Todo {
    fn new() -> Self {
        Self(BTreeSet::new())
    }

    #[inline(always)]
    fn taken(key: TodoKey) -> Taken {
        Taken {
            rule_ix: key.3,
            refs: (key.2).0,
            carried: Carried,
        }
    }

    /// Enters the candidate of the match `mix`.
    #[inline(always)]
    fn insert(&mut self, rank: u64, refs: &Refs, rule_ix: usize, _mix: u32) {
        self.0.insert(todo_key(rank, refs, rule_ix));
    }

    /// Takes the candidate with this key out, on behalf of the match
    /// `mix`.
    #[inline(always)]
    fn remove(&mut self, rank: u64, refs: &Refs, rule_ix: usize, _mix: u32) {
        self.0.remove(&todo_key(rank, refs, rule_ix));
    }

    /// Takes the first candidate.
    #[inline(always)]
    fn pop_first(&mut self) -> Option<Taken> {
        self.0.pop_first().map(Self::taken)
    }

    /// Takes the first candidate whose rule is `wanted`. The candidates
    /// before it stay.
    #[inline(always)]
    fn take_first(&mut self, mut wanted: impl FnMut(usize) -> bool) -> Option<Taken> {
        let key = self.0.iter().find(|k| wanted(k.3)).cloned();
        if let Some(key) = &key {
            self.0.remove(key);
        }
        key.map(Self::taken)
    }

    /// Takes the first candidate from the position (`rank`, `refs`) on
    /// that is `wanted`. `wanted` is told whether the candidate stands
    /// at that position itself (same rank, same ref sequence), and its
    /// rule.
    #[inline(always)]
    fn take_from(
        &mut self,
        rank: u64,
        refs: &Refs,
        mut wanted: impl FnMut(bool, usize) -> bool,
    ) -> Option<Taken> {
        // From the position on = in the set from the key of the
        // position with the first rule on.
        let bound = todo_key(rank, refs, 0);
        let key = self
            .0
            .range(bound.clone()..)
            .find(|k| wanted(k.0 == bound.0 && k.2 == bound.2, k.3))
            .cloned();
        if let Some(key) = &key {
            self.0.remove(key);
        }
        key.map(Self::taken)
    }

    /// Number of candidates.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }

    /// The keys of the candidates.
    #[cfg(test)]
    fn keys(&self) -> Vec<TodoKey> {
        self.0.iter().cloned().collect()
    }
}
