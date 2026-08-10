//! The engine: the delta-local cascade.
//!
//! Spec §1.6 implemented verbatim:
//! - add: expand anchored → create a match record, refs into the
//!   participant lists, todo insert. μ = the ref sequence itself.
//! - delete/modify: `adj[k]` directly yields the affected matches —
//!   eagerly mark dead; no lazy viability, no re-enum, no index.
//! - Duplicate check BEFORE application (ids are purely derivable).
//! - The todo list = the ONLY extra structure.
//!
//! Sequencing (memory design sketch): minimal engine first (add-only
//! cascade, NACs, rank order, duplication verdict) — retraction/
//! contradiction/fold/backtracking are ported from first-generation afterward.

pub mod matcher;

use std::collections::BTreeSet;

use crate::ident::{GhostId, Status};

use crate::engine::matcher::{find_matches, find_matches_with_fixed, Bindings};
use crate::graph::{Graph, ValueResolver};
use crate::plan::{apply_creation, DirectedRule, Ref};

fn self_node_tt(g: &Graph, id: &GhostId) -> bool {
    g.node(id)
        .is_some_and(|n| n.status == Status::TentativeTombstone)
}

/// Match record (spec §1): a record living in the same world as nodes
/// and connections — participants via `Engine::by_element`.
#[derive(Debug, Clone)]
pub struct MatchRec {
    pub rule_ix: usize,
    pub refs: Bindings,
    pub dead: bool,
    /// Provenance edge match → cascade entry: `Some(cascade-ix)` if
    /// this match WAS APPLIED (for 1:1, the `applied` guard prevents a
    /// double application), otherwise `None`. Materializes the edge so
    /// `retract_for` can follow provenance without a cascade scan.
    pub entry: Option<u32>,
}

/// Cascade entry: what was applied (ref sequence = μ = identity).
#[derive(Debug, Clone)]
pub struct Entry {
    pub rule_ix: usize,
    pub rank: u64,
    pub refs: Bindings,
    pub created: Vec<GhostId>,
    /// Created edges (connection ids). Kept separate from `created`
    /// because retraction retracts them via `set_connection_status`
    /// and they have no provenance children (not queued).
    pub created_edges: Vec<GhostId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Termination {
    Duplication,
    Convergence,
    StepLimit,
    /// V7: a candidate wanted to reuse tombstone substance —
    /// a contradiction with history.
    Contradiction,
}

/// Backtracking bound: the intrinsic selection position (rule rank,
/// ref sequence) of the undone choice — derivable from every entry, no
/// state-dependent numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionBound {
    pub rank: u64,
    pub refs: Bindings,
}

/// Todo key: (rank descending, ref sequence descending, rule index
/// ascending) — the same ordering doctrine as the first-generation engine's
/// intrinsic μ, here the ref sequence is the key.
type TodoKey = (std::cmp::Reverse<u64>, std::cmp::Reverse<Bindings>, usize);

fn todo_key(rank: u64, refs: &Bindings, rule_ix: usize) -> TodoKey {
    (
        std::cmp::Reverse(rank),
        std::cmp::Reverse(refs.clone()),
        rule_ix,
    )
}

/// The engine.
pub struct Engine<'r> {
    pub rules: &'r [DirectedRule],
    /// Match store (records logically live in the map; physically
    /// here, referenced from `by_element` — realization of the spec
    /// map).
    matches: Vec<MatchRec>,
    /// adj[k] → match refs (spec §1.6: "which matches contain k?").
    by_element: crate::hash::FxHashMap<GhostId, Vec<u32>>,
    /// Duplicate memory: ref sequences already applied per rule.
    applied: BTreeSet<(usize, Bindings)>,
    todo: BTreeSet<TodoKey>,
    pub saw_contradiction: bool,
    /// (rule_ix, refs) → match store index for todo resolution.
    by_key: crate::hash::FxHashMap<(usize, Bindings), u32>,
    pub cascade: Vec<Entry>,
    /// Runtime activation (tool configuration, e.g. Benchmarx
    /// decisions): None = all active. Inactive candidates stay in the
    /// todo (a configuration change reactivates them).
    pub active: Option<Vec<bool>>,
    /// TT collection of the retraction: `retract_match` notes here
    /// which nodes/edges it tentatively retracted — `consolidate`
    /// processes ONLY these (O(Δ)), instead of scanning the whole
    /// model.
    pending_tt_nodes: Vec<GhostId>,
    pending_tt_edges: Vec<GhostId>,
}

impl<'r> Engine<'r> {
    pub fn new(rules: &'r [DirectedRule]) -> Self {
        Self {
            rules,
            matches: Vec::new(),
            by_element: Default::default(),
            applied: BTreeSet::new(),
            todo: BTreeSet::new(),
            by_key: Default::default(),
            saw_contradiction: false,
            cascade: Vec::new(),
            active: None,
            pending_tt_nodes: Vec::new(),
            pending_tt_edges: Vec::new(),
        }
    }

