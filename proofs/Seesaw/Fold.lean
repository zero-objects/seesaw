/-
  Seesaw.Fold — termination of consolidation (V₁₆).

  Sources:
    * def:fold, def:null (formal_core.tex): fold removes nullified ops by
      fixpoint iteration of the three nullification rules.
    * V₁₆ (Terminierung der Konsolidierung): D_x finite, the nullification set C
      grows monotonically bounded by |∪ᵢ dᵢ.op*|; a fixpoint is reached within
      that many iterations.
-/

namespace Seesaw

/-- **V₁₆ (Terminierung der Konsolidierung).**
    Let `c i` be the size of the nullification set `C` after `i` fold iterations.
    If `c` is bounded by `B` (the total op count) and each iteration either has
    already reached a fixpoint or strictly enlarges `C` (finds a new nullified
    op), then a fixpoint is reached within `B` iterations. This is the fixpoint
    termination of `fold` (def:fold). -/
theorem fold_fixpoint (c : Nat → Nat) (B : Nat)
    (bound : ∀ i, c i ≤ B)
    (progress : ∀ i, c (i + 1) = c i ∨ c i + 1 ≤ c (i + 1)) :
    ∃ i, i ≤ B ∧ c (i + 1) = c i := by
  -- Constructive search: for each prefix [0,n] either every step strictly grows,
  -- or a fixpoint already occurred.
  have search : ∀ n, (∀ i, i ≤ n → c i + 1 ≤ c (i + 1))
                     ∨ (∃ i, i ≤ n ∧ c (i + 1) = c i) := by
    intro n
    induction n with
    | zero =>
        rcases progress 0 with he | hlt
        · exact Or.inr ⟨0, Nat.le_refl 0, he⟩
        · refine Or.inl ?_
          intro i hi
          have : i = 0 := Nat.le_zero.mp hi
          subst this; exact hlt
    | succ k ih =>
        rcases ih with hall | hex
        · rcases progress (k + 1) with he | hlt
          · exact Or.inr ⟨k + 1, Nat.le_refl _, he⟩
          · refine Or.inl ?_
            intro i hi
            rcases Nat.lt_or_eq_of_le hi with h | h
            · exact hall i (Nat.le_of_lt_succ h)
            · subst h; exact hlt
        · rcases hex with ⟨i, hi, he⟩
          exact Or.inr ⟨i, Nat.le_succ_of_le hi, he⟩
  -- All-strict growth on [0,B] would push c(B+1) past its own bound.
  have grow : (∀ i, i ≤ B → c i + 1 ≤ c (i + 1)) → ∀ i, i ≤ B + 1 → i ≤ c i := by
    intro hall i
    induction i with
    | zero => intro _; exact Nat.zero_le _
    | succ k ih =>
        intro hk
        have hkB : k ≤ B := by omega
        have hs := hall k hkB
        have ihk := ih (by omega)
        omega
  rcases search B with hall | hex
  · exfalso
    have h1 := grow hall (B + 1) (Nat.le_refl _)
    have h2 := bound (B + 1)
    omega
  · exact hex

end Seesaw
