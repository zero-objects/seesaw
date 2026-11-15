# Auswahl der Optionen (`perf/select`): Gate und Messung, 2026-11-15, codepit

Endstand `perf/select` @ b9b3ce9, Board-Reservierung 993. Der Rechner
war nicht ruhig (Load 13 bis 20). Instruktionszahlen hängen nicht an
der Last, Zeiten schon.

b9b3ce9 schließt einen Fund der Leseprüfung (unten). Gate,
Instruktionen und Konsumenten sind auf b9b3ce9 wiederholt, Zeit und
paralleler Seed stammen vom Stand davor (Engine-Code von fee8a4f).

Der Zweig übernimmt aus `perf/options` (dea3f1c) die Optionen A, E, G,
I und M fest, B als API, C und H hinter dem Feature `low_memory`, J
hinter dem Feature `parallel_seed`. D und F sind entfernt.

## Stände (Spalte 1 der TSV)

| Label | Stand | Features |
|---|---|---|
| `v202` | Engine 2.0.2 (f2b0e53) mit der Suite von `perf/main` | keine |
| `main` | `perf/main` (aaccbcd) | keine |
| `none` | `perf/options` (dea3f1c) | keine. Für die pde-Zellen und die Zellen mit vielen Engines, die `perf/main` nicht hat. Die Engine entspricht dort `perf/main`. |
| `sel` | `perf/select` | keine |
| `lowmem` | `perf/select` | `low_memory` |
| `par` | `perf/select` | `parallel_seed` |

Alle Messbinaries sind mit einer Codegen-Einheit gebaut
(`CARGO_PROFILE_BENCH_CODEGEN_UNITS=1`).

## Dateien

- `gate.txt`: Ergebniszeilen des Gates auf b9b3ce9. Spalte 1 nennt den
  Teil (Tabelle unten).
- `gate-fee8a4f.txt`: dasselbe Gate auf fee8a4f, vor dem Fix. Alle Teile
  grün, je 5 Tests weniger.
- `sel-cgu1.tsv`: ein Lauf je Zelle und Stand, 20 Zellen. Spalten wie
  in der Suite: Label, Zelle, Samples, User-CPU min und median (ms),
  Wall (ms), Peak-RSS (MB), Schritte, angewandte Schritte, Knoten,
  Fingerprint, dann `instr=`, `cycles=`, `llc_miss=`, `l1d_miss=`,
  `br_miss=`. `v202` lief auf sieben Zellen, `main` auf zwölf, `none`
  auf acht. Fingerprint, angewandte Schritte und Knotenzahl sind je
  Zelle über alle Stände gleich. Gemessen auf b7b3a54.
- `fin-cgu1.tsv`: `sel` und `lowmem` auf fee8a4f neu gebaut und neu
  gemessen. Größte Abweichung der Instruktionen gegen `sel-cgu1.tsv`:
  0,006 %.
- `uid-cgu1.tsv`: `sel` und `lowmem` auf b9b3ce9. Fingerprint,
  angewandte Schritte und Knoten gleich `fin-cgu1.tsv`. Instruktionen:
  `lowmem` höchstens 0,009 % anders, `sel` in den synthetischen Zellen
  höchstens 0,003 %, in den pde-Zellen +0,01 bis +0,14 %
  (`pde/edit/open2-edit`).
- `uid-small-cgu1.tsv`: die 13 kleinen Zellen für `sel` und `lowmem` auf
  b9b3ce9. Schritte, angewandte Schritte, Knoten und Fingerprint gleich
  `v202` in `small-cgu1.tsv`.
- `small-cgu1.tsv`: die 13 Zellen der Größen `k50/n2000` und `f2000`
  für `v202`, `sel` und `lowmem`. Schritte, angewandte Schritte, Knoten
  und Fingerprint sind je Zelle gleich.
- `time.tsv`: Zeit im Wechsel, drei Runden, fünf Zellen, je Runde und
  Stand ein Lauf mit zwei Samples (`main` oder `none`, `sel`,
  `lowmem`). Ausgewertet wird das Minimum der User-CPU (Spalte 4).
- `time2.tsv`: drei weitere Runden `sel` gegen `lowmem`.
- `par.tsv`: Zellen des parallelen Seeds mit dem Binary `par`, drei
  Samples. Spalte 6 ist die Wall-Zeit.
