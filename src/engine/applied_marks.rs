// Part of `engine` (included by `mod.rs` with `opt_e`): what `step`
// knows of a pulled candidate hangs on the number of its match, which
// the candidate carries. Nothing is looked up by the ref sequence.
//
// Without `opt_e`, `step` hashes the ref sequence of every candidate it
// pulls: for the question whether it is applied already, to note it as
// applied, and to find its match for the provenance edge. All three
// are about the match filed under the key (rule, ref sequence) in
// `by_key`, and the candidate carries that match:
//
// - `record` enters a candidate when no match is filed under its key,
//   and files its match there. The key is taken out of `by_key` by a
//   retraction, which takes the candidate out of the todo in the same
//   call, and by `step` for a candidate it has just pulled. So while a
//   candidate is in the todo, the match under its key is its own.
// - A key is noted as applied when its candidate is pulled, and is
//   forgotten when it leaves `by_key` by a retraction. So the note
//   stands exactly as long as the match it was made for is filed under
//   the key, and can be kept as a mark of that match.
//
// A test build keeps the set of applied keys next to the marks and
// looks the match up by its key, and compares.

/// What the graph says about the anchors of the correspondences a
/// pulled candidate would establish.
enum Recognised {
    /// An anchor is tentatively retracted.
    Tentative,
    /// Every anchor carries its correspondence already
    /// ([`is_translated`]).
    Translated,
    /// Neither.
    Open,
}

/// [`anchor_translated`] for an anchor whose participations are at
/// hand.
fn anchor_translated_in(
    g: &Graph,
    names: &mut RuleNames<'_>,
    ix: usize,
    parts: &crate::graph::Parts,
    typ: &str,
    endpoint_typ: &str,
    anchor: &GhostId,
) -> bool {
    names.corr_typ(g, ix, typ).is_some_and(|t| {
        let ep = names.endpoint_typ(g, ix, endpoint_typ);
        parts.of_type(t).any(|p| {
            let corr_ok = g
                .connection(&p.connection)
                .is_some_and(|c| matches!(c.status, Status::Solid | Status::Ghost))
                && g.node_alive(&p.other);
            corr_ok
                && ep.is_some_and(|ept| {
                    g.parts(&p.other)
                        .any(|q| q.other != *anchor && q.other_typ == ept && g.node_alive(&q.other))
                })
        })
    })
}

/// One access per anchor gives its status and its participations: the
/// question whether an anchor is tentatively retracted and the
/// question whether it is translated read the same slot of the graph.
/// Both read the graph only, so asking them anchor by anchor gives the
/// answers of asking the first of all anchors and then the second.
fn recognise(
    g: &Graph,
    rule: &DirectedRule,
    refs: &[GhostId],
    names: &mut RuleNames<'_>,
) -> Recognised {
    let mut tentative = false;
    let mut translated = !rule.corr_recognition.is_empty();
    for (ix, (typ, pos, endpoint_typ)) in rule.corr_recognition.iter().enumerate() {
        let anchor = &refs[*pos];
        match g.node_parts(anchor) {
            Some((node, parts)) => {
                if node.status == Status::TentativeTombstone {
                    tentative = true;
                    break;
                }
                translated = translated
                    && anchor_translated_in(g, names, ix, parts, typ, endpoint_typ, anchor);
            }
            // Not a node: not tentatively retracted, and without
            // participations not translated.
            None => translated = false,
        }
    }
    let recognised = if tentative {
        Recognised::Tentative
    } else if translated {
        Recognised::Translated
    } else {
        Recognised::Open
    };
    #[cfg(any(test, seesaw_checks))]
    {
        let expected = if rule
            .corr_recognition
            .iter()
            .any(|(_, pos, _)| self_node_tt(g, &refs[*pos]))
        {
            0
        } else if is_translated(g, rule, refs, names) {
            1
        } else {
            2
        };
        let got = match recognised {
            Recognised::Tentative => 0,
            Recognised::Translated => 1,
            Recognised::Open => 2,
        };
        assert_eq!(
            got, expected,
            "one pass over the anchors differs from the two questions"
        );
    }
    recognised
}

