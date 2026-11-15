//! Values compared by number (the feature `opt_f` is on).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crate::graph::{Graph, Node, ValueResolver};
use crate::hash::FxHashMap;
use crate::rules::predicate::Predicate;

/// The numbers of the constants the `Equals` predicates of one rule
/// set compare values with, as an anchor index gives them: one number
/// per distinct text, fixed once the index is built.
///
/// A resolver that keeps a number beside each value
/// (`ValueResolver::baseline_number`) keeps the number that the text of
/// the value has here ([`ValueNumbers::number_of`]) and notes which
/// numbering it belongs to ([`ValueNumbers::token`]).
#[derive(Debug)]
pub struct ValueNumbers {
    token: u32,
    by_text: FxHashMap<String, u32>,
}

/// The tokens handed out so far. A token is never given twice in one
/// process (short of 2^32 numberings), so a number noted with a token
/// belongs to the numbering that carries it.
static TOKENS: AtomicU32 = AtomicU32::new(0);

impl ValueNumbers {
    /// The number of a text that is none of the constants.
    pub const NONE: u32 = u32::MAX;

    pub(crate) fn new() -> Self {
        let mut token = 0;
        // 0 marks a cell that holds no number.
        while token == 0 {
            token = TOKENS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        }
        Self {
            token,
            by_text: FxHashMap::default(),
        }
    }

    /// Identifies this numbering among all of the process; never 0.
    #[must_use]
    pub fn token(&self) -> u32 {
        self.token
    }

    /// The number of `text`, [`ValueNumbers::NONE`] if it is none of
    /// the constants.
    #[must_use]
    pub fn number_of(&self, text: &str) -> u32 {
        self.by_text.get(text).copied().unwrap_or(Self::NONE)
    }

    /// The number of the constant `text`, a new one if it has none yet.
    fn number(&mut self, text: &str) -> u32 {
        if let Some(&n) = self.by_text.get(text) {
            return n;
        }
        let n = self.by_text.len() as u32;
        assert!(n < UNKNOWN, "more constants than numbers");
        self.by_text.insert(text.to_string(), n);
        n
    }
}

/// The numbering of an anchor index. The index fills it while it is
/// built; the plans of its searches share it afterwards.
#[derive(Debug)]
pub(crate) struct Numbering(Arc<ValueNumbers>);

impl Default for Numbering {
    fn default() -> Self {
        Self(Arc::new(ValueNumbers::new()))
    }
}

impl Numbering {
    #[inline]
    pub(crate) fn get(&self) -> Numbers<'_> {
        Numbers(&self.0)
    }

    pub(crate) fn edit(&mut self) -> NumbersMut<'_> {
        NumbersMut(
            Arc::get_mut(&mut self.0)
                .expect("no plan shares the numbering while the index is built"),
        )
    }

    pub(crate) fn share(&self) -> Shared {
        Shared(Some(self.0.clone()))
    }
}

/// The numbering, for reading.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Numbers<'a>(&'a ValueNumbers);

/// The numbering, for numbering a further constant.
#[derive(Debug)]
pub(crate) struct NumbersMut<'a>(&'a mut ValueNumbers);

impl NumbersMut<'_> {
    #[inline]
    pub(crate) fn read(&self) -> Numbers<'_> {
        Numbers(self.0)
    }
}

/// What a search plan keeps of the numbering of the index that built
/// it. A plan built without an index keeps none.
#[derive(Debug, Default)]
pub(crate) struct Shared(Option<Arc<ValueNumbers>>);

/// What a step of a search plan keeps of the constant its node is
/// compared with: the number of the constant,
/// [`ValueNumbers::NONE`] without one.
#[derive(Debug)]
pub(crate) struct Wanted(u32);

impl Default for Wanted {
    fn default() -> Self {
        Self(ValueNumbers::NONE)
    }
}

pub(crate) fn wanted(value: Option<&Predicate>, numbers: Numbers<'_>) -> Wanted {
    match value {
        Some(Predicate::Equals(text)) => Wanted(numbers.0.number_of(text)),
        _ => Wanted::default(),
    }
}

/// What a value check knows from the plan: the numbering of the index
/// that built the plan and the number of the constant of the step.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hint<'a> {
    numbers: Option<&'a ValueNumbers>,
    wanted: u32,
}

impl<'a> Hint<'a> {
    #[inline]
    pub(crate) fn of(shared: &'a Shared, wanted: &Wanted) -> Self {
        Self {
            numbers: shared.0.as_deref(),
            wanted: wanted.0,
        }
    }
}

/// Beside a stored value: the number of its text in the numbering that
/// last asked for it, with that numbering's token (upper half the
/// token, lower half the number; 0 while nobody asked).
///
/// One word, read and written whole: threads that ask at the same time
/// write the same word for the same numbering, and a reader sees either
/// a word of its numbering or one it replaces.
#[derive(Debug, Default)]
pub(crate) struct Cell(AtomicU64);

