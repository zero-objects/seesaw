//! The [`AnchorIndex`] an engine holds when no host handed one in.
//!
//! One private interface ([`Own`]), two implementations, chosen by the
//! feature `opt_i`:
//!
//! - Without it the engine builds an index for its rule slice and owns
//!   it.
//! - With it the indices built this way are kept for the process, found
//!   again by what an index reads from the rules (pattern nodes with
//!   type and value predicate, links). An engine over a rule set the
//!   process has seen takes the index that is there, also when the
//!   rules were loaded anew. The last [`KEPT`] rule sets are kept.
//!
//! An index is a function of what it reads from the rules, so the index
//! of an equal rule set answers the same. Equal means here: the same
//! content, compared byte by byte in the form the digest of
//! [`AnchorIndex::fits`] is taken over.

use super::AnchorIndex;
use crate::plan::DirectedRule;

#[cfg(not(feature = "opt_i"))]
mod imp {
    use super::{AnchorIndex, DirectedRule};

    /// An index the engine built for its rule slice.
    pub(crate) struct Own(Box<AnchorIndex>);

    impl Own {
        pub(crate) fn for_rules(rules: &[DirectedRule]) -> Self {
            Self(Box::new(AnchorIndex::build(rules)))
        }

        /// Is this the index for the slice `rules`?
        pub(crate) fn accepts(&self, rules: &[DirectedRule]) -> bool {
            self.0.built_for(rules)
        }
    }

    impl std::ops::Deref for Own {
        type Target = AnchorIndex;

        fn deref(&self) -> &AnchorIndex {
            &self.0
        }
    }
}

#[cfg(feature = "opt_i")]
mod imp {
    use std::hash::Hasher;
    use std::sync::{Arc, Mutex, PoisonError};

    use super::{AnchorIndex, DirectedRule};

    /// Rule sets whose index the process keeps.
    pub(crate) const KEPT: usize = 32;

    /// What an index reads from a rule set, as the bytes
    /// [`super::super::feed`] writes, with a hash of them for a first
    /// comparison. Two rule sets are equal here when the bytes are.
    struct Read {
        hash: u64,
        bytes: Box<[u8]>,
    }

    /// Collects what `feed` writes.
    struct Bytes(Vec<u8>);

    impl Hasher for Bytes {
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }

        fn finish(&self) -> u64 {
            unreachable!("the bytes are compared, not a hash")
        }
    }

    impl Read {
        fn of(rules: &[DirectedRule]) -> Self {
            let mut bytes = Bytes(Vec::new());
            super::super::feed(rules, &mut bytes);
            let mut h = crate::hash::FxHasher::default();
            h.write(&bytes.0);
            Self {
                hash: h.finish(),
                bytes: bytes.0.into_boxed_slice(),
            }
        }

        fn equals(&self, other: &Self) -> bool {
            self.hash == other.hash && self.bytes == other.bytes
        }
    }

    /// The kept indices, the one used last in front.
    static SHARED: Mutex<Vec<(Read, Arc<AnchorIndex>)>> = Mutex::new(Vec::new());

    /// The index kept for `read`; it moves to the front.
    fn find(kept: &mut Vec<(Read, Arc<AnchorIndex>)>, read: &Read) -> Option<Arc<AnchorIndex>> {
        let at = kept.iter().position(|(r, _)| r.equals(read))?;
        let hit = kept.remove(at);
        let index = hit.1.clone();
        kept.insert(0, hit);
        Some(index)
    }

    /// The index of the rule set, built unless the process keeps it.
    fn shared(rules: &[DirectedRule]) -> Arc<AnchorIndex> {
        let read = Read::of(rules);
        // A panic of another thread leaves the list as it was between
        // two of its changes: every state of it is valid.
        if let Some(index) = find(
            &mut SHARED.lock().unwrap_or_else(PoisonError::into_inner),
            &read,
        ) {
            return index;
        }
        // Built outside the lock; two threads that miss at once both
        // build, the second takes the index the first put in.
        let built = Arc::new(AnchorIndex::build(rules));
        let mut kept = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = find(&mut kept, &read) {
            return index;
        }
        kept.insert(0, (read, built.clone()));
        kept.truncate(KEPT);
        built
    }

    /// The index of the engine's rule set, shared with the engines of
    /// the process over an equal rule set, and the rule slice it was
    /// taken for (address, length).
    pub(crate) struct Own {
        index: Arc<AnchorIndex>,
        taken_for: (usize, usize),
    }

    impl Own {
        pub(crate) fn for_rules(rules: &[DirectedRule]) -> Self {
            Self {
                index: shared(rules),
                taken_for: (rules.as_ptr() as usize, rules.len()),
            }
        }

        /// Is this the index for the slice `rules`? The same cheap
        /// check as without the feature: address and length.
        pub(crate) fn accepts(&self, rules: &[DirectedRule]) -> bool {
            self.taken_for == (rules.as_ptr() as usize, rules.len())
        }
    }

    impl std::ops::Deref for Own {
        type Target = AnchorIndex;

        fn deref(&self) -> &AnchorIndex {
            &self.index
        }
    }

    /// The number of rule sets kept (for tests).
    #[cfg(test)]
    pub(crate) fn kept() -> usize {
        SHARED.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// Is the index of this rule set kept (for tests)?
    #[cfg(test)]
    pub(crate) fn keeps(rules: &[DirectedRule]) -> bool {
        let read = Read::of(rules);
        SHARED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|(r, _)| r.equals(&read))
    }
}

