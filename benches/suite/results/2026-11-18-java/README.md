# Java-Port: Messung der Behebungen und der Seed-Form, 2026-11-18, codepit

Zweig `perf/java`. Gemessen sind drei Java-Stände (`S0`, `S1a`, `S1b`)
gegen die Rust-Implementierung auf `perf/select` (fa12974). Gemessen wird
nur, ausgewertet mit `java/bench/jreport.py`. Die Entscheidung zur Seed-Form
steht im eigenen Abschnitt.

## Stände

Java-Stand je Zeile von `ladder/ladder.txt`. Die Messstrecke (`JavaSuite`)
kommt aus HEAD und läuft gegen das JAR der Hauptklassen des Stands.

| Label | SHA | Commit |
|---|---|---|
| `S0` | 02c3e39 | bench: README der Messleiter, Aufrufpfad und Wortwahl |
| `S1a` | 540f03d | java: linkRemoved mit einer Verbindungs-Id wie 2.0.2 |
| `S1b` | 0a3f81a | java: Gang des Seeds betritt keine kleineren Knoten (Seed-Teil von Rust 17b7e52) |

- `S0` ist der Stand der letzten Messleiter.
- `S1a` enthält die Matcher-Behebung (Wertgleichheit, `SAME_VALUE`) und vier
  Fälle, in denen Java wieder wie 2.0.2 verfährt. Der Seed über den Index
  hat die Java-Form (`firstOnly`).
- `S1b` enthält zusätzlich die Rust-Form des Seeds (94f8363 und 0a3f81a).
  Beide Commits sind zurückgenommen (siehe unten), `S1b` ist nur der
  gemessene Stand.

Rust: `perf/select` ohne Änderung, Binary `sel` ohne Features, Binary `par`
mit `parallel_seed`, beide mit `CARGO_PROFILE_BENCH_CODEGEN_UNITS=1`.

## Zellen und Parameter

- Java, `ladder/cells.tsv`: 9 Zellen (`langpack/seed/k700/n2000`,
  `langpack/reseed/k700/n20000/t1|t4|t8`, `f2p/reseed/f20000/t1|t4|t8`,
  `langpack/delta-delete/k700/n20000/d200|d2000`) auf `S0`, `S1a`, `S1b`,
  `--warmup 3 --reps 5`, drei Runden, 81 Zeilen.
- Java, `ladder/pde.tsv`: `pde/load|graph|open` auf den Spuren `java40`
  (alle drei Stände), `open2` und `open5` (nur `S1a`, `S1b`),
  `--warmup 1 --reps 2`, drei Runden, 63 Zeilen. Ein Prozess je Stand und
  Zelle, `SEESAW_PDE_DUMP` auf den vier Spuren in `/root/seesaw-java/pde-dump`.
- Rust, `ladder/rust.tsv`: dieselben Zellen, die sechs `reseed`-Zellen auf
  `par`, die übrigen 12 (darunter alle neun pde-Zellen) auf `sel`,
  `--samples 5`, drei Runden, 54 Zeilen.
- JVM: `-Xms8g -Xmx8g -XX:+UseParallelGC`, jeder Lauf ein frischer Prozess.
- Reihenfolge je Runde: Java-Zellen (alle Stände je Zelle im Wechsel), Java
  pde, Rust.
- Spalten der Java-TSV wie in `../2026-11-16-java/README.md`.

## Ablauf

Treiber `ladder/step.sh` (Kopie von `/root/seesaw-java/step.sh`), Neuaufbau
`ladder/resetup.sh`, Leiter `ladder/ladder.txt`. Jeder Aufruf war ein
Board-Job unter 5 Minuten. In den Kopien heißen die Ergebnisdateien noch
`t4-*`, in diesem Verzeichnis fehlt das Präfix (`t4-cells.tsv` ist
`cells.tsv`, `t4-step.state` ist `step.state`).