- `cons.tsv`: Konsumenten gegen den Engine-Code von b9b3ce9 (B). Die
  Vergleichswerte mit 2.0.2 stehen in
  `../2026-11-15-codepit/consumers.tsv`. `cons-7f84ad9.tsv`: derselbe
  Lauf mit dem Engine-Code vor dem Fix.

Die pde-Zellen brauchen `SEESAW_PDE_DUMP` (Protokolle, nicht im
Repository, siehe `../2026-11-15-pde/README.md`).

## Gate

Alle Teile auf b9b3ce9, 19:56 bis 20:07 UTC.

| Teil | Ergebniszeile |
|---|---|
| `stand` | b9b3ce9 0 offene Aenderungen |
| `stand` | rustc 1.95.0 (59807616e 2026-04-14) |
| `stand` | cargo 1.88.0 (873a06493 2025-05-10) |
| `stand` | 19:56:02 up 10 days, 11:59,  0 users,  load average: 15.01, 17.26, 16.51 |
| `g1` | `FMT_RC=0` |
| `g1` | `CLIPPY_RC=0 [none]` |
| `g1` | `TEST_RC=0 [none] groups=18 passed=206 failed=0 ignored=7` |
| `g2` | `FMT_RC=0` |
| `g2` | `CLIPPY_RC=0 [low_memory]` |
| `g2` | `TEST_RC=0 [low_memory] groups=18 passed=207 failed=0 ignored=7` |
| `g3` | `FMT_RC=0` |
| `g3` | `CLIPPY_RC=0 [parallel_seed]` |
| `g3` | `TEST_RC=0 [parallel_seed] groups=18 passed=209 failed=0 ignored=7` |
| `g4` | `FMT_RC=0` |
| `g4` | `CLIPPY_RC=0 [low_memory+parallel_seed]` |
| `g4` | `TEST_RC=0 [low_memory+parallel_seed] groups=18 passed=210 failed=0 ignored=7` |
| `c1` | `CHECKS_RC=0 [none] groups=18 passed=206 failed=0 ignored=7` |
| `c2` | `CHECKS_RC=0 [low_memory+parallel_seed] groups=18 passed=210 failed=0 ignored=7` |
| `tr` | `PERF_TRACE_RC=0 groups=18 passed=206 failed=0 ignored=7` |
| `r1` | `R188_TEST_RC=0 groups=18 passed=206 failed=0 ignored=7` |
| `r2` | `R188_CLIPPY_RC=0` |
| `r2` | `R188_CHECK_BEIDE_RC=0` |
| `mi` | `WASM_RC=0` |
| `mi` | `WASM_LOW_MEMORY_RC=0` |
| `mi` | `WASM_PARALLEL_SEED_RC=0` |
| `mi` | `DOC_RC=0` |
| `mi` | `DOC_BEIDE_RC=0` |
| `mi` | `PUBLISH_DRY_RUN_RC=0` |
| `m1` | `MVN_RC=0 []` |
| `m1` | `MVN run=150 failures=0 errors=0 skipped=4` |
| `m2` | `MVN_RC=0 [-DargLine=-Dseesaw.anchorIndex=false]` |
| `m2` | `MVN run=150 failures=0 errors=0 skipped=4` |
| `m3` | `MVN_RC=0 [-DargLine=-Dseesaw.checks=true]` |
| `m3` | `MVN run=150 failures=0 errors=0 skipped=4` |
| `b1` | `BE_BUILD_RC=0` |
| `b2` | `BE_TESTS_RC=0 groups=18 passed=206 failed=0 ignored=7` |
| `b3` | `BE_BENCH_BUILD_RC=0` |
| `b3` | s390x gegen x86_64: 6 von 6 Zellen mit gleichen Schritten, angewandten Schritten, Knoten und Fingerprint |
| `d1` | `DIFF_RC=0 [none] faelle=standard groups=4 passed=8 failed=0 ignored=0` |
| `d2` | `DIFF_RC=0 [low_memory+parallel_seed] faelle=standard groups=4 passed=8 failed=0 ignored=0` |
| `d3` | `DIFF_RC=0 [none] faelle=1500 groups=4 passed=7 failed=0 ignored=0` |
| `d4` | `DIFF_RC=0 [low_memory+parallel_seed] faelle=1500 groups=4 passed=7 failed=0 ignored=0` |
| `f1` | `FAMILY_RC=0 [none] faelle=400 groups=1 passed=1 failed=0 ignored=0` |
| `f2` | `FAMILY_RC=0 [low_memory+parallel_seed] faelle=400 groups=1 passed=1 failed=0 ignored=0` |
| `dc1` | `DIFF_UNTER_CHECKS_RC=0 [none] groups=4 passed=8 failed=0 ignored=0` |
| `dc2` | `DIFF_UNTER_CHECKS_RC=0 [low_memory+parallel_seed] groups=4 passed=8 failed=0 ignored=0` |

