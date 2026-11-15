// Part of `engine` (included by `mod.rs` with `opt_d`): the todo is a
// heap.
//
// A heap gives the first candidate and takes a new one, and nothing
// else. What the engine asks besides is built on that:
//
// - Taking a candidate out by its key (a match died): the candidate
//   stays in the heap and its match is struck from `live`. It is
//   dropped when it comes up. `record` enters one candidate per match,
//   so the number of the match names the candidate. A removal names the
//   match it is made for. Without `opt_d` it takes out whatever
//   candidate has the key, which can be that of a later match with the
//   same rule and ref sequence (the earlier one was forgotten by `step`
//   and not killed). Such two matches stand in the same lists of
//   `by_element`, and every removal runs over a whole list, so both are
//   struck in the same call and the todo is the same after it.
// - The first candidate of an admitted rule, and the first from a
//   position on: the candidates before it are taken off the heap and
//   put back. That costs the logarithm per candidate passed over,
//   where the ordered set walks or jumps.
//
// A test build keeps the ordered set of keys next to the heap and
// compares every candidate taken.

/// What a candidate carries besides its key: the number of its match.
type Carried = u32;

/// A candidate taken from the todo.
struct Taken {
    rule_ix: usize,
    refs: Refs,
    carried: Carried,
}

/// A candidate in the heap, 40 bytes. The order is the selection order
/// ([`TodoKey`]) turned round: the first candidate is the greatest.
struct Candidate {
    rank: u64,
    /// [`short_head`] of `refs`.
    head: u64,
    refs: Refs,
    rule_ix: u32,
    /// The match the candidate stands for.
    mix: u32,
}

/// The first 8 bytes of the first id of a ref sequence, big-endian; 0
/// for an empty sequence. Like [`head`], half as long: it orders like
/// the sequence wherever two of them differ, so comparing it before
/// the sequence changes nothing about the order, and a candidate with
/// it moves 40 bytes where the key of the ordered set has 48.
fn short_head(refs: &[GhostId]) -> u64 {
    refs.first().map_or(0, |id| {
        let mut b = [0u8; 8];
        b.copy_from_slice(&id.as_bytes()[..8]);
        u64::from_be_bytes(b)
    })
}

impl Candidate {
    /// Does the candidate stand before the position (`rank`, `refs`)
    /// with the first rule, in selection order? `head` is that of
    /// `refs`.
    fn before(&self, rank: u64, head: u64, refs: &Refs) -> bool {
        (self.rank, self.head, &self.refs) > (rank, head, refs)
    }

    fn taken(self) -> Taken {
        Taken {
            rule_ix: self.rule_ix as usize,
            refs: self.refs,
            carried: self.mix,
        }
    }

