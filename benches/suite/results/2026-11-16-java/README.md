# Java-Port der Performance-Arbeit: Messleiter, 2026-11-16 und 2026-11-17, codepit

Zweig `perf/java`. Gemessen ist die Java-Implementierung auf 16 Ständen
(L0 bis Ende) gegen die Rust-Implementierung auf `perf/select`
(fa12974). Gemessen wird nur, ausgewertet mit `java/bench/jreport.py`.
Was die Zahlen bedeuten, steht hier nicht.

## Stände

Java-Stand je Zeile von `java/bench/ladder.txt`. Die Messstrecke
(`JavaSuite`) kommt aus HEAD und läuft gegen das Haupt-JAR des Stands.
Die SHAs gelten nach dem Umformulieren der Commit-Nachrichten.

| Label | SHA | Commit |
|---|---|---|
| `L0` | fa12974 | suite: Gate und Messung der Auswahl (perf/select) |
| `T4` | 8d45be1 | java: Id ueber vier Woerter, Hash einmal |
| `T5a` | 4031e6c | java: partsByOtherType ohne Objekte je Aufruf |
| `T5` | 4ec47f7 | java: Parts, Doku und Test berichtigt |
| `T6a` | 3f4b3d4 | java: resolveValue ohne Liste je Aufruf |
| `T6` | 99c5300 | java: Matcher liest die Verbindung des Kandidaten, ohne Zwischenlisten |
| `T7a` | 1130956 | java: Match-Schluessel ohne Hex-String |
| `T7b` | 3c45369 | java: byElement ohne Boxing |
| `T7` | 99b93a8 | java: Ref-Folge geteilt, applied als Hash-Menge |
| `T8` | c1beffb | java: Ruecknahme ohne Kopien, Plan-IDs je Schritt einmal |
| `T9` | bb7ece3 | java: Grundlage von Option E vollstaendig begruendet |
| `T10` | a33721b | java: Verbindungen nicht in byElement, Nebenindex je Graph |
| `T11` | 02cb0a8 | java: AnchorIndex auf Stand Rust, Gang in einem Durchlauf |
| `T12` | 74a8631 | java: Index je Regelsatz geteilt, Namen je Graph einmal |
| `T13` | 49cf169 | java: Seed auf mehreren Threads |
| `Ende` | 246227e | java: CPU des Prozesses je Zelle, Load Average je Runde der Leiter |

`T4`, `T5`, `T6`, `T7` bis `T13` sind je der letzte Commit des Tasks im
Plan. `T5a`, `T6a`, `T7a` und `T7b` sind Java-eigene Zwischenstände
(kein Rust-Gegenstück) und nur mit den 20 Zellen der Messstrecke
gemessen. `Ende` ist HEAD nach der Fix-Welle der Leseprüfung. Die
Rust-Seite ist `perf/select` ohne Änderung, Binary `sel` ohne Features,
Binary `par` mit `parallel_seed`, beide mit einer Codegen-Einheit
(`CARGO_PROFILE_BENCH_CODEGEN_UNITS=1`).

## Zellen und Parameter

- Java, `ladder.tsv`: 20 Zellen (`CELLS` in `jsuite.sh`) auf allen 16
  Ständen, `--warmup 3 --reps 5`, drei Runden. 960 Zeilen, davon 792
  Messzeilen und 168 Zeilen `nicht_verfuegbar` (56 Paare aus Zelle und
  Stand, je drei Runden).
- Java, `ladder-pde.tsv`: `pde/load/java40`, `pde/graph/java40`,
  `pde/open/java40` auf den zwölf Ständen ohne die Java-eigenen
  Zwischenstände (`L0 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 Ende`),
  `--warmup 1 --reps 2`, drei Runden, ein Prozess je Stand und Zelle,
  `SEESAW_PDE_DUMP` auf `java40.trace`. 108 Zeilen.
- Rust, `rust-ladder.tsv`: dieselben Zellen ohne `reseed` auf `sel`
  (17 Zellen, darunter die drei pde-Zellen), die sechs `reseed`-Zellen
  auf `par`, `--samples 5`, drei Runden. 69 Zeilen.
- JVM: `-Xms8g -Xmx8g -XX:+UseParallelGC`, jeder Lauf ein frischer
  Prozess.
- Reihenfolge je Runde: Java `CELLS` (alle Stände je Zelle im Wechsel),
  Java pde, Rust. Die Runden folgten aufeinander (Runde 1 Java vor
  Runde 1 Rust und so weiter).
