/-
  Seesaw.Delta — the append-only delta graph: cascade freeze (V₃) and strict
  length monotonicity (V₆).

  Sources:
    * def:deltagraph (Ch. 9): the delta graph is append-only; each cascade step
      adds exactly one entry.
    * V₃ (Kaskaden-Freeze): G_{t1} is immutable during the cascade — the cascade
      appends only in D, never writes the baseline.
    * V₆ (Strikte Monotonie der Länge): |D_x| = x + 1.
-/

namespace Seesaw

/-- The delta graph, an append-only list of entries (def:deltagraph). -/
abbrev Delta (α : Type) := List α

/-- One cascade step appends exactly one entry. -/
def append1 {α : Type} (D : Delta α) (e : α) : Delta α := D ++ [e]

theorem append1_length {α : Type} (D : Delta α) (e : α) :
    (append1 D e).length = D.length + 1 := by
  simp [append1]

/-- Append-only: each step strictly grows the delta. -/
theorem append1_strict_mono {α : Type} (D : Delta α) (e : α) :
    D.length < (append1 D e).length := by
  simp [append1]

/-- The delta after `x` cascade steps: entry `d₀` plus one appended entry per
    step (from a generator `g`). -/
def deltaRun {α : Type} (d0 : α) (g : Nat → α) : Nat → Delta α
  | 0     => [d0]
  | x + 1 => append1 (deltaRun d0 g x) (g x)

/-- **V₆ (Strikte Längen-Monotonie): `|D_x| = x + 1`.**
    The append-only invariant of the cascade (def:deltagraph). -/
theorem deltaRun_length {α : Type} (d0 : α) (g : Nat → α) (x : Nat) :
    (deltaRun d0 g x).length = x + 1 := by
  induction x with
  | zero => rfl
  | succ n ih => simp only [deltaRun, append1_length, ih]

/-- Strict monotonicity in the step index, a direct corollary. -/
theorem deltaRun_strict_mono {α : Type} (d0 : α) (g : Nat → α) (x : Nat) :
    (deltaRun d0 g x).length < (deltaRun d0 g (x + 1)).length := by
  rw [deltaRun_length, deltaRun_length]; omega

/-! ## Cascade freeze (V₃) -/

/-- A cascade state: a frozen baseline plus the growing delta. -/
structure CState (G α : Type) where
  baseline : G
  delta    : Delta α

/-- A cascade step appends to the delta and never touches the baseline. -/
def cascadeStep {G α : Type} (s : CState G α) (e : α) : CState G α :=
  { s with delta := append1 s.delta e }

/-- **V₃ (Kaskaden-Freeze): the baseline is immutable under a cascade step.**
    Operational, by construction — the step writes only into `delta`. -/
theorem cascadeStep_freeze {G α : Type} (s : CState G α) (e : α) :
    (cascadeStep s e).baseline = s.baseline := rfl

/-- V₃ over an entire cascade: the baseline is invariant under any sequence of
    cascade steps. -/
theorem cascadeSteps_freeze {G α : Type} (es : List α) (s : CState G α) :
    (es.foldl cascadeStep s).baseline = s.baseline := by
  induction es generalizing s with
  | nil => rfl
  | cons e es ih => rw [List.foldl_cons, ih (cascadeStep s e), cascadeStep_freeze]

end Seesaw
