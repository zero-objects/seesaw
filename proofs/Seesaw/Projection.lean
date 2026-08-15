/-
  Seesaw.Projection — the ghost projection φ_L: projection stability (V₄) and
  cascade isolation (V₅).

  Sources:
    * def:projection, def:status (formal_core.tex): φ_L applies ops to a
      status-annotated graph.
    * V₄ (Projektionsstabilität) / Lemma v4-full: φ_L is well-defined as a
      *function* (total and deterministic), by induction over |D_x|.
    * V₅ (Kaskaden-Isolation): the cascade does not touch structure that
      hangs from none of its correspondences.

  ## Why V₅ is stated over CORRESPONDENCES (2026-08-13)

  Three formulations, in the order they were held and discarded.

  1. Until 2026-08-10, over STATUS: "no op modifies a SOLID element". It
     matched the code, which retracted only `GHOST` products, and it was a
     defect: deleting the source of a materialized product produced no
     tombstone at all.

  2. Until 2026-08-13, over PROVENANCE: whatever a cascade entry records
     as its product falls with that entry, materialized or not. That fixed
     the defect, but it is directional. Which side ends up in `created`
     depends only on which way the translation ran first, so the statement
     said different things about the same pair of elements depending on
     the direction of travel.

  3. Now, over CORRESPONDENCES. A correspondence spans nodes AND edges,
     and an element hangs from it or it does not. That is symmetric by
     construction, and it is what the engine actually decides: a fallen
     correspondence takes down what hangs from it, in either direction
     (Sandra 2026-08-12).

  The attribute case is not special. What the rule format calls
  `bindings` is an attribute constraint between two leaves, hence a
  correspondence in its own right, on the leaf level — same identity
  derivation, same behaviour. A leaf may hang from several
  correspondences; each falls on its own, and none drags the others.

  * `Status` records the MATERIALIZATION state of an element.
  * `Carries` records WHICH CORRESPONDENCE HOLDS it, and survives
    materialization untouched.

  They are independent, and the distinction is NOT host-supplied versus
  generated: a host-supplied UML class is `SOLID` and, once translated,
  is held by its correspondence just as its generated counterpart is.
  What separates the two cases is the correspondence, not the origin.
  An element outside every realised correspondence is `SOLID` and free;
  an element held by one is `SOLID` and retractable through it. Status
  alone cannot tell them apart, which is precisely what the first defect
  showed.
-/
import Seesaw.Basic

namespace Seesaw

/-- A status assignment over elements (the status function of def:status). -/
def StatusMap (E : Type) := E → Status

/-- Which correspondence holds which element. Decidable, so `applyOp`
    stays computable; in the engine this is the set of elements linked to
    a correspondence node — its two anchors, and for an attribute
    correspondence its two leaves.

    Independent of `StatusMap`: no operation on statuses can change it,
    which is exactly how materialization preserves it. -/
def Carries (Corr E : Type) := Corr → E → Bool

/-- A ghost-projection op: it writes status `write` at element `target`.
    `corr` names the correspondence that induces the op, or `none` for an
    op induced by none, which holds nothing.

    SCOPE: `applyScript` models cascade-induced overlay operations only.
    The observer delta — what the host itself changes — is represented
    separately as the step from one baseline to the next
    (G_{t₀} → G_{t₁}) and is not an op here. V₅ therefore says nothing
    about explicit host edits; it says the cascade does not touch what
    hangs from none of its correspondences. -/
structure POp (Corr E : Type) where
  target : E
  write  : Status
  corr   : Option Corr

/-- Does the inducing correspondence hold its target? An op with no
    correspondence holds nothing, so an external delta can never claim a
    solid element. -/
def POp.holds {Corr E : Type} (car : Carries Corr E) (o : POp Corr E) : Bool :=
  match o.corr with
  | none    => false
  | some c  => car c o.target

/-- An element no correspondence holds. This is what V₅ protects:
    baseline structure the host supplied, and anything else outside every
    correspondence. -/
def Free {Corr E : Type} (car : Carries Corr E) (e : E) : Prop :=
  ∀ c : Corr, car c e = false

/-- Apply one op to a status map.

    The write is gated: a `SOLID` element is modified only by a
    correspondence that holds it. Non-solid elements are written as
    before, and the gate is what lets retraction reach past a
    materialization without ever touching free structure. -/
