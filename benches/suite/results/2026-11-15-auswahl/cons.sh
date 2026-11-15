#!/bin/sh
# cons.sh <A|B> <schritt>: ein Testschritt der Konsumenten gegen einen Engine-Stand, Ergebniszeilen nach results/cons.tsv.
set -u
v=$1; step=$2
W=/root/seesaw-sel/cons/$v; R=/root/seesaw-sel/results; L=/tmp/cons-$v-$step.log
sum() { awk '/^test result/ {p+=$4; f+=$6; i+=$8; g++} END {printf "groups=%d passed=%d failed=%d ignored=%d", g, p, f, i}' "$L"; }
case $step in
  core)   cd $W/seesaw_rev2/pilot && timeout 290 cargo test --release --no-fail-fast -p seesaw-core --lib --tests >$L 2>&1; rc=$? ;;
  dcrt)   cd $W/seesaw_rev2/pilot && timeout 290 cargo test --release -p seesaw-core --test drycleaner_roundtrip -- --ignored --nocapture >$L 2>&1; rc=$? ;;
  dcperf) cd $W/seesaw_rev2/pilot && SEESAW_PERF_N=5 timeout 290 cargo test --release -p seesaw-core --test drycleaner_roundtrip drycleaner_perf_forward -- --ignored --nocapture >$L 2>&1; rc=$?; grep -E 'PERF-SUITE' $L | sed "s/^/$v /" | tee -a $R/cons.tsv ;;
  gate)   cd $W/dry-cleaner && timeout 290 cargo test --locked --release -p chaos-generator --test roundtrip gate_sample -- --ignored --nocapture >$L 2>&1; rc=$? ;;
  fwd[0-9])
          # pipeline-forward in Teilen: Bibliothek und je ein Drittel der Testziele.
          cd $W/dry-cleaner && n=${step#fwd}
          targets=$(ls crates/pipeline-forward/tests/*.rs | xargs -n1 basename | sed 's/\.rs$//' | awk -v n=$n 'NR%3==n-1 {printf " --test %s", $0}')
          [ $n = 1 ] && targets="--lib $targets"
          timeout 290 cargo test --locked --release --no-fail-fast -p pipeline-forward $targets >$L 2>&1; rc=$? ;;
  *)      cd $W/dry-cleaner && timeout 290 cargo test --locked --release --no-fail-fast -p $step >$L 2>&1; rc=$? ;;
esac
echo "$v	$step	rc=$rc	$(sum)" | tee -a $R/cons.tsv
grep -E '^test .* FAILED|^error(\[|:)|panicked at' $L | head -12
