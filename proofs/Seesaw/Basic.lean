/-
  Seesaw.Basic — core datatypes of the ghost overlay and parent-rooted identity.

  Sources:
    * def:status  (formal_core.tex): status-annotated ghost graph.
    * def:ghostid (formal_core.tex): parent-rooted identity id(e) = H(id(parent) || edgedata || sigma(e)).
-/

namespace Seesaw

/-! ## Node/edge status (def:status)

A ghost graph is a typed attributed graph with a status function
`status : V ∪ E → {SOLID, GHOST, TENT, TOMB}`.
-/

/-- Element status in the ghost overlay (def:status). -/
inductive Status where
  /-- belongs to the frozen baseline `L0`/`R0`. -/
  | solid
  /-- virtually added during the cascade. -/
  | ghost
  /-- tentative tombstone: still visible to matching, reclaimable by resurrection. -/
  | tent
  /-- virtually deleted. -/
  | tomb
deriving DecidableEq, Repr

/-- Visibility to pattern matching. A `tent` element remains visible so a later
    rule application deriving the same parent-rooted identity can reclaim it
    (resurrection); a `tomb` element is invisible. -/
def Status.visible : Status → Bool
  | .solid => true
  | .ghost => true
  | .tent  => true
  | .tomb  => false

/-- Materialized presence at consolidation time: `solid`/`ghost` survive into a
    baseline; a still-tentative element resolves to `tomb`, so it does not
    materialize. -/
def Status.materialized : Status → Bool
  | .solid => true
  | .ghost => true
  | .tent  => false
  | .tomb  => false

/-- Small well-formedness fact: anything that materializes was visible to
    matching. (The converse fails — `tent` is visible but does not materialize,
    which is exactly the reclaimable-window property of def:status.) -/
theorem materialized_visible (s : Status) :
    s.materialized = true → s.visible = true := by
  cases s <;> simp [Status.materialized, Status.visible]

/-- The reclaimable window is nonempty: `tent` is visible yet not materialized. -/
theorem tent_reclaimable :
    Status.tent.visible = true ∧ Status.tent.materialized = false := by
  constructor <;> rfl

/-! ## Parent-rooted identity (def:ghostid)

`id(e) = H( id(parent(e)) || edgedata(e) || sigma(e) )`, where `sigma(e)` is the
local structure hash over the element's *identity* attributes. Attributes whose
values are propagated across a correspondence are deliberately excluded from
`sigma`, so a source-side rename re-derives the *same* identity.
-/

/-- The structural tuple that is hashed by `H` (def:ghostid). All three fields
    are the *identity-relevant* structure: parent identity, edge data, and the
    identity-attribute hash `sigma`. Mutable propagated values never appear here. -/
structure IdInput where
  parent   : Nat   -- id(parent(e)); solid baseline anchors the recursion.
  edgedata : Nat
  sigma    : Nat   -- local structure hash over identity attributes only.
deriving DecidableEq, Repr

/-- The collision-resistant hash `H` (BLAKE3 in the reference implementation).
    Kept opaque: the proofs depend only on its structural properties, never on
    the concrete digest algorithm. -/
opaque H : IdInput → Nat

/-- IDEALIZATION (explicit assumption): `H` is injective on structural inputs —
    i.e. distinct hashed tuples yield distinct identities. The model starts from
    an already structured `IdInput` and therefore abstracts over the concrete
    byte encoding; injectivity of that encoding is a separate implementation
    obligation. `H_injective` models collision-freedom of BLAKE3 on the encoded
    inputs. It is an *assumption*, not a theorem: a true hash has collisions with
    negligible probability, and no mechanized proof of unconditional injectivity
    exists. Every result that relies on structural identity separation is
    therefore conditional on `H_injective`. -/
axiom H_injective : ∀ x y : IdInput, H x = H y → x = y

/-- A ghost element. `idAttrs` feed `sigma`; `valAttr` is a mutable value that is
    propagated across a correspondence and therefore *excluded* from `sigma`. -/
structure Element where
  parent   : Nat
  edgedata : Nat
  idAttrs  : Nat   -- identity attributes  → contribute to sigma
  valAttr  : Nat   -- propagated mutable value → excluded from sigma
deriving DecidableEq, Repr

/-- `sigma` over the identity attributes only (def:ghostid): it ignores `valAttr`
    by construction. -/
def sigma (e : Element) : Nat := e.idAttrs

/-- The parent-rooted identity of an element. -/
def idOf (e : Element) : Nat := H ⟨e.parent, e.edgedata, sigma e⟩

/-- RENAME STABILITY (central design claim of def:ghostid).
    Changing only the mutable, propagated value attribute leaves the identity
    unchanged — so a source-side rename updates the existing target instead of
    minting a duplicate. Proof is by construction: `sigma` does not read
    `valAttr`, so the hashed tuple is unchanged. -/
theorem rename_stable (e : Element) (v' : Nat) :
    idOf { e with valAttr := v' } = idOf e := by
  simp [idOf, sigma]

/-- COLLISION-FREEDOM CONSEQUENCE (conditional on `H_injective`).
    Elements whose identity-relevant structure differs get distinct identities —
    the property that lets duplication detection reduce a graph-isomorphism test
    to a hash comparison (Lemma v9-full, V_18(iii)). -/
theorem distinct_struct_distinct_id {e₁ e₂ : Element}
    (h : (⟨e₁.parent, e₁.edgedata, sigma e₁⟩ : IdInput)
         ≠ ⟨e₂.parent, e₂.edgedata, sigma e₂⟩) :
    idOf e₁ ≠ idOf e₂ := by
  intro heq
  exact h (H_injective _ _ heq)

end Seesaw
