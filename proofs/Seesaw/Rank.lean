/-
  Seesaw.Rank — rank-ordered match selection and V_13 (rank injectivity).

  Sources:
    * def:rank    (formal_core.tex): rank(o) = rho(r) * M + mu_{r,phi}(m).
    * lem:rank-inj / Lemma v13-full (Ch. 9): distinct ops have distinct ranks,
      given M > max match count. Two-case proof: rho-part separates distinct
      rules, mu-part separates distinct matches of the same rule.
-/

namespace Seesaw

/-- An emitted op, reduced to its two rank ingredients:
    `rule` is the injective rule rank `rho(r)`, `mu` is the canonical,
    content-intrinsic match position `mu_{r,phi}(m)`. -/
structure Op where
  rule : Nat   -- rho(r), injective over rules
  mu   : Nat   -- mu_{r,phi}(m), canonical match key
deriving DecidableEq, Repr

/-- The composite op rank with base `M` (def:rank): `rank(o) = rule * M + mu`,
    with `M` chosen so the rule term dominates. -/
def rank (M : Nat) (o : Op) : Nat := o.rule * M + o.mu

/-- Base-`M` uniqueness: a value `a * M + b` with `b < M` determines both digits.
    This is the arithmetic heart of the rank argument — the "M dominates"
    condition is exactly `b < M`. -/
theorem baseM_unique {M a b c d : Nat}
    (hb : b < M) (hd : d < M) (h : a * M + b = c * M + d) :
    a = c ∧ b = d := by
  have hM : 0 < M := Nat.lt_of_le_of_lt (Nat.zero_le b) hb
  -- read off the low digit via `% M`
  have hb' : (a * M + b) % M = b := by
    rw [Nat.add_comm (a * M) b, Nat.add_mul_mod_self_right]; exact Nat.mod_eq_of_lt hb
  have hd' : (c * M + d) % M = d := by
    rw [Nat.add_comm (c * M) d, Nat.add_mul_mod_self_right]; exact Nat.mod_eq_of_lt hd
  have hbd : b = d := by rw [← hb', ← hd', h]
  -- cancel the (now equal) low digits, then cancel the common factor `M`
  have hX : a * M = c * M := by omega
  have hac : a = c := Nat.eq_of_mul_eq_mul_right hM hX
  exact ⟨hac, hbd⟩

/-- **V_13 (rank injectivity), lem:rank-inj / Lemma v13-full.**
    If `M` dominates both match positions (`mu < M`), distinct ops get distinct
    ranks. Contrapositive form: equal ranks force equal ops. The two paper cases
    (distinct rules → `rho` separates; same rule, distinct matches → `mu`
    separates) are both discharged by `baseM_unique`. -/
theorem rank_injective {M : Nat} {o₁ o₂ : Op}
    (h1 : o₁.mu < M) (h2 : o₂.mu < M)
    (h : rank M o₁ = rank M o₂) : o₁ = o₂ := by
  have := baseM_unique h1 h2 h        -- rule₁ = rule₂ ∧ mu₁ = mu₂
  cases o₁; cases o₂
  simp_all

/-- Packaged as genuine injectivity of `rank M` on the subtype of ops whose match
    position is below the domination bound `M`. -/
theorem rank_injective' {M : Nat} {o₁ o₂ : Op}
    (h1 : o₁.mu < M) (h2 : o₂.mu < M) (hne : o₁ ≠ o₂) :
    rank M o₁ ≠ rank M o₂ := by
  intro h; exact hne (rank_injective h1 h2 h)

/-! ## V₁₄ — Totalität und Determinismus (def:op-rang / def:match-enum)

The op-rank/selection map is a total function of the op and deterministic; μ is
content-intrinsic (a function of the match's binding key, the tuple of ghost
identities), never of a mutable enumeration index (def:rank). By construction. -/

/-- **V₁₄, totality:** the rank is defined for every op. -/
theorem rank_total (M : Nat) (o : Op) : ∃ r, rank M o = r := ⟨_, rfl⟩

/-- **V₁₄, determinism:** equal ops receive equal rank. -/
theorem rank_deterministic (M : Nat) {o₁ o₂ : Op} (h : o₁ = o₂) :
    rank M o₁ = rank M o₂ := by rw [h]

/-- The canonical, content-intrinsic match enumeration μ (def:rank): the position
    is a function of the binding key — the tuple of ghost identities the match
    binds — never of a mutable index. -/
def muEnum (key : List Nat) : Nat :=
  key.foldl (fun acc k => acc * 1000003 + k) 0

/-- μ is deterministic over content: equal binding keys yield the same position,
    so the death of one candidate cannot shift another's rank (def:rank). -/
theorem muEnum_deterministic {k₁ k₂ : List Nat} (h : k₁ = k₂) : muEnum k₁ = muEnum k₂ := by
  rw [h]

/-! ## V₁₅ — Wohlgeordneter Selektionsraum

Trivial from V₁₃ + V₁₄: ranking embeds ops into ℕ, so the selection order is
well-founded (a finite totally-ordered candidate set is well-ordered). -/

/-- **V₁₅ (Wohlgeordneter Selektionsraum):** the selection order (lower rank
    first) is well-founded — every nonempty candidate set has a least-rank
    element, so cascade selection is well-defined. -/
theorem selection_wf (M : Nat) :
    WellFounded (fun o₁ o₂ : Op => rank M o₁ < rank M o₂) :=
  (invImage (rank M) Nat.lt_wfRel).wf

/-- The selection order is total on distinct candidates: with dominated μ, any
    two distinct ops are strictly rank-comparable (trichotomy + V₁₃). -/
theorem selection_total {M : Nat} {o₁ o₂ : Op}
    (h1 : o₁.mu < M) (h2 : o₂.mu < M) (hne : o₁ ≠ o₂) :
    rank M o₁ < rank M o₂ ∨ rank M o₂ < rank M o₁ := by
  rcases Nat.lt_trichotomy (rank M o₁) (rank M o₂) with h | h | h
  · exact Or.inl h
  · exact absurd (rank_injective h1 h2 h) hne
  · exact Or.inr h

end Seesaw
