//! Transformations as chains of domain-free primitives.
//!
//! The chain enters the identity of derived leaves via `ident_bytes`.
//! Arguments are part of the identity.

use crate::hash::FxHashMap;

/// A primitive. `Prefix`/`Suffix` carry their argument.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Prim {
    Identity,
    Capitalize,
    Decapitalize,
    Prefix(String),
    Suffix(String),
    /// Only produced as an inverse, never read from the format.
    StripPrefix(String),
    StripSuffix(String),
}

impl Prim {
    fn apply(&self, input: &str) -> Option<String> {
        match self {
            Self::Identity => Some(input.to_string()),
            Self::Capitalize => Some(change_first(input, true)),
            Self::Decapitalize => Some(change_first(input, false)),
            Self::Prefix(p) => Some(format!("{p}{input}")),
            Self::Suffix(s) => Some(format!("{input}{s}")),
            Self::StripPrefix(p) => input.strip_prefix(p.as_str()).map(str::to_string),
            Self::StripSuffix(s) => input.strip_suffix(s.as_str()).map(str::to_string),
        }
    }

    fn inverse(&self) -> Prim {
        match self {
            Self::Identity => Self::Identity,
            Self::Capitalize => Self::Decapitalize,
            Self::Decapitalize => Self::Capitalize,
            Self::Prefix(p) => Self::StripPrefix(p.clone()),
            Self::Suffix(s) => Self::StripSuffix(s.clone()),
            Self::StripPrefix(p) => Self::Prefix(p.clone()),
            Self::StripSuffix(s) => Self::Suffix(s.clone()),
        }
    }
}

fn change_first(input: &str, upper: bool) -> String {
    let mut it = input.chars();
    match it.next() {
        None => String::new(),
        Some(c) => {
            let head: String = if upper {
                c.to_uppercase().collect()
            } else {
                c.to_lowercase().collect()
            };
            head + it.as_str()
        }
    }
}

/// A chain, applied in list order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Chain(pub Vec<Prim>);

impl Chain {
    pub fn apply(&self, input: &str) -> Option<String> {
        let mut cur = input.to_string();
        for p in &self.0 {
            cur = p.apply(&cur)?;
        }
        Some(cur)
    }

    /// Element-wise reversed.
    pub fn inverse(&self) -> Chain {
        Chain(self.0.iter().rev().map(Prim::inverse).collect())
    }

    /// Backward direction with a consistency check: returns the source
    /// for a target value, but only if the forward chain maps that
    /// source back onto the target value. The criterion is consistency
    /// with the target value, not equality with an original that the
    /// backward direction doesn't know.
    pub fn invert_checked(&self, target: &str) -> Option<String> {
        let source = self.inverse().apply(target)?;
        (self.apply(&source).as_deref() == Some(target)).then_some(source)
    }