- Spalten der Java-TSV: Label, Zelle, Läufe, Thread-CPU min und
  Median, Wall min, Heap nach dem ersten gemessenen Lauf (MB), Schritte,
  angewandte Schritte, Knoten, Fingerprint, `alloc=`, `gc_ms=`, `gc_n=`, `searches=`, `warmup=`,
  `proc_cpu_ms=`. Die Rust-TSV hat die Spalten der Suite (siehe
  `../2026-11-15-auswahl/README.md`).

## Ablauf

Der Treiber `ladder/step.sh` (Kopie vom Messrechner,
`/root/seesaw-java/step.sh`) führt je Aufruf den nächsten Schritt aus,
immer unter 270 Sekunden, und gibt Exit-Code 0 (weiter), 3 (fertig)
oder 1 (Fehler) zurück. Jeder Aufruf war ein Board-Job, darunter keiner
über 5 Minuten. Ein Aufruf schreibt vorher `uptime` nach
`step-uptime.txt`. 381 Aufrufe stehen dort. Die Java-Zellen laufen über
`jsuite.sh ladder` (setzt aus der TSV fort, baut fehlende JARs und die
Messstrecke), pde und Rust im Treiber selbst. Ein Lauf, der zweimal
scheitert, würde in `step-fehler.txt` stehen. Die Datei ist leer.
`ladder/resetup.sh` baut `/root/seesaw-java` nach einer neuen
Reservierung auf (Quellen, pde-Spur, Stände, Treiber, bisherige
Ergebnisse). `step.state` hält die abgeschlossenen Schritte
(`r<n> cells`, `r<n> pde`, `r<n> rust`).

Reservierungen auf codepit, beide exklusiv: 1010 für die ersten 92
Zeilen von `ladder.tsv` (Jobs j6754 bis j6781, Aufforderung zur Freigabe
beendete sie), danach 1013 (ab j6790). Zeitraum der Aufrufe laut
`step-uptime.txt`: 2026-11-16 19:12:54 UTC bis 2026-11-17 15:27:21 UTC.

`rbuild-sel.err`, `rbuild-par.err` und `rust-ladder.err` (Fehlerausgabe
von Bau und Läufen) sind leer.

## Ergebnis

Die vollen Tabellen je Zelle über alle Stände (Thread-CPU min, Wall min,
alloc, `gc_ms`, `proc_cpu_ms`, je Minimum über die Runden, Zahl der
Zeilen je Wert) stehen in `ladder/report-cells.txt` und
`ladder/report-pde.txt`. Beide erzeugt `jreport.py`:

```
cd benches/suite/results/2026-11-16-java/ladder
A=../../2026-11-15-auswahl
python3 ../../../../java/bench/jreport.py --ladder ../../../../java/bench/ladder.txt \
  --ref $A/uid-cgu1.tsv:sel --ref $A/par.tsv:par --rust rust-ladder.tsv \
  --out report-cells.txt ladder.tsv
python3 ../../../../java/bench/jreport.py --ladder ../../../../java/bench/ladder.txt \
  --ref $A/uid-cgu1.tsv:sel --ref $A/par.tsv:par --rust rust-ladder.tsv \
  --out report-pde.txt ladder-pde.tsv
```

Kurzform: L0 gegen Ende, je Zelle Minimum über drei Runden.
Gewinn = Wert bei L0 geteilt durch Wert bei Ende. Faktor =
Java-Wert geteilt durch das Minimum der Rust-Zeilen derselben Zelle.
Spalte `CPU` ist bei Java die Thread-CPU des aufrufenden Threads
(Spalte 4), bei Rust die User-CPU (Spalte 4). In den `reseed`-Zellen
gilt Wall (Spalte 6) auf beiden Seiten (Ruling R25): die Thread-CPU
erfasst dort die Threads der Suche nicht und ist nicht vergleichbar.
Rust nutzt in den `reseed`-Zellen das Binary `par`.

### Zellen der Messstrecke

