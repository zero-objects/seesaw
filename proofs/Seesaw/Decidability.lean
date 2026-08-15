/-
  Seesaw.Decidability — Stufe 5a: non-erasure predicate (V₇), decidability of
  dup/contra (V₉), consistency preservation on navigation (V₂₀).

  Sources:
    * V₇ (Nicht-Erasure-Invariante): admissibility of a reconciliation op (a
      definition/predicate).
    * V₉ / Lemma v9-full: dup and contra are decidable; dup reduces to a hash
      comparison via def:ghostid, with a graph-isomorphism kernel as fallback.
    * V₂₀: every baseline reached by navigation is type-conformant.

  NAMED IDEALIZATION #3 (graph-isomorphism kernel). The general structural
  graph-isomorphism decision (`GIso`, `giso_decidable`) is the report's fallback
  crux; it is isolated as a named axiom. The hash path (`dup_by_hash`) is
  axiom-free and reduces dup to `Nat` equality — the common case.
-/
import Seesaw.Basic

namespace Seesaw

/-! ### V₇ — non-erasure admissibility as a decidable predicate -/

/-- An op erasing element `e` is admissible iff `e` is not still referenced by a
    valid op (def of V₇). Decidable by construction. -/
def Admissible (referenced : Nat → Bool) (e : Nat) : Prop := referenced e = false

instance (referenced : Nat → Bool) (e : Nat) : Decidable (Admissible referenced e) := by
  unfold Admissible; infer_instance

/-! ### V₉ — decidability of dup and contra -/

/-- **V₉, hash path (axiom-free).** Duplication of two candidates reduces to
    equality of their parent-rooted identities (def:ghostid) — a `Nat`
    comparison, decidable, no graph isomorphism needed. -/
def dupByHash (id₁ id₂ : Nat) : Prop := id₁ = id₂

instance (id₁ id₂ : Nat) : Decidable (dupByHash id₁ id₂) := by
  unfold dupByHash; infer_instance

/-- Two structurally equal ghost elements are a hash-duplicate (from
    `iso_ghosts` congruence in Basic — same structure ⇒ same id). -/
theorem dupByHash_of_same_struct (e₁ e₂ : Element)
    (h : (⟨e₁.parent, e₁.edgedata, sigma e₁⟩ : IdInput)
         = ⟨e₂.parent, e₂.edgedata, sigma e₂⟩) :
    dupByHash (idOf e₁) (idOf e₂) := by
  unfold dupByHash idOf; rw [h]

/-- General structural graph-isomorphism relation on ghost patterns (by root id),
    with its decision kernel — the NAMED idealization for the worst case. -/
axiom GIso : Nat → Nat → Prop
axiom giso_decidable : DecidableRel GIso

/-- **V₉, general dup:** duplication over a finite candidate list is decidable
    via the GI kernel. -/
def dup (cands : List Nat) (x : Nat) : Prop := ∃ y ∈ cands, GIso x y

noncomputable instance (cands : List Nat) (x : Nat) : Decidable (dup cands x) := by
  unfold dup
  haveI := giso_decidable
  infer_instance

/-- **V₉, contra:** contradiction is a disjunction of primitive local checks plus
    an ancestor-membership test — decidable with no GI kernel. -/
def contra (directClash attrClash : Bool) (ancestors : List Nat) (y : Nat) : Prop :=
  directClash = true ∨ attrClash = true ∨ y ∈ ancestors

instance (directClash attrClash : Bool) (ancestors : List Nat) (y : Nat) :
    Decidable (contra directClash attrClash ancestors y) := by
  unfold contra; infer_instance

/-! ### V₂₀ — consistency preservation under navigation -/

/-- Iterate a step (baseline navigation). -/
def iter {S : Type} (f : S → S) : Nat → S → S
  | 0,     s => s
  | n + 1, s => f (iter f n s)

/-- **V₂₀ (Konsistenz-Erhaltung bei Navigation).** If the start baseline is
    type-conformant and every navigation step preserves conformance, then every
    reachable baseline is type-conformant. -/
theorem conformance_preserved {S : Type} (conform : S → Prop) (f : S → S)
    (hstep : ∀ s, conform s → conform (f s)) :
    ∀ (n : Nat) (s₀ : S), conform s₀ → conform (iter f n s₀) := by
  intro n
  induction n with
  | zero => intro s₀ h₀; exact h₀
  | succ k ih => intro s₀ h₀; exact hstep _ (ih s₀ h₀)

end Seesaw
