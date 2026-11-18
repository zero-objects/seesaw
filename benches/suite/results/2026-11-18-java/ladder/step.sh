#!/bin/sh
# step.sh (codepit): naechster Schritt der Messung Task 4, ein Aufruf je Board-Job unter 270 s.
# Exit 0: weiter aufrufen. Exit 3: alles fertig. Exit 1: Fehler, Zeile FEHLER im Log.
# Reihenfolge: Messstrecke bauen, JARs S0 S1a S1b, Rust sel und par, dann je Runde r = 1..3:
# Java Messstrecke, Java pde, Rust. Ergebnisse in $RES, Dateien t4-*.
# Probe: PROBE=1 nimmt je Art eine Zelle, eine Runde, RES=/root/seesaw-java/results-probe.
set -u
R=/root/seesaw-java; B=$R/src/java/bench
RES=${RES:-$R/results}
export SEESAW_PDE_DUMP=$R/pde-dump
export LADDER=$R/ladder-t4.txt
LIMIT=270; start=$(date +%s)
STATE=$RES/t4-step.state; FAIL=$RES/t4-fehler.txt
LABELS="S0 S1a S1b"
RUNDEN=3
J_CELLS="langpack/seed/k700/n2000
langpack/reseed/k700/n20000/t1 langpack/reseed/k700/n20000/t4 langpack/reseed/k700/n20000/t8
f2p/reseed/f20000/t1 f2p/reseed/f20000/t4 f2p/reseed/f20000/t8
langpack/delta-delete/k700/n20000/d200 langpack/delta-delete/k700/n20000/d2000"
PDE_PAIRS=""
for t in java40 open2 open5; do for p in load graph open; do for l in $LABELS; do
  # open2 und open5 laufen an S0 wegen des Matcher-Fehlers nicht durch
  [ "$l" = S0 ] && [ "$t" != java40 ] && continue
  PDE_PAIRS="$PDE_PAIRS $l:pde/$p/$t"
done; done; done
SEL_CELLS="langpack/seed/k700/n2000 langpack/delta-delete/k700/n20000/d200 langpack/delta-delete/k700/n20000/d2000
pde/load/java40 pde/graph/java40 pde/open/java40 pde/load/open2 pde/graph/open2 pde/open/open2
pde/load/open5 pde/graph/open5 pde/open/open5"
PAR_CELLS="langpack/reseed/k700/n20000/t1 langpack/reseed/k700/n20000/t4 langpack/reseed/k700/n20000/t8
f2p/reseed/f20000/t1 f2p/reseed/f20000/t4 f2p/reseed/f20000/t8"
if [ "${PROBE:-0}" = 1 ]; then
  RES=$R/results-probe; STATE=$RES/t4-step.state; FAIL=$RES/t4-fehler.txt; RUNDEN=1
  J_CELLS="langpack/seed/k700/n2000"
  PDE_PAIRS="S0:pde/load/java40 S1b:pde/load/java40"
  SEL_CELLS="langpack/seed/k700/n2000 pde/load/java40"
  PAR_CELLS="langpack/reseed/k700/n20000/t1"
fi
mkdir -p $RES; touch $STATE $FAIL
echo "$(date -u +%FT%TZ) $(uptime)" >> $RES/t4-uptime.txt
left() { echo $(( LIMIT - ($(date +%s) - start) )); }

# 0a. Messstrecke bauen
if ! grep -q '^bench$' $STATE; then
  out=$(cd $B && sh jsuite.sh bench 2>&1); echo "$out"
  echo "$out" | grep -q "^bench gebaut" || { echo "FEHLER bench"; exit 1; }
  echo bench >> $STATE; exit 0
fi
# 0b. JARs, einer je Aufruf
for l in $LABELS; do
  if [ ! -f $R/jars/$l.jar ] || ! cmp -s $R/jars/$l.commit $R/stands/$l/COMMIT; then
    c=$(awk -v l=$l '$1==l {print $2}' $LADDER)
    (cd $B && sh jsuite.sh build $l $c) || { echo "FEHLER build $l"; exit 1; }
    exit 0
  fi
