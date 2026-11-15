// The names a rule's plan refers to, resolved in a graph's tables at
// every use: a type, a constant and a transformation chain are
// interned each time a node is created, a type is looked up each time
// it is asked for. This is the engine without the feature `opt_g`;
// with it `names_kept.rs` takes this file's place behind the same
// interface.

/// The names of every rule of one engine. Nothing is kept.
#[derive(Debug, Default)]
pub(crate) struct Names;

/// The names of one rule outside an engine. Nothing is kept.
#[derive(Debug)]
pub(crate) struct OwnNames;

/// The names of one rule, for the time of a step. Nothing is kept.
#[derive(Debug)]
pub(crate) struct RuleNames<'a>(std::marker::PhantomData<&'a mut ()>);

impl Names {
    /// The names of rule `rule_ix` of `rules`, for a step against `g`.
    #[inline(always)]
    pub(crate) fn for_step(
        &mut self,
        _g: &Graph,
        _rules: &[DirectedRule],
        _rule_ix: usize,
    ) -> RuleNames<'_> {
        RuleNames(std::marker::PhantomData)
    }
}

impl OwnNames {
    #[inline(always)]
    pub(crate) fn of(_rule: &DirectedRule) -> Self {
        Self
    }

    #[inline(always)]
    pub(crate) fn names(&mut self) -> RuleNames<'_> {
        RuleNames(std::marker::PhantomData)
    }
}

impl RuleNames<'_> {
    /// The correspondence node `id` of plan position `plan_ix`.
    #[inline(always)]
    pub(crate) fn add_corr(
        &mut self,
        g: &mut Graph,
        _plan_ix: usize,
        id: GhostId,
        typ: &str,
    ) -> GhostId {
        g.add_corr_at(id, typ)
    }

    /// The ghost node `id` of plan position `plan_ix`.
    #[inline(always)]
    pub(crate) fn add_ghost(
        &mut self,
        g: &mut Graph,
        _plan_ix: usize,
        id: GhostId,
        typ: &str,
    ) -> GhostId {
        g.add_ghost_at(id, typ)
    }

    /// The derived leaf `id` of plan position `plan_ix`.
    #[inline(always)]
    pub(crate) fn add_derived_leaf(
        &mut self,
        g: &mut Graph,
        _plan_ix: usize,
        id: GhostId,
        typ: &str,
        source: GhostId,
        transform: &PlanTransform,
    ) -> GhostId {
        g.add_derived_leaf_at(id, typ, source, transform)
    }

    /// The constant leaf `id` of plan position `plan_ix`.
    #[inline(always)]
    pub(crate) fn add_konst_leaf(
        &mut self,
        g: &mut Graph,
        _plan_ix: usize,
        id: GhostId,
        typ: &str,
        value: &str,
    ) -> GhostId {
        g.add_konst_leaf_at(id, typ, value)
    }

    /// The attribute correspondence of the dynamic leaf at plan
    /// position `plan_ix`: a correspondence of type `{corr}_{typ}` at
    /// the leaf's source.
    #[inline(always)]
    pub(crate) fn add_attr_corr(
        &mut self,
        g: &mut Graph,
        _plan_ix: usize,
        source: &GhostId,
        corr: &str,
        typ: &str,
        bindings: &[GhostId],
    ) -> GhostId {
        g.add_corr(source, &format!("{corr}_{typ}"), bindings)
    }

    /// The source leaf of the dynamic leaf at plan position `plan_ix`:
    /// the first matchable child of `anchor` with the type `attr`.
    #[inline(always)]
    pub(crate) fn child_leaf(
        &mut self,
        g: &Graph,
        _plan_ix: usize,
        anchor: &GhostId,
        attr: &str,
    ) -> Option<GhostId> {
        g.child_leaf_of_type(anchor, attr)
    }

    /// The correspondence type of entry `ix` of the rule's
    /// `corr_recognition`, if the graph knows it.
    #[inline(always)]
    pub(crate) fn corr_typ(&mut self, g: &Graph, _ix: usize, typ: &str) -> Option<TypeId> {
        g.types.lookup(typ)
    }

    /// The endpoint type of entry `ix` of the rule's
    /// `corr_recognition`, if the graph knows it.
    #[inline(always)]
    pub(crate) fn endpoint_typ(&mut self, g: &Graph, _ix: usize, typ: &str) -> Option<TypeId> {
        g.types.lookup(typ)
    }
}