- Reservierung 1042 auf codepit, exklusiv. Jobs 7515 bis 7579.
- Zeitraum laut `ladder/uptime.txt`: 2026-11-18 15:38:06 UTC bis 17:11:19 UTC
  (65 Zeilen, je eine vor jedem Aufruf).
- Load Average je Minute: kleinster Wert 3,9, Median 7,0, Spitze 22,3
  (Kerne: 16). Je Zelle der Java-Runden steht er in `ladder/cells.tsv.uptime`.
- `ladder/fehler.txt`, `rbuild-sel.err`, `rbuild-par.err` und `rust.err`
  sind leer.

## Ergebnis

Die vollen Tabellen je Zelle (Thread-CPU min, Wall min, alloc, `gc_ms`,
`proc_cpu_ms`, je Minimum über die Runden) stehen in
`ladder/report-cells.txt` und `ladder/report-pde.txt`. Erzeugt mit:

```
cd benches/suite/results/2026-11-18-java/ladder
A=../../2026-11-15-auswahl
J=../../../../../java/bench/jreport.py
for k in cells pde; do
  python3 $J --ladder ladder.txt --ref $A/uid-cgu1.tsv:sel --ref $A/par.tsv:par \
    --ref rust.tsv --rust rust.tsv --alle-staende --out report-$k.txt $k.tsv
done
```

`--alle-staende` ist für diese Messung zu `jreport.py` hinzugekommen. Die
Kurzform des Ladder-Modus zeigt sonst nur den ersten und den letzten Stand.
Ohne die Option bleibt die Ausgabe unverändert.

Minimum über drei Runden, ms. Spalte `CPU` ist bei Java die Thread-CPU des
aufrufenden Threads, bei Rust die User-CPU. In den `reseed`-Zellen gilt Wall
auf beiden Seiten (Ruling R25), die Thread-CPU erfasst dort die Threads der
Suche nicht.

### Zellen der Messstrecke

| Zelle | Spalte | S0 | S1a | S1b | Rust |
|---|---|---|---|---|---|
| `langpack/seed/k700/n2000` | CPU | 57,8 | 57,5 | 58,7 | 19,2 |
| `langpack/reseed/k700/n20000/t1` | Wall | 339,4 | 339,9 | 518,4 | 289,6 |
| `langpack/reseed/k700/n20000/t4` | Wall | 139,3 | 136,2 | 223,6 | 108,8 |
| `langpack/reseed/k700/n20000/t8` | Wall | 108,9 | 107,8 | 172,3 | 81,4 |
| `f2p/reseed/f20000/t1` | Wall | 585,8 | 576,7 | 577,2 | 396,1 |
| `f2p/reseed/f20000/t4` | Wall | 332,0 | 333,3 | 330,1 | 248,0 |
| `f2p/reseed/f20000/t8` | Wall | 273,8 | 278,2 | 272,0 | 222,0 |
| `langpack/delta-delete/k700/n20000/d200` | CPU | 31,7 | 32,4 | 33,5 | 18,2 |
| `langpack/delta-delete/k700/n20000/d2000` | CPU | 148,6 | 154,9 | 155,9 | 100,8 |

### pde

| Zelle | Spalte | S0 | S1a | S1b | Rust |
|---|---|---|---|---|---|
| `pde/load/java40` | CPU | 357,7 | 352,1 | 347,4 | 225,8 |
| `pde/graph/java40` | CPU | 811,7 | 816,1 | 817,8 | 182,2 |
| `pde/open/java40` | CPU | 7554,0 | 7430,4 | 7580,3 | 3860,8 |
| `pde/load/open2` | CPU | - | 370,0 | 362,8 | 226,0 |
| `pde/graph/open2` | CPU | - | 39,3 | 39,3 | 3,2 |
| `pde/open/open2` | CPU | - | 137,4 | 137,8 | 55,5 |
| `pde/load/open5` | CPU | - | 364,8 | 360,2 | 227,3 |
| `pde/graph/open5` | CPU | - | 111,0 | 107,1 | 17,4 |
| `pde/open/open5` | CPU | - | 6292,8 | 6456,9 | 3755,3 |

