//! Values compared by their text (the feature `opt_f` is off).

use std::marker::PhantomData;

use crate::graph::{Graph, Node, ValueResolver};
use crate::hash::FxHashMap;
use crate::rules::predicate::Predicate;

/// The numbering of an anchor index: nothing.
#[derive(Debug, Default)]
pub(crate) struct Numbering {}

impl Numbering {
    #[inline]
    pub(crate) fn get(&self) -> Numbers<'_> {
        Numbers(PhantomData)
    }

    #[inline]
    pub(crate) fn edit(&mut self) -> NumbersMut<'_> {
        NumbersMut(PhantomData)
    }

    #[inline]
    pub(crate) fn share(&self) -> Shared {
        Shared {}
    }
}

/// The numbering, for reading: nothing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Numbers<'a>(PhantomData<&'a ()>);

/// The numbering, for numbering a further constant: nothing.
#[derive(Debug)]
pub(crate) struct NumbersMut<'a>(PhantomData<&'a mut ()>);

impl NumbersMut<'_> {
    #[inline]
    pub(crate) fn read(&self) -> Numbers<'_> {
        Numbers(PhantomData)
    }
}

/// What a search plan keeps of the numbering: nothing.
#[derive(Debug, Default)]
pub(crate) struct Shared {}

/// What a step of a search plan keeps of its constant: nothing.
#[derive(Debug, Default)]
pub(crate) struct Wanted {}

#[inline]
pub(crate) fn wanted(_value: Option<&Predicate>, _numbers: Numbers<'_>) -> Wanted {
    Wanted {}
}

/// What a value check knows from the plan: nothing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hint<'a>(PhantomData<&'a ()>);

impl<'a> Hint<'a> {
    #[inline]
    pub(crate) fn of(_shared: &'a Shared, _wanted: &Wanted) -> Self {
        Self(PhantomData)
    }
}

/// Beside a stored value: nothing.
#[derive(Debug, Default, Clone)]
pub(crate) struct Cell {}

/// The value of a node as a lookup sees it: its text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seen<'a>(Option<&'a str>);

impl<'a> Seen<'a> {
    #[inline]
    pub(crate) fn none() -> Self {
        Self(None)
    }

    #[inline]
    pub(crate) fn text(self) -> Option<&'a str> {
        self.0
    }
}

/// Things filed by the constant of an `Equals` predicate.
#[derive(Debug)]
pub(crate) struct ByValue<T>(FxHashMap<String, T>);

impl<T> Default for ByValue<T> {
    fn default() -> Self {
        Self(FxHashMap::default())
    }
}

impl<T> ByValue<T> {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What is filed under the value a node has.
    #[inline]
    pub(crate) fn get(&self, seen: Seen<'_>, _numbers: Numbers<'_>) -> Option<&T> {
        seen.0.and_then(|s| self.0.get(s))
    }

    /// What is filed under the constant `text`.
    #[inline]
    pub(crate) fn find(&self, text: &str, _numbers: Numbers<'_>) -> Option<&T> {
        self.0.get(text)
    }

    #[inline]
    pub(crate) fn find_mut(&mut self, text: &str, _numbers: Numbers<'_>) -> Option<&mut T> {
        self.0.get_mut(text)
    }

    /// Files `value` under the constant `text`.
    #[inline]
    pub(crate) fn put(&mut self, text: &str, _numbers: NumbersMut<'_>, value: T) {
        self.0.insert(text.to_string(), value);
    }
}

/// Hands the value of `node` to `f` ([`Graph::with_node_value`]).
#[inline]
pub(crate) fn see<R>(
    g: &Graph,
    resolver: &dyn ValueResolver,
    node: &Node,
    _numbers: Numbers<'_>,
    f: impl FnOnce(Seen<'_>) -> R,
) -> R {
    g.with_node_value(node, resolver, |v| f(Seen(v)))
}

/// Does the value of `node` satisfy `p`?
#[inline]
pub(crate) fn holds(
    g: &Graph,
    resolver: &dyn ValueResolver,
    p: &Predicate,
    node: &Node,
    _hint: Hint<'_>,
) -> bool {
    g.with_node_value(node, resolver, |v| p.matches(v))
}