    #[cfg(any(test, seesaw_checks))]
    fn key(&self) -> TodoKey {
        todo_key(self.rank, &self.refs, self.rule_ix as usize)
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Rank, head and ref sequence descending, rule ascending, as
        // in `TodoKey`. Two candidates with the same key are never
        // both live; the match number makes the order total.
        self.rank
            .cmp(&other.rank)
            .then_with(|| self.head.cmp(&other.head))
            .then_with(|| self.refs.cmp(&other.refs))
            .then_with(|| other.rule_ix.cmp(&self.rule_ix))
            .then_with(|| other.mix.cmp(&self.mix))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for Candidate {}

/// The todo: the candidates not yet pulled, the first in selection
/// order ([`TodoKey`]) on top.
struct Todo {
    heap: std::collections::BinaryHeap<Candidate>,
    /// One bit per match: its candidate is in the todo.
    live: Vec<u64>,
    /// Working memory of a selection: the candidates it passed over.
    passed: Vec<Candidate>,
    /// Test check: the keys of the live candidates, as the todo is
    /// kept without `opt_d`.
    #[cfg(any(test, seesaw_checks))]
    check: BTreeSet<TodoKey>,
}

impl Todo {
    fn new() -> Self {
        Self {
            heap: std::collections::BinaryHeap::new(),
            live: Vec::new(),
            passed: Vec::new(),
            #[cfg(any(test, seesaw_checks))]
            check: BTreeSet::new(),
        }
    }

    fn is_live(&self, mix: u32) -> bool {
        self.live
            .get((mix >> 6) as usize)
            .is_some_and(|word| word >> (mix & 63) & 1 == 1)
    }

    /// Strikes the match from `live`. `true`: it was live.
    fn strike(&mut self, mix: u32) -> bool {
        match self.live.get_mut((mix >> 6) as usize) {
            Some(word) => {
                let bit = 1u64 << (mix & 63);
                let was = *word & bit != 0;
                *word &= !bit;
                was
            }
            None => false,
        }
    }

    /// Enters the candidate of the match `mix`.
    fn insert(&mut self, rank: u64, refs: &Refs, rule_ix: usize, mix: u32) {
        debug_assert!(rule_ix <= u32::MAX as usize);
        self.heap.push(Candidate {
            rank,
            head: short_head(refs),
            refs: refs.clone(),
            rule_ix: rule_ix as u32,
            mix,
        });
        let word = (mix >> 6) as usize;
        if word >= self.live.len() {
            self.live.resize(word + 1, 0);
        }
        self.live[word] |= 1u64 << (mix & 63);
        #[cfg(any(test, seesaw_checks))]
        self.check.insert(todo_key(rank, refs, rule_ix));
    }

    /// Takes the candidate with this key out, on behalf of the match
    /// `mix` (see the head of this file).
    fn remove(&mut self, rank: u64, refs: &Refs, rule_ix: usize, mix: u32) {
        self.strike(mix);
        #[cfg(any(test, seesaw_checks))]
        self.check.remove(&todo_key(rank, refs, rule_ix));
        #[cfg(not(any(test, seesaw_checks)))]
        let _ = (rank, refs, rule_ix);
    }

    /// Takes the first candidate.
    fn pop_first(&mut self) -> Option<Taken> {
        while let Some(c) = self.heap.pop() {
            if self.strike(c.mix) {
                #[cfg(any(test, seesaw_checks))]
                assert_eq!(
                    self.check.pop_first(),
                    Some(c.key()),
                    "the heap gives another first candidate than the ordered set"
                );
                return Some(c.taken());
            }
        }
        #[cfg(any(test, seesaw_checks))]
        assert!(
            self.check.is_empty(),
            "the heap is empty and the ordered set is not"
        );
        None
    }

    /// Takes the first candidate whose rule is `wanted`. The candidates
    /// before it stay.
    fn take_first(&mut self, mut wanted: impl FnMut(usize) -> bool) -> Option<Taken> {
        let mut found = None;
        while let Some(c) = self.heap.pop() {
            if !self.is_live(c.mix) {
                continue;
            }
            if wanted(c.rule_ix as usize) {
                found = Some(c);
                break;
            }
            self.passed.push(c);
        }
        self.heap.extend(self.passed.drain(..));
        #[cfg(any(test, seesaw_checks))]
        {
            let expected = self.check.iter().find(|k| wanted(k.3)).cloned();
            assert_eq!(
                found.as_ref().map(Candidate::key),
                expected,
                "the heap gives another first admitted candidate than the ordered set"
            );
            if let Some(key) = &expected {
                self.check.remove(key);
            }
        }
        let c = found?;
        self.strike(c.mix);
        Some(c.taken())
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
        let at = short_head(refs);
        let mut found = None;
        while let Some(c) = self.heap.pop() {
            if !self.is_live(c.mix) {
                continue;
            }
            if !c.before(rank, at, refs)
                && wanted(c.rank == rank && c.refs == *refs, c.rule_ix as usize)
            {
                found = Some(c);
                break;
            }
            self.passed.push(c);
        }
        self.heap.extend(self.passed.drain(..));
        #[cfg(any(test, seesaw_checks))]
        {
            let bound = todo_key(rank, refs, 0);
            let expected = self
                .check
                .range(bound.clone()..)
                .find(|k| wanted(k.0 == bound.0 && k.2 == bound.2, k.3))
                .cloned();
            assert_eq!(
                found.as_ref().map(Candidate::key),
                expected,
                "the heap gives another candidate below the bound than the ordered set"
            );
            if let Some(key) = &expected {
                self.check.remove(key);
            }
        }
        let c = found?;
        self.strike(c.mix);
        Some(c.taken())
    }

    /// Number of candidates.
    /// The rule of every candidate in the todo (the struck ones the
    /// heap still holds are left out).
    #[cfg(feature = "opt_b")]
    fn rules(&self) -> impl Iterator<Item = usize> + '_ {
        self.heap
            .iter()
            .filter(|c| self.is_live(c.mix))
            .map(|c| c.rule_ix as usize)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.live
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// The keys of the candidates.
    #[cfg(test)]
    fn keys(&self) -> Vec<TodoKey> {
        self.heap
            .iter()
            .filter(|c| self.is_live(c.mix))
            .map(Candidate::key)
            .collect()
    }
}