impl Clone for Cell {
    /// The copy stands beside a copy of the same text.
    fn clone(&self) -> Self {
        Self(AtomicU64::new(self.0.load(Ordering::Relaxed)))
    }
}

impl Cell {
    /// The number of `text`, the text this cell stands beside.
    #[inline]
    pub(crate) fn number(&self, text: &str, numbers: &ValueNumbers) -> u32 {
        let word = self.0.load(Ordering::Relaxed);
        if (word >> 32) as u32 == numbers.token {
            return word as u32;
        }
        let n = numbers.number_of(text);
        self.0.store(
            (u64::from(numbers.token) << 32) | u64::from(n),
            Ordering::Relaxed,
        );
        n
    }
}

/// The value of a node as a lookup sees it: its text and, where the
/// store keeps it, the number of that text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seen<'a> {
    text: Option<&'a str>,
    /// [`UNKNOWN`]: no number known, the text decides.
    number: u32,
}

/// In [`Seen`]: the store keeps no number for the value. No constant
/// has this number: a numbering stays far below it.
const UNKNOWN: u32 = u32::MAX - 1;

impl<'a> Seen<'a> {
    #[inline]
    pub(crate) fn none() -> Self {
        Self {
            text: None,
            number: UNKNOWN,
        }
    }

    #[inline]
    pub(crate) fn text(self) -> Option<&'a str> {
        self.text
    }
}

/// Things filed by the constant of an `Equals` predicate, under its
/// number: a table over the range of the numbers filed here, so that a
/// lookup is one index. The constants of one place of the index are
/// numbered close together (rule after rule), the range stays near the
/// count.
#[derive(Debug)]
pub(crate) struct ByValue<T> {
    /// The number of `slots[0]`.
    base: u32,
    slots: Vec<Option<T>>,
}

impl<T> Default for ByValue<T> {
    fn default() -> Self {
        Self {
            base: 0,
            slots: Vec::new(),
        }
    }
}

impl<T> ByValue<T> {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// What is filed under the number `n`. [`ValueNumbers::NONE`] and
    /// every other number outside the range has nothing.
    #[inline]
    fn at(&self, n: u32) -> Option<&T> {
        self.slots.get(n.wrapping_sub(self.base) as usize)?.as_ref()
    }

    /// What is filed under the value a node has.
    #[inline]
    pub(crate) fn get(&self, seen: Seen<'_>, numbers: Numbers<'_>) -> Option<&T> {
        if seen.number != UNKNOWN {
            return self.at(seen.number);
        }
        self.at(numbers.0.number_of(seen.text?))
    }

    /// What is filed under the constant `text`.
    pub(crate) fn find(&self, text: &str, numbers: Numbers<'_>) -> Option<&T> {
        self.at(numbers.0.number_of(text))
    }

    pub(crate) fn find_mut(&mut self, text: &str, numbers: Numbers<'_>) -> Option<&mut T> {
        let n = numbers.0.number_of(text);
        self.slots
            .get_mut(n.wrapping_sub(self.base) as usize)?
            .as_mut()
    }

    /// Files `value` under the constant `text`.
    pub(crate) fn put(&mut self, text: &str, numbers: NumbersMut<'_>, value: T) {
        let n = numbers.0.number(text);
        if self.slots.is_empty() {
            self.base = n;
        } else if n < self.base {
            let more = (self.base - n) as usize;
            self.slots.splice(0..0, (0..more).map(|_| None));
            self.base = n;
        }
        let at = (n - self.base) as usize;
        if at >= self.slots.len() {
            self.slots.resize_with(at + 1, || None);
        }
        self.slots[at] = Some(value);
    }
}

