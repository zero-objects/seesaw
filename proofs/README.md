# Seesaw Rev2 — Mechanisierung der Theorie

Die mechanisierten Beweise zum Paper. Sie decken die 23
Verifikations-Verpflichtungen V₁–V₂₃ des formalen Kerns ab; das Paper
zitiert sie in Abschnitt „Mechanised Verification".

## Toolchain

- Proof-Assistant: **Lean 4** (v4.15.0), via `elan` in `~/.elan`.
- **Lean-Core only, kein Mathlib.** Alle 23 Vs tragen in Lean-Core; das hält die
  Trusted Base klein und den Build schnell (< 1 s statt Stunden). Mathlib bringt
  hier nichts, weil `CategoryTheory` kein adhäsives/DPO-Paket hat — der DPO-Teil
  läuft ohnehin über benannte Schnittstellen-Axiome (s. Idealisierungen).
- Build: `source ~/.elan/env && lake build` in diesem Verzeichnis.

## Stand: alle 23 Vs formalisiert und bewiesen (`lake build` grün, kein `sorry`)

**V₅ ist über die Korrespondenz formuliert, nicht über den Status.**
Früher lautete es „keine Operation verändert ein SOLID-Element". Das
passte zur damaligen Engine, die nur GHOST-Erzeugnisse zurückzog, und
war genau der Defekt: das Löschen der Quelle eines materialisierten
Erzeugnisses setzte keinen Tombstone (behoben in 2.0.1). Der Status
beschreibt die Materialisierung, nicht die Herkunft. Die Herkunft trägt
die Korrespondenz — `Carries` sagt, welche Korrespondenz ein Element
hält, `Free` heißt: von keiner gehalten. V₅ sagt damit, was es immer
meinte: Struktur, die die Kaskade nicht erzeugt hat, bleibt
unberührt.

Vollständig relativ zu **vier benannten, isolierten Idealisierungen** (unten).
17 Vs sind ohne projektspezifische Annahme bewiesen (nur Lean-Standard,
teils axiomfrei), 6 ruhen auf je einer der drei Schnittstellen-Idealisierungen.
Die vierte, die Kollisionsfreiheit des Struktur-Hashes, steckt nur im
Hilfslemma `distinct_struct_distinct_id`, nicht im V₁₈-Satz selbst. Nichts
global axiomatisiert, `#print axioms` pro Lemma geprüft.

| Datei | Inhalt | Bezug |
|---|---|---|
| `Seesaw/Basic.lean` | `Status`; Sichtbarkeit vs. Materialisierung; `IdInput`/`H`/`Element`/`idOf`; `rename_stable`; `distinct_struct_distinct_id` | def:status, def:ghostid |
| `Seesaw/Delta.lean` | `deltaRun_length` (\|D_x\|=x+1); `cascadeStep_freeze`/`cascadeSteps_freeze` | **V₆**, **V₃** |
| `Seesaw/Projection.lean` | `applyScript_preserves_free_solid`; `materialized_element_stays_retractable`; `consolidate_preserves_free_solid`; `Applies_total` + `Applies_functional` | **V₅**, **V₄** |
| `Seesaw/Rank.lean` | `rank_injective`; `rank_total`/`_deterministic`/`muEnum_deterministic`; `selection_wf` + `selection_total` | **V₁₃**, **V₁₄**, **V₁₅** |
| `Seesaw/Fold.lean` | `fold_fixpoint` (konstruktiv) | **V₁₆** |
| `Seesaw/Termination.lean` | `Cascade.wf` + `Cascade.length_le_measure` (N(R)-Schranke) | **V₁₀-Basis** |
| `Seesaw/Retraction.lean` | `Retraction.wf`/`length_le_depth`/`preserves_admissible`; `acyclic`; `lexNat_wf` + `BT.wf` | **V₈**, **V₁₂**, **V₁₁** |
| `Seesaw/Consolidation.lean` | `matAt`; `consolidation_faithful`/`consolidation_effect_equiv`; `consolidation_idempotent` + `present_op_essential`; `iso_ghosts_same_id`/`sameTarget_collapse` | **V₁₇**, **V₁₈** |
| `Seesaw/Dpo.lean` | `derivation_correct`; `effEq_refl/symm/trans` + `effEq_congr`; `invScript_length` + `invScript_invScript` | **V₁**, **V₂**, **V₁₉** |
| `Seesaw/Decidability.lean` | `Admissible` (dec.); `dupByHash_of_same_struct` + `dup` (dec.) + `contra` (dec.); `conformance_preserved` | **V₇**, **V₉**, **V₂₀** |
| `Seesaw/Integration.lean` | `emf_observer_equiv` + `emf_morphism`; `lsp_observer_equiv`; `transaction_freeze` | **V₂₁**, **V₂₂**, **V₂₃** |

