//! The seed with its search on several threads (feature `opt_j`, not
//! on wasm32).
//!
//! [`Engine::seed`] searches and records in turn, but no search reads
//! what a record writes: the searches read the graph, the rules and the
//! anchor index, `record` writes the match store. So all searches of a
//! seed can run before the first match is recorded, and they can run
//! side by side. [`Engine::seed_with_threads`] does that and then
//! records the matches in the order of `seed`: rule by rule, the
//! matches of one rule in μ order.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::anchor::{self, AnchorIndex};
use super::matcher::{find_matches, find_matches_planned, Bindings, Scratch};
use super::Engine;
use crate::graph::{Graph, ValueResolver};
use crate::ident::GhostId;
use crate::plan::DirectedRule;

/// Nodes per walk piece and candidates per search piece: small enough
/// that the threads share the work evenly, large enough that taking a
/// piece does not count.
const PIECE: usize = 256;

/// Runs `work` for every index below `n` on up to `threads` threads,
/// each with a working memory of its own from `init`. The results come
/// back in index order, whatever thread ran which index.
fn spread<S, T: Send>(
    n: usize,
    threads: usize,
    init: impl Fn() -> S + Sync,
    work: impl Fn(&mut S, usize) -> T + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, T)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads.min(n).max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut memory = init();
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        out.push((i, work(&mut memory, i)));
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
            .collect()
    });
    done.sort_unstable_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, t)| t).collect()
}

/// `super::index_seed` with the walks and the searches on `threads`
/// threads: the same matches handed to `each` in the same order.
///
/// The walk of a node and the search at a place read the graph, the
/// rules and the index only, so they give the same whichever thread
/// runs them. The walks are cut into pieces of `piece` nodes; the
/// triples of all pieces, sorted, are the triples of one
/// `AnchorIndex::candidates` over all nodes. The searches are cut into
/// pieces of `piece` triples; the matches of one rule from all pieces
/// are the matches the searches of that rule find one after the other,
/// and `each` gets them sorted by their ref sequence as there.
#[allow(clippy::too_many_arguments)]
fn index_seed_spread(
    index: &AnchorIndex,
    rules: &[DirectedRule],
    g: &Graph,
    resolver: &(dyn ValueResolver + Sync),
    admitted: &[bool],
    threads: usize,
    piece: usize,
    mut each: impl FnMut(usize, &[GhostId]),
) {
    let piece = piece.max(1);
    let mut nodes: Vec<GhostId> = g
        .iter_nodes()
        .filter(|n| n.status.is_matchable())
        .map(|n| n.id)
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    let nodes = &nodes;

    let mut candidates: Vec<(usize, usize, usize)> = spread(
        nodes.len().div_ceil(piece),
        threads,
        anchor::Walk::default,
        |walk, p| {
            let from = p * piece;
            let to = (from + piece).min(nodes.len());
            let mut found = index.candidates(
                g,
                resolver,
                &nodes[from..to],
                walk,
                |ri| admitted[ri],
                |ni, id| *id < nodes[from + ni],
            );
            for triple in &mut found {
                triple.1 += from;
            }
            found
        },
    )
    .into_iter()
    .flatten()
    .collect();
    // (rule, node, position): the order `candidates` returns over all
    // nodes at once. No triple comes twice.
    candidates.sort_unstable();
    let candidates = &candidates;

    // Per piece: the rule of every match found, and the matches one
    // after the other.
    let found: Vec<(Vec<usize>, Vec<GhostId>)> = spread(
        candidates.len().div_ceil(piece),
        threads,
        Scratch::default,
        |scratch, p| {
            let from = p * piece;
            let to = (from + piece).min(candidates.len());
            let mut of_rule = Vec::new();
            let mut ids = Vec::new();
            for &(ri, ni, pos) in &candidates[from..to] {
                let pattern = &rules[ri].pattern;
                let at = nodes[ni];
                let smaller = |id: &GhostId| *id < at;
                trace!(anchored_searches);
                find_matches_planned(
                    g,
                    resolver,
                    pattern,
                    index.plan(ri, pos, pattern),
                    pos,
                    at,
                    Some(&smaller),
                    scratch,
                );
                for m in scratch.matches(pattern) {
                    trace!(anchored_found);
                    of_rule.push(ri);
                    ids.extend_from_slice(m);
                }
            }
            (of_rule, ids)
        },
    );
    // All matches in the order of the triples, so rule by rule, with
    // the start of each.
    let mut all: Vec<GhostId> = Vec::new();
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for (of_rule, ids) in found {
        let mut at = all.len();
        for ri in of_rule {
            starts.push((ri, at));
            at += rules[ri].pattern.nodes.len();
        }
        all.extend(ids);
    }

    let mut next = 0;
    let mut order: Vec<usize> = Vec::new();
    for (ri, rule) in rules.iter().enumerate() {
        if !admitted[ri] {
            continue;
        }
        let pattern = &rule.pattern;
        let n = pattern.nodes.len();
        if n == 0 {
            for m in find_matches(g, resolver, pattern) {
                each(ri, &m);
            }
            continue;
        }
        order.clear();
        while let Some(&(_, at)) = starts.get(next).filter(|s| s.0 == ri) {
            next += 1;
            order.push(at);
        }
        order.sort_unstable_by(|&a, &b| all[a..a + n].cmp(&all[b..b + n]));
        for &at in &order {
            each(ri, &all[at..at + n]);
        }
    }
    debug_assert_eq!(next, starts.len(), "a match of a rule not admitted");
}