| Zelle | Spalte | L0 ms | Ende ms | Gewinn | Rust ms | Faktor L0 | Faktor Ende |
|---|---|---|---|---|---|---|---|
| `langpack/add/k700/n2000` | CPU | 377,9 | 57,4 | 6,58 | 19,7 | 19,2 | 2,9 |
| `langpack/seed/k700/n2000` | CPU | 384,2 | 62,3 | 6,17 | 19,6 | 19,6 | 3,2 |
| `langpack/backward/k700/n2000` | CPU | 269,8 | 58,9 | 4,58 | 16,3 | 16,6 | 3,6 |
| `langpack/add/k700/n20000` | CPU | 3959,9 | 645,9 | 6,13 | 246,0 | 16,1 | 2,6 |
| `langpack/delta-add/k700/n20000/d200` | CPU | 29,7 | 7,8 | 3,81 | 0,9 | 33,0 | 8,7 |
| `langpack/delta-change/k700/n20000/d200` | CPU | 843,7 | 186,8 | 4,52 | 67,8 | 12,4 | 2,8 |
| `langpack/delta-delete/k700/n20000/d200` | CPU | 192,1 | 34,2 | 5,62 | 17,7 | 10,9 | 1,9 |
| `langpack/delta-delete/k700/n20000/d2000` | CPU | 1022,9 | 160,4 | 6,38 | 102,0 | 10,0 | 1,6 |
| `f2p/batch/f20000` | CPU | 6819,8 | 2138,0 | 3,19 | 703,8 | 9,7 | 3,0 |
| `f2p/add/f20000` | CPU | 10025,0 | 2301,2 | 4,36 | 814,6 | 12,3 | 2,8 |
| `f2p/delta-add/f20000/d1000` | CPU | 416,5 | 133,2 | 3,13 | 28,6 | 14,6 | 4,7 |
| `f2p/delta-delete/f20000/d1000` | CPU | 19,8 | 7,4 | 2,68 | 4,3 | 4,6 | 1,7 |
| `langpack/engines/k700/n200/e34` | CPU | 1437,2 | 230,7 | 6,23 | 75,7 | 19,0 | 3,0 |
| `langpack/index/k700/e100` | CPU | 803,5 | 146,5 | 5,48 | 55,4 | 14,5 | 2,6 |
| `langpack/reseed/k700/n20000/t1` | Wall | 2251,7 | 351,5 | 6,41 | 306,5 | 7,3 | 1,1 |
| `langpack/reseed/k700/n20000/t4` | Wall | - | 137,5 | - | 113,6 | - | 1,2 |
| `langpack/reseed/k700/n20000/t8` | Wall | - | 106,4 | - | 79,8 | - | 1,3 |
| `f2p/reseed/f20000/t1` | Wall | 3587,5 | 585,0 | 6,13 | 397,5 | 9,0 | 1,5 |
| `f2p/reseed/f20000/t4` | Wall | - | 336,3 | - | 240,0 | - | 1,4 |
| `f2p/reseed/f20000/t8` | Wall | - | 274,3 | - | 225,2 | - | 1,2 |

`reseed` t4 und t8 sind vor T13 `nicht_verfuegbar` (siehe unten), die
Spalte `L0` und der Gewinn sind dort leer.

### pde (java40)

| Zelle | Spalte | L0 ms | Ende ms | Gewinn | Rust ms | Faktor L0 | Faktor Ende |
|---|---|---|---|---|---|---|---|
| `pde/load/java40` | CPU | 349,0 | 368,7 | 0,95 | 228,3 | 1,5 | 1,6 |
| `pde/graph/java40` | CPU | 866,5 | 811,7 | 1,07 | 179,7 | 4,8 | 4,5 |
| `pde/open/java40` | CPU | 46042,9 | 7519,4 | 6,12 | 3937,1 | 11,7 | 1,9 |

Der Faktor in den pde-Zellen ist Java-Thread-CPU geteilt durch
Rust-User-CPU der Zelle auf `sel`.

## Fingerprint

`jreport.py` vergleicht Schritte, angewandte Schritte, Knoten und
Fingerprint jeder Zeile mit der Referenz derselben Zelle: `sel` aus
`../2026-11-15-auswahl/uid-cgu1.tsv`, die `reseed`-Zellen (Java und
Rust) aus `../2026-11-15-auswahl/par.tsv`. Ergebnis (Ausgabe am Ende
von `report-cells.txt` und `report-pde.txt`):

| Datei | Zeilen | gleich | abweichend | ohne Referenz |
|---|---|---|---|---|
| `ladder.tsv` (Java) | 792 | 792 | 0 | 0 |
| `ladder-pde.tsv` (Java) | 108 | 108 | 0 | 0 |
| `rust-ladder.tsv` (Rust, Zellen von `ladder.tsv`) | 60 | 60 | 0 | 0 |
| `rust-ladder.tsv` (Rust, pde) | 9 | 9 | 0 | 0 |

Dazu 168 Zeilen `nicht_verfuegbar` in `ladder.tsv`. Dieselbe Prüfung
von Hand (awk) kam auf die gleichen Zahlen. Der Lauf von `jreport.py`
auf codepit (Job 7293) erzeugt Berichte mit gleicher Prüfsumme wie auf
dem Mac.

## Load Average

