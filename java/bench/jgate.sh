#!/bin/sh
# jgate.sh [teil ...]: Prüfungen des Java-Zweigs, auf codepit über das Board. Jede Ergebniszeile
# beginnt mit dem Namen des Teils. Ohne Argument laufen alle Teile nacheinander; das dauert
# länger als 5 Minuten, über das Board deshalb jeden Teil als eigenen Job fahren
# (mvn1, mvn2, mvn3, fp).
#
#   mvn   mvn test in drei Stellungen, je Stellung die Zeilen
#         "mvn MVN_RC=… run=… failures=… errors=… skipped=… [argument]" und
#         "mvn VERDIKT=bestanden [argument]" oder "mvn VERDIKT=durchgefallen <gründe> [argument]".
#         Bestanden nur bei MVN_RC=0, run>0, failures=0 und errors=0.
#           mvn1  ohne Argument
#           mvn2  -DargLine=-Dseesaw.anchorIndex=false
#           mvn3  -DargLine=-Dseesaw.checks=true
#   fp    jede Zelle von JavaSuite einmal (--warmup 0 --reps 1); Schritte, angewandte Schritte,
#         Knoten und Fingerprint gegen die Rust-Zellen mit Label sel aus
#         benches/suite/results/2026-11-15-auswahl/uid-cgu1.tsv und uid-small-cgu1.tsv.
#         Erwartet wird jede dieser Rust-Zellen außer denen, die die Java-Seite nicht hat
#         (NOT_JAVA); eine erwartete Zelle ohne Java-Ergebnis meldet "fp <zelle> FEHLT".
#         Eine Zeile je Zelle, dann "fp FP_RC=… zellen=… erwartet=… gleich=… abweichend=…
#         fehlend=… ohne_referenz=… fehlgeschlagen=…". FP_RC=0 nur, wenn jede erwartete Zelle
#         gleich ist, keine Java-Zelle abweicht oder ohne Referenz ist und zellen>0.
#
# Exit-Code 1, wenn ein Teil nicht besteht. Gefahren wird der Baum, in dem dieses Skript liegt
# (auf codepit /root/seesaw-java/src).
set -u

J=$(cd "$(dirname "$0")/.." && pwd)
R=$(cd "$J/.." && pwd)
REF=$R/benches/suite/results/2026-11-15-auswahl
JVM="-Xms8g -Xmx8g -XX:+UseParallelGC"
MAIN=net.sandrakessler.seesaw.bench.JavaSuite
# Rust-Zellen der Referenz, die JavaSuite nicht hat (Regex über den Zellnamen).
NOT_JAVA='^pde/|^langpack/engines/|^langpack/index/|/delta-change-same/'
status=0

# Klassenpfad der Laufzeit-Abhängigkeiten nach $1. Offline; fehlt dem lokalen Repository das
# Plugin, einmal ohne -o.
deps_cp() {
    timeout 100 mvn -q -o -B dependency:build-classpath -DincludeScope=runtime -Dmdep.outputFile="$1" \
        || timeout 100 mvn -q -B dependency:build-classpath -DincludeScope=runtime -Dmdep.outputFile="$1"
}

mvn_one() {
    case $1 in
        1) arg="" ;;
        2) arg="-DargLine=-Dseesaw.anchorIndex=false" ;;
        3) arg="-DargLine=-Dseesaw.checks=true" ;;
    esac
    cd "$J" || exit 1
    rm -rf target/surefire-reports
    timeout 280 mvn -q -o -B test $arg >/tmp/jgate-mvn$1.log 2>&1
    rc=$?
    find target/surefire-reports -name '*.txt' 2>/dev/null | xargs -r grep -h 'Tests run' \
        | awk -F'[:,]' -v rc=$rc -v arg="$arg" '
            { r += $2; f += $4; e += $6; s += $8 }
            END {
                printf "mvn MVN_RC=%d run=%d failures=%d errors=%d skipped=%d [%s]\n", rc, r, f, e, s, arg
                why = ""
                if (rc != 0) why = why " MVN_RC=" rc
                if (r == 0) why = why " run=0"
                if (f != 0) why = why " failures=" f
                if (e != 0) why = why " errors=" e
                if (why == "") print "mvn VERDIKT=bestanden [" arg "]"
                else print "mvn VERDIKT=durchgefallen" why " [" arg "]"
                exit (why == "" ? 0 : 1)
            }' || status=1
    grep -E '^\[ERROR\]|FAIL' /tmp/jgate-mvn$1.log | head -8 | sed 's/^/mvn   /'
}