### Die vier benannten Idealisierungen (isoliert, dokumentiert, nicht global)

1. **BLAKE3-Kollisionsfreiheit** `H_injective` (Basic) — nur in
   `distinct_struct_distinct_id` (V₁₈-Vorbereitung). Mechanisch unbeweisbar wie
   bei jeder Hash-Funktion.
2. **DPO-Pushout-Existenz/-Eindeutigkeit** `TGraph`, `PMorph`, `pushout`,
   `inject`, `pcomp` (Dpo) — trägt V₁, V₂. (`CategoryTheory` liefert kein DPO;
   Schnittstellen-Axiome sind der vom Report vorgesehene Weg.)
3. **Graph-Isomorphie-Entscheidungskern** `GIso`, `giso_decidable`
   (Decidability) — nur `dup` (V₉, Worst-Case). Der Hash-Pfad
   `dupByHash_of_same_struct` ist idealisierungsfrei.
4. **EMF/JDT-Umgebungskontrakt** `Phi`/`muEMF`/`emf_faithful`,
   `Psi`/`obsAST`/`ast_faithful`, `Serializes`/`no_concurrent_write`
   (Integration) — trägt V₂₁, V₂₂, V₂₃.

**Idealisierungsfrei (nur `propext`/`Quot.sound`, teils gar kein Axiom):** V₃,
V₄, V₅, V₆, V₈, V₁₀-Basis, V₁₁, V₁₂, V₁₃, V₁₄, V₁₅, V₁₆, V₁₇, V₁₈ (Kern), V₇,
V₁₉, V₂₀. **Kein `Classical.choice`, kein `sorry`.**

### Prinzipiell offen / bewusst als Kontrakt markiert

- **V₂₃** gilt nur RELATIV zum Serialisierungs-Kontrakt der
  `TransactionalEditingDomain` — ein System-Kontrakt, kein reiner Kalkül-Satz
  (im Code so kommentiert).
- **V₉** allgemeiner Graph-Isomorphie-Kern und **V₁** DPO-Existenz bleiben
  Idealisierungen (s. o.); der Report verweist hier ebenfalls auf externe
  Standardresultate.
- **V₁₇/V₁₈** sind hier auf dem Rollup-Override-Kern (max-κ-Materialisierung)
  bewiesen; die volle Nullifikations-Fallunterscheidung (Add-Del-Paar, V₁₂-
  induziert) ist als dominierte Entfernung bzw. `present_op_essential` abgedeckt.

## Inventar V₁–V₂₃ mit Schwierigkeits-Einschätzung

Klassifikation aus `09_verifikation.tex`: ✓ trivial · • Skizze ausreichend ·
△ echte Lücke. „Mech" = Aufwand der Mechanisierung.

Spalte „Mech" = Mechanisierungs-Status/-Aufwand. ✅ = machine-checked.