## Instruktionen (Mio.)

| Zelle | 2.0.2 | vorher | Auswahl | `low_memory` |
|---|---|---|---|---|
| `langpack/add/k700/n2000` | 158 773 | 178 | 155 (−13 %) | 159 (+2 %) |
| `langpack/add/k700/n20000` | – | 1 305 | 1 038 (−20 %) | 1 071 (+3 %) |
| `langpack/backward/k700/n2000` | 81 825 | 150 | 138 (−8 %) | 139 (+1 %) |
| `langpack/seed/k700/n2000` | 108 919 | 181 | 158 (−12 %) | 161 (+2 %) |
| `langpack/delta-add/k700/n20000/d200` | – | 24 | 11 (−51 %) | 12 (+2 %) |
| `langpack/delta-change/k700/n20000/d200` | – | 288 | 252 (−13 %) | 262 (+4 %) |
| `langpack/delta-delete/k700/n20000/d200` | – | 59 | 53 (−10 %) | 57 (+8 %) |
| `langpack/delta-delete/k700/n20000/d2000` | – | 314 | 281 (−11 %) | 303 (+8 %) |
| `f2p/batch/f20000` | 8 842 | 3 196 | 2 661 (−17 %) | 2 723 (+2 %) |
| `f2p/add/f20000` | 14 512 | 3 544 | 3 009 (−15 %) | 3 081 (+2 %) |
| `f2p/delta-add/f20000/d1000` | 593 | 199 | 189 (−5 %) | 191 (+1 %) |
| `f2p/delta-delete/f20000/d1000` | 21 | 10 | 9 (−3 %) | 10 (+6 %) |
| `pde/open/java40` | – | 17 475 | 12 897 (−26 %) | 13 178 (+2 %) |
| `pde/open/open5` | – | 56 752 | 31 816 (−44 %) | 32 933 (+4 %) |
| `pde/open/open2` | – | 545 | 390 (−28 %) | 397 (+2 %) |
| `pde/edit/open2-edit` | – | 23 | 23 (−2 %) | 24 (+4 %) |
| `pde/load/java40` | – | 2 197 | 2 187 (−0 %) | 2 187 (+0 %) |
| `pde/graph/java40` | – | 933 | 934 (+0 %) | 918 (−2 %) |
| `langpack/engines/k700/n200/e34` | – | 2 180 | 472 (−78 %) | 480 (+2 %) |
| `langpack/index/k700/e100` | – | 5 021 | 213 (−96 %) | 214 (+0 %) |

„vorher“ ist `main`, in den pde-Zellen und in `langpack/engines` und
`langpack/index` der Stand `none`. Prozent: Auswahl gegen vorher,
`low_memory` gegen Auswahl. `sel` und `lowmem` aus `uid-cgu1.tsv`
(b9b3ce9), die übrigen Stände aus `sel-cgu1.tsv`.

## Peak-RSS (MB)

Prozent wie bei den Instruktionen.

