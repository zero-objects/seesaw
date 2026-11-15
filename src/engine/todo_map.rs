// Part of `engine` (included by `mod.rs` with `opt_e` and without
// `opt_d`): the todo is an ordered map from the key of a candidate to
// the number of its match.

/// What a candidate carries besides its key: the number of its match.
type Carried = u32;

/// A candidate taken from the todo.
struct Taken {
    rule_ix: usize,
    refs: Refs,
    carried: Carried,
}

/// The todo: the candidates not yet pulled, in selection order
/// ([`TodoKey`]), each with the number of its match. The number takes
/// no part in the order.
struct Todo(std::collections::BTreeMap<TodoKey, u32>);

impl Todo {
    fn new() -> Self {
        Self(std::collections::BTreeMap::new())
    }

    fn taken((key, mix): (TodoKey, u32)) -> Taken {
        Taken {
            rule_ix: key.3,
            refs: (key.2).0,
            carried: mix,
        }
    }

    /// Enters the candidate of the match `mix`. `record` enters a key
    /// when no match is filed under it and files `mix` under it, so
    /// the number a key carries is the match filed under the key.
    fn insert(&mut self, rank: u64, refs: &Refs, rule_ix: usize, mix: u32) {
        self.0.insert(todo_key(rank, refs, rule_ix), mix);
    }

    /// Takes the candidate with this key out, on behalf of the match
    /// `mix`.
    fn remove(&mut self, rank: u64, refs: &Refs, rule_ix: usize, _mix: u32) {
        self.0.remove(&todo_key(rank, refs, rule_ix));
    }

    /// Takes the first candidate.
    fn pop_first(&mut self) -> Option<Taken> {
        self.0.pop_first().map(Self::taken)
    }

    /// Takes the first candidate whose rule is `wanted`. The candidates
    /// before it stay.
    fn take_first(&mut self, mut wanted: impl FnMut(usize) -> bool) -> Option<Taken> {
        let key = self.0.keys().find(|k| wanted(k.3)).cloned()?;
        self.0.remove_entry(&key).map(Self::taken)
    }

    /// Takes the first candidate from the position (`rank`, `refs`) on
    /// that is `wanted`. `wanted` is told whether the candidate stands
    /// at that position itself (same rank, same ref sequence), and its
    /// rule.
    fn take_from(
        &mut self,
        rank: u64,
        refs: &Refs,
        mut wanted: impl FnMut(bool, usize) -> bool,
    ) -> Option<Taken> {
        let bound = todo_key(rank, refs, 0);
        let key = self
            .0
            .range(bound.clone()..)
            .map(|(k, _)| k)
            .find(|k| wanted(k.0 == bound.0 && k.2 == bound.2, k.3))
            .cloned()?;
        self.0.remove_entry(&key).map(Self::taken)
    }

    /// Number of candidates.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }

    /// The keys of the candidates.
    #[cfg(test)]
    fn keys(&self) -> Vec<TodoKey> {
        self.0.keys().cloned().collect()
    }
}
