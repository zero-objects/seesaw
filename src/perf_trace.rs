//! Deterministic structural counters of the engine (perf diagnostics).
//!
//! Feature-gated (`perf_trace`, default off): in a normal build this
//! module does not exist and the `trace!` call sites expand to nothing,
//! so the hot path is unchanged. With `--features perf_trace` the
//! counters accumulate the structural work of a run. They do not depend
//! on the machine or its load, so two builds can be compared exactly.
//!
//! The counters are per thread. [`reset`] before a run, [`snapshot`]
//! after it.

use std::cell::RefCell;

/// Accumulated counters since the last [`reset`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Trace {
    /// Applied cascade steps.
    pub steps_applied: u64,
    /// Todo entries looked at while selecting the next step, including
    /// the ones skipped because their rule is not admitted.
    pub todo_scanned: u64,
    /// `record` calls (matches handed to the match store).
    pub record_calls: u64,
    /// `record` calls that found the match already stored.
    pub record_known: u64,
    /// Searches over a whole rule (`seed`).
    pub seed_searches: u64,
    /// Anchored searches of the delta-local expansion.
    pub anchored_searches: u64,
    /// Matches the anchored searches returned.
    pub anchored_found: u64,
    /// Graph nodes entered by the walks of the anchor index.
    pub anchor_walk_visits: u64,
    /// Places (rule, position) the anchor index compared against a
    /// walk.
    pub anchor_places: u64,
    /// Search plans built.
    pub plan_builds: u64,
    /// Candidates enumerated for a pattern position.
    pub candidates: u64,
    /// `node_ok` calls.
    pub node_checks: u64,
    /// Link checks against the graph (`connected`).
    pub link_checks: u64,
    /// Value resolutions (`resolve_value`).
    pub value_resolutions: u64,
    /// Identity hashes computed (blake3).
    pub ident_hashes: u64,
}

impl Trace {
    /// Field names and values in declaration order (for tables).
    #[must_use]
    pub fn fields(&self) -> [(&'static str, u64); 15] {
        [
            ("steps_applied", self.steps_applied),
            ("todo_scanned", self.todo_scanned),
            ("record_calls", self.record_calls),
            ("record_known", self.record_known),
            ("seed_searches", self.seed_searches),
            ("anchored_searches", self.anchored_searches),
            ("anchored_found", self.anchored_found),
            ("anchor_walk_visits", self.anchor_walk_visits),
            ("anchor_places", self.anchor_places),
            ("plan_builds", self.plan_builds),
            ("candidates", self.candidates),
            ("node_checks", self.node_checks),
            ("link_checks", self.link_checks),
            ("value_resolutions", self.value_resolutions),
            ("ident_hashes", self.ident_hashes),
        ]
    }
}

thread_local! {
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

/// Sets all counters of this thread to zero.
pub fn reset() {
    TRACE.with(|t| *t.borrow_mut() = Trace::default());
}

/// The counters of this thread.
#[must_use]
pub fn snapshot() -> Trace {
    TRACE.with(|t| t.borrow().clone())
}

/// Applies `f` to the counters of this thread (used by `trace!`).
pub fn add(f: impl FnOnce(&mut Trace)) {
    TRACE.with(|t| f(&mut t.borrow_mut()));
}