/// Hands the value of `node` to `f`: [`Graph::with_node_value`] with
/// the number of the value where its store keeps one. The cases are
/// those of `with_node_value`, in its order.
#[inline]
pub(crate) fn see<R>(
    g: &Graph,
    resolver: &dyn ValueResolver,
    node: &Node,
    numbers: Numbers<'_>,
    f: impl FnOnce(Seen<'_>) -> R,
) -> R {
    let numbers = numbers.0;
    if let Some(k) = node.konst {
        trace!(value_resolutions);
        let text = g.konsts.value(k);
        let number = g.konsts.cell(k).number(text, numbers);
        return f(Seen {
            text: Some(text),
            number,
        });
    }
    if node.derived.is_none() {
        if let Some((text, number)) = resolver.baseline_number(&node.id, numbers) {
            trace!(value_resolutions);
            return f(Seen {
                text: Some(text),
                number,
            });
        }
        if let Some(text) = resolver.baseline_str(&node.id) {
            trace!(value_resolutions);
            return f(Seen {
                text: Some(text),
                number: UNKNOWN,
            });
        }
    }
    f(Seen {
        text: g.resolve_value(&node.id, resolver).as_deref(),
        number: UNKNOWN,
    })
}

/// Does the value of `node` satisfy `p`? Where the plan knows the
/// number of the constant of an `Equals` predicate and the value has a
/// number, the two numbers are compared.
#[inline]
pub(crate) fn holds(
    g: &Graph,
    resolver: &dyn ValueResolver,
    p: &Predicate,
    node: &Node,
    hint: Hint<'_>,
) -> bool {
    if hint.wanted != ValueNumbers::NONE {
        if let Some(numbers) = hint.numbers {
            return see(g, resolver, node, Numbers(numbers), |seen| {
                if seen.number == UNKNOWN {
                    p.matches(seen.text)
                } else {
                    seen.number == hint.wanted
                }
            });
        }
    }
    g.with_node_value(node, resolver, |v| p.matches(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::ValueStore;
    use crate::ident::GhostId;

    fn numbers(texts: &[&str]) -> ValueNumbers {
        let mut n = ValueNumbers::new();
        for t in texts {
            n.number(t);
        }
        n
    }

    /// The table answers like a map from the constant to what was filed,
    /// whatever the order of the numbers filed and the gaps between
    /// them.
    #[test]
    fn the_table_answers_like_a_map() {
        let texts: Vec<String> = (0..40).map(|i| format!("v{i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let mut all = numbers(&refs);
        // Filed: every third constant, the later ones first.
        let mut table: ByValue<usize> = ByValue::default();
        let mut map = std::collections::HashMap::new();
        assert!(table.is_empty());
        for i in [30usize, 12, 36, 3, 21, 9] {
            table.put(&texts[i], NumbersMut(&mut all), i);
            map.insert(texts[i].clone(), i);
        }
        assert!(!table.is_empty());
        for t in &texts {
            assert_eq!(table.find(t, Numbers(&all)), map.get(t), "{t}");
            let seen = Seen {
                text: Some(t),
                number: all.number_of(t),
            };
            assert_eq!(table.get(seen, Numbers(&all)), map.get(t), "{t} by number");
            let seen = Seen {
                text: Some(t),
                number: UNKNOWN,
            };
            assert_eq!(table.get(seen, Numbers(&all)), map.get(t), "{t} by text");
        }
        // A text that is none of the constants, with and without number.
        assert_eq!(table.find("other", Numbers(&all)), None);
        let seen = Seen {
            text: Some("other"),
            number: ValueNumbers::NONE,
        };
        assert_eq!(table.get(seen, Numbers(&all)), None);
        assert_eq!(table.get(Seen::none(), Numbers(&all)), None);
        if let Some(v) = table.find_mut(&texts[12], Numbers(&all)) {
            *v = 99;
        }
        assert_eq!(table.find(&texts[12], Numbers(&all)), Some(&99));
    }

    /// A cell answers with the number of its text in the numbering that
    /// asks, also when two numberings ask in turn.
    #[test]
    fn a_cell_follows_the_numbering_that_asks() {
        let a = numbers(&["x", "y"]);
        let b = numbers(&["y", "x", "z"]);
        assert_ne!(a.token(), b.token());
        let cell = Cell::default();
        for _ in 0..3 {
            assert_eq!(cell.number("y", &a), a.number_of("y"));
            assert_eq!(cell.number("y", &b), b.number_of("y"));
        }
        let none = Cell::default();
        assert_eq!(none.number("q", &a), ValueNumbers::NONE);
        assert_eq!(none.number("q", &a), ValueNumbers::NONE);
        // The copy of a cell stands beside a copy of the same text.
        assert_eq!(cell.clone().number("y", &b), b.number_of("y"));
    }

    /// The store answers with the number of the value it holds now: a
    /// value written again takes a new cell.
    #[test]
    fn the_store_numbers_the_value_it_holds() {
        let n = numbers(&["k1", "k2"]);
        let id = GhostId::from_raw([7; 32]);
        let mut vs = ValueStore::default();
        vs.insert(id, "k1");
        assert_eq!(vs.baseline_number(&id, &n), Some(("k1", n.number_of("k1"))));
        vs.insert(id, "k2");
        assert_eq!(vs.baseline_number(&id, &n), Some(("k2", n.number_of("k2"))));
        vs.insert(id, "elsewhere");
        assert_eq!(
            vs.baseline_number(&id, &n),
            Some(("elsewhere", ValueNumbers::NONE))
        );
        assert_eq!(vs.baseline_number(&GhostId::from_raw([8; 32]), &n), None);
    }
}
