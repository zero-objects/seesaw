#!/bin/sh
# opt.sh <name> <cmd> <features|none> [args]: ein Pruef- oder Messschritt auf codepit, jeder unter 5 Minuten.
#   gate   <f>            fmt, clippy -D warnings, cargo test, je mit den Features
#   checks <f>            cargo test mit --cfg seesaw_checks und den Features
#   diff   <f> [faelle]   Differenztest gegen 2.0.2 (ohne family_agrees, wenn faelle > 400)
#   family <f> [faelle]   nur family_agrees (Standard 128)
#   instr  <f> <label>    Suite bauen, ein Lauf je Standardzelle, Zeilen nach results/<name>.tsv, Tabelle aller Labels
#   cells  <f> <label> <zelle>...   wie instr, aber die genannten Zellen
#   check  <f>            nur cargo check --all-targets (schnell)
set -u
n=$1; cmd=$2; f=${3:-none}; shift 3 2>/dev/null
D=/root/seesaw-opt/$n/src; R=/root/seesaw-opt/results; mkdir -p $R
cd $D || exit 1
ff=""; nf=""; tag=none
if [ "$f" != none ] && [ -n "$f" ]; then ff="--features $f"; nf="--features $f"; tag=$(echo "$f" | tr ',' '+'); fi
sum() { awk '/^test result/ {p+=$4; f+=$6; i+=$8; g++} END {print "groups=" g, "passed=" p, "failed=" f, "ignored=" i}' "$1"; }
bad() { grep -E 'FAILED|panicked at|^error' -A10 "$1" | head -60; }
CELLS="langpack/add/k700/n2000 langpack/add/k700/n20000 langpack/backward/k700/n2000 langpack/seed/k700/n2000 langpack/delta-add/k700/n20000/d200 langpack/delta-change/k700/n20000/d200 langpack/delta-delete/k700/n20000/d200 f2p/batch/f20000 f2p/add/f20000 f2p/delta-add/f20000/d1000 f2p/delta-delete/f20000/d1000"
table() {
  awk -F'\t' '{split($12,a,"="); v[$1,$2]=a[2]/1e6; fp[$1,$2]=$11; if(!sl[$1]++)L[++n]=$1; if(!sc[$2]++)C[++m]=$2}
    END {printf "cell"; for(i=1;i<=n;i++)printf " | %s",L[i]; printf " | fp\n";
         for(j=1;j<=m;j++){c=C[j]; printf "%s",c; ok="gleich"; ref="";
           for(i=1;i<=n;i++){ if((L[i],c) in v){printf " | %.1f",v[L[i],c]; if(ref=="")ref=fp[L[i],c]; else if(fp[L[i],c]!=ref)ok="ANDERS"} else printf " | -"}
           printf " | %s\n", ok}}' $R/$n.tsv
}
case $cmd in
  check)
    cargo check --all-targets $ff >/tmp/$n-check.log 2>&1; echo "CHECK_RC=$? [$tag]"; grep -E '^(warning|error)' -A12 /tmp/$n-check.log | head -80 ;;
  gate)
    cargo fmt --all -- --check >/dev/null 2>&1; echo "FMT_RC=$?"
    cargo clippy --all-targets $ff -- -D warnings >/tmp/$n-clippy.log 2>&1; echo "CLIPPY_RC=$? [$tag]"; grep -E '^(warning|error)' -A12 /tmp/$n-clippy.log | head -60
    timeout 250 cargo test $ff >/tmp/$n-test.log 2>&1; echo "TEST_RC=$? [$tag] $(sum /tmp/$n-test.log)"; bad /tmp/$n-test.log ;;
  checks)
    RUSTFLAGS='--cfg seesaw_checks' CARGO_TARGET_DIR=target/checks timeout 280 cargo test $ff >/tmp/$n-checks.log 2>&1; echo "CHECKS_RC=$? [$tag] $(sum /tmp/$n-checks.log)"; bad /tmp/$n-checks.log ;;
  diff)
    cases=${1:-}; cd difftest || exit 1; [ -n "$cases" ] && export DIFFTEST_CASES=$cases
    if [ -n "$cases" ] && [ "$cases" -gt 400 ]; then
      timeout 280 cargo test --release $nf -- --skip family_agrees >/tmp/$n-diff.log 2>&1; rc=$?
    else
      timeout 280 cargo test --release $nf >/tmp/$n-diff.log 2>&1; rc=$?
    fi
    echo "DIFF_RC=$rc [$tag] faelle=${cases:-standard} $(sum /tmp/$n-diff.log)"; grep -E '^test .* (ok|FAILED)$' /tmp/$n-diff.log | tr '\n' ';'; echo; bad /tmp/$n-diff.log ;;
  family)
    cases=${1:-128}; cd difftest || exit 1
    DIFFTEST_CASES=$cases timeout 280 cargo test --release $nf --test graphs -- family_agrees >/tmp/$n-family.log 2>&1; echo "FAMILY_RC=$? [$tag] faelle=$cases $(sum /tmp/$n-family.log)"; bad /tmp/$n-family.log ;;
  instr|cells)
    label=$1; shift; [ $cmd = cells ] && CELLS="$*"
    # Das Binary dieses Feature-Satzes aus der Ausgabe von cargo: bei einem schon gebauten
    # Satz wird nicht neu gelinkt, das neueste Binary im Verzeichnis waere dann ein anderes.
    B=$(CARGO_TARGET_DIR=target/time cargo bench -q --bench suite $ff --no-run --message-format=json 2>/tmp/$n-build.err | grep -o '"executable":"[^"]*/suite-[^"]*"' | tail -1 | sed 's/^"executable":"//; s/"$//')
    [ -n "$B" ] && [ -x "$B" ] || { echo "BUILD_FAILED [$tag]"; grep -E '^error' -A14 /tmp/$n-build.err | head -60; exit 1; }
    echo "binary [$tag]: $(basename $B)"
    touch $R/$n.tsv; grep -v "^$label	" $R/$n.tsv > $R/$n.tsv.tmp; mv $R/$n.tsv.tmp $R/$n.tsv
    for c in $CELLS; do timeout 120 $B run $c --samples 1 --label $label --tsv $R/$n.tsv >/dev/null 2>>/tmp/$n-run.err || echo "RUN_FAILED $c"; done
    table ;;
  *) echo "unbekannt: $cmd"; exit 2 ;;
esac
