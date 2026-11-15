#!/bin/sh
# fin.sh <teil>...: Teile des Abschluss-Gates auf dem Endstand von perf/select.
# Jede Ergebniszeile geht mit dem Namen des Teils nach results/gate.txt. Jeder Teil unter 5 Minuten.
set -u
cd /root/seesaw-sel || exit 1
G=/root/seesaw-sel/results/gate.txt
sum() { awk '/^test result/ {p+=$4; f+=$6; i+=$8; g++} END {print "groups=" g, "passed=" p, "failed=" f, "ignored=" i}' "$1"; }
diffchecks() {
  f=$1; nf=""; tag=none; [ "$f" != none ] && { nf="--features $f"; tag=$(echo "$f" | tr ',' '+'); }
  cd /root/seesaw-sel/sel/src/difftest || return 1
  RUSTFLAGS='--cfg seesaw_checks' CARGO_TARGET_DIR=target/checks timeout 285 cargo test --release $nf >/tmp/sel-diffchecks-$tag.log 2>&1
  echo "DIFF_UNTER_CHECKS_RC=$? [$tag] $(sum /tmp/sel-diffchecks-$tag.log)"
  grep -E 'FAILED|panicked at|^error' -A8 /tmp/sel-diffchecks-$tag.log | head -30
  cd /root/seesaw-sel
}
for part in "$@"; do
  {
  case $part in
    stand) cat sel/STAND; rustc --version; cargo +1.88.0 --version; uptime ;;
    g1) sh sel.sh sel gate none ;;
    g2) sh sel.sh sel gate low_memory ;;
    g3) sh sel.sh sel gate parallel_seed ;;
    g4) sh sel.sh sel gate low_memory,parallel_seed ;;
    c1) sh sel.sh sel checks none ;;
    c2) sh sel.sh sel checks low_memory,parallel_seed ;;
    tr) sh gate2.sh trace ;;
    r1) sh gate2.sh r188 ;;
    r2) sh gate2.sh r188b ;;
    mi) sh gate2.sh misc ;;
    m1) sh gate2.sh mvn1 ;;
    m2) sh gate2.sh mvn2 ;;
    m3) sh gate2.sh mvn3 ;;
    b1) sh gate2.sh be1 ;;
    b2) sh gate2.sh be2 ;;
    b3) sh gate2.sh be3 ;;
    d1) sh sel.sh sel diff none ;;
    d2) sh sel.sh sel diff low_memory,parallel_seed ;;
    d3) sh sel.sh sel diff none 1500 ;;
    d4) sh sel.sh sel diff low_memory,parallel_seed 1500 ;;
    f1) sh sel.sh sel family none 400 ;;
    f2) sh sel.sh sel family low_memory,parallel_seed 400 ;;
    dc1) diffchecks none ;;
    dc2) diffchecks low_memory,parallel_seed ;;
    *) echo "unbekannt: $part" ;;
  esac
  } 2>&1 | sed "s/^/$part	/" | tee -a $G
done