done
# 0c. Rust-Binaries sel (ohne Features) und par (parallel_seed)
for spec in sel:none par:parallel_seed; do
  l=${spec%%:*}; f=${spec#*:}; ff=""; [ "$f" != none ] && ff="--features $f"
  [ -x $R/bins/$l ] && continue
  mkdir -p $R/bins
  exe=$(cd $R/src && CARGO_PROFILE_BENCH_CODEGEN_UNITS=1 CARGO_TARGET_DIR=$R/target-rust timeout 250 cargo bench --offline -q --bench suite $ff --no-run --message-format=json 2>$RES/t4-rbuild-$l.err | grep -o '"executable":"[^"]*/suite-[^"]*"' | tail -1 | sed 's/^"executable":"//; s/"$//')
  if [ -n "$exe" ] && [ -x "$exe" ]; then cp "$exe" $R/bins/$l; echo "gebaut $l"
  elif grep -q "^error" $RES/t4-rbuild-$l.err; then echo "FEHLER bau $l"; tail -5 $RES/t4-rbuild-$l.err; exit 1
  else echo "bau $l nicht fertig, erneut"; fi
  exit 0
done

count() { if [ -f "$1" ]; then awk -F'\t' -v l="$2" -v c="$3" '$1==l && $2==c' "$1" | wc -l; else echo 0; fi; }
longest() { if [ -f "$1" ]; then awk -F'\t' -v l="$2" -v c="$3" '$2==c {last=$3; if ($1==l && $3>m) m=$3} END {print (m>0?m:last)+0}' "$1"; else echo 0; fi; }
fails() { grep -c "^$1 r=$2 $3 $4\$" $FAIL; }
done_mark() { grep -q "^$1\$" $STATE; }
fits() { d=$1; first=$2; if [ "$d" -eq 0 ]; then [ "$first" = 1 ]; else [ $(( d * 5 / 4 )) -lt "$(left)" ]; fi; }

r=1
while [ $r -le $RUNDEN ]; do
  # 1. Java Messstrecke, Runde r (jsuite.sh ladder, Zellen als Argumente)
  if ! done_mark "r$r cells"; then
    out=$(cd $B && ROUNDS=$r LIMIT=$(left) sh jsuite.sh ladder --tsv $RES/t4-cells.tsv $J_CELLS 2>&1 | tail -3)
    echo "$out"
    if echo "$out" | grep -q "ladder fertig"; then echo "r$r cells" >> $STATE
    elif ! echo "$out" | grep -q "ladder offen"; then echo "FEHLER cells r=$r"; exit 1; fi
    echo "schritt r=$r cells"; exit 0
  fi
  # 2. Java pde, Runde r, ein Prozess je Stand und Zelle (Warmup 1, Reps 2)
  if ! done_mark "r$r pde"; then
    ran=0; open=0
    for pair in $PDE_PAIRS; do
      l=${pair%%:*}; c=${pair#*:}
      [ "$(count $RES/t4-pde.tsv $l $c)" -ge $r ] && continue
      [ "$(fails pde $r $l $c)" -ge 2 ] && continue
      first=$([ $ran = 0 ] && echo 1 || echo 0)
      if fits "$(longest $RES/t4-pde.tsv.dauer $l $c)" $first; then
        before=$(count $RES/t4-pde.tsv $l $c)
        (cd $B && LIMIT=$(left) sh jsuite.sh run --warmup 1 --reps 2 $RES/t4-pde.tsv $l $c 2>&1 | tail -2)
        [ "$(count $RES/t4-pde.tsv $l $c)" -gt "$before" ] || echo "pde r=$r $l $c" >> $FAIL
        ran=$((ran+1))
      else open=$((open+1)); fi
    done
    [ $open = 0 ] && [ $ran = 0 ] && echo "r$r pde" >> $STATE
    echo "schritt r=$r pde gelaufen=$ran offen=$open"; exit 0
  fi
  # 3. Rust, Runde r, ein Prozess je Zelle mit 5 Samples
  if ! done_mark "r$r rust"; then
    ran=0; open=0
    for spec in $(for c in $SEL_CELLS; do echo "sel:$c"; done; for c in $PAR_CELLS; do echo "par:$c"; done); do
      l=${spec%%:*}; c=${spec#*:}
      [ "$(count $RES/t4-rust.tsv $l $c)" -ge $r ] && continue
      [ "$(fails rust $r $l $c)" -ge 2 ] && continue
      first=$([ $ran = 0 ] && echo 1 || echo 0)
      if fits "$(longest $RES/t4-rust.dauer $l $c)" $first; then
        t0=$(date +%s); before=$(count $RES/t4-rust.tsv $l $c)
        timeout $(left) $R/bins/$l run $c --samples 5 --label $l --tsv $RES/t4-rust.tsv >/dev/null 2>>$RES/t4-rust.err
        if [ "$(count $RES/t4-rust.tsv $l $c)" -gt "$before" ]; then printf '%s\t%s\t%s\n' $l $c $(( $(date +%s) - t0 )) >> $RES/t4-rust.dauer
        else echo "rust r=$r $l $c" >> $FAIL; fi
        ran=$((ran+1))
      else open=$((open+1)); fi
    done
    [ $open = 0 ] && [ $ran = 0 ] && echo "r$r rust" >> $STATE
    echo "schritt r=$r rust gelaufen=$ran offen=$open"; exit 0
  fi
  r=$((r+1))
done
echo "alles fertig"; exit 3