    /// ONE normal form under the three rules listed below -- not the
    /// shortest chain with the same effect. Effect-preserving
    /// shortenings not covered by these rules are deliberately left as
    /// is (`[Capitalize, Capitalize]`, `[Prefix(a), StripPrefix(a)]`;
    /// reasoning under CONSIDERED AND REJECTED below).
    ///
    /// Without them, the encoding would still be injective (different
    /// chains yield different bytes), but not canonical: semantically
    /// equal chains would have different identities, and two rules
    /// that write the same transformation differently would produce
    /// two derived nodes instead of one. With the earlier enum tag
    /// this was impossible; with chains it's reachable --
    /// `PrimDecl::Identity` and an empty prefix argument are both
    /// writable from the format.
    ///
    /// Three rules, each effect-preserving:
    /// 1. Drop `Identity` (`apply` is the identity function).
    /// 2. Drop affix steps with an empty argument. `Prefix("")` and
    ///    `Suffix("")` return the input unchanged; `StripPrefix("")`
    ///    and `StripSuffix("")` always succeed and return it unchanged.
    /// 3. Merge adjacent same-kind affix steps. The chain runs in list
    ///    order, so the argument order differs by kind:
    ///    `[Prefix(a), Prefix(b)]` = `Prefix(b ++ a)`,
    ///    `[Suffix(a), Suffix(b)]` = `Suffix(a ++ b)`,
    ///    `[StripPrefix(a), StripPrefix(b)]` = `StripPrefix(a ++ b)`,
    ///    `[StripSuffix(a), StripSuffix(b)]` = `StripSuffix(b ++ a)`.
    ///    Also equal in the failure case: the merged form fails
    ///    exactly when one of the two individual steps fails.
    ///
    /// CONSIDERED AND REJECTED:
    /// - `[Capitalize, Capitalize]` → `[Capitalize]` (analogous for
    ///   `Decapitalize`). Idempotence hinges on Unicode edge cases
    ///   (multi-character uppercasing like `ß` → `SS`, `ŉ` → `ʼN`)
    ///   that I can't prove, only spot-check. From the format the form
    ///   isn't sensibly writable anyway.
    /// - `[Capitalize, Decapitalize]` → `[Decapitalize]`. WRONG:
    ///   `Capitalize` of `ß` is `SS`, `Decapitalize` of that is `sS`,
    ///   a direct `Decapitalize` would be `ß`.
    /// - `[Prefix(a), StripPrefix(a)]` → `[]` (effect-equal, because
    ///   `StripPrefix` can never fail here). Not adopted: the
    ///   shortening can create new adjacencies and would need a
    ///   fixpoint iteration, and `StripPrefix` isn't writable from the
    ///   format at all -- it only arises from `inverse`, and never
    ///   next to its own `Prefix` there.
    /// - Reordering non-adjacent same-kind steps. Affixes and case
    ///   operations don't commute (`[Capitalize, Prefix("get")]`
    ///   against `[Prefix("get"), Capitalize]`).
    pub fn normalized(&self) -> Chain {
        let mut out: Vec<Prim> = Vec::with_capacity(self.0.len());
        for p in &self.0 {
            // Rules 1 + 2: no-op steps drop out.
            match p {
                Prim::Identity => continue,
                Prim::Prefix(a) | Prim::Suffix(a) | Prim::StripPrefix(a) | Prim::StripSuffix(a)
                    if a.is_empty() =>
                {
                    continue
                }
                _ => {}
            }
            // Rule 3: merge with the predecessor if same-kind.
            let merged = match (out.last(), p) {
                (Some(Prim::Prefix(x)), Prim::Prefix(y)) => Some(Prim::Prefix(format!("{y}{x}"))),
                (Some(Prim::Suffix(x)), Prim::Suffix(y)) => Some(Prim::Suffix(format!("{x}{y}"))),
                (Some(Prim::StripPrefix(x)), Prim::StripPrefix(y)) => {
                    Some(Prim::StripPrefix(format!("{x}{y}")))
                }
                (Some(Prim::StripSuffix(x)), Prim::StripSuffix(y)) => {
                    Some(Prim::StripSuffix(format!("{y}{x}")))
                }
                _ => None,
            };
            match merged {
                Some(m) => *out.last_mut().expect("predecessor exists") = m,
                None => out.push(p.clone()),
            }
        }
        Chain(out)
    }

    /// Canonical bytes for identity derivation -- via the normal form,
    /// so semantically equal chains produce the same identity.
    pub fn ident_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for p in &self.normalized().0 {
            let (tag, arg): (u8, &str) = match p {
                Prim::Identity => (0, ""),
                Prim::Capitalize => (1, ""),
                Prim::Decapitalize => (2, ""),
                Prim::Prefix(a) => (3, a),
                Prim::Suffix(a) => (4, a),
                Prim::StripPrefix(a) => (5, a),
                Prim::StripSuffix(a) => (6, a),
            };
            out.push(tag);
            out.extend_from_slice(&(arg.len() as u32).to_le_bytes());
            out.extend_from_slice(arg.as_bytes());
        }
        out
    }
}

/// Interning handle. Chains live in the table, nodes only carry the id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainId(pub u32);

#[derive(Debug, Clone, Default)]
pub struct ChainTable {
    by_chain: FxHashMap<Chain, ChainId>,
    chains: Vec<Chain>,
}