    pub fn step(&mut self, g: &mut Graph, resolver: &dyn ValueResolver) -> Option<bool> {
        self.step_with_limit(g, resolver, None)
    }

    fn record(&mut self, rule_ix: usize, refs: Bindings) {
        let key = (rule_ix, refs.clone());
        if self.by_key.contains_key(&key) {
            return;
        }
        let ix = self.matches.len() as u32;
        for id in &refs {
            self.by_element.entry(*id).or_default().push(ix);
        }
        self.todo
            .insert(todo_key(self.rules[rule_ix].rank, &refs, rule_ix));
        self.by_key.insert(key, ix);
        self.matches.push(MatchRec {
            rule_ix,
            refs,
            dead: false,
            entry: None,
        });
    }

    /// Initial enumeration: all rules in full (seeds) — with stage 8
    /// (add stream) this special path mostly disappears.
    pub fn seed(&mut self, g: &Graph, resolver: &dyn ValueResolver) {
        for ri in 0..self.rules.len() {
            self.seed_rule(g, resolver, ri);
        }
    }

    fn seed_rule(&mut self, g: &Graph, resolver: &dyn ValueResolver, ri: usize) {
        for m in find_matches(g, resolver, &self.rules[ri].pattern) {
            self.record(ri, m);
        }
    }

    /// Δ routing (first-generation parity, rc7): a directed rule is active when the
    /// delta touches one of its input types — the direction lives in
    /// the delta, not in a pass switch.
    pub fn seed_routed(&mut self, g: &Graph, resolver: &dyn ValueResolver, delta_types: &[String]) {
        for ri in 0..self.rules.len() {
            if self.rules[ri]
                .input_types
                .iter()
                .any(|t| delta_types.iter().any(|d| d == t))
            {
                self.seed_rule(g, resolver, ri);
            }
        }
    }

    /// Add stream (spec §1.6): anchor externally added elements DELTA-
    /// LOCALLY — without a full `seed`. Symmetric to `element_removed`;
    /// call `step` afterward until saturation. The candidates for the
    /// neighboring positions come via the edge index
    /// (`parts_by_other_type`), not via a type scan of the model — the
    /// cost hangs off the delta's local neighborhood, not the model
    /// size.
    pub fn elements_added(
        &mut self,
        g: &Graph,
        resolver: &dyn ValueResolver,
        new_nodes: &[GhostId],
    ) {
        self.expand_at(g, resolver, new_nodes);
    }

    /// Delta-local expansion: anchor new elements (spec §1.6 add).
    fn expand_at(&mut self, g: &Graph, resolver: &dyn ValueResolver, new_nodes: &[GhostId]) {
        for (ri, rule) in self.rules.iter().enumerate() {
            for &id in new_nodes {
                let Some(node) = g.node(&id) else { continue };
                for (pos, pn) in rule.pattern.nodes.iter().enumerate() {
                    if pn.typ != node.typ {
                        continue;
                    }
                    let mut fixed = vec![None; rule.pattern.nodes.len()];
                    fixed[pos] = Some(id);
                    for m in find_matches_with_fixed(g, resolver, &rule.pattern, &fixed) {
                        self.record(ri, m);
                    }
                }
            }
        }
    }

    /// Retraction (M5.3/M5.5): an element has dropped out —
    /// applied entries whose match contained it lose their
    /// justification. Their CREATED elements are tentatively
    /// tombstoned (a provenance walk: entries' created lists,
    /// recursively over entries anchored on created elements).
    /// Consolidation (`consolidate`) decides TT → tombstone or
    /// resurrection (if a new derivation reclaimed the same structural
    /// identity).
    pub fn retract_for(&mut self, g: &mut Graph, removed: &GhostId) {
        let mut queue: Vec<GhostId> = vec![*removed];
        self.drain_retraction(g, &mut queue);
    }

