/-
  Seesaw.Consolidation — Stufe 3: semantic fidelity (V₁₇) and minimality /
  effect-isomorphy (V₁₈). No idealization axiom — this is core theory.

  Sources:
    * def:null, def:fold (formal_core.tex): consolidation removes nullified ops
      (rollup override, add-del pairs, V₁₂-induced) by fixpoint.
    * V₁₇ / Lemma v17-full: mat(φ_L(D_x)) = mat(φ_L(D̂_x)).
    * V₁₈ / Lemma v18-full: (i) effect-equiv from V₁₇, (ii) structural minimality
      (fixpoint), (iii) effect-isomorphy — isomorphic ghost structures share a
      GhostId and collapse under rollup.

  Model. Ops carry a rollup rank κ; applied in κ-ascending order, the effective
  ("materialized") value at each element is the LAST op targeting it, i.e. the
  max-κ op — the rollup override of def:null(i). `matAt` reads exactly that op.
  Consolidation is a `filter` that keeps at least the effective op per target;
  V₁₇ then says materialization is unchanged.
-/
import Seesaw.Basic

namespace Seesaw

/-- The effect an op writes: a present value, or a tombstone (absent). -/
inductive Effect where
  | present (v : Nat)
  | absent
deriving DecidableEq, Repr

/-- A consolidation op: writes `effect` at element `target`, with rollup rank
    `kappa` (the κ of def:rollup-order). -/
structure COp where
  target : Nat
  kappa  : Nat
  effect : Effect
deriving DecidableEq, Repr

/-- Predicate: op targets element `t`. -/
def matchT (t : Nat) (o : COp) : Bool := o.target == t

def effVal : Effect → Option Nat
  | .present v => some v
  | .absent    => none

/-- Materialization at element `t`: the effect of the LAST (max-κ) op targeting
    `t` — the rollup override — or `none` if no op targets it. `mat` of def:fold. -/
def matAt (D : List COp) (t : Nat) : Option Nat :=
  match D.reverse.find? (matchT t) with
  | some o => effVal o.effect
  | none   => none

/-! ### List helpers (self-contained) -/

theorem reverse_filter {α : Type} (Q : α → Bool) (l : List α) :
    (l.filter Q).reverse = l.reverse.filter Q := by
  induction l with
  | nil => rfl
  | cons a l ih =>
      by_cases hb : Q a = true
      · rw [List.filter_cons, if_pos hb, List.reverse_cons, ih, List.reverse_cons,
            List.filter_append, List.filter_cons, if_pos hb, List.filter_nil]
      · rw [List.filter_cons, if_neg hb, ih, List.reverse_cons, List.filter_append,
            List.filter_cons, if_neg hb, List.filter_nil, List.append_nil]

theorem find?_none_filter {α : Type} (P Q : α → Bool) (l : List α)
    (h : l.find? P = none) : (l.filter Q).find? P = none := by
  rw [List.find?_eq_none] at h ⊢
  intro x hx
  exact h x (List.mem_filter.mp hx).1

/-- Strengthening the predicate: if `e` is the first `P`-match and also passes
    `Q`, then `e` is the first `(Q ∧ P)`-match — everything before `e` already
    fails `P`, hence fails `Q ∧ P`. -/
theorem find?_and_of_first {α : Type} (P Q : α → Bool) :
    ∀ (l : List α) (e : α), l.find? P = some e → Q e = true →
      l.find? (fun x => Q x && P x) = some e := by
  intro l
  induction l with
  | nil => intro e h _; simp at h
  | cons a l ih =>
      intro e h hQ
      cases hPa : P a with
      | true =>
          rw [List.find?_cons, hPa] at h
          dsimp only at h
          simp only [Option.some.injEq] at h
          subst h
          simp [List.find?_cons, hQ, hPa]
      | false =>
          rw [List.find?_cons, hPa] at h
          dsimp only at h
          rw [List.find?_cons]
          simp only [hPa, Bool.and_false]
          exact ih e h hQ

/-- If `e` is the first `P`-match in `l` and `e` survives `Q`, it is still the
    first `P`-match after filtering by `Q`. -/
theorem find?_filter_of_first {α : Type} (P Q : α → Bool) (l : List α) (e : α)
    (h : l.find? P = some e) (hQ : Q e = true) :
    (l.filter Q).find? P = some e := by
  rw [List.find?_filter]
  have hpred : (fun a => decide (Q a = true ∧ P a = true)) = (fun a => Q a && P a) := by
    funext a; by_cases hq : Q a = true <;> by_cases hp : P a = true <;> simp [hq, hp]
  rw [hpred]
  exact find?_and_of_first P Q l e h hQ