| Zelle | vorher | Auswahl | `low_memory` |
|---|---|---|---|
| `langpack/add/k700/n2000` | 42 | 44 (+5 %) | 41 (−7 %) |
| `langpack/add/k700/n20000` | 215 | 186 (−13 %) | 146 (−22 %) |
| `langpack/backward/k700/n2000` | 71 | 73 (+3 %) | 69 (−5 %) |
| `langpack/seed/k700/n2000` | 42 | 44 (+5 %) | 41 (−7 %) |
| `langpack/delta-add/k700/n20000/d200` | 229 | 188 (−18 %) | 149 (−21 %) |
| `langpack/delta-change/k700/n20000/d200` | 216 | 194 (−10 %) | 155 (−20 %) |
| `langpack/delta-delete/k700/n20000/d200` | 216 | 193 (−11 %) | 152 (−21 %) |
| `langpack/delta-delete/k700/n20000/d2000` | 215 | 195 (−9 %) | 155 (−21 %) |
| `f2p/batch/f20000` | 743 | 634 (−15 %) | 481 (−24 %) |
| `f2p/add/f20000` | 744 | 636 (−15 %) | 483 (−24 %) |
| `f2p/delta-add/f20000/d1000` | 745 | 647 (−13 %) | 498 (−23 %) |
| `f2p/delta-delete/f20000/d1000` | 745 | 636 (−15 %) | 484 (−24 %) |
| `pde/open/java40` | 1048 | 997 (−5 %) | 833 (−16 %) |
| `pde/open/open5` | 2171 | 1919 (−12 %) | 1896 (−1 %) |
| `pde/open/open2` | 187 | 191 (+2 %) | 187 (−2 %) |
| `pde/edit/open2-edit` | 189 | 193 (+2 %) | 187 (−3 %) |
| `pde/load/java40` | 229 | 232 (+1 %) | 232 (+0 %) |
| `pde/graph/java40` | 1048 | 998 (−5 %) | 825 (−17 %) |
| `langpack/engines/k700/n200/e34` | 470 | 271 (−42 %) | 262 (−3 %) |
| `langpack/index/k700/e100` | 1175 | 589 (−50 %) | 588 (−0 %) |

## Zeit (User-CPU, ms, Minimum)

Gemessen mit dem Engine-Code von fee8a4f, vor dem Fix.

Drei Runden im Wechsel (`time.tsv`):

| Zelle | vorher | Auswahl | `low_memory` |
|---|---|---|---|
| `f2p/add/f20000` | 1 088,8 | 997,5 (−8 %) | 925,2 (−7 %) |
| `f2p/batch/f20000` | 1 033,4 | 764,9 (−26 %) | 763,4 (−0 %) |
| `langpack/add/k700/n20000` | 329,7 | 257,8 (−22 %) | 259,4 (+1 %) |
| `pde/open/java40` | 5 168,5 | 4 401,0 (−15 %) | 4 738,6 (+8 %) |
| `pde/open/open5` | 7 345,7 | 4 062,4 (−45 %) | 4 278,0 (+5 %) |

Bei zwei Samples nimmt das zweite den Ankerindex aus dem Vorrat des
Prozesses. Die Instruktionen der Zeitläufe von `sel` und `lowmem`
liegen deshalb unter denen aus `sel-cgu1.tsv` (ein Sample), in
`langpack/add/k700/n20000` 976 statt 1 038 Mio.

`low_memory` gegen die Auswahl, sechs Runden (`time.tsv` und
`time2.tsv`), Abstand je Runde in Prozent:

| Zelle | Auswahl | `low_memory` | je Runde |
|---|---|---|---|
| `f2p/add/f20000` | 967,5 | 925,2 | +0, −6, −7, +4, −1, −4 |
| `f2p/batch/f20000` | 764,9 | 763,4 | +3, −4, −0, −1, −0, +5 |
| `langpack/add/k700/n20000` | 257,8 | 259,4 | +2, +0, +1, +9, −9, +16 |
| `pde/open/java40` | 4 358,1 | 4 738,6 | +7, +8, +2, +6, +12, +13 |
| `pde/open/open5` | 3 970,6 | 4 262,3 | +7, +5, +5, +7, +9, +9 |

## Paralleler Seed (Wall-Zeit, ms, Minimum aus drei Samples)

Gemessen mit dem Engine-Code von fee8a4f, vor dem Fix.

| Zelle | 1 Thread | 4 Threads | 8 Threads |
|---|---|---|---|
| `langpack/reseed/k700/n20000` | 300,7 | 115,2 | 99,5 |
| `f2p/reseed/f20000` | 437,9 | 261,7 | 252,2 |

## Konsumenten (`cons.tsv`)

