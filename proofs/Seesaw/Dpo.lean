/-
  Seesaw.Dpo — Stufe 4: derivation correctness (V₁), effect-equivalence
  congruence (V₂), net-delta invertibility (V₁₉).

  Sources:
    * def:delta, def:observer, lem:derivation (formal_core.tex);
      Lemma v1-full, v2-full, v19-full.
    * V₁: obs is well-defined up to effect-equivalence — the induced DPO pushout
      morphism is unique.
    * V₂: effect-equivalence is a congruence w.r.t. composition.
    * V₁₉: the net delta has a computable involutive inverse of equal size.

  NAMED IDEALIZATION #2 (DPO pushout). Mathlib's CategoryTheory ships no
  adhesive-category / DPO API, so — as the report and the coordinator both
  anticipate — the existence and uniqueness of the DPO pushout is isolated as
  named interface axioms (`TGraph`, `PMorph`, `pushout`, `inject`, `pcomp`),
  NOT proved and NOT global. Every lemma resting on them is flagged by
  `#print axioms`. V₁₉'s inverse construction is fully constructive (axiom-free).
-/

namespace Seesaw

/-! ### Op model for the observer inverse (V₁₉) — constructive, axiom-free -/

/-- The five op kinds of def:delta. `setA` carries new and old value so it is
    self-inverting. -/
inductive DOp where
  | addN (id : Nat)
  | delN (id : Nat)
  | addE (id : Nat)
  | delE (id : Nat)
  | setA (id key vNew vOld : Nat)
deriving DecidableEq, Repr

/-- Pointwise op inverse (proof of Lemma v19-full's constructive step). -/
def invOp : DOp → DOp
  | .addN i          => .delN i
  | .delN i          => .addN i
  | .addE i          => .delE i
  | .delE i          => .addE i
  | .setA i k vN vO  => .setA i k vO vN

theorem invOp_invOp (o : DOp) : invOp (invOp o) = o := by
  cases o <;> rfl

/-- The inverse op-script: reverse and invert each op. -/
def invScript (s : List DOp) : List DOp := (s.map invOp).reverse

/-- **V₁₉, complexity:** the inverse has the same length — computable in O(|Δ|). -/
theorem invScript_length (s : List DOp) : (invScript s).length = s.length := by
  unfold invScript
  rw [List.length_reverse, List.length_map]

/-- **V₁₉, involution (effect-inverse):** inverting twice is the identity. -/
theorem invScript_invScript (s : List DOp) : invScript (invScript s) = s := by
  have hid : (invOp ∘ invOp) = id := funext invOp_invOp
  simp only [invScript, List.map_reverse, List.reverse_reverse, List.map_map, hid,
             List.map_id]

/-! ### DPO pushout idealization (V₁, V₂) -/

/-- Typed attributed graphs (opaque). -/
axiom TGraph : Type
/-- The induced-morphism space (opaque). -/
axiom PMorph : Type
/-- DPO pushout of two endpoint graphs — existence of the induced morphism. -/
axiom pushout : TGraph → TGraph → PMorph
/-- Op-script application to a graph. -/
axiom inject : List DOp → TGraph → TGraph
/-- Composition of induced morphisms (functoriality of the pushout). -/
axiom pcomp : PMorph → PMorph → PMorph

/-- The morphism a script induces on a graph: the pushout of its endpoints. This
    is a function of the endpoints only — the content of V₁ (uniqueness). -/
noncomputable def scriptMorph (s : List DOp) (G0 : TGraph) : PMorph := pushout G0 (inject s G0)

/-- **V₁ (Derivationskorrektheit) / Lemma v1-full.** Any two op-scripts taking
    `G0` to the same `G1` induce the same morphism (the DPO pushout of the shared
    endpoints). -/
theorem derivation_correct (G0 G1 : TGraph) (s₁ s₂ : List DOp)
    (h₁ : inject s₁ G0 = G1) (h₂ : inject s₂ G0 = G1) :
    scriptMorph s₁ G0 = scriptMorph s₂ G0 := by
  unfold scriptMorph
  rw [h₁, h₂]

/-- Effect-equivalence: equality of induced morphisms (def:delta ∼-relation). -/
def EffEq (m₁ m₂ : PMorph) : Prop := m₁ = m₂

theorem effEq_refl (m : PMorph) : EffEq m m := rfl
theorem effEq_symm {a b : PMorph} (h : EffEq a b) : EffEq b a := h.symm
theorem effEq_trans {a b c : PMorph} (h₁ : EffEq a b) (h₂ : EffEq b c) : EffEq a c := h₁.trans h₂

/-- **V₂ (Effekt-Äquivalenz als Kongruenz) / Lemma v2-full.** Effect-equivalence
    is a congruence w.r.t. morphism composition. -/
theorem effEq_congr {a a' b b' : PMorph}
    (h₁ : EffEq a a') (h₂ : EffEq b b') :
    EffEq (pcomp a b) (pcomp a' b') := by
  unfold EffEq at *
  rw [h₁, h₂]

end Seesaw
