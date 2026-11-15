// Part of `plan` (included by `plan.rs`): the names a rule's plan
// refers to, with the ids they resolved to. The type of every node a
// rule creates, the constant and the transformation chain of its
// leaves and the types of its correspondence recognition are resolved
// in a graph's tables once and kept per rule. 2.0.2 resolved them at
// every use.
//
// A name is resolved at the moment 2.0.2 resolved it, the first time,
// so the tables receive their entries in the same order and every id
// is the same. What is kept is a fact
// about one table: the tables only grow, and each table instance
// carries a number of its own (`TableUid`), a copy another one. The
// ids are dropped when the graph at hand has other tables or the
// engine other rules.

use crate::graph::{KonstId, LeafTransform};

/// The names of every rule of one engine, with the ids they resolved
/// to.
#[derive(Debug, Default)]
pub(crate) struct Names {
    /// The rule slice the entries belong to (address and length: the
    /// engine's `rules` is public and may be set to another slice,
    /// which lives as long as the first).
    rules: (usize, usize),
    /// The tables the ids were resolved in: types, constants, chains.
    /// No table has the number 0.
    tables: (u64, u64, u64),
    /// Per rule where its places start in `create` and in
    /// `recognition`, and one more entry for the end.
    starts: Vec<(u32, u32)>,
    /// One place per node of every creation plan, rule after rule.
    create: Vec<Kept>,
    /// One place per entry of every `corr_recognition`, rule after
    /// rule: correspondence type, endpoint type.
    recognition: Vec<(Option<TypeId>, Option<TypeId>)>,
}

/// The names of one rule outside an engine, with the ids they resolved
/// to.
#[derive(Debug)]
pub(crate) struct OwnNames {
    create: Vec<Kept>,
    recognition: Vec<(Option<TypeId>, Option<TypeId>)>,
}

/// The names of one rule, with the ids they resolved to, for the time
/// of a step.
#[derive(Debug)]
pub(crate) struct RuleNames<'a> {
    /// Per node of the creation plan.
    create: &'a mut [Kept],
    /// Per entry of `corr_recognition`.
    recognition: &'a mut [(Option<TypeId>, Option<TypeId>)],
}

/// What is kept for one node of a creation plan. `None`: not resolved
/// yet.
#[derive(Debug, Default, Clone)]
struct Kept {
    /// The type of the node.
    typ: Option<TypeId>,
    /// The type of the source leaf of a dynamic leaf.
    attr: Option<TypeId>,
    /// The chain of a derived leaf.
    transform: Option<LeafTransform>,
    /// The constant of a constant leaf.
    konst: Option<KonstId>,
    /// The type of the attribute correspondence of a dynamic leaf: its
    /// name and its id.
    attr_corr: Option<(String, TypeId)>,
}

/// The type `name` in the tables of `g`, interned the first time.
#[inline]
fn typ_of(kept: &mut Option<TypeId>, g: &mut Graph, name: &str) -> TypeId {
    match *kept {
        Some(typ) => typ,
        None => {
            let typ = g.types.intern(name);
            *kept = Some(typ);
            typ
        }
    }
}

/// The type `name` if the tables of `g` know it, looked up until they
/// do.
#[inline]
fn known_typ(kept: &mut Option<TypeId>, g: &Graph, name: &str) -> Option<TypeId> {
    if kept.is_none() {
        *kept = g.types.lookup(name);
    }
    *kept
}

impl Names {
    /// The names of rule `rule_ix` of `rules`, for a step against `g`.
    #[inline]
    pub(crate) fn for_step(
        &mut self,
        g: &Graph,
        rules: &[DirectedRule],
        rule_ix: usize,
    ) -> RuleNames<'_> {
        let tables = (g.types.uid.get(), g.konsts.uid.get(), g.chains.uid.get());
        let slice = (rules.as_ptr() as usize, rules.len());
        if self.tables != tables || self.rules != slice {
            self.forget(rules);
            self.tables = tables;
            self.rules = slice;
        }
        let (from, to) = (self.starts[rule_ix], self.starts[rule_ix + 1]);
        RuleNames {
            create: &mut self.create[from.0 as usize..to.0 as usize],
            recognition: &mut self.recognition[from.1 as usize..to.1 as usize],
        }
    }

    /// Empty places for `rules`: one allocation each for all plans and
    /// all recognitions, not one per rule (a rule set of thousands of
    /// rules of which each is applied once or twice would pay more for
    /// the places than it saves).
    fn forget(&mut self, rules: &[DirectedRule]) {
        self.starts.clear();
        let (mut create, mut recognition) = (0u32, 0u32);
        for rule in rules {
            self.starts.push((create, recognition));
            create += rule.create_nodes.len() as u32;
            recognition += rule.corr_recognition.len() as u32;
        }
        self.starts.push((create, recognition));
        self.create.clear();
        self.create.resize(create as usize, Kept::default());
        self.recognition.clear();
        self.recognition.resize(recognition as usize, (None, None));
    }
}