    /// A single match loses its justification: invalidate it (dead +
    /// todo), forget the record (applied/by_key, reclaim ability) and
    /// — if it was applied — follow the provenance edge to its
    /// products (TT + queue). Shared core of `retract_for` and
    /// `link_removed`.
    fn retract_match(&mut self, g: &mut Graph, mix: u32, queue: &mut Vec<GhostId>) {
        let (rule_ix, refs, entry, was_dead) = {
            let m = &self.matches[mix as usize];
            (m.rule_ix, m.refs.clone(), m.entry, m.dead)
        };
        // Invalidate the candidate (like `element_removed`): dead + out
        // of the todo. Idempotent if `element_removed` already ran.
        if !was_dead {
            self.matches[mix as usize].dead = true;
            self.todo
                .remove(&todo_key(self.rules[rule_ix].rank, &refs, rule_ix));
        }
        // Reclaim ability: FORGET the record (applied/by_key), so an
        // identical re-derivation can APPLY again and thereby
        // resurrect (M5 reclaim via insert_node/connect).
        let key = (rule_ix, refs);
        self.applied.remove(&key);
        self.by_key.remove(&key);
        // If the match WAS APPLIED (provenance edge set), follow it:
        // tentatively tombstone its products and continue via the
        // queue. The next by_element lookup finds the children for
        // each `created` entry.
        if let Some(eix) = entry {
            let created = self.cascade[eix as usize].created.clone();
            let created_edges = self.cascade[eix as usize].created_edges.clone();
            for c in created {
                // A product of THIS entry, so its provenance is proven
                // by the loop itself — the status does not decide.
                //
                // Until 2026-08-10 the condition read `== Ghost`, which
                // ended retraction at a materialization: a folded
                // product is `Solid` and was left standing, so the
                // delta produced no tombstone at all. Provenance is the
                // criterion, not the lifecycle state; `add_baseline`
                // nodes appear in no `created` and stay untouched.
                //
                // TENTATIVE, not final: the consolidation at the end of
                // the run decides. Re-derived within this run means the
                // element is reclaimed, otherwise it resolves to
                // `Tombstone`. At rest only `Ghost` and `Solid` remain.
                if g.node(&c).is_some_and(|n| n.status.is_matchable()) {
                    g.set_node_status(&c, Status::TentativeTombstone);
                    self.pending_tt_nodes.push(c);
                }
                queue.push(c);
            }
            // Tentatively retract the CREATED EDGES: reclaim via
            // `connect` lifts TT again (same endpoints re-derived),
            // otherwise `consolidate` finalizes to tombstone. Edges
            // have no provenance children ⇒ not queued.
            for e in created_edges {
                // Same reasoning as for the nodes above: provenance
                // decides, not the lifecycle state. A materialized
                // edge is `Solid` and was left standing until
                // 2026-08-10, so a retracted node kept its connections.
                if g.connection(&e).is_some_and(|c| c.status.is_matchable()) {
                    g.set_connection_status(&e, Status::TentativeTombstone);
                    self.pending_tt_edges.push(e);
                }
            }
        }
    }

    /// Provenance walk over the queue (core of `retract_for`).
    fn drain_retraction(&mut self, g: &mut Graph, queue: &mut Vec<GhostId>) {
        let mut seen: BTreeSet<GhostId> = BTreeSet::new();
        while let Some(id) = queue.pop() {
            if !seen.insert(id) {
                continue;
            }
            // Affected matches DIRECTLY via the by_element index (spec
            // §1.6) — no scan over applied/by_key/cascade. Every match
            // whose refs contain `id` shows up here, because `record`
            // maintains the index for every ref element. This covers
            // both applied and suppressed duplicate matches (rename:
            // attach dropped out, create must become applicable again).
            let mixes: Vec<u32> = self.by_element.get(&id).cloned().unwrap_or_default();
            for mix in mixes {
                self.retract_match(g, mix, queue);
            }
        }
    }

    /// Consolidation (M5.5): TentativeTombstone → Tombstone, unless a
    /// new derivation has reclaimed the element (a reclaim resets the
    /// status in `insert_node`/`connect`).
    pub fn consolidate(&mut self, g: &mut Graph) {
        // Processes ONLY the TT collected by retraction — no full
        // model scan. An element that was RECLAIMED in the meantime is
        // still in the list, but is no longer TT (connect set it to
        // Ghost) ⇒ the status check skips it.
        for id in self.pending_tt_nodes.drain(..) {
            if g.node(&id)
                .is_some_and(|n| n.status == Status::TentativeTombstone)
            {
                g.set_node_status(&id, Status::Tombstone);
            }
        }
        for id in self.pending_tt_edges.drain(..) {
            if g.connection(&id)
                .is_some_and(|c| c.status == Status::TentativeTombstone)
            {
                g.set_connection_status(&id, Status::Tombstone);
            }
        }
    }

    /// Edge removal (spec §1.6, delete on a connection): matches whose
    /// refs contain BOTH endpoints lose their justification — the match
    /// memory only indexes nodes (`by_element`), it doesn't know a used
    /// link; without this call, a tombstoned edge silently strips the
    /// justification from a match (case 19 finding: UpdateRef didn't
    /// re-derive dstName because the rc8-#2 recognition saw the
    /// surviving corr). Deliberately an OVER-approximation: even a
    /// match that uses both nodes without the removed edge gets
    /// retracted — the TT/resurrection semantics heal that, because the
    /// identical re-derivation reclaims on the next sync (id-stable).
    /// Symmetric to `element_removed`/`retract_for`; call
    /// `seed`/`step` until saturation and `consolidate` afterward.
    pub fn link_removed(&mut self, g: &mut Graph, a: &GhostId, b: &GhostId) {
        let Some(in_a) = self.by_element.get(a) else {
            return;
        };
        let Some(in_b) = self.by_element.get(b) else {
            return;
        };
        let set_b: BTreeSet<u32> = in_b.iter().copied().collect();
        let mixes: Vec<u32> = in_a.iter().copied().filter(|m| set_b.contains(m)).collect();
        let mut queue: Vec<GhostId> = Vec::new();
        for mix in mixes {
            self.retract_match(g, mix, &mut queue);
        }
        self.drain_retraction(g, &mut queue);
    }