impl ChainTable {
    /// Interns the NORMAL FORM: semantically equal chains share one
    /// id, and `chain(id)` returns the same chain that also enters the
    /// identity.
    pub fn intern(&mut self, prims: &[Prim]) -> ChainId {
        let c = Chain(prims.to_vec()).normalized();
        if let Some(id) = self.by_chain.get(&c) {
            return *id;
        }
        let id = ChainId(self.chains.len() as u32);
        self.chains.push(c.clone());
        self.by_chain.insert(c, id);
        id
    }

    pub fn chain(&self, id: ChainId) -> &Chain {
        &self.chains[id.0 as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getter_chain_applies_in_list_order() {
        let mut t = ChainTable::default();
        let id = t.intern(&[Prim::Capitalize, Prim::Prefix("get".into())]);
        assert_eq!(t.chain(id).apply("name").as_deref(), Some("getName"));
    }

    #[test]
    fn inverse_reverses_the_chain() {
        let mut t = ChainTable::default();
        let id = t.intern(&[Prim::Capitalize, Prim::Prefix("get".into())]);
        let inv = t.chain(id).inverse();
        assert_eq!(inv.apply("getName").as_deref(), Some("name"));
    }

    #[test]
    fn prefix_without_a_match_fails() {
        let mut t = ChainTable::default();
        let id = t.intern(&[Prim::Prefix("get".into())]);
        assert_eq!(t.chain(id).inverse().apply("setName"), None);
    }

    // Name deliberately not "..._returns_only_consistent_sources": both
    // branches already run before the consistency check kicks in.
    // "getName" is the success case; "setName" fails because the
    // backward chain itself isn't applicable (StripPrefix("get") finds
    // no match) -- the same case as in
    // `prefix_without_a_match_fails`, here only observed via
    // `invert_checked` instead of directly via `inverse().apply`. The
    // actual consistency check is covered by
    // `invert_checked_discards_unreachable_target_values`.
    #[test]
    fn invert_checked_returns_source_on_hit_none_when_backward_fails() {
        let mut t = ChainTable::default();
        let id = t.intern(&[Prim::Capitalize, Prim::Prefix("get".into())]);
        let c = t.chain(id);
        assert_eq!(c.invert_checked("getName").as_deref(), Some("name"));
        assert_eq!(c.invert_checked("setName"), None);
    }

    /// Covers the actual consistency check: the backward chain returns
    /// a source, but the forward chain does not map that source onto
    /// the target value being sought -- so the target value isn't
    /// reachable at all and must be discarded instead of returning the
    /// (wrong) source.
    #[test]
    fn invert_checked_discards_unreachable_target_values() {
        let mut t = ChainTable::default();
        let id = t.intern(&[Prim::Capitalize]);
        let c = t.chain(id);
        // inverse().apply("a") == Some("a") (Decapitalize of "a" is "a"),
        // but apply("a") == Some("A") != "a" -- "a" is not reachable as
        // a target value of this chain.
        assert_eq!(c.inverse().apply("a").as_deref(), Some("a"));
        assert_eq!(c.apply("a").as_deref(), Some("A"));
        assert_eq!(c.invert_checked("a"), None);
    }

    #[test]
    fn invert_checked_is_consistent_not_original_faithful() {
        let mut t = ChainTable::default();
        let id = t.intern(&[Prim::Capitalize]);
        let c = t.chain(id);
        // capitalize("URL") == "URL"; the backward direction returns
        // "uRL", and that's correct because it maps forward back onto
        // "URL".
        assert_eq!(c.invert_checked("URL").as_deref(), Some("uRL"));
        assert_eq!(c.apply("uRL").as_deref(), Some("URL"));
    }

    #[test]
    fn interning_is_deterministic_and_shares() {
        let mut t = ChainTable::default();
        let a = t.intern(&[Prim::Identity]);
        let b = t.intern(&[Prim::Identity]);
        assert_eq!(a, b, "same chain, same id");
    }

    /// The normal form is effect-preserving: for a sample of inputs,
    /// the chain and its normal form return the same value (including
    /// the failure case `None`).
    fn same_effect(c: &Chain) {
        for input in ["", "x", "ab", "getName", "C cmd", "ÄÖÜ", "ß"] {
            assert_eq!(
                c.apply(input),
                c.normalized().apply(input),
                "normal form changes the effect of {c:?} on {input:?}"
            );
        }
    }

    #[test]
    fn normal_form_drops_no_op_steps() {
        let c = Chain(vec![
            Prim::Identity,
            Prim::Prefix("".into()),
            Prim::Capitalize,
            Prim::Suffix("".into()),
            Prim::StripPrefix("".into()),
            Prim::StripSuffix("".into()),
            Prim::Identity,
        ]);
        assert_eq!(c.normalized(), Chain(vec![Prim::Capitalize]));
        same_effect(&c);
    }

    #[test]
    fn normal_form_merges_adjacent_affixes() {
        // List order: "a" goes on first, then "b" goes on in front of
        // that ⇒ "ba".
        let pre = Chain(vec![Prim::Prefix("a".into()), Prim::Prefix("b".into())]);
        assert_eq!(pre.normalized(), Chain(vec![Prim::Prefix("ba".into())]));
        same_effect(&pre);

        let suf = Chain(vec![Prim::Suffix("a".into()), Prim::Suffix("b".into())]);
        assert_eq!(suf.normalized(), Chain(vec![Prim::Suffix("ab".into())]));
        same_effect(&suf);

        let sp = Chain(vec![
            Prim::StripPrefix("a".into()),
            Prim::StripPrefix("b".into()),
        ]);
        assert_eq!(sp.normalized(), Chain(vec![Prim::StripPrefix("ab".into())]));
        same_effect(&sp);

        let ss = Chain(vec![
            Prim::StripSuffix("a".into()),
            Prim::StripSuffix("b".into()),
        ]);
        assert_eq!(ss.normalized(), Chain(vec![Prim::StripSuffix("ba".into())]));
        same_effect(&ss);
    }

    #[test]
    fn normal_form_leaves_dissimilar_neighbors_alone() {
        // Case operations don't commute with affixes and are therefore
        // left untouched.
        let c = Chain(vec![
            Prim::Prefix("a".into()),
            Prim::Capitalize,
            Prim::Prefix("b".into()),
        ]);
        assert_eq!(c.normalized(), c);
        same_effect(&c);
        // And the two orderings stay distinguishable.
        let x = Chain(vec![Prim::Capitalize, Prim::Prefix("get".into())]);
        let y = Chain(vec![Prim::Prefix("get".into()), Prim::Capitalize]);
        assert_ne!(x.ident_bytes(), y.ident_bytes());
    }

    #[test]
    fn semantically_equal_chains_share_identity_and_id() {
        let mut t = ChainTable::default();
        let empty = Chain(vec![]);
        let with_identity = Chain(vec![Prim::Identity, Prim::Identity]);
        let empty_prefix = Chain(vec![Prim::Prefix("".into())]);
        assert_eq!(empty.ident_bytes(), with_identity.ident_bytes());
        assert_eq!(empty.ident_bytes(), empty_prefix.ident_bytes());
        assert_eq!(t.intern(&empty.0), t.intern(&with_identity.0));
        assert_eq!(t.intern(&empty.0), t.intern(&empty_prefix.0));

        let two = Chain(vec![Prim::Prefix("a".into()), Prim::Prefix("b".into())]);
        let one = Chain(vec![Prim::Prefix("ba".into())]);
        assert_eq!(two.ident_bytes(), one.ident_bytes());
        assert_eq!(t.intern(&two.0), t.intern(&one.0));
        // The table returns the normal form.
        let id = t.intern(&two.0);
        assert_eq!(t.chain(id), &one);
    }

    #[test]
    fn ident_bytes_distinguish_arguments() {
        let mut t = ChainTable::default();
        let a = t.intern(&[Prim::Prefix("get".into())]);
        let b = t.intern(&[Prim::Prefix("set".into())]);
        assert_ne!(t.chain(a).ident_bytes(), t.chain(b).ident_bytes());
    }
}