/// Duplicate memory: the matches whose candidate was pulled and
/// applied, or recognised as a duplicate.
struct Applied {
    /// One bit per match.
    marked: Vec<u64>,
    /// Number of marks.
    count: usize,
    /// Test check: the applied keys, as they are kept without `opt_e`.
    #[cfg(any(test, seesaw_checks))]
    check: std::collections::HashSet<(usize, Refs), crate::hash::FxBuildHasher>,
}

impl Applied {
    fn new() -> Self {
        Self {
            marked: Vec::new(),
            count: 0,
            #[cfg(any(test, seesaw_checks))]
            check: Default::default(),
        }
    }

    /// Makes room for the marks of `more` matches.
    #[cfg(feature = "opt_b")]
    fn reserve(&mut self, more: usize) {
        self.marked.reserve(more.div_ceil(64));
    }

    /// Is the candidate (`rule_ix`, `refs`) of the match `carried`
    /// applied already?
    fn contains(&self, rule_ix: usize, refs: &Refs, carried: Carried) -> bool {
        let marked = self
            .marked
            .get((carried >> 6) as usize)
            .is_some_and(|word| word >> (carried & 63) & 1 == 1);
        #[cfg(any(test, seesaw_checks))]
        assert_eq!(
            marked,
            self.check.contains(&(rule_ix, refs.clone())),
            "the mark of the match differs from the set of applied keys"
        );
        #[cfg(not(any(test, seesaw_checks)))]
        let _ = (rule_ix, refs);
        marked
    }

    /// Notes the pulled candidate of the match `carried` as applied.
    fn insert(&mut self, rule_ix: usize, refs: &Refs, carried: Carried) {
        let word = (carried >> 6) as usize;
        if word >= self.marked.len() {
            self.marked.resize(word + 1, 0);
        }
        let bit = 1u64 << (carried & 63);
        if self.marked[word] & bit == 0 {
            self.marked[word] |= bit;
            self.count += 1;
        }
        #[cfg(any(test, seesaw_checks))]
        {
            self.check.insert((rule_ix, refs.clone()));
            assert_eq!(
                self.count,
                self.check.len(),
                "marks and applied keys differ in number"
            );
        }
        #[cfg(not(any(test, seesaw_checks)))]
        let _ = (rule_ix, refs);
    }

    /// [`Applied::insert`] for a ref sequence the caller is done with.
    fn insert_owned(&mut self, rule_ix: usize, refs: Refs, carried: Carried) {
        self.insert(rule_ix, &refs, carried);
    }

    /// Forgets that `key` was applied (first half; the second is
    /// [`Applied::forget_match`]).
    fn forget(&mut self, key: &(usize, Refs)) {
        #[cfg(any(test, seesaw_checks))]
        self.check.remove(key);
        #[cfg(not(any(test, seesaw_checks)))]
        let _ = key;
    }

    /// Second half of forgetting a key: `current` is the match that
    /// was filed under it, and the one that carries its mark.
    fn forget_match(&mut self, current: Option<u32>) {
        if let Some(mix) = current {
            if let Some(word) = self.marked.get_mut((mix >> 6) as usize) {
                let bit = 1u64 << (mix & 63);
                if *word & bit != 0 {
                    *word &= !bit;
                    self.count -= 1;
                }
            }
        }
        #[cfg(any(test, seesaw_checks))]
        assert_eq!(
            self.count,
            self.check.len(),
            "marks and applied keys differ in number"
        );
    }

    /// Is nothing applied?
    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Engine<'_> {
    /// The match of the pulled candidate (`rule_ix`, `refs`): the one
    /// the candidate carries.
    fn pulled_match(&self, rule_ix: usize, refs: &Refs, carried: Carried) -> Option<u32> {
        #[cfg(any(test, seesaw_checks))]
        assert_eq!(
            self.by_key.get(&(rule_ix, refs.clone())).copied(),
            Some(carried),
            "the candidate does not carry the match filed under its key"
        );
        #[cfg(not(any(test, seesaw_checks)))]
        let _ = (rule_ix, refs);
        Some(carried)
    }
}