    /// delete/modify (spec §1.6): EAGERLY kill the element's matches.
    pub fn element_removed(&mut self, id: &GhostId) {
        let Some(refs) = self.by_element.get(id).cloned() else {
            return;
        };
        for mix in refs {
            let m = &mut self.matches[mix as usize];
            if !m.dead {
                m.dead = true;
                self.todo
                    .remove(&todo_key(self.rules[m.rule_ix].rank, &m.refs, m.rule_ix));
            }
        }
    }

    /// Duplicate check BEFORE application: does everything the plan
    /// would create already exist, matchable? (Ids are purely
    /// derivable.)
    fn creation_exists(&self, g: &Graph, rule: &DirectedRule, refs: &Bindings) -> bool {
        let mut ids: Vec<GhostId> = Vec::with_capacity(rule.create_nodes.len());
        for cn in &rule.create_nodes {
            let parent = match cn.parent {
                Ref::Matched(p) => refs[p],
                Ref::New(ix) => ids[ix],
            };
            let id = if cn.corr_full_match {
                crate::graph::preview_corr_id(&parent, &cn.typ, refs)
            } else if let Some((anchor, ref attr, ref t)) = cn.derived_dyn {
                match g.child_leaf_of_type(&refs[anchor], attr) {
                    // apply-if-present: without a source nothing would
                    // be created.
                    None => {
                        ids.push(parent); // placeholder (never referenced)
                        continue;
                    }
                    Some(src) => crate::graph::preview_derived_id(&parent, &cn.typ, &src, t),
                }
            } else {
                match (&cn.konst, &cn.derived) {
                    (Some(_), _) => crate::graph::preview_konst_id(
                        &parent,
                        &cn.typ,
                        &rule.name,
                        ids.len() as u32,
                    ),
                    (None, Some((leaf, t))) => {
                        crate::graph::preview_derived_id(&parent, &cn.typ, &refs[*leaf], t)
                    }
                    (None, None) => crate::graph::preview_ghost_id(&parent, &cn.typ),
                }
            };
            if g.node_alive(&id) {
                ids.push(id);
            } else {
                return false;
            }
        }
        for &(a, b) in &rule.create_links {
            let s = match a {
                Ref::Matched(p) => refs[p],
                Ref::New(ix) => ids[ix],
            };
            let t = match b {
                Ref::Matched(p) => refs[p],
                Ref::New(ix) => ids[ix],
            };
            if !g.connected_alive(&s, &t) {
                return false;
            }
        }
        true
    }