| V | Aussage | Baut auf | Report | Mech |
|---|---|---|---|---|
| V₁ | Derivationskorrektheit (obs wohldef. bis Effekt-Äq.) | DPO-Pushout | •/✓ | ✅ `derivation_correct` (Idealisierung #2) |
| V₂ | Effekt-Äquivalenz ist Kongruenz | V₁ | • | ✅ `effEq_congr` (+refl/symm/trans) (Idealisierung #2) |
| V₃ | Kaskaden-Freeze (G_t1 immutable) | Konstruktion | ✓ | ✅ `cascadeStep_freeze`, `cascadeSteps_freeze` |
| V₄ | Projektionsstabilität (φ_L wohldef.) | Induktion \|D\| | • | ✅ `Applies_total` + `Applies_functional` |
| V₅ | Kaskaden-Isolation (was die Kaskade nicht erzeugt hat, bleibt unberührt) | V₃, def:status | ✓ | ✅ `applyScript_preserves_free_solid`, Gegenstück `materialized_element_stays_retractable` |
| V₆ | Strikte Längen-Monotonie (\|D_x\|=x+1) | append-only | ✓ | ✅ `deltaRun_length` |
| V₇ | Nicht-Erasure (eigentlich Definition) | — | ✓ | ✅ `Admissible` (dec.); von V₈ via `preserves_admissible` genutzt |
| V₈ | Retraktion terminiert + erhält V₇ | Wohlfund. depth | • | ✅ `Retraction.wf` + `length_le_depth` + `preserves_admissible` |
| V₉ | Entscheidbarkeit dup/contra | GI, Hash-Shortcut | • | ✅ `dup` (dec., Idealisierung #3) + `contra` (dec., frei) + `dupByHash_of_same_struct` (frei) |
| V₁₀ | Kaskaden-Terminierung | **Kontraktivität** | △ | ✅ Basis (`Cascade.wf` + `length_le_measure`); offen: Kriterien (i)/(ii) → Maß |
| V₁₁ | Backtracking-Terminierung | V₁₀, lexikograph. Maß | △ | ✅ `BT.wf` (lex-Maß, `lexNat_wf`) |
| V₁₂ | Retraktions-Induces-DAG azyklisch | struct-dep, V₈ | • | ✅ `Retraction.acyclic` |
| V₁₃ | **Rang-Injektivität** | Basis-M, ρ injektiv | • | ✅ `rank_injective` |
| V₁₄ | Totalität + Determinismus von μ | Konstruktion | ✓ | ✅ `rank_total`/`rank_deterministic`/`muEnum_deterministic` |
| V₁₅ | Wohlgeordneter Selektionsraum | V₁₃+V₁₄ | ✓ | ✅ `selection_wf` + `selection_total` |
| V₁₆ | Terminierung Konsolidierung | monoton, Fixpunkt | ✓ | ✅ `fold_fixpoint` |
| V₁₇ | Semantische Treue der Konsolidierung | Induktion \|C\|, 3 Fälle | • | ✅ `consolidation_faithful` (frei) |
| V₁₈ | Minimalität + Effekt-Isomorphie | V₁₇, Fixpunkt, GhostId | • | ✅ `consolidation_effect_equiv` + `consolidation_idempotent` + `present_op_essential` + `iso_ghosts_same_id` (frei) |
| V₁₉ | Invertierbarkeit Netto-Delta O(\|Δ\|) | Observer-Symmetrie, V₁ | • | ✅ `invScript_length` + `invScript_invScript` (frei) |
| V₂₀ | Konsistenz bei Navigation (Typkonformität) | Materialisierung | ✓ | ✅ `conformance_preserved` (frei) |
| V₂₁ | EMF-Observer-Äquivalenz | μ_EMF Mapping, V₁ | neu | ✅ `emf_observer_equiv` + `emf_morphism` (Idealisierung #4) |
| V₂₂ | LSP/JDT-Observer-Äquivalenz | Tree-Diff, V₁ | neu (Skizze) | ✅ `lsp_observer_equiv` (Idealisierung #4) |
| V₂₃ | Kaskaden-Freeze respektiert EMF-Transaktionen | V₃, CommandStack | neu | ✅ `transaction_freeze` (Idealisierung #4, System-Kontrakt) |

**Alle 23 Vs formalisiert.** „frei" = ohne Idealisierung (nur Lean-Standard).
Idealisierung #2 = DPO-Pushout, #3 = GI-Kern, #4 = EMF/JDT-Kontrakt, #1 = BLAKE3
`H_injective` (nur `distinct_struct_distinct_id`). Details oben.

## Stufenplan — abgeschlossen

**Stufe 0–2 — ✅** V₁₃; V₁₀-Basis; GhostId + Status; V₃/V₄/V₅/V₆/V₁₄/V₁₅/V₁₆;
V₈/V₁₂/V₁₁. Alle in Lean-Core, idealisierungsfrei.

**Stufe 3 — ✅** V₁₇/V₁₈: Materialisierung `matAt` (max-κ-Rollup),
`consolidation_faithful`, Minimalität/Fixpunkt, Effekt-Isomorphie via GhostId.
Idealisierungsfrei.

**Stufe 4 — ✅** V₁/V₂ über benannte DPO-Pushout-Axiome; V₁₉ (Op-Inverse)
idealisierungsfrei.

**Stufe 5 — ✅** V₇ (Prädikat), V₉ (Hash-Pfad frei + GI-Kern-Axiom), V₂₀
(Konformanz-Erhalt frei), V₂₁/V₂₂/V₂₃ über den benannten EMF/JDT-Kontrakt (V₂₃
explizit System-Kontrakt).

---

Ursprüngliche Planbeschreibung (Stufe 1–5):

**Stufe 1 — kostengünstige Einzelbeweise, kein Mathlib, kein Semantik-Modell.**
V₆, V₃/V₅, V₁₄/V₁₅, V₁₆, V₄. Reine Struktur-/Induktions-/Arithmetik-Argumente.
Erweitert den verifizierten Kern breit, geringes Risiko.

**Stufe 2 — Wohlfundiertheits-Familie auf `Cascade.wf` aufsetzend.**
V₈ (Retraktion, depth-Maß), V₁₂ (Induces-DAG azyklisch), V₁₁ (BT-Baum-
Finitheit, König). Braucht ein explizites `depth`/Abhängigkeits-Modell; hier
lohnt der Wechsel auf Mathlib (`WellFounded`, `Finset`, `Relation`-Bibliothek).

**Stufe 3 — Konsolidierungs-Semantik.** Materialisierungsfunktion `mat` und
Rollup-Ordnung κ modellieren, dann V₁₇, dann V₁₈ (Teil iii ist über
`distinct_struct_distinct_id` schon vorbereitet). Größter Einzelbrocken vor den
Integrations-Vs; Mathlib nötig.

**Stufe 4 — Kategorielles Fundament.** V₁, V₂, V₁₉. DPO-Pushouts in adhäsiven
Kategorien. Entweder Mathlib-`CategoryTheory` (kein fertiges adhäsives/DPO-Paket
— erheblicher Eigenbau) oder Abstraktion der benötigten Pushout-Eigenschaften
als Schnittstellen-Axiome (pragmatischer, erklärt die Idealisierung offen).

**Stufe 5 — Entscheidbarkeit + Integration.** V₉ (Hash-Reduktion mechanisieren,
GI-Kern als Annahme), V₂₀, V₂₁/V₂₂/V₂₃ (Observer-Äquivalenz gegen EMF/JDT — nur
so weit formalisierbar, wie die externe Semantik axiomatisiert wird).

**Harte Stellen / bewusste Idealisierungen.**
- BLAKE3-Kollisionsfreiheit → bleibt Axiom `H_injective`.
- Graph-Isomorphie (V₉) → GI-Entscheidungskern als Annahme; nur die Hash-
  Reduktion wird bewiesen.
- DPO-Pushout-Existenz/-Eindeutigkeit (V₁) → entweder voller Mathlib-Aufbau
  oder Schnittstellen-Axiome.
- EMF/JDT-Notification-Semantik (V₂₁–V₂₃) → nur relativ zu einem axiomatisierten
  Umgebungsmodell beweisbar; teils System-Kontrakt statt Satz.

Der Plan ist abgearbeitet: alle 23 Vs sind mechanisiert, einschließlich der
Integrations-Vs. Die Aufwandsschätzung darüber ist der Stand vor der
Durchführung und bleibt nur als Planungshistorie stehen.