`step-uptime.txt` (381 Zeilen, je eine vor jedem Aufruf): Load Average
der letzten Minute von 3,39 bis 41,61, Median 17,74, bei 227 von 381
Zeilen über 16 (Kerne: 16). Je Runde, aus `ladder.tsv.uptime`:
Runde 1 16,39 bis 41,61 (Median 24,36), Runde 2 6,66 bis 35,85
(Median 16,96), Runde 3 3,39 bis 17,87 (Median 7,22). codepit hatte
trotz exklusiver Reservierung Last von außen. Bei der Grundlinie L0
(`l0-faktor.txt`) zeigte `top` um 02:27 keinen Prozess mit CPU-Anteil,
bei Load Average 16,02. Die Herkunft der Last ist nicht geprüft.

## Lesart und Einschränkungen

- Ab Stand T12 (Option I) enthält die gemessene Zeit der Zellen mit
  einer Engine je Lauf keinen Indexbau mehr: der Aufwärmlauf füllt den
  Vorrat des Prozesses, wie bei Rust-Samples in einem Prozess. Die
  Spalte `alloc` enthält die behaltenen Indizes.
- `proc_cpu_ms` (CPU des ganzen Prozesses, alle Threads, GC und JIT)
  hat 10-ms-Stufen. Ursache vermutlich die Taktauflösung, nicht geprüft.
- `ladder.tsv.uptime` hat Zeilen mit `unterbrochen`. Jeder Board-Job
  unterbricht eine Runde, `jsuite.sh ladder` schreibt dann statt `nach`
  die Zeile `unterbrochen`. Zeilen `vor` und `nach` stehen deshalb je
  Job, nicht je Runde. Eine Zeile `nach` gibt es nur dort, wo ein Aufruf
  die Runde beendet (drei, je Runde eine). Der Load Average einer Runde
  ist also die Folge der Werte über ihre Jobs.
- `reseed` t4 und t8 sind vor T13 `nicht_verfuegbar`: `seedWithThreads`
  fehlt in diesen Ständen. Die Zeile hat die Form
  `<label>\t<zelle>\tnicht_verfuegbar\t<grund>`. `reseed` t1 läuft auf
  allen Ständen als `seed`.
- Java-Zeitspalte und Rust-Zeitspalte sind nicht dieselbe Messgröße:
  Thread-CPU des aufrufenden Threads gegen User-CPU des Prozesses (bei
  Rust ohne `parallel_seed` ein Thread). Der Faktor setzt beide ins
  Verhältnis, ohne sie anzugleichen.
- Ruling R32: Die Schätzung der Laufdauer in `jsuite.sh ladder` wurde
  nach Block 1 und 2 von "längste Dauer der Zelle" auf "Dauer desselben
  Paars (Label, Zelle), sonst zuletzt gemessene Dauer der Zelle"
  umgestellt (und im Treiber `step.sh` für pde und Rust). Messwerte,
  `--warmup`, `--reps` und Stände blieben gleich. Vorher lief bei f2p je
  Job nur ein Lauf.
- Pause in Runde 2: Die Java-Zellen standen bei 306 von 320 Zeilen.
  Laut `step-uptime.txt` liegt zwischen den Aufrufen von 2026-11-17
  03:09:46 UTC und 10:23:37 UTC kein weiterer (die Steuerung war
  unterbrochen, die Reservierung 1013 blieb aktiv). Die Runden liefen
  nicht ohne Unterbrechung. Die Zeit der Messläufe selbst ist davon
  nicht betroffen, die Last von außen kann sich in der Pause geändert
  haben (Runde 2 und 3 haben niedrigere Werte als Runde 1, siehe oben).
- Die Minima über drei Runden stammen aus Läufen bei verschiedener Last.
  Ob die Last die Minima beeinflusst hat, ist nicht geprüft.

## Dateien

- `ladder/ladder.tsv`, `ladder.tsv.dauer`, `ladder.tsv.uptime`: Java,
  20 Zellen. `.dauer` je Lauf `<label> <zelle> <sekunden>`.
- `ladder/ladder-pde.tsv`, `ladder-pde.tsv.dauer`: Java, pde java40.
- `ladder/rust-ladder.tsv`, `rust-ladder.dauer`: Rust, `sel` und `par`.
- `ladder/step.sh`, `ladder/resetup.sh`: Treiber und Neuaufbau (Kopien).
- `ladder/step-uptime.txt`, `ladder/step.state`, `ladder/step-fehler.txt`,
  `ladder/rbuild-*.err`, `ladder/rust-ladder.err`: Protokoll des Treibers.
- `ladder/report-cells.txt`, `ladder/report-pde.txt`: volle Tabellen,
  von `jreport.py` erzeugt.
- `l0-faktor.txt`, `base.tsv`, `rust-sel.tsv`: Grundlinie L0 mit
  früheren Zahlen (Reservierung 999, ein Lauf von L0 gegen Rust `sel`).
  Die Leiter misst L0 neu, die Zahlen weichen davon ab.