    /// One step: pull the highest-ranked live candidate.
    /// `ceiling`: only candidates strictly BELOW the bound (def. 3.10,
    /// position-local — the caller manages the position).
    pub fn step_with_limit(
        &mut self,
        g: &mut Graph,
        resolver: &dyn ValueResolver,
        ceiling: Option<&SelectionBound>,
    ) -> Option<bool> {
        // `true` in Some = applied; None = todo empty (below the bound).
        loop {
            let key = match ceiling {
                None => self.todo.iter().next().cloned()?,
                Some(b) => {
                    // Strictly below the bound = in the BTreeSet
                    // strictly AFTER the bound key (reverse order).
                    let bound = todo_key(b.rank, &b.refs, 0);
                    self.todo
                        .range(bound.clone()..)
                        .find(|k| !(k.0 == bound.0 && k.1 == bound.1))
                        .cloned()?
                }
            };
            // Runtime mask: inactive candidates are left alone.
            //
            // The mask must INTERSECT with the ceiling, not replace it.
            // Until 2026-08-10 this searched `self.todo.iter()`, the
            // whole queue, and overwrote the choice made above — with a
            // mask set, the ceiling had no effect at all and
            // backtracking could pick a candidate at or above its own
            // bound. Found in the review of that day.
            let key = match &self.active {
                None => key,
                Some(active) => match ceiling {
                    None => self.todo.iter().find(|k| active[k.2]).cloned()?,
                    Some(b) => {
                        let bound = todo_key(b.rank, &b.refs, 0);
                        self.todo
                            .range(bound.clone()..)
                            .find(|k| !(k.0 == bound.0 && k.1 == bound.1) && active[k.2])
                            .cloned()?
                    }
                },
            };
            self.todo.remove(&key);
            let (rule_ix, refs) = (key.2, (key.1).0.clone());
            let rule = &self.rules[rule_ix];
            if self.applied.contains(&(rule_ix, refs.clone())) {
                continue;
            }
            // TT ANCHORS don't fire: the resurrection window keeps TT
            // matchable for re-derivations (context reclaim) — but as
            // an ESTABLISHES anchor, something retracted doesn't work
            // (otherwise reclaim zombies). Context refs may be TT
            // (reclaim through use). A forgotten match is found again
            // by the next seed, if resurrected.
            if rule
                .corr_recognition
                .iter()
                .any(|(_, pos, _)| self_node_tt(g, &refs[*pos]))
            {
                self.by_key.remove(&(rule_ix, refs));
                continue;
            }
            // rc8-#2: the anchor already carries a corr of
            // this type with a MATCHABLE counterpart — the element is
            // translated, regardless of which direction/variant ⇒ a
            // duplicate.
            let translated = !rule.corr_recognition.is_empty()
                && rule
                    .corr_recognition
                    .iter()
                    .all(|(typ, pos, endpoint_typ)| {
                        g.types.lookup(typ).is_some_and(|t| {
                            let ep = g.types.lookup(endpoint_typ);
                            g.parts_by_other_type(&refs[*pos], t).any(|p| {
                                // TT does NOT count (M5 window): something
                                // tentatively retracted doesn't suppress a
                                // new derivation.
                                let corr_ok = g.connection(&p.connection).is_some_and(|c| {
                                    matches!(c.status, Status::Solid | Status::Ghost)
                                }) && g.node_alive(&p.other);
                                // Check the endpoint type: the corr's
                                // counterpart (≠ the anchor) must match the
                                // expected type (single-corr-type rule
                                // sets).
                                corr_ok
                                    && ep.is_some_and(|ept| {
                                        g.parts(&p.other).any(|q| {
                                            q.other != refs[*pos]
                                                && q.other_typ == ept
                                                && g.node_alive(&q.other)
                                        })
                                    })
                            })
                        })
                    });
            if translated || self.creation_exists(g, rule, &refs) {
                // Duplicate: nothing new — note it for the verdict.
                self.applied.insert((rule_ix, refs));
                return Some(false);
            }
            // V7 guard: reusing tombstone substance is a contradiction
            // with history (not TT — that's the legitimate resurrection
            // zone).
            let contradicts = {
                let mut ids: Vec<GhostId> = Vec::new();
                let mut bad = false;
                'outer: for cn in &rule.create_nodes {
                    let parent = match cn.parent {
                        Ref::Matched(p) => refs[p],
                        Ref::New(ix) => ids[ix],
                    };
                    let id = if cn.corr_full_match {
                        crate::graph::preview_corr_id(&parent, &cn.typ, &refs)
                    } else if let Some((anchor, ref attr, ref t)) = cn.derived_dyn {
                        match g.child_leaf_of_type(&refs[anchor], attr) {
                            None => {
                                ids.push(parent);
                                continue;
                            }
                            Some(src) => {
                                crate::graph::preview_derived_id(&parent, &cn.typ, &src, t)
                            }
                        }
                    } else {
                        match (&cn.konst, &cn.derived) {
                            (Some(_), _) => crate::graph::preview_konst_id(
                                &parent,
                                &cn.typ,
                                &rule.name,
                                ids.len() as u32,
                            ),
                            (None, Some((leaf, t))) => {
                                crate::graph::preview_derived_id(&parent, &cn.typ, &refs[*leaf], t)
                            }
                            (None, None) => crate::graph::preview_ghost_id(&parent, &cn.typ),
                        }
                    };
                    if let Some(n) = g.node(&id) {
                        if n.status == Status::Tombstone {
                            bad = true;
                            break 'outer;
                        }
                    }
                    ids.push(id);
                }
                bad
            };
            if contradicts {
                self.saw_contradiction = true;
                continue;
            }
            let (created, created_edges) = apply_creation(g, rule, &refs);
            let eix = self.cascade.len() as u32;
            self.cascade.push(Entry {
                rule_ix,
                rank: rule.rank,
                refs: refs.clone(),
                created: created.clone(),
                created_edges,
            });
            self.applied.insert((rule_ix, refs.clone()));
            // Materialize the provenance edge: the applied match points
            // at its cascade entry (so `retract_for` needs no scan).
            if let Some(&mix) = self.by_key.get(&(rule_ix, refs.clone())) {
                self.matches[mix as usize].entry = Some(eix);
            }
            self.expand_at(g, resolver, &created);
            return Some(true);
        }
    }

    /// Saturation verdict (first-generation parity): applied or recognized-as-
    /// duplicate matches are "dead" candidates ⇒ Duplication; only an
    /// empty history converges.
    fn verdict(&self) -> Termination {
        if self.saw_contradiction {
            Termination::Contradiction
        } else if self.applied.is_empty() {
            Termination::Convergence
        } else {
            Termination::Duplication
        }
    }

    /// Cascade until saturation.
    pub fn run(
        &mut self,
        g: &mut Graph,
        resolver: &dyn ValueResolver,
        max_steps: usize,
    ) -> Termination {
        self.seed(g, resolver);
        for _ in 0..max_steps {
            if self.step(g, resolver).is_none() {
                return self.verdict();
            }
        }
        Termination::StepLimit
    }
}

