/-
  Seesaw.Retraction — Stufe 2, built on Seesaw.Termination.

  Retraction cascade termination + V₇-preservation (V₈), induces-DAG acyclicity
  (V₁₂), and backtracking termination via a lexicographic measure (V₁₁).

  Sources:
    * def:retr-kaskade, def:struct-dep (Ch. 9): retraction propagates one hop
      along the correspondence topology; structural depth is a well-founded
      order.
    * V₈ / Lemma v8-full: the retraction cascade terminates (well-founded on
      structural depth) and each induced op preserves V₇.
    * V₁₂ / Lemma v12-full: the induces relation o ⤳ o' is acyclic, because each
      cascade extension is strictly descending in the depth order.
    * V₁₁ / Satz terminierung (b): the backtracking tree is finite; each BT
      recursion strictly decreases the lexicographic measure
      (active-prefix length, rank-limit).
-/
import Seesaw.Termination

namespace Seesaw

/-! ## Retraction cascade (V₈) -/

/-- A retraction cascade with a structural-depth measure (def:struct-dep).
    `induces e e'` means the retraction of `e` induces an op on `e'`; every
    induced target is strictly shallower — the well-founded core of V₈. -/
structure Retraction (E : Type) where
  depth    : E → Nat
  induces  : E → E → Prop
  descends : ∀ e e', induces e e' → depth e' < depth e

/-- A retraction is a cascade with measure = structural depth. -/
def Retraction.toCascade {E : Type} (R : Retraction E) : Cascade E where
  measure     := R.depth
  step        := R.induces
  contractive := R.descends

/-- **V₈ (Terminierung der Retraktions-Kaskade):** the induces relation is
    well-founded — the retraction cascade terminates. -/
theorem Retraction.wf {E : Type} (R : Retraction E) :
    WellFounded (fun a b => R.induces b a) :=
  Subrelation.wf
    (fun {a b} h => R.descends b a h)
    (invImage R.depth Nat.lt_wfRel).wf

/-- V₈ quantitative: a retraction cascade runs at most `depth(root)` steps. -/
theorem Retraction.length_le_depth {E : Type} (R : Retraction E)
    (f : Nat → E) (n : Nat) (h : IsRun R.toCascade f n) :
    n ≤ R.depth (f 0) :=
  R.toCascade.length_le_measure f n h

/-! ## V₇-preservation over the retraction closure (V₈, second half) -/

/-- Reflexive-transitive reachability along a relation (self-contained). -/
inductive Reach {E : Type} (r : E → E → Prop) : E → E → Prop
  | refl (a : E) : Reach r a a
  | tail {a b c : E} : Reach r a b → r b c → Reach r a c

/-- **V₈, V₇-preservation:** if the primary op is admissible and every induces
    edge carries admissibility from source to target (the structural
    precondition of def:retr-kaskade), then every element in the retraction
    closure is admissible. By induction on the reachability derivation. -/
theorem Retraction.preserves_admissible {E : Type} (R : Retraction E)
    (adm : E → Prop)
    (hstep : ∀ e e', R.induces e e' → adm e → adm e')
    {root e : E} (hroot : adm root) (hreach : Reach R.induces root e) :
    adm e := by
  induction hreach with
  | refl => exact hroot
  | tail _ hr ih => exact hstep _ _ hr ih

/-! ## Induces-DAG acyclicity (V₁₂) -/

/-- Transitive reachability (at least one step). -/
inductive TReach {E : Type} (r : E → E → Prop) : E → E → Prop
  | single {a b : E} : r a b → TReach r a b
  | tail {a b c : E} : TReach r a b → r b c → TReach r a c

/-- Along any nonempty induces-path the structural depth strictly decreases. -/
theorem Retraction.tclosure_descends {E : Type} (R : Retraction E) {a b : E} :
    TReach R.induces a b → R.depth b < R.depth a := by
  intro h
  induction h with
  | single hr => exact R.descends _ _ hr
  | tail _ hr ih => exact Nat.lt_trans (R.descends _ _ hr) ih

/-- **V₁₂ (Induces-DAG azyklisch):** no element transitively induces itself.
    The induces relation is acyclic, because each edge strictly lowers depth. -/
theorem Retraction.acyclic {E : Type} (R : Retraction E) (a : E) :
    ¬ TReach R.induces a a := by
  intro h
  exact absurd (R.tclosure_descends h) (Nat.lt_irrefl _)

/-! ## Backtracking termination via a lexicographic measure (V₁₁) -/

/-- Lexicographic strict order on ℕ×ℕ: prefix length primary, rank-limit
    secondary (the order named in Satz terminierung (b)(iii)). -/
def LexNat (p q : Nat × Nat) : Prop :=
  p.1 < q.1 ∨ (p.1 = q.1 ∧ p.2 < q.2)

/-- The lexicographic order on ℕ×ℕ is well-founded (proved from the
    well-foundedness of `<` on each component; no Mathlib). -/
theorem lexNat_wf : WellFounded LexNat := by
  constructor
  intro ⟨a, b⟩
  have ha : Acc Nat.lt a := Nat.lt_wfRel.wf.apply a
  induction ha generalizing b with
  | intro a _ha iha =>
      have hb : Acc Nat.lt b := Nat.lt_wfRel.wf.apply b
      induction hb with
      | intro b _hb ihb =>
          constructor
          intro q hq
          obtain ⟨c, d⟩ := q
          rcases hq with h1 | ⟨he, h2⟩
          · exact iha c h1 d
          · subst he; exact ihb d h2

/-- A backtracking operator whose every recursion strictly decreases the
    lexicographic measure (active-prefix length, rank-limit), as established by
    Satz terminierung (b): (iii) each BT recursion shrinks the active prefix or
    lowers the position rank-limit. -/
structure BT (σ : Type) where
  measure  : σ → Nat × Nat
  step     : σ → σ → Prop
  descends : ∀ s s', step s s' → LexNat (measure s') (measure s)

/-- **V₁₁ (Backtracking-Terminierung):** the BT recursion is well-founded — no
    infinite backtracking. -/
theorem BT.wf {σ : Type} (T : BT σ) :
    WellFounded (fun a b => T.step b a) :=
  Subrelation.wf
    (fun {a b} h => T.descends b a h)
    (invImage T.measure ⟨LexNat, lexNat_wf⟩).wf

end Seesaw