`-` heißt nicht gemessen (siehe unten, `open2` und `open5` an `S0`).

## Fingerprint

`jreport.py` vergleicht Schritte, angewandte Schritte, Knoten und
Fingerprint jeder Zeile mit der Referenz derselben Zelle. Referenzen:
`sel` aus `../2026-11-15-auswahl/uid-cgu1.tsv`, `par` aus
`../2026-11-15-auswahl/par.tsv` und `ladder/rust.tsv` selbst (Rust derselben
Messung). Die Referenzen widersprechen einander für keine Zelle (sonst
bricht `jreport.py` ab). Ergebnis (Ende von `report-cells.txt` und
`report-pde.txt`):

| Datei | Zeilen | gleich | abweichend | ohne Referenz |
|---|---|---|---|---|
| `cells.tsv` (Java) | 81 | 81 | 0 | 0 |
| `pde.tsv` (Java) | 63 | 63 | 0 | 0 |
| `rust.tsv` (Rust, Zellen von `cells.tsv`) | 27 | 27 | 0 | 0 |
| `rust.tsv` (Rust, pde) | 27 | 27 | 0 | 0 |

Damit sind 144 Java-Zeilen gleich der Rust-Zeile derselben Messung, keine
weicht ab. Allein gegen die Dateien von `../2026-11-15-auswahl` (ohne
`rust.tsv` als Referenz) haben 120 Java-Zeilen und 42 Rust-Zeilen eine
Referenz und sind gleich. Für die übrigen 24 Java- und 12 Rust-Zeilen
(`pde/load` und `pde/graph` auf `open2` und `open5`) hat die Auswahl keine
Zelle, `jreport.py` meldet sie dort als ohne Referenz.

## Entscheidung zur Seed-Form

Sandra hat am 2026-11-18 entschieden: Java behält die Java-Form des Seeds
über den Index (`firstOnly`). Die Rust-Form (94f8363, 0a3f81a) war
übernommen und ist mit `git revert` zurückgenommen (5ece54c, 29c7bf8). Der
Java-Code ist danach gleich dem Stand 540f03d (`git diff 540f03d HEAD --
java/src` leer).

Messwerte, `S1a` (Java-Form) gegen `S1b` (Rust-Form), Wall min über drei
Runden, ms, je Runde in Klammern:

| Zelle | S1a | S1b | Verhältnis S1b zu S1a | Rust |
|---|---|---|---|---|
| `langpack/reseed/k700/n20000/t1` | 339,9 (339,9 / 352,9 / 348,6) | 518,4 (518,4 / 520,0 / 523,8) | 1,53 | 289,6 |
| `langpack/reseed/k700/n20000/t4` | 136,2 (143,3 / 141,1 / 136,2) | 223,6 (225,2 / 223,6 / 275,2) | 1,64 | 108,8 |
| `langpack/reseed/k700/n20000/t8` | 107,8 (107,8 / 112,1 / 112,4) | 172,3 (172,3 / 188,9 / 180,9) | 1,60 | 81,4 |

- `searches` in `langpack/reseed/k700/n20000/t1`: 43 592 bei `S0` und `S1a`,
  47 902 bei `S1b`. In `langpack/seed/k700/n2000`: 4952 bei `S0` und `S1a`,
  4953 bei `S1b`.
- `f2p/reseed/f20000` t1, t4, t8: `S1b` gegen `S1a` 577,2 gegen 576,7
  (+0,1 Prozent), 330,1 gegen 333,3 (-1,0 Prozent), 272,0 gegen 278,2
  (-2,2 Prozent). Die Streuung der Runden ist nicht ausgewertet.
- `pde`-Zellen, `S1b` gegen `S1a`, Thread-CPU: java40 -1,3 bis +2,0 Prozent,
  open2 -1,9 bis +0,3 Prozent, open5 -3,5 bis +2,6 Prozent.
