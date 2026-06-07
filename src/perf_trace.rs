//! Deterministic structural counters for the cascade (perf diagnostics).
//!
//! Feature-gated (`perf_trace`, off by default) → in a normal build this
//! module does not exist and there are no call-sites; the hot path is
//! unchanged (bit-identical). With `--features perf_trace` a few counters
//! accumulate the *structural* work per phase, to see which phase grows
//! super-linearly with graph size (= the real quadratic driver(s)) —
//! contention-immune, unlike timing.

use std::cell::RefCell;

/// Counters accumulated over one cascade run.
#[derive(Debug, Default, Clone)]
pub struct Trace {
    /// Number of cascade_step_cached calls.
    pub steps: u64,
    /// Σ over steps of the built candidate list (live, emitted).
    pub collect_built: u64,
    /// Σ over steps of all matches walked in collect (incl. dead).
    pub collect_scanned: u64,
    /// Σ of candidates examined in select_and_apply_cached.
    pub select_examined: u64,
    /// Σ of order entries visited in the walk (incl. skipped dead/NAC) —
    /// measures the skip prefix (perf-D3 diagnostic).
    pub walk_visited: u64,
    /// produce() calls.
    pub produce_calls: u64,
    /// is_duplicate() calls.
    pub is_duplicate_calls: u64,
    /// anchored find_matches_with_fixed calls in cache.update.
    pub update_anchored_calls: u64,
    /// matches returned by anchored matching.
    pub update_anchored_found: u64,
    /// full re-enumerations (removal/mutation) in cache.update.
    pub update_full_reenum: u64,
    /// matches returned by full re-enumeration.
    pub update_full_found: u64,
    /// collect_scanned at the FIRST collect call (match set at the start).
    pub collect_first: u64,
    /// collect_scanned at the LAST collect call (match set at the end).
    /// first==last ⇒ the match set is static (does not grow).
    pub collect_last: u64,
    /// Op kinds in the APPLIED deltas (to understand what produce emits).
    pub applied_add_node: u64,
    pub applied_add_edge: u64,
    pub applied_del: u64,
    pub applied_set_attr: u64,
}

thread_local! {
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

pub fn reset() {
    TRACE.with(|t| *t.borrow_mut() = Trace::default());
}

pub fn snapshot() -> Trace {
    TRACE.with(|t| t.borrow().clone())
}

pub fn step() {
    TRACE.with(|t| t.borrow_mut().steps += 1);
}

pub fn collect(built: usize, scanned: usize) {
    TRACE.with(|t| {
        let mut t = t.borrow_mut();
        t.collect_built += built as u64;
        t.collect_scanned += scanned as u64;
        if t.collect_first == 0 {
            t.collect_first = scanned as u64;
        }
        t.collect_last = scanned as u64;
    });
}

/// Op-kind distribution of one applied delta.
pub fn applied(add_node: usize, add_edge: usize, del: usize, set_attr: usize) {
    TRACE.with(|t| {
        let mut t = t.borrow_mut();
        t.applied_add_node += add_node as u64;
        t.applied_add_edge += add_edge as u64;
        t.applied_del += del as u64;
        t.applied_set_attr += set_attr as u64;
    });
}

pub fn select_examined(n: usize) {
    TRACE.with(|t| t.borrow_mut().select_examined += n as u64);
}

pub fn walk_visited(n: usize) {
    TRACE.with(|t| t.borrow_mut().walk_visited += n as u64);
}

pub fn produce() {
    TRACE.with(|t| t.borrow_mut().produce_calls += 1);
}

pub fn is_duplicate() {
    TRACE.with(|t| t.borrow_mut().is_duplicate_calls += 1);
}

pub fn update_anchored(calls: usize, found: usize) {
    TRACE.with(|t| {
        let mut t = t.borrow_mut();
        t.update_anchored_calls += calls as u64;
        t.update_anchored_found += found as u64;
    });
}

pub fn update_full(found: usize) {
    TRACE.with(|t| {
        let mut t = t.borrow_mut();
        t.update_full_reenum += 1;
        t.update_full_found += found as u64;
    });
}