/// The matches of one `find_matches` per rule of `active`, the searches
/// on `threads` threads; per rule in μ order as `find_matches` returns
/// them.
fn scan_spread(
    rules: &[DirectedRule],
    active: &[usize],
    g: &Graph,
    resolver: &(dyn ValueResolver + Sync),
    threads: usize,
) -> Vec<Vec<Bindings>> {
    spread(
        active.len(),
        threads,
        || (),
        |(), i| {
            trace!(seed_searches);
            find_matches(g, resolver, &rules[active[i]].pattern)
        },
    )
}

impl Engine<'_> {
    /// [`Engine::seed`] with the searches on up to `threads` threads.
    ///
    /// Finds the matches `seed` finds and records them in the order of
    /// `seed`; what the engine does afterwards is the same. The threads
    /// read the graph, the rules and the anchor index; the resolver is
    /// asked from several threads at once, hence `Sync`. With `threads`
    /// of 0 or 1 this is `seed`.
    ///
    /// The threads are started for this call and joined before it
    /// returns (`std::thread::scope`). Whether they pay depends on the
    /// size of the graph: starting them costs some tens of
    /// microseconds each.
    ///
    /// With the feature `perf_trace` the counters of the searches are
    /// those of the threads that ran them and are not in the snapshot
    /// of the calling thread.
    pub fn seed_with_threads(
        &mut self,
        g: &Graph,
        resolver: &(dyn ValueResolver + Sync),
        threads: usize,
    ) {
        if threads <= 1 {
            self.seed(g, resolver);
            return;
        }
        #[cfg(any(test, seesaw_checks))]
        self.check_seed(g, resolver);
        let rules = self.rules;
        if self.seed_through_index(g) {
            let admitted: Vec<bool> = (0..rules.len()).map(|ri| self.rule_active(ri)).collect();
            let anchor = self.take_index();
            index_seed_spread(
                anchor.index(),
                rules,
                g,
                resolver,
                &admitted,
                threads,
                PIECE,
                |ri, m| self.record(g, ri, m),
            );
            self.anchor = anchor;
        } else {
            let active: Vec<usize> = (0..rules.len())
                .filter(|&ri| self.rule_active(ri))
                .collect();
            let found = scan_spread(rules, &active, g, resolver, threads);
            for (matches, &ri) in found.iter().zip(&active) {
                for m in matches {
                    self.record(g, ri, m);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::anchor::tests::{build, rules_json, Tree};
    use crate::graph::ValueStore;

    /// A tree of `n` language nodes in which every node has a rule:
    /// classes and others below any node, each method below the class
    /// before it, flags on every third node.
    fn tree(n: usize) -> Tree {
        Tree {
            nodes: (0..n)
                .map(|i| {
                    // Entry `i` is node `i + 1`; a parent is an index
                    // into the nodes so far, 0 the root.
                    let (rule, parent) = match i % 4 {
                        0 => ("cls", i / 2),
                        1 => ("x", i / 2),
                        2 => ("cls", i / 3),
                        _ => ("m", i),
                    };
                    let flag = [None, Some(true), Some(false)][i % 3];
                    (rule.to_string(), parent, flag)
                })
                .collect(),
        }
    }

    type Sequence = Vec<(usize, Vec<GhostId>)>;

    /// What `seed` records, in its order: the sequence of
    /// `super::super::index_seed`.
    fn through_index(rules: &[DirectedRule], g: &Graph, vs: &ValueStore) -> Sequence {
        let index = AnchorIndex::build(rules);
        let admitted = vec![true; rules.len()];
        let mut out = Vec::new();
        super::super::index_seed(
            &index,
            rules,
            g,
            vs,
            &mut anchor::Walk::default(),
            &mut Scratch::default(),
            &admitted,
            |ri, m| out.push((ri, m.to_vec())),
        );
        out
    }

    /// A translated model: the tree, the rules, and the cascade run to
    /// saturation. A seed over it finds the matches of every rule.
    fn translated(nodes: usize) -> (Graph, ValueStore, Vec<DirectedRule>) {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        let all = build(&tree(nodes), &mut g, &mut vs);
        let rules = crate::rules::load(&rules_json(), &mut g).expect("rules load");
        {
            let mut e = Engine::new(&rules);
            e.elements_added(&g, &vs, &all);
            let mut steps = 0;
            while e.step(&mut g, &vs).is_some() {
                steps += 1;
                assert!(steps < 100_000, "no saturation");
            }
        }
        (g, vs, rules)
    }

    /// The spread seed through the index hands out the matches of the
    /// seed in its order, for every cut of the work.
    #[test]
    fn the_spread_index_seed_keeps_matches_and_order() {
        let (g, vs, rules) = translated(300);
        let expected = through_index(&rules, &g, &vs);
        assert!(expected.len() > 300, "the model has matches");
        let index = AnchorIndex::build(&rules);
        let admitted = vec![true; rules.len()];
        for (threads, piece) in [(2, 1), (3, 7), (4, 64), (8, 100_000), (1, 5)] {
            let mut got: Sequence = Vec::new();
            index_seed_spread(
                &index,
                &rules,
                &g,
                &vs,
                &admitted,
                threads,
                piece,
                |ri, m| got.push((ri, m.to_vec())),
            );
            assert_eq!(got, expected, "{threads} threads, pieces of {piece}");
        }
        // Rules left out are left out of the walks and the searches.
        let admitted: Vec<bool> = (0..rules.len()).map(|ri| ri % 2 == 0).collect();
        let expected: Sequence = expected.into_iter().filter(|(ri, _)| ri % 2 == 0).collect();
        let mut got: Sequence = Vec::new();
        index_seed_spread(&index, &rules, &g, &vs, &admitted, 3, 5, |ri, m| {
            got.push((ri, m.to_vec()))
        });
        assert_eq!(got, expected, "with rules left out");
    }

    /// The spread scan returns per rule what `find_matches` returns.
    #[test]
    fn the_spread_scan_keeps_matches_and_order() {
        let (g, vs, rules) = translated(120);
        let active: Vec<usize> = (0..rules.len()).collect();
        let expected: Vec<Vec<Bindings>> = rules
            .iter()
            .map(|r| find_matches(&g, &vs, &r.pattern))
            .collect();
        assert!(expected.iter().map(Vec::len).sum::<usize>() > 120);
        for threads in [2, 3, 16] {
            assert_eq!(
                scan_spread(&rules, &active, &g, &vs, threads),
                expected,
                "{threads} threads"
            );
        }
    }

    /// What an engine does after `seed_with_threads` is what it does
    /// after `seed`, on both ways of the seed and on a model before and
    /// after its translation.
    #[test]
    fn seed_with_threads_runs_the_cascade_of_seed() {
        // Returns the results of the steps, the cascade (rule, refs,
        // created, created edges), the nodes with their status, the
        // contradiction flag and the way the seed took.
        let run = |nodes: usize, threads: usize, factor: usize, again: bool| {
            let mut g = Graph::new();
            let mut vs = ValueStore::default();
            let all = build(&tree(nodes), &mut g, &mut vs);
            let rules = crate::rules::load(&rules_json(), &mut g).expect("rules load");
            // More rules over the same types push the seed to the
            // index, one copy of the rules leaves it at the scan.
            let rules: Vec<DirectedRule> = (0..factor).flat_map(|_| rules.clone()).collect();
            if again {
                let mut first = Engine::new(&rules);
                first.elements_added(&g, &vs, &all);
                while first.step(&mut g, &vs).is_some() {}
            }
            let mut e = Engine::new(&rules);
            let way = e.seed_through_index(&g);
            if threads == 0 {
                e.seed(&g, &vs);
            } else {
                e.seed_with_threads(&g, &vs, threads);
            }
            let mut steps = Vec::new();
            while let Some(applied) = e.step(&mut g, &vs) {
                steps.push(applied);
                assert!(steps.len() < 100_000, "no saturation");
            }
            let cascade: Vec<_> = e
                .cascade
                .iter()
                .map(|c| {
                    (
                        c.rule_ix,
                        c.refs.clone(),
                        c.created.clone(),
                        c.created_edges.clone(),
                    )
                })
                .collect();
            let mut graph: Vec<_> = g
                .nodes()
                .map(|n| (n.id, format!("{:?}", n.status)))
                .collect();
            graph.sort();
            (steps, cascade, graph, e.saw_contradiction, way)
        };
        let mut ways = std::collections::BTreeSet::new();
        for again in [false, true] {
            for factor in [1, 8] {
                let expected = run(200, 0, factor, again);
                assert!(!expected.0.is_empty(), "the seed found matches");
                ways.insert(expected.4);
                for threads in [2, 5] {
                    assert_eq!(
                        run(200, threads, factor, again),
                        expected,
                        "{threads} threads, {factor} copies of the rules, again: {again}"
                    );
                }
            }
        }
        assert_eq!(ways.len(), 2, "both ways of the seed were taken");
    }
}
