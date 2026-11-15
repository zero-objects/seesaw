# Zellen `pde`, 2026-11-15, codepit

Wiedergabe von Protokollen eines pde-Laufs (`benches/suite/pde.rs`).
Engine-Stand `5dce323` ohne Features (Branch `perf/pde-cell`), Board-
Reservierung 980, Rechner mit anderen Läufen geteilt: Zeiten sind
Einzelwerte, Vergleichsgröße ist die Zahl der Instruktionen.

Die Protokolle liegen nicht im Repository (`SEESAW_PDE_DUMP`). Ihre
Entstehung steht in der README des Protokollverzeichnisses. Aufgezeichnet
ist das Öffnen von Testdateien von commonmark-java (Tag
`commonmark-parent-0.32.0`) in einem Abzug des pde-Arbeitsbaums vom
15.11.2026 (HEAD `18fd90eb` mit 422 offenen Änderungen): 143
Regeldateien, 10 284 gerichtete Regeln, 83 Engines, darunter `lang-java`
mit 2 806 und `uml` mit 1 372 gerichteten Regeln.

## Protokolle

| Name | Inhalt | Aufrufe des Hosts an Graph und Store | `step` (T / F / N) |
|---|---|---|---|
| `java40` | 40 Dateien, bis vor die Welle von `test-unit` | 169 757 `add_baseline`, 172 855 `connect`, 140 782 Werte | 96 483 / 126 373 / 17 |
| `open5` | 5 Dateien, vollständig | 24 185 / 24 896 / 19 331 | 9 197 / 1 338 069 / 21 |
| `open2` | 2 Dateien, vollständig | 9 193 / 9 539 / 7 280 | 1 923 / 3 343 / 21 |
| `open2-edit` | 2 Dateien, danach drei Speichervorgänge | 9 275 / 9 623 / 7 378 | 1 978 / 3 351 / 101 |

T: `Some(true)`, F: `Some(false)`, N: `None`.

## Gleichheit mit dem pde-Lauf

Die Wiedergabe prüft jeden Aufruf gegen das Protokoll: die ID aus
`add_baseline` und `connect`, das Ergebnis jedes `step`, nach jedem
Aufruf an einer Engine die Länge ihrer Kaskade und die Knotenzahl des
Graphen. Alle Zellen laufen ohne Abweichung durch. Summen gegen die
Ausgabe von pde selbst:

| Protokoll | Größe | pde | Zelle |
|---|---|---|---|
| `open2` | Knoten gesamt | 16 016 | 16 016 |
| `open2` | Schritte `lang-java` / `test-unit` / `uml` | 2 739 / 1 176 / 1 349 | 2 739 / 1 176 / 1 349 |
| `open5` | Knoten gesamt | 58 752 | 58 752 |
| `open5` | Schritte `lang-java` / `test-unit` / `uml` | 13 435 / 1 327 958 / 5 871 | 13 435 / 1 327 958 / 5 871 |
| `open2-edit` | Knoten gesamt | 16 261 | 16 261 |

Für `java40` gibt es keine Ausgabe von pde (der Lauf endet nicht in
270 s), die Prüfung je Aufruf gilt auch dort.

## Messwerte (`pde.tsv`)

| Zelle | Instruktionen Mio. | User-CPU ms | Instr. je Zyklus | Peak-RSS MB |
|---|---|---|---|---|
| `pde/load/java40` | 2 211 | 264 | 2,00 | 229 |
| `pde/graph/java40` | 936 | 230 | 1,02 | 1 048 |
| `pde/open/java40` | 17 089 | 5 594 | 0,74 | 1 048 |
| `pde/open/open5` | 55 931 | 7 751 | 1,76 | 2 172 |
| `pde/open/open2` | 534 | 89 | 1,46 | 187 |
| `pde/edit/open2-edit` | 23,3 | 3,5 | 1,38 | 190 |

`pde/load` ist für alle Protokolle gleich (2 211 Mio.): Die Regelbasis
hängt an der Registry, nicht am Projekt.

## Profile (`hot-*.txt`, `hot.sh`, `instructions:u`)

`pde/open/java40` (13 684 Samples), Eigenanteile: `Parts::of_type`
29,7 %, `AnchorIndex::walk` 16,8 %, Closure in `matcher::enumerate`
11,3 %, `Engine::record` 5,3 %. Inklusiv: `expand_at` 82,9 %,
`AnchorIndex::candidates` 47,9 %, `find_matches_planned` 25,0 %,
`apply_creation_at` 10,7 %, `record` 9,4 %.

`pde/open/open5` (33 116 Samples), Eigenanteile: `Engine::record`
29,2 %, `Graph::connection_id_typed` 10,8 %, Sortieren der gefundenen
Matches (`quicksort`) 10,7 %. Inklusiv: `record` 70,9 %, Sortieren
16,8 %, `connection_id_typed` 15,1 %, `BTreeMap::insert` 10,4 %.

`pde/load/java40` (2 099 Samples), inklusiv: `AnchorIndex::build`
37,2 %, `RuleFile::from_json` 22,9 %, `rules::load_file` 20,6 %.

`pde/graph/java40` (602 Samples), inklusiv: `Graph::connect` 45,4 %,
`Graph::add_baseline` 28,2 %, `blake3::Hasher::update` 15,8 %.