fp() {
    cd "$J" || exit 1
    timeout 200 mvn -q -o -B test-compile >/tmp/jgate-fp-build.log 2>&1 \
        && deps_cp /tmp/jgate-fp.cp >>/tmp/jgate-fp-build.log 2>&1
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "fp FP_RC=1 BUILD_RC=$rc"
        grep -E '^\[ERROR\]' /tmp/jgate-fp-build.log | head -10 | sed 's/^/fp   /'
        status=1
        return
    fi
    cp="target/classes:target/test-classes:$(cat /tmp/jgate-fp.cp)"
    tsv=/tmp/jgate-fp.tsv
    rm -f $tsv
    failed=0
    if ! java -cp "$cp" $MAIN list >/tmp/jgate-fp.list 2>/tmp/jgate-fp.err; then
        echo "fp LIST_FAILED"
        head -3 /tmp/jgate-fp.err | sed 's/^/fp   /'
        : >/tmp/jgate-fp.list
    fi
    for c in $(cut -f1 /tmp/jgate-fp.list); do
        timeout 120 java $JVM -cp "$cp" $MAIN run "$c" --label fp --warmup 0 --reps 1 --tsv $tsv \
            >/dev/null 2>/tmp/jgate-fp.err
        rc=$?
        if [ $rc -ne 0 ]; then
            failed=$((failed + 1))
            echo "fp $c RUN_FAILED rc=$rc"
            head -3 /tmp/jgate-fp.err | sed 's/^/fp   /'
        fi
    done
    touch $tsv
    cat "$REF/uid-cgu1.tsv" "$REF/uid-small-cgu1.tsv" | awk -F'\t' -v failed=$failed -v skip="$NOT_JAVA" '
        FNR == NR {
            if ($1 == "sel" && !($2 in ref)) {
                ref[$2] = $8 " " $9 " " $10 " " $11
                if ($2 !~ skip) expected[++m] = $2
            }
            next
        }
        {
            n++
            seen[$2] = 1
            got = $8 " " $9 " " $10 " " $11
            if (!($2 in ref)) { none++; print "fp " $2 " OHNE_REFERENZ java=" got; next }
            if (got == ref[$2]) { same++; print "fp " $2 " gleich " got }
            else { diff++; print "fp " $2 " ABWEICHUNG java=" got " rust=" ref[$2] }
        }
        END {
            for (i = 1; i <= m; i++) {
                if (!(expected[i] in seen)) { missing++; print "fp " expected[i] " FEHLT rust=" ref[expected[i]] }
            }
            rc = (diff + none + failed + missing > 0 || n == 0 || m == 0) ? 1 : 0
            print "fp FP_RC=" rc " zellen=" n + 0 " erwartet=" m + 0 " gleich=" same + 0 \
                " abweichend=" diff + 0 " fehlend=" missing + 0 " ohne_referenz=" none + 0 \
                " fehlgeschlagen=" failed
            exit rc
        }' - $tsv || status=1
}

[ $# -gt 0 ] || set -- mvn fp
for part in "$@"; do
    case $part in
        mvn) mvn_one 1; mvn_one 2; mvn_one 3 ;;
        mvn1) mvn_one 1 ;;
        mvn2) mvn_one 2 ;;
        mvn3) mvn_one 3 ;;
        fp) fp ;;
        *) echo "$part unbekannter Teil"; exit 2 ;;
    esac
done
exit $status