impl OwnNames {
    pub(crate) fn of(rule: &DirectedRule) -> Self {
        Self {
            create: vec![Kept::default(); rule.create_nodes.len()],
            recognition: vec![(None, None); rule.corr_recognition.len()],
        }
    }

    pub(crate) fn names(&mut self) -> RuleNames<'_> {
        RuleNames {
            create: &mut self.create,
            recognition: &mut self.recognition,
        }
    }
}

impl RuleNames<'_> {
    /// The correspondence node `id` of plan position `plan_ix`.
    #[inline]
    pub(crate) fn add_corr(
        &mut self,
        g: &mut Graph,
        plan_ix: usize,
        id: GhostId,
        typ: &str,
    ) -> GhostId {
        let typ = typ_of(&mut self.create[plan_ix].typ, g, typ);
        g.add_corr_typed(id, typ)
    }

    /// The ghost node `id` of plan position `plan_ix`.
    #[inline]
    pub(crate) fn add_ghost(
        &mut self,
        g: &mut Graph,
        plan_ix: usize,
        id: GhostId,
        typ: &str,
    ) -> GhostId {
        let typ = typ_of(&mut self.create[plan_ix].typ, g, typ);
        g.add_ghost_typed(id, typ)
    }

    /// The derived leaf `id` of plan position `plan_ix`.
    #[inline]
    pub(crate) fn add_derived_leaf(
        &mut self,
        g: &mut Graph,
        plan_ix: usize,
        id: GhostId,
        typ: &str,
        source: GhostId,
        transform: &PlanTransform,
    ) -> GhostId {
        let kept = &mut self.create[plan_ix];
        let transform = match kept.transform {
            Some(transform) => transform,
            None => {
                let interned = g.intern_plan(transform);
                kept.transform = Some(interned);
                interned
            }
        };
        let typ = typ_of(&mut kept.typ, g, typ);
        g.add_derived_leaf_typed(id, typ, source, transform)
    }

    /// The constant leaf `id` of plan position `plan_ix`.
    #[inline]
    pub(crate) fn add_konst_leaf(
        &mut self,
        g: &mut Graph,
        plan_ix: usize,
        id: GhostId,
        typ: &str,
        value: &str,
    ) -> GhostId {
        let kept = &mut self.create[plan_ix];
        let typ = typ_of(&mut kept.typ, g, typ);
        let konst = match kept.konst {
            Some(konst) => konst,
            None => {
                let konst = g.konsts.intern(value);
                kept.konst = Some(konst);
                konst
            }
        };
        g.add_konst_leaf_typed(id, typ, konst)
    }

    /// The attribute correspondence of the dynamic leaf at plan
    /// position `plan_ix`: a correspondence of type `{corr}_{typ}` at
    /// the leaf's source.
    pub(crate) fn add_attr_corr(
        &mut self,
        g: &mut Graph,
        plan_ix: usize,
        source: &GhostId,
        corr: &str,
        typ: &str,
        bindings: &[GhostId],
    ) -> GhostId {
        let (name, typ) = self.create[plan_ix].attr_corr.get_or_insert_with(|| {
            let name = format!("{corr}_{typ}");
            let typ = g.types.intern(&name);
            (name, typ)
        });
        let id = crate::graph::preview_corr_id(source, name, bindings);
        g.add_corr_typed(id, *typ)
    }

    /// The source leaf of the dynamic leaf at plan position `plan_ix`:
    /// the first matchable child of `anchor` with the type `attr`.
    #[inline]
    pub(crate) fn child_leaf(
        &mut self,
        g: &Graph,
        plan_ix: usize,
        anchor: &GhostId,
        attr: &str,
    ) -> Option<GhostId> {
        let typ = known_typ(&mut self.create[plan_ix].attr, g, attr)?;
        g.child_leaf_of_typ(anchor, typ)
    }

    /// The correspondence type of entry `ix` of the rule's
    /// `corr_recognition`, if the graph knows it.
    #[inline]
    pub(crate) fn corr_typ(&mut self, g: &Graph, ix: usize, typ: &str) -> Option<TypeId> {
        known_typ(&mut self.recognition[ix].0, g, typ)
    }

    /// The endpoint type of entry `ix` of the rule's
    /// `corr_recognition`, if the graph knows it.
    #[inline]
    pub(crate) fn endpoint_typ(&mut self, g: &Graph, ix: usize, typ: &str) -> Option<TypeId> {
        known_typ(&mut self.recognition[ix].1, g, typ)
    }
}
