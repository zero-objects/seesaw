/-
  Seesaw Rev2 — Mechanized core of the formal theory.

  This is the FIRST mechanization step of the paper-level proofs
  (companion technical report, Chapter 9, Lemmata V_1 .. V_23).
  See proofs/README.md for the full inventory and the staged plan.

  Scope of this file set (verified in Lean 4 core, no Mathlib):
    * Seesaw.Basic       — Status (Solid/Ghost/Tent/Tomb), GhostId (parent-rooted
                           identity, def:ghostid), Element, rename stability,
                           collision-freedom idealization.
    * Seesaw.Rank        — V_13 rank injectivity (thm:rank-inj / Lemma v13-full).
    * Seesaw.Termination — Termination basis under contractivity
                           (thm:termination / Satz terminierung, part a):
                           a strictly-decreasing progress measure gives
                           well-foundedness AND a concrete N(R) length bound.
-/
import Seesaw.Basic
import Seesaw.Delta
import Seesaw.Projection
import Seesaw.Rank
import Seesaw.Fold
import Seesaw.Termination
import Seesaw.Retraction
import Seesaw.Consolidation
import Seesaw.Dpo
import Seesaw.Decidability
import Seesaw.Integration
