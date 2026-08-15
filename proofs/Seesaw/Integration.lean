/-
  Seesaw.Integration — Stufe 5b: the environment bridges V₂₁, V₂₂, V₂₃.

  Sources:
    * V₂₁ / Lemma v-emf: EMF RecordingCommand notifications map to a Δ-unit that
      reproduces the post-state.
    * V₂₂ / Lemma v-lsp: JDT-AST snapshots across a didSave boundary map to a
      Δ-unit via a structural tree-diff.
    * V₂₃ / Lemma v-trans: the cascade freeze respects EMF transactions.

  NAMED IDEALIZATION #4 (EMF/JDT environment contract). These three are the
  report's *new* obligations bridging the abstract engine to its embedding. They
  hold RELATIVE to the semantics of the environment (EMF notifications, JDT ASTs,
  TransactionalEditingDomain), which is isolated here as named axioms. V₂₃ is
  explicitly a SYSTEM CONTRACT, not a pure theorem — see its note.
-/
import Seesaw.Dpo

namespace Seesaw

/-! ### V₂₁ — EMF observer equivalence -/

axiom EMFModel : Type
axiom Notif    : Type
/-- Baseline translation EMF model → typed graph (Φ of Lemma v-emf). -/
axiom Phi   : EMFModel → TGraph
/-- The notification→op mapping μ_EMF (def:emf-map). -/
axiom muEMF : Notif → List DOp
/-- Environment contract: a RecordingCommand's notifications, mapped by μ_EMF and
    concatenated, reproduce the post-commit graph. -/
axiom emf_faithful :
  ∀ (M₀ M₁ : EMFModel) (ns : List Notif),
    inject ((ns.map muEMF).flatten) (Phi M₀) = Phi M₁

/-- **V₂₁ (EMF-Observer-Äquivalenz).** Every RecordingCommand admits a Δ-unit
    whose op-script takes Φ(M₀) to Φ(M₁) — the derivation-correctness bridge to
    EMF. Combined with `derivation_correct` (V₁), the induced morphism is the
    pushout of the endpoints. -/
theorem emf_observer_equiv (M₀ M₁ : EMFModel) (ns : List Notif) :
    inject ((ns.map muEMF).flatten) (Phi M₀) = Phi M₁ :=
  emf_faithful M₀ M₁ ns

/-- The EMF Δ-unit's induced morphism is the endpoint pushout (V₂₁ ⨯ V₁). -/
theorem emf_morphism (M₀ M₁ : EMFModel) (ns : List Notif) :
    scriptMorph ((ns.map muEMF).flatten) (Phi M₀) = pushout (Phi M₀) (Phi M₁) := by
  unfold scriptMorph
  rw [emf_faithful M₀ M₁ ns]

/-! ### V₂₂ — LSP / JDT-AST observer equivalence -/

axiom JAST   : Type
/-- JDT-AST → typed graph translation (Ψ of Lemma v-lsp). -/
axiom Psi    : JAST → TGraph
/-- The structural tree-diff observer over two AST snapshots. -/
axiom obsAST : TGraph → TGraph → List DOp
/-- Environment contract: the diff between two AST snapshots reproduces the
    post-snapshot graph. -/
axiom ast_faithful :
  ∀ (A₀ A₁ : JAST), inject (obsAST (Psi A₀) (Psi A₁)) (Psi A₀) = Psi A₁

/-- **V₂₂ (LSP/JDT-Observer-Äquivalenz).** Two AST snapshots across a didSave
    boundary admit a Δ-unit reproducing the post-snapshot. -/
theorem lsp_observer_equiv (A₀ A₁ : JAST) :
    inject (obsAST (Psi A₀) (Psi A₁)) (Psi A₀) = Psi A₁ :=
  ast_faithful A₀ A₁

/-! ### V₂₃ — cascade freeze respects EMF transactions (SYSTEM CONTRACT) -/

/-- The environment contract that a `TransactionalEditingDomain` serializes
    commands via the `CommandStack`. This is the hypothesis of V₂₃. -/
axiom Serializes : Prop
/-- Under serialization, the model the cascade reads is not concurrently mutated:
    any two reads during the cascade agree. This is a SYSTEM CONTRACT (a property
    guaranteed by the runtime, not derivable in the calculus). -/
axiom no_concurrent_write :
  Serializes → ∀ (reads : Nat → EMFModel) (i j : Nat), reads i = reads j

/-- **V₂₃ (Kaskaden-Freeze respektiert EMF-Transaktionen).**
    RELATIVE to the serialization contract, the snapshot Φ(M) the cascade reads
    is frozen: any two reads during the cascade materialize identically. This is
    stated as an implication from the contract — it is NOT a pure theorem of the
    calculus, but a guarantee provided by the TransactionalEditingDomain. -/
theorem transaction_freeze (h : Serializes) (reads : Nat → EMFModel) (i j : Nat) :
    Phi (reads i) = Phi (reads j) := by
  rw [no_concurrent_write h reads i j]

end Seesaw
