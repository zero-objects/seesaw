// Which instance a table or a graph is: a number of its own for every
// table and every graph that is created, and for every copy.

/// The identity of one table or graph instance.
///
/// The engine keeps the ids a rule's names resolved to in a table and
/// has to know whether the table at hand is that table.
/// The tables only grow, so what was resolved in an instance stays
/// true for it. The same holds for the connections of a graph, which
/// the engine's index of unheld links is about. A copy gets a number
/// of its own: copy and original can receive different entries
/// afterwards.
#[derive(Debug)]
pub(crate) struct TableUid(u64);

impl TableUid {
    /// The number of this instance. Never 0.
    #[inline]
    pub(crate) fn get(&self) -> u64 {
        self.0
    }
}

impl Default for TableUid {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl Clone for TableUid {
    fn clone(&self) -> Self {
        Self::default()
    }
}
