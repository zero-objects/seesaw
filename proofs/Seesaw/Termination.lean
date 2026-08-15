/-
  Seesaw.Termination — termination basis under contractivity.

  Sources:
    * def:contractive / def:kontraktiv (Ch. 9): a rule set is contractive iff
      every cascade has a finite length bound N(R). Sufficient criterion (iii):
      each rule application reduces a well-founded progress measure in the ghost
      state.
    * thm:termination / Satz terminierung, part (a): every cascade terminates
      after at most N(R) steps.

  We formalize the progress-measure criterion (iii) — the constructive core — and
  derive BOTH a qualitative result (well-foundedness: no infinite cascade) and a
  quantitative one (a concrete step bound = the initial measure = N(R)).

  What is NOT covered here (see README staged plan): reducing the syntactic
  criteria (i) stratification and (ii) NAC-gating to a progress measure, and the
  backtracking-tree finiteness of part (b). Those build on this basis.
-/

namespace Seesaw

/-- A cascade equipped with a well-founded progress measure (Def. kontraktiv,
    criterion (iii)). `step s s'` is a real cascade step; contractivity says
    every step strictly decreases the measure. -/
structure Cascade (σ : Type) where
  measure     : σ → Nat
  step        : σ → σ → Prop
  contractive : ∀ s s', step s s' → measure s' < measure s

/-- **Qualitative termination (V_10 basis, Satz part a).**
    Under a strictly-decreasing progress measure the successor relation is
    well-founded: there is no infinite cascade `s₀ → s₁ → s₂ → …`.
    Proof: `step` embeds into the well-founded order `measure · < measure ·`. -/
theorem Cascade.wf {σ : Type} (C : Cascade σ) :
    WellFounded (fun a b => C.step b a) :=
  Subrelation.wf
    (fun {a b} hab => C.contractive b a hab)
    (invImage C.measure Nat.lt_wfRel).wf

/-- A finite run: `f 0, f 1, …, f n` where each of the first `n` transitions is a
    real cascade step. -/
def IsRun {σ : Type} (C : Cascade σ) (f : Nat → σ) (n : Nat) : Prop :=
  ∀ i, i < n → C.step (f i) (f (i + 1))

/-- Key monotonicity: after `i` steps the measure has dropped by at least `i`,
    i.e. `measure (f i) + i ≤ measure (f 0)`. -/
theorem Cascade.measure_drop {σ : Type} (C : Cascade σ)
    (f : Nat → σ) (n : Nat) (h : IsRun C f n) :
    ∀ i, i ≤ n → C.measure (f i) + i ≤ C.measure (f 0) := by
  intro i
  induction i with
  | zero => intro _; simp
  | succ k ih =>
    intro hk
    have hk' : k ≤ n := Nat.le_of_lt (Nat.lt_of_lt_of_le (Nat.lt_succ_self k) hk)
    have hstep : C.step (f k) (f (k + 1)) := h k (Nat.lt_of_lt_of_le (Nat.lt_succ_self k) hk)
    have hdec : C.measure (f (k + 1)) < C.measure (f k) := C.contractive _ _ hstep
    have ihk := ih hk'
    omega

/-- **Quantitative termination (Satz part a, the N(R) bound).**
    Any run has length at most the initial measure. Instantiated with a cascade,
    the initial progress measure IS the length bound `N(R)`: a cascade cannot run
    longer than its starting measure, hence it terminates. -/
theorem Cascade.length_le_measure {σ : Type} (C : Cascade σ)
    (f : Nat → σ) (n : Nat) (h : IsRun C f n) :
    n ≤ C.measure (f 0) := by
  have hn := C.measure_drop f n h n (Nat.le_refl n)
  omega

end Seesaw