pub(crate) use imp::Own;
#[cfg(all(test, feature = "opt_i"))]
pub(crate) use imp::{keeps, kept, KEPT};

#[cfg(all(test, feature = "opt_i"))]
mod tests {
    use super::{keeps, kept, KEPT};
    use crate::engine::{AnchorSlot, Engine};
    use crate::graph::{Graph, ValueStore};
    use crate::plan::DirectedRule;

    /// One rule; `value` is the constant of its leaves.
    fn rules_json(value: &str) -> String {
        let leaf = |name: &str, typ: &str| {
            serde_json::json!({
                "name": name, "type": typ,
                "predicate": {"kind": "equals", "value": value},
                "constant": value
            })
        };
        serde_json::json!({"format": 3, "name": "own_tests", "rules": [{
            "name": "Root", "rank": 900,
            "left": {"anchor": "n", "nodes": [
                {"name": "f", "type": "F"},
                {"name": "n", "type": "J"},
                leaf("n_rule", "rule")
            ], "links": [["f", "n"], ["n", "n_rule"]]},
            "right": {"anchor": "m", "nodes": [
                {"name": "p", "type": "P", "context": true},
                {"name": "m", "type": "S"},
                leaf("m_feature", "feature")
            ], "links": [["p", "m"], ["m", "m_feature"]]},
            "corrs": [
                {"type": "CN", "left": "n", "right": "m", "role": "establishes"},
                {"type": "CF", "left": "f", "right": "m", "role": "establishes"}
            ]
        }]})
        .to_string()
    }

    /// A graph with one node, the rules loaded into it, and the address
    /// of the index an engine over them takes at its first expansion.
    fn index_of(value: &str) -> (usize, Vec<DirectedRule>) {
        let mut g = Graph::new();
        let vs = ValueStore::default();
        let f = g.add_baseline("file", "F");
        let rules = crate::rules::load(&rules_json(value), &mut g).expect("rules load");
        let at = {
            let mut e = Engine::new(&rules);
            e.elements_added(&g, &vs, &[f]);
            match &e.anchor {
                AnchorSlot::Owned(own) => &**own as *const _ as usize,
                _ => panic!("the engine took no index of its own"),
            }
        };
        (at, rules)
    }

    /// Rules loaded twice are two slices with the same content: both
    /// engines read one index. Other rules get another.
    #[test]
    fn an_equal_rule_set_takes_the_index_that_is_there() {
        let (first, rules) = index_of("shared-a");
        let (second, again) = index_of("shared-a");
        assert_ne!(rules.as_ptr(), again.as_ptr());
        assert_eq!(first, second, "one index for the equal rule set");
        let (other, _) = index_of("shared-b");
        assert_ne!(first, other, "another index for other rules");
    }

    /// The process keeps the indices of the last `KEPT` rule sets.
    #[test]
    fn the_kept_indices_are_bounded() {
        let (_, oldest) = index_of("bounded-0");
        assert!(keeps(&oldest));
        let mut last = Vec::new();
        for i in 1..=KEPT {
            last = index_of(&format!("bounded-{i}")).1;
        }
        assert!(kept() <= KEPT);
        assert!(keeps(&last), "the rule set used last is kept");
        assert!(!keeps(&oldest), "the rule set pushed out is not");
    }
}