/-! ### V₁₇ — semantic fidelity -/

/-- **V₁₇ (Semantische Treue der Konsolidierung).**
    If consolidation `keep`s at least the effective (max-κ) op at every element,
    then the materialization is unchanged: removing only dominated / cancelled
    ops preserves `mat`. This is the induction of Lemma v17-full made structural
    — the surviving op per target IS the rolled-up winner. -/
theorem consolidation_faithful (D : List COp) (keep : COp → Bool)
    (hkeep : ∀ t o, D.reverse.find? (matchT t) = some o → keep o = true) :
    ∀ t, matAt (D.filter keep) t = matAt D t := by
  intro t
  have key : (D.filter keep).reverse.find? (matchT t)
             = D.reverse.find? (matchT t) := by
    rw [reverse_filter]
    cases h : D.reverse.find? (matchT t) with
    | none   => exact find?_none_filter _ _ _ h
    | some o => exact find?_filter_of_first _ _ _ o h (hkeep t o h)
  unfold matAt
  rw [key]

/-! ### V₁₈ — minimality and effect-isomorphy -/

/-- **V₁₈ (i): effect-equivalence.** The consolidated delta materializes exactly
    like the original (direct from V₁₇). -/
theorem consolidation_effect_equiv (D : List COp) (keep : COp → Bool)
    (hkeep : ∀ t o, D.reverse.find? (matchT t) = some o → keep o = true) :
    matAt (D.filter keep) = matAt D :=
  funext (consolidation_faithful D keep hkeep)

/-- **V₁₈ (ii): structural minimality as a fixpoint.** Re-running consolidation
    with the same predicate removes nothing more — `fold` is a fixpoint
    (def:fold). -/
theorem consolidation_idempotent (D : List COp) (keep : COp → Bool) :
    (D.filter keep).filter keep = D.filter keep := by
  simp [List.filter_filter, Bool.and_self]

/-- Minimality witness: a `present` value that is the sole op at its element is
    essential — dropping every op at that element changes `mat` (some v ↦ none).
    So a consolidated delta carrying real work cannot be shrunk without changing
    the materialization. -/
theorem present_op_essential (D : List COp) (t : Nat) (o : COp)
    (hfound : D.reverse.find? (matchT t) = some o) (v : Nat)
    (hv : o.effect = Effect.present v) :
    matAt D t = some v
    ∧ matAt (D.filter (fun o' => ! matchT t o')) t = none := by
  constructor
  · simp only [matAt, hfound, hv, effVal]
  · -- after removing everything targeting t, no op targets t
    unfold matAt
    have hnone : (D.filter (fun o' => ! matchT t o')).reverse.find? (matchT t) = none := by
      rw [reverse_filter, List.find?_eq_none]
      intro x hx
      have hx2 := (List.mem_filter.mp hx).2
      simpa using hx2
    rw [hnone]

/-- **V₁₈ (iii): effect-isomorphy via GhostId.** Structurally isomorphic ghost
    elements carry the SAME parent-rooted identity (def:ghostid, congruence), so
    ops emitted for them target the same key and collapse under rollup — at most
    one survives. The converse (distinct structure ⇒ distinct id) is
    `distinct_struct_distinct_id` (Basic), so no isomorphic duplicates remain. -/
theorem iso_ghosts_same_id (e₁ e₂ : Element)
    (h : (⟨e₁.parent, e₁.edgedata, sigma e₁⟩ : IdInput)
         = ⟨e₂.parent, e₂.edgedata, sigma e₂⟩) :
    idOf e₁ = idOf e₂ := by
  unfold idOf
  rw [h]

/-- Collapse: no matter how many ops share an element `t`, `mat` is determined by
    a single one (the rollup winner). Isomorphic ghosts (same id ⇒ same target)
    therefore collapse to one materialized representation. -/
theorem sameTarget_collapse (D : List COp) (t : Nat) :
    ∃ w : Option COp, D.reverse.find? (matchT t) = w
      ∧ matAt D t = (match w with | some o => effVal o.effect | none => none) := by
  exact ⟨D.reverse.find? (matchT t), rfl, rfl⟩

end Seesaw