B ist die Dev-Kopie `pilot/crates/seesaw-core` (Archiv 8ae5018) mit
`src/` von b9b3ce9 außer `rules/export.rs`, dazu dry-cleaner (Archiv
aacaf01). md5 je Datei gegen den Zweig verglichen.

| Gruppe | 2.0.2 | `perf/select` |
|---|---|---|
| `core` | 175 bestanden, 9 ignoriert | 208 bestanden, 9 ignoriert |
| `dcrt`, `dcperf`, `fwd1` bis `fwd3`, `gate`, `chaos-generator`, `pipeline-*` | wie rechts | gleiche Zahlen, kein Fehlschlag |
| dry-cleaner vorwärts, 34 Fixtures, Minimum | 369,67 ms | 120,07 ms |

`core` sind die Tests der Engine selbst, der Zweig hat 33 mehr.
`live_nodes=33463` und `rules=4006` sind in beiden gleich.

## Leseprüfung des Index der nicht gehaltenen Links

Ein zweiter Leser (nur Code und git, nichts ausgeführt) hat
`src/engine/links.rs` gegen die Ablage von 2.0.2 (`links_filed.rs` auf
`perf/options`) geprüft.

- **Gleichheit:** dieselbe Liste in Reihenfolge und Vielfachheit für
  bekannte Verbindungen, unbekannte IDs vor und nach dem Aufbau, von
  `record` nachgetragene Matches, später entstandene Verbindungen,
  materialisierte Kopien und doppelt gemeldete IDs. Bedingung: jeder
  Graph behält seine Typtabelle.
- **Fund (behoben in b9b3ce9):** Der Index erkannte seinen Graphen an
  der Nummer der Typtabelle. `types` ist öffentlich. Setzt ein Host es
  per Move in einen anderen Graphen, antwortete der alte Index für den
  neuen: `[]` statt der Matches, die 2.0.2 findet. Der Graph hat jetzt
  eine eigene, private Nummer. Der neue Test
  `the_index_is_about_the_graph_not_its_type_table` scheitert ohne den
  Fix an der Prüfung gegen die Ablage von 2.0.2 (`links.rs:163`, links
  `[]`, rechts `[1]`) und besteht mit ihm.
- **Testlücken (geschlossen in b9b3ce9):** vier weitere Tests. Ein von
  `record` nachgetragener Match wird aus dem Index beantwortet. Eine
  später in Musterrichtung entstandene Verbindung gibt dieselbe
  Antwort. Eine in der materialisierten Kopie weggelassene Verbindung
  tötet genau ihren Match. Ein doppelter Musterlink legt den Match
  doppelt ab.
- **Kosten (nicht gemessen):** Vor dem ersten Aufbau kostet `record`
  eine Abfrage. Danach legt `record` die nicht gehaltenen Links jedes
  neuen Matches ab, bis zum nächsten Graphwechsel. Nach einem Wechsel
  auf eine materialisierte Kopie baut die Engine den Index auch ohne
  Frage des Hosts, wenn eine Retraktion nach weggelassenen Erzeugnissen
  fragt. Im CHANGELOG nachgetragen.
- **`forget_shared`:** kein Fund. Die Doku nennt jetzt Verdrängung und
  Neubau.

## Nicht in dieser Reihe

- Eine Zeitmessung auf ruhigem Rechner. Zeit und paralleler Seed nicht
  auf b9b3ce9 wiederholt.
- Die Tests von zerodds-pde gegen diesen Stand.
- Vermutung des Lesers, nicht beobachtet: Der Test
  `an_equal_rule_set_takes_the_index_that_is_there` (`own.rs`)
  vergleicht Adressen. Verdrängen parallel laufende Tests dazwischen
  32 Regelsätze, kann er zufällig scheitern oder bestehen. In den
  Testläufen beider Gates (je neun Läufe der Rust-Tests: `g1` bis
  `g4`, `c1`, `c2`, `tr`, `r1`, `b2`) kam das nicht vor.

## Skripte

`fin.sh` (Teile des Gates), `sel.sh`, `gate2.sh`, `meas.sh` und
`cons.sh` sind die Skripte dieser Reihe, wie sie unter
`/root/seesaw-sel` auf codepit lagen. `cells20.txt` und `cells13.txt`
sind die Zelllisten der Messung.