- Fingerprints von `S1a` und `S1b` sind in allen Zellen gleich.
- Gemessen ist die Wirkung auf die Wall-Zeit und die Zahl der Suchen. Die
  Ursache der Mehrzeit bei `langpack/reseed` ist nicht untersucht.

## Zunahme bei `delta-delete` von `S0` zu `S1a`

| Zelle | S0 | S1a | S1b |
|---|---|---|---|
| `langpack/delta-delete/k700/n20000/d200` | 31,7 | 32,4 | 33,5 |
| `langpack/delta-delete/k700/n20000/d2000` | 148,6 | 154,9 | 155,9 |

Gemessen: `S1a` ist bei d200 um 0,7 ms, bei d2000 um 6,3 ms langsamer als
`S0` (Thread-CPU min, drei Runden). Nicht getrennt gemessen, welche der
Behebungen zwischen 02c3e39 und 540f03d sie verursacht. Zwischen `S1a` und
`S1b` sind es 1,1 ms bei d200 und 1,0 ms bei d2000. Die Streuung der Runden
ist nicht ausgewertet.

## `open2` und `open5` an `S0`

Die Spuren `open2` und `open5` laufen an `S0` nicht durch: der Replay
weicht an einem Matcher-Fehler vom Protokoll ab (Behebung `SAME_VALUE`, siehe
CHANGELOG, "Java: matches missed behind a value equality"). `open2` an
Zeile 27 636, `open5` an Zeile 76 200 (Angaben aus dem CHANGELOG, in dieser
Messung nicht erneut erhoben, der Brief der Messung nennt den Matcher-Fehler
als Grund). Der Treiber lässt `open2` und `open5` an `S0`
deshalb aus (`PDE_PAIRS` in `ladder/step.sh`). An `S1a` und `S1b` laufen sie
durch, mit Fingerprint gleich Rust.

## Gate

`jgate.sh` auf dem Stand nach den Reverts (Reservierung 1049, nicht
exklusiv, alle Teile einzeln, Jobs 7580 bis 7599): `mvn` in drei Stellungen
je 225/0/0/5, `fp` 31/31 gleich, `pde` java40, open5, open2, open2-edit
bestanden, `pde-voll:open2-edit` bestanden, `diff` 13 Stücke und die
Gegenprobe bestanden. Einzelheiten stehen im Bericht der Aufgabe.

## Lesart und Einschränkungen

- Die Minima über drei Runden stammen aus Läufen bei verschiedener Last
  (3,9 bis 22,3). Ob die Last die Minima beeinflusst hat, ist nicht geprüft.
- Java-Zeitspalte und Rust-Zeitspalte sind nicht dieselbe Messgröße (siehe
  oben).
- Die `reseed`-Zellen laufen mit einer Aufwärmphase von drei Läufen, der
  Index-Vorrat des Prozesses ist danach gefüllt (seit T12).
- `proc_cpu_ms` hat 10-ms-Stufen.
- Die Reservierung 1042 war exklusiv. Last von außen war trotzdem vorhanden
  (siehe Load Average), ihre Herkunft ist nicht geprüft.

## Dateien

- `ladder/cells.tsv`, `cells.tsv.dauer`, `cells.tsv.uptime`: Java, 9 Zellen.
- `ladder/pde.tsv`, `pde.tsv.dauer`: Java, pde.
- `ladder/rust.tsv`, `rust.dauer`, `rust.err`: Rust, `sel` und `par`.
- `ladder/ladder.txt`, `step.sh`, `resetup.sh`: Leiter, Treiber, Neuaufbau.
- `ladder/step.state`, `uptime.txt`, `fehler.txt`, `rbuild-*.err`: Protokoll
  des Treibers.
- `ladder/report-cells.txt`, `report-pde.txt`: volle Tabellen von
  `jreport.py`.
