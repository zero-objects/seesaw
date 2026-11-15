# Abschlussreihe 2026-11-15, codepit

Engine-Stand `5dce323` (gleich `aff0c2c`, der Commit danach ändert nur
`difftest/`). Rechner und Aufruf wie in `../2026-11-14-codepit-instr/`,
Board-Reservierung 978, exklusiv, RAM-Grenze 14,5 GB. Vergleichsstand
ist 2.0.2 (`f2b0e53`) mit der Suite dieses Stands.

## Dateien

- `final.tsv`: ein Lauf je Zelle mit Hardware-Zählern, Label `b2` =
  2.0.2, `ib` = `5dce323`. Angewandte Schritte, Knotenzahl und
  Fingerprint sind in den sieben gemeinsamen Zellen gleich.
- `final-time.tsv`: Zeit im Wechsel, je Zelle dreimal `b2` mit einem
  Sample und dreimal `ib` mit zwei Samples. Ausgewertet wird das
  Minimum der User-CPU je Stand. Die absoluten Zeiten von 2.0.2 liegen
  9 bis 30 % über denen vom 14.11., die Ursache ist nicht untersucht.
- `one.tsv`: ein Lauf je Standardzelle, Label `ib`.
- `trace.tsv`: Strukturzähler (`--features perf_trace`), Label `ib`.
- `fingerprints-2.0.2-5dce323.txt`: dreizehn Zellen der Größen
  `k50/n2000` und `f2000` mit 2.0.2 und mit `5dce323`. Schritte,
  angewandte Schritte, Knotenzahl und Fingerprint sind je Zelle gleich.
- `s390x-5dce323.txt`: sechs Zellen auf s390x unter qemu und auf
  x86_64, Fingerprints gleich. Tests auf s390x: 188 bestanden,
  0 fehlgeschlagen, 7 ignoriert.
- `hot-5dce323-*.txt`, `miss-5dce323-*.txt`: `hot.sh`, Anteile je
  Funktion an der Messstrecke für `instructions:u` und
  `cache-misses:u`.
- `lines-5dce323-*.txt`, `misslines-5dce323-*.txt`: `lines.sh`,
  Eigenanteil von `record`, `step_with_limit` und `insert_connection`
  nach eingebetteter Funktion und Quellzeile.
- `consumers.tsv`: Tests der Dev-Kopie `pilot/crates/seesaw-core`
  (`8ae5018`, mit den dry-cleaner-Fixtures) und der dry-cleaner-Crates
  (`aacaf01`) mit dem Engine-Code von `5dce323` (B). Die Zeilen A
  (2.0.2) stehen nur für `dcperf`, die übrigen Zahlen von A in
  `../2026-11-15-codepit/consumers.tsv`.
- `fingerprints.tsv`: Rohzeilen zu `fingerprints-2.0.2-5dce323.txt`.
- `checks.json`: Liste der Prüfungen mit Stand und Ergebnis, Quelle der
  Prüftabelle im Bericht.

## Gate auf `5dce323`

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | rc 0 |
| `cargo test` | 188 bestanden, 0 fehlgeschlagen, 7 ignoriert |
| `cargo test --features perf_trace` | 188 bestanden, 0 fehlgeschlagen, 7 ignoriert |
| `RUSTFLAGS="--cfg seesaw_checks" cargo test` | 188 bestanden, 0 fehlgeschlagen, 7 ignoriert |
| `RUSTFLAGS="-D warnings" cargo +1.88.0 test`, `clippy --all-targets` | 188 bestanden, 0 fehlgeschlagen, 7 ignoriert, rc 0 |
| `cargo +1.88.0 check --lib --target wasm32-unknown-unknown` | rc 0 |
| `cargo doc --no-deps` mit `-D warnings` | rc 0 |
| `cargo publish --dry-run --allow-dirty` | rc 0, 88 Dateien, nichts hochgeladen |
| `mvn test` in `java/` | 129 gelaufen, 0 Fehlschläge, 0 Fehler, 3 übersprungen |
| `difftest/`, `cargo test --release` | 8 Tests bestanden |
| `difftest/` mit `DIFFTEST_CASES=1500` (ohne `family_agrees`) | 6 Eigenschaften bestanden |
| `difftest/` `family_agrees` mit `DIFFTEST_CASES=400` | bestanden |

## Nicht in dieser Reihe

- Tests von zerodds-pde gegen diesen Stand.
- Java-Pack von pde als Messzelle.
