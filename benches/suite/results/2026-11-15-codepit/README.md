# Messreihe 2026-11-15 (vierte), codepit

Rechner und Aufruf wie in `../2026-11-14-codepit-instr/`, Board-Reservierungen
912, 919 und 955. Der Rechner war nicht durchgehend exklusiv, Zeiten sind
deshalb nur dort angegeben, wo zwei Stände im selben Lauf gemessen wurden.
Vergleichsgröße ist die Zahl der Instruktionen der Messstrecke (`instr=`).

Die erste Spalte jeder Zeile nennt den Commit des Engine-Codes. Die Suite
(`benches/`) ist die des jeweiligen Arbeitsstands.

| Commit | Stand |
|---|---|
| 32f0aee | Ende der dritten Messreihe |
| da142fe | Index-Aufbau ohne Pfadkopien, Seed über den Index, Hardware-Zähler in der Suite |
| 62310fb | übersetzte Matches wieder eingetragen (Verhalten von 2.0.2) |
| 17b7e52 | Gang lässt die Knoten aus, die die Suche auslässt |
| 49bd2f4 | Signaturen von 2.0.2, Wurzelpaar nicht in `visited` |
| e9e69cd | Plan-IDs einmal je Schritt, `connect_reporting`, `record`, Gang mit einem Nachschlag je Nachbar |
| 35042e8 | Rücknahme ohne Kopien, Participation-Liste beginnt mit zwei Plätzen |
| c8277a4 | drei Funde des Reviews (kein heißer Pfad) |
| aff0c2c | Erzeuger einer Verbindung aus ihren Enden |

## Dateien

- `one.tsv`: ein Lauf je Standardzelle und Stand.
- `hw.tsv`: drei Läufe mit je drei Samples, Hardware-Zähler (Stand da142fe):
  Instruktionen gleich, Zyklen ±2 %, Fehlzugriffe des letzten Cache ±0,5 %.
- `trace.tsv`, `trace-e9e69cd.txt`: Strukturzähler (`--features perf_trace`).
- `fingerprints-2.0.2-e9e69cd.txt`: dreizehn Zellen der Größen `k50/n2000`
  und `f2000` mit 2.0.2 und mit e9e69cd. Schritte, angewandte Schritte,
  Knotenzahl und Fingerprint sind je Zelle gleich. Darunter vier Zellen
  nur mit e9e69cd.
- `s390x-c8277a4.txt`: Tests und sechs Zellen auf s390x unter qemu (Big
  Endian). 188 Tests bestanden, Fingerprints gleich x86_64.
- `checks-35042e8.txt`: Tests mit `--cfg seesaw_checks` (188 bestanden) und
  `cargo +1.88.0 check --lib --target wasm32-unknown-unknown` (rc=0).
- `consumers.tsv`: Tests der Dev-Kopie `pilot/crates/seesaw-core` (mit den
  dry-cleaner-Fixtures) und der dry-cleaner-Crates, je mit dem Engine-Code
  von 2.0.2 (A) und von e9e69cd (B). `core`: Bibliothek und
  Integrationstests der Dev-Kopie. `dcrt`: die zwei ignorierten
  Roundtrip-Tests. `dcperf`: Vorwärtsläufe der 34 Fixtures, Zeile
  `PERF-SUITE`. `fwd1` bis `fwd3`: `pipeline-forward` in drei Teilen.
  `gate`: `gate_sample` aus `chaos-generator`. In jedem Schritt gleiche
  Zahlen in A und B, kein fehlgeschlagener Test. B hat in `core` elf
  Tests mehr (neue Tests der Engine).
- `hot-*.txt`: `hot.sh`, Anteile je Funktion an der Messstrecke.
- `lines-*.txt`: `lines.sh`, Eigenanteil von `step_with_limit` nach
  eingebetteter Funktion und Quellzeile.
- `miss-*.txt`, `misslines-*.txt`: dasselbe für das Ereignis
  `cache-misses:u`.

## Nicht in dieser Reihe

- Der Differenztest (`difftest/`) lief zuletzt auf e9e69cd: sechs von
  sieben Eigenschaften mit je 1 500 Fällen bestanden, `family_agrees` lief
  dabei in die Zeitgrenze (mit 400 Fällen auf 62310fb bestanden). Auf den
  Ständen danach und mit der Operation `DeleteProductEdge` ist er noch
  nicht gelaufen.
- Rust 1.88 mit `-D warnings`, `--features perf_trace`, `cargo doc`,
  `cargo publish --dry-run` und die Java-Tests liefen zuletzt auf 32f0aee.
- Zeit je Zelle am Endstand ist nicht gemessen.