// ══ Tests (stage 4, minimal engine) ═════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::ValueStore;
    use crate::graph::*;
    use crate::rules::format::RuleFile;

    /// Vorwaerts- und Rueckwaertsplan der Familienregel, ueber den
    /// einen Ladeweg des Crates.
    fn father_rule(g: &mut Graph) -> Vec<crate::plan::DirectedRule> {
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "engine_tests",
            "rules": [{
                "name": "Father_2_Male",
                "rank": 850,
                "left": {
                    "anchor": "fam",
                    "nodes": [
                        {"name": "fam", "type": "Family"},
                        {"name": "father", "type": "Father"},
                        {"name": "member", "type": "Member"},
                        {"name": "first", "type": "firstName"}
                    ],
                    "links": [["fam", "father"], ["father", "member"],
                              ["member", "first"]]
                },
                "right": {
                    "anchor": "male",
                    "nodes": [
                        {"name": "male", "type": "Male"},
                        {"name": "name", "type": "name"}
                    ],
                    "links": [["male", "name"]]
                },
                "corrs": [
                    {"type": "PersonCorr", "left": "member", "right": "male",
                     "role": "establishes",
                     "bindings": [{"left": "first", "right": "name"}]}
                ]
            }]
        }))
        .expect("Regeldatei parst");
        crate::rules::load_file(&file, g).expect("Regeldatei laedt")
    }

    /// Die Familienregel mit anderem Namen, Rang und Corr-Typ.
    fn variant_rule(name: &str, rank: u64, corr: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "rank": rank,
            "left": {
                "anchor": "fam",
                "nodes": [
                    {"name": "fam", "type": "Family"},
                    {"name": "father", "type": "Father"},
                    {"name": "member", "type": "Member"},
                    {"name": "first", "type": "firstName"}
                ],
                "links": [["fam", "father"], ["father", "member"],
                          ["member", "first"]]
            },
            "right": {
                "anchor": "male",
                "nodes": [
                    {"name": "male", "type": "Male"},
                    {"name": "name", "type": "name"}
                ],
                "links": [["male", "name"]]
            },
            "corrs": [
                {"type": corr, "left": "member", "right": "male",
                 "role": "establishes",
                 "bindings": [{"left": "first", "right": "name"}]}
            ]
        })
    }

    fn seed(n: usize) -> (Graph, ValueStore) {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();
        for i in 0..n {
            let f = g.add_baseline(&format!("f{i}"), "Family");
            let r = g.add_baseline(&format!("f{i}/father"), "Father");
            let m = g.add_baseline(&format!("f{i}/father/m"), "Member");
            let leaf = g.add_baseline(&format!("f{i}/father/m/fn"), "firstName");
            g.connect(f, r, Status::Solid);
            g.connect(r, m, Status::Solid);
            g.connect(m, leaf, Status::Solid);
            vs.insert(leaf, format!("John{i}"));
        }
        (g, vs)
    }

    #[test]
    fn cascade_saturates_deterministically() {
        let (mut g, vs) = seed(5);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        let t = e.run(&mut g, &vs, 10_000);
        assert_eq!(t, Termination::Duplication);
        assert_eq!(e.cascade.len(), 5);
        // Determinism: the second run is identical.
        let (mut g2, vs2) = seed(5);
        let mut e2 = Engine::new(&rules);
        let t2 = e2.run(&mut g2, &vs2, 10_000);
        assert_eq!(t2, Termination::Duplication);
        // Bidirectional (rc8-#2 ported): fwd+bwd together saturate
        // without ping-pong — the recognition stops the opposite
        // direction.
        let (mut g3, vs3) = seed(5);
        let mut l3 = father_rule(&mut g3);
        let f3 = l3.remove(0);
        let b3 = l3.remove(0);
        let both = vec![f3, b3];
        let mut e3 = Engine::new(&both);
        let t3 = e3.run(&mut g3, &vs3, 10_000);
        assert_eq!(t3, Termination::Duplication);
        assert_eq!(
            e3.cascade.len(),
            5,
            "no ping-pong: only 5 forward applications"
        );
        let a: Vec<_> = e
            .cascade
            .iter()
            .map(|x| (x.rule_ix, x.refs.clone()))
            .collect();
        let b: Vec<_> = e2
            .cascade
            .iter()
            .map(|x| (x.rule_ix, x.refs.clone()))
            .collect();
        assert_eq!(a, b, "μ order ⇒ identical cascade");
    }

    #[test]
    fn delete_kills_eagerly_via_by_element() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        assert_eq!(e.todo.len(), 2);
        // Tombstone f0's Member → its match dies IMMEDIATELY, locally.
        let member = g
            .iter_nodes()
            .find(|n| g.types.name(n.typ) == "Member")
            .unwrap()
            .id;
        g.set_node_status(&member, Status::Tombstone);
        e.element_removed(&member);
        assert_eq!(e.todo.len(), 1, "eager, no re-enum, no lazy");
        let t = e.run_remaining(&mut g, &vs);
        assert_eq!(
            t,
            Termination::Duplication,
            "one application = dead candidates"
        );
        assert_eq!(e.cascade.len(), 1, "only f1 gets translated");
    }

    impl<'r> Engine<'r> {
        /// Test helper: run without a fresh seed.
        fn run_remaining(&mut self, g: &mut Graph, resolver: &dyn ValueResolver) -> Termination {
            loop {
                if self.step(g, resolver).is_none() {
                    return self.verdict();
                }
            }
        }
    }

    #[test]
    fn retraction_walk_and_consolidation() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        let t = e.run(&mut g, &vs, 100);
        assert_eq!(t, Termination::Duplication);
        assert_eq!(e.cascade.len(), 2);
        let created_f0 = e.cascade[0].created.clone();
        // f0's Member drops out → retraction walk + consolidation.
        let member = e.cascade[0].refs[2];
        g.set_node_status(&member, Status::Tombstone);
        e.retract_for(&mut g, &member);
        e.consolidate(&mut g);
        for c in &created_f0 {
            assert_eq!(
                g.node(c).unwrap().status,
                Status::Tombstone,
                "f0's product chain (corr, Male, name) is tombstoned"
            );
        }
        // f1's products live.
        for c in &e.cascade[1].created {
            assert_eq!(g.node(c).unwrap().status, Status::Ghost);
        }
    }

    #[test]
    fn resurrection_on_a_new_derivation() {
        let (mut g, vs) = seed(1);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd.clone()];
        let mut e = Engine::new(&rules);
        let _ = e.run(&mut g, &vs, 100);
        let created = e.cascade[0].created.clone();
        let member = e.cascade[0].refs[2];
        // Retraction WITHOUT a real drop-out (Member lives) — then a
        // reapplication reclaims the same identities BEFORE
        // consolidation: resurrection instead of tombstone.
        e.retract_for(&mut g, &member);
        for c in &created {
            assert_eq!(g.node(c).unwrap().status, Status::TentativeTombstone);
        }
        let refs = e.cascade[0].refs.clone();
        let _ = crate::plan::apply_creation(&mut g, &fwd, &refs);
        e.consolidate(&mut g);
        for c in &created {
            assert_eq!(
                g.node(c).unwrap().status,
                Status::Ghost,
                "M5.5: an identical derivation reclaims the identity"
            );
        }
    }

    #[test]
    fn fold_materializes_without_tombstones() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        let _ = e.run(&mut g, &vs, 100);
        let member = e.cascade[0].refs[2];
        g.set_node_status(&member, Status::Tombstone);
        e.retract_for(&mut g, &member);
        e.consolidate(&mut g);
        let folded = g.materialize();
        assert!(folded.node(&member).is_none(), "the tombstone drops out");
        for c in &e.cascade[0].created {
            assert!(folded.node(c).is_none());
        }
        for c in &e.cascade[1].created {
            assert_eq!(folded.node(c).unwrap().status, Status::Solid, "Ghost→Solid");
        }
        // A derived leaf keeps its provenance — the value stays resolvable.
        let name_leaf = e.cascade[1].created[2];
        assert!(folded.resolve_value(&name_leaf, &vs).is_some());
    }

    #[test]
    fn v7_contradiction_on_tombstone_reuse() {
        let (mut g, vs) = seed(1);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        // Tombstone the product identity up front: derive the corr id.
        let m = crate::engine::matcher::find_matches(&g, &vs, &rules[0].pattern);
        let corr_id = crate::graph::preview_ghost_id(&m[0][2], "PersonCorr");
        let anchor = m[0][2];
        let _ = g.add_ghost(&anchor, "PersonCorr");
        g.set_node_status(&corr_id, Status::Tombstone);
        let mut e = Engine::new(&rules);
        let t = e.run(&mut g, &vs, 100);
        assert_eq!(t, Termination::Contradiction, "V7: tombstone substance");
        assert_eq!(e.cascade.len(), 0);
    }

    #[test]
    fn backtracking_bound_filters_strictly() {
        let (mut g, vs) = seed(2);
        let fwd = father_rule(&mut g).remove(0);
        let rules = vec![fwd];
        let mut e = Engine::new(&rules);
        e.seed(&g, &vs);
        // Without a bound: the highest-ranked (largest ref sequence) first.
        let first = e.step(&mut g, &vs);
        assert_eq!(first, Some(true));
        let applied_refs = e.cascade[0].refs.clone();
        // A fresh engine with the bound = the first choice ⇒ it's
        // skipped, the second one comes up.
        let (mut g2, vs2) = seed(2);
        let mut e2 = Engine::new(&rules);
        e2.seed(&g2, &vs2);
        let bound = SelectionBound {
            rank: rules[0].rank,
            refs: applied_refs.clone(),
        };
        let s = e2.step_with_limit(&mut g2, &vs2, Some(&bound));
        assert_eq!(s, Some(true));
        assert_ne!(e2.cascade[0].refs, applied_refs, "strictly below the bound");
    }

    #[test]
    #[ignore = "manual perf measurement: engine, F2P shape"]
    fn bench_v2_f2p_scaling_smoke() {
        for n in [1_000usize, 10_000, 50_000] {
            let (mut g, vs) = seed(n);
            let mut lowered = father_rule(&mut g);
            let fwd = lowered.remove(0);
            let bwd = lowered.remove(0);
            let rules = vec![fwd, bwd];
            let mut e = Engine::new(&rules);
            let t0 = std::time::Instant::now();
            let t = e.run(&mut g, &vs, 10_000_000);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(t, Termination::Duplication);
            assert_eq!(e.cascade.len(), n);
            eprintln!(
                "V2-SMOKE n={n:<7} steps={} ms={ms:>9.1} us_per_family={:.1}",
                e.cascade.len(),
                ms * 1000.0 / n as f64
            );
        }
    }

    /// The runtime mask must INTERSECT with the rank ceiling, not
    /// replace it.
    ///
    /// Two rules over the same match space. With a ceiling at the
    /// high-ranked one and both rules active, only the low-ranked one
    /// may fire. Until 2026-08-10 the mask branch searched the whole
    /// queue and handed back the high-ranked candidate, so the ceiling
    /// was silently void whenever `active` was set — exactly the
    /// combination backtracking needs.
    #[test]
    fn mask_and_ceiling_intersect() {
        let (mut g, vs) = seed(1);
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "mask_and_ceiling",
            "rules": [
                variant_rule("Low", 10, "CorrLow"),
                variant_rule("High", 900, "CorrHigh"),
            ]
        }))
        .expect("Regeldatei parst");
        let lowered = crate::rules::load_file(&file, &mut g).expect("laedt");
        let rules: Vec<DirectedRule> = lowered.into_iter().step_by(2).collect();

        let mut e = Engine::new(&rules);
        e.active = Some(vec![true, true]);
        e.seed(&g, &vs);

        // Schranke beim hochrangigen Kandidaten: nur der niedrige darf
        // gewaehlt werden.
        let bound = {
            let high = e
                .todo
                .iter()
                .find(|k| k.2 == 1)
                .expect("der hochrangige steht in der Queue")
                .clone();
            SelectionBound {
                rank: (bound_rank(&high)),
                refs: (bound_refs(&high)),
            }
        };
        assert!(e.step_with_limit(&mut g, &vs, Some(&bound)).is_some());
        assert_eq!(
            e.cascade.len(),
            1,
            "genau eine Anwendung unterhalb der Schranke"
        );
        assert_eq!(
            e.rules[e.cascade[0].rule_ix].name, "Low\u{2192}",
            "die Schranke muss den hochrangigen ausschliessen, auch mit gesetzter Maske"
        );
    }

    fn bound_rank(k: &TodoKey) -> u64 {
        k.0 .0
    }

    fn bound_refs(k: &TodoKey) -> Bindings {
        k.1 .0.clone()
    }

    #[test]
    fn rank_order_decides() {
        // Two rules, same match space, different ranks — the higher-
        // ranked one applies first.
        let (mut g, vs) = seed(1);
        // Two rules over the same match space, different ranks and
        // different corr types so both actually create.
        let file: RuleFile = serde_json::from_value(serde_json::json!({
            "format": 3,
            "name": "rank_order",
            "rules": [
                variant_rule("Low", 10, "CorrLow"),
                variant_rule("High", 900, "CorrHigh"),
            ]
        }))
        .expect("Regeldatei parst");
        let mut lowered = crate::rules::load_file(&file, &mut g).expect("laedt");
        let hf = lowered.remove(2);
        let lf = lowered.remove(0);
        let rules = vec![lf, hf];
        let mut e = Engine::new(&rules);
        let _ = e.run(&mut g, &vs, 100);
        assert_eq!(e.cascade[0].rank, 900, "the highest rank first");
    }
}