def applyOp {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (o : POp Corr E) : StatusMap E :=
  fun e =>
    if e = o.target ∧ (σ o.target ≠ Status.solid ∨ o.holds car = true) then o.write else σ e

/-- **V₅ (Kaskaden-Isolation), single step: no op modifies a SOLID element
    that no correspondence holds.** -/
theorem applyOp_preserves_free_solid {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (o : POp Corr E) (e : E) :
    σ e = Status.solid → Free car e → applyOp car σ o e = Status.solid := by
  intro hsolid hfree
  show (if e = o.target ∧ (σ o.target ≠ Status.solid ∨ o.holds car = true) then o.write
        else σ e) = Status.solid
  by_cases hc : e = o.target ∧ (σ o.target ≠ Status.solid ∨ o.holds car = true)
  · rcases hc with ⟨rfl, hgate⟩
    cases hgate with
    | inl hne => exact absurd hsolid hne
    | inr hholds =>
        -- The op claims to hold `e`, but no correspondence holds `e`.
        exfalso
        have hfalse : o.holds car = false := by
          unfold POp.holds
          cases o.corr with
          | none   => rfl
          | some c => exact hfree c
        rw [hfalse] at hholds
        exact Bool.noConfusion hholds
  · rw [if_neg hc]; exact hsolid

/-- The counterpart, and the reason the status formulation had to go: a
    materialized element IS retractable by a correspondence that holds it.

    Before 2026-08-10 this was false in the engine — the check was on
    `GHOST`, so the write below did not happen and a delta left the
    product standing. -/
theorem applyOp_reaches_held_solid {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (o : POp Corr E) :
    o.holds car = true → applyOp car σ o o.target = o.write := by
  intro hholds
  show (if o.target = o.target ∧ (σ o.target ≠ Status.solid ∨ o.holds car = true)
        then o.write else σ o.target) = o.write
  rw [if_pos ⟨rfl, Or.inr hholds⟩]

/-- **Symmetry, and the reason the provenance formulation had to go.**

    A correspondence holds BOTH of its elements, so a fallen
    correspondence reaches either of them with the same statement. Under
    provenance this was not expressible: only one side was a product, and
    which one depended on the direction of travel.

    This is the deletion direction added on 2026-08-12 — deleting the
    generated element deletes its source — and the generating direction,
    as one theorem. -/
theorem applyOp_reaches_either_end {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (c : Corr) (a b : E)
    (wa wb : Status) (ha : car c a = true) (hb : car c b = true) :
    applyOp car σ ⟨a, wa, some c⟩ a = wa ∧ applyOp car σ ⟨b, wb, some c⟩ b = wb := by
  constructor
  · exact applyOp_reaches_held_solid car σ ⟨a, wa, some c⟩ ha
  · exact applyOp_reaches_held_solid car σ ⟨b, wb, some c⟩ hb

/-! ## Materialization

This is POST-CONSOLIDATION materialization: consolidation has already run
and resolved every tentative element (`Status.materialized` in
`Basic.lean` states which statuses survive into a baseline). What remains
here is the last move, turning surviving ghosts into baseline elements.

`Carries` is not a `StatusMap` and is therefore untouched by it — the
property the whole construction rests on. -/

/-- Materialization after consolidation: surviving ghosts become solid,
    every other status is left as it is. Tentative elements do not occur
    at this point; consolidation resolved them beforehand. -/
def materialize {E : Type} (σ : StatusMap E) : StatusMap E :=
  fun e => match σ e with
    | Status.ghost => Status.solid
    | s            => s

/-- Materialization only ever produces solid out of ghost. -/
theorem materialize_ghost {E : Type} (σ : StatusMap E) (e : E) :
    σ e = Status.ghost → materialize σ e = Status.solid := by
  intro h; show (match σ e with | Status.ghost => Status.solid | s => s) = Status.solid
  rw [h]

/-- **The retraction semantics in one statement: an element remains
    retractable across materialization.**

    A ghost element held by a correspondence, once materialized, is solid
    — and that correspondence still reaches it. Under the status-gated
    rule this theorem is false, which was the first defect. -/
theorem materialized_element_stays_retractable {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (o : POp Corr E) :
    σ o.target = Status.ghost → o.holds car = true →
    materialize σ o.target = Status.solid ∧
      applyOp car (materialize σ) o o.target = o.write := by
  intro hghost hholds
  exact ⟨materialize_ghost σ o.target hghost,
         applyOp_reaches_held_solid car (materialize σ) o hholds⟩

/-- And the isolation half of the same picture: materializing does not make
    free structure reachable. -/
theorem materialize_preserves_free_solid {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (o : POp Corr E) (e : E) :
    materialize σ e = Status.solid → Free car e →
    applyOp car (materialize σ) o e = Status.solid :=
  applyOp_preserves_free_solid car (materialize σ) o e

/-! ## Consolidation

The stage the model was missing. Retraction writes `TENT`, and only
consolidation decides whether that becomes a tombstone or whether the
element was reclaimed in the same run. The whole deletion semantics
rests on it: a correspondence carries a deletion only once it has
survived this stage as fallen. Mid-run, a fallen correspondence says
nothing.

`Carries` is not a `StatusMap` and is therefore untouched by it, just as
by materialization. -/

/-- Consolidation: what is still tentative at the end of the run did not
    get reclaimed and resolves to a tombstone. Everything else stands.

    This is `Status.materialized` from `Basic.lean` read as a step: that
    predicate says which statuses survive into a baseline, and `tent` is
    the one that does not. -/
def consolidate {E : Type} (σ : StatusMap E) : StatusMap E :=
  fun e => match σ e with
    | Status.tent => Status.tomb
    | s           => s

/-- Consolidation resolves every tentative element. -/
theorem consolidate_resolves_tent {E : Type} (σ : StatusMap E) (e : E) :
    σ e = Status.tent → consolidate σ e = Status.tomb := by
  intro h; show (match σ e with | Status.tent => Status.tomb | s => s) = Status.tomb
  rw [h]

/-- **At rest nothing is tentative.** After consolidation every element
    carries a status that survives into a baseline, which is what
    `Status.materialized` states. -/
theorem consolidate_leaves_nothing_tentative {E : Type} (σ : StatusMap E) (e : E) :
    consolidate σ e ≠ Status.tent := by
  show (match σ e with | Status.tent => Status.tomb | s => s) ≠ Status.tent
  cases h : σ e <;> simp [h]

/-- Consolidation does not touch a solid element, so V₅ passes through it
    unchanged: what the cascade did not hold, consolidation does not
    take either. -/
theorem consolidate_preserves_free_solid {Corr E : Type}
    (car : Carries Corr E) (σ : StatusMap E) (e : E) :
    σ e = Status.solid → Free car e → consolidate σ e = Status.solid := by
  intro hsolid _
  show (match σ e with | Status.tent => Status.tomb | s => s) = Status.solid
  rw [hsolid]

/-- The ghost projection φ_L: fold the op-script over the status map (def:projection). -/
def applyScript {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (ops : List (POp Corr E)) : StatusMap E :=
  ops.foldl (applyOp car) σ

@[simp] theorem applyScript_nil {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) :
    applyScript car σ ([] : List (POp Corr E)) = σ := rfl

@[simp] theorem applyScript_cons {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (o : POp Corr E) (os : List (POp Corr E)) :
    applyScript car σ (o :: os) = applyScript car (applyOp car σ o) os := rfl

/-- **V₅ over the whole cascade: a SOLID element that no correspondence
    holds survives every op-script.** -/
theorem applyScript_preserves_free_solid {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (ops : List (POp Corr E)) (σ : StatusMap E) (e : E) :
    σ e = Status.solid → Free car e → (applyScript car σ ops) e = Status.solid := by
  intro h hfree
  induction ops generalizing σ with
  | nil => exact h
  | cons o os ih =>
      rw [applyScript_cons]
      exact ih (applyOp car σ o) (applyOp_preserves_free_solid car σ o e h hfree)

/-! ## Projection stability (V₄): φ_L is a well-defined function

The report proves V₄ by induction over |D_x|: each op has exactly one result, so
the projection is a function, not merely a relation. We model the inductive
"applies" relation (over the op-script, i.e. over |D_x|), show it computes
exactly `applyScript`, and read off totality and determinism.

V₄ does not depend on the gate: it holds because each op has one result,
whatever decides whether that result is written. The `Carries` parameter
therefore only threads through. -/

/-- The inductive ghost-projection relation, structured over the op-script
    (i.e. over |D_x|, as in Lemma v4-full). -/
inductive Applies {Corr E : Type} [DecidableEq E] (car : Carries Corr E) :
    StatusMap E → List (POp Corr E) → StatusMap E → Prop
  | nil (σ : StatusMap E) : Applies car σ [] σ
  | cons (σ : StatusMap E) (o : POp Corr E) (os : List (POp Corr E)) (σ' : StatusMap E) :
      Applies car (applyOp car σ o) os σ' → Applies car σ (o :: os) σ'

/-- The projection derivation always exists and computes `applyScript`. -/
theorem Applies_applyScript {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (ops : List (POp Corr E)) (σ : StatusMap E) :
    Applies car σ ops (applyScript car σ ops) := by
  induction ops generalizing σ with
  | nil => exact Applies.nil σ
  | cons o os ih => exact Applies.cons σ o os _ (ih (applyOp car σ o))

/-- The inductive projection computes exactly `applyScript` — the key step of the
    induction over |D_x|. -/
theorem Applies_iff {Corr E : Type} [DecidableEq E]
    {car : Carries Corr E} {σ : StatusMap E} {ops : List (POp Corr E)}
    {σ' : StatusMap E} :
    Applies car σ ops σ' ↔ σ' = applyScript car σ ops := by
  constructor
  · intro h
    induction h with
    | nil σ => rfl
    | cons σ o os σ' _ ih => rw [ih, applyScript_cons]
  · intro h
    subst h
    exact Applies_applyScript car ops σ

/-- **V₄, totality:** the projection is defined for every op-script. -/
theorem Applies_total {Corr E : Type} [DecidableEq E]
    (car : Carries Corr E) (σ : StatusMap E) (ops : List (POp Corr E)) :
    ∃ σ', Applies car σ ops σ' :=
  ⟨applyScript car σ ops, Applies_iff.mpr rfl⟩

/-- **V₄, determinism:** the projection has at most one result — φ_L is a
    function, not merely a relation. -/
theorem Applies_functional {Corr E : Type} [DecidableEq E]
    {car : Carries Corr E} {σ : StatusMap E} {ops : List (POp Corr E)}
    {σ₁ σ₂ : StatusMap E} :
    Applies car σ ops σ₁ → Applies car σ ops σ₂ → σ₁ = σ₂ := by
  intro h1 h2
  rw [Applies_iff] at h1 h2
  rw [h1, h2]

end Seesaw
