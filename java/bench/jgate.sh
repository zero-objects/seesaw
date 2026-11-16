#!/bin/sh
# jgate.sh [teil ...]: Prüfungen des Java-Zweigs, auf codepit über das Board. Jede Ergebniszeile
# beginnt mit dem Namen des Teils. Ohne Argument laufen alle Teile nacheinander; das dauert
# länger als 5 Minuten, über das Board deshalb jeden Teil als eigenen Job fahren
# (mvn1, mvn2, mvn3, fp, pde:java40, pde:open5, pde:open2 pde:open2-edit).
#
#   mvn   mvn test in drei Stellungen, je Stellung die Zeilen
#         "mvn MVN_RC=… run=… failures=… errors=… skipped=… [argument]" und
#         "mvn VERDIKT=bestanden [argument]" oder "mvn VERDIKT=durchgefallen <gründe> [argument]".
#         Bestanden nur bei MVN_RC=0, run>0, failures=0 und errors=0.
#           mvn1  ohne Argument
#           mvn2  -DargLine=-Dseesaw.anchorIndex=false
#           mvn3  -DargLine=-Dseesaw.checks=true
#   fp    jede Zelle von JavaSuite einmal (--warmup 0 --reps 1) außer den Zellen pde/… (die
#         prüft der Teil pde); Schritte, angewandte Schritte, Knoten und Fingerprint gegen die
#         Rust-Zellen mit Label sel aus benches/suite/results/2026-11-15-auswahl/uid-cgu1.tsv und
#         uid-small-cgu1.tsv. Erwartet wird jede dieser Rust-Zellen außer denen, die die
#         Java-Seite nicht hat (NOT_JAVA), und außer pde/…; eine erwartete Zelle ohne
#         Java-Ergebnis meldet "fp <zelle> FEHLT". Eine Zeile je Zelle, dann "fp FP_RC=…
#         zellen=… erwartet=… gleich=… abweichend=… fehlend=… ohne_referenz=… fehlgeschlagen=…".
#         FP_RC=0 nur, wenn jede erwartete Zelle gleich ist, keine Java-Zelle abweicht oder ohne
#         Referenz ist und zellen>0.
#   pde   die Protokolle des Hosts zerodds-pde (PdeReplay) im Verzeichnis SEESAW_PDE_DUMP
#         (Vorgabe /root/seesaw-java/pde-dump). Erwartet wird jedes Protokoll, zu dem uid-cgu1.tsv
#         Rust-Zellen pde/<teil>/<name> mit Label sel hat (java40, open5, open2, open2-edit). Je
#         Rust-Zelle läuft die gleichnamige Java-Zelle einmal (--warmup 0 --reps 1, Zeitgrenze
#         PDE_TIMEOUT, Vorgabe 270 s); die Nachfahrt prüft jeden Aufruf gegen das Protokoll und
#         bricht bei der ersten Abweichung mit Zeilennummer ab. Eine Zeile je Zelle mit
#         "gleich", "ABWEICHUNG", "FEHLT" oder "RUN_FAILED" (mit der Meldung der Nachfahrt), der
#         Dauer des Prozesses (sekunden=) und der Thread-CPU der Messstrecke (cpu_ms=, Spalte 4
#         der Zeile von JavaSuite; ein Lauf ohne Aufwärmen, kein Messwert), dann je Protokoll "pde <name> VERDIKT=bestanden zellen=…" oder
#         "pde <name> VERDIKT=durchgefallen <gründe> zellen=…". Bestanden nur, wenn jede
#         Rust-Zelle des Protokolls ohne Abweichung durchlief und Schritte, angewandte Schritte,
#         Knoten und Fingerprint gleich der Rust-Zelle sind.
#         Bekannte Abweichungen von Java 2.0.2 stehen in PDE_EXPECTED (Vorgabe
#         bench/pde-abweichungen.txt, Format und Grund dort). Für ein Protokoll mit Eintrag besteht
#         eine Zelle nur, wenn ihre erste Abweichung genau an der Zeile des Eintrags mit genau
#         dessen Meldung liegt ("bekannte Abweichung an Zeile …"). Eine andere Abweichung
#         (ANDERE_ABWEICHUNG), ein Lauf ohne Abweichung (OHNE_ABWEICHUNG, eine
#         Verhaltensänderung gegenüber 2.0.2) oder ein anderer Fehler fallen durch. Fehlt die
#         Datei oder ist eine Zeile ungültig, fällt der ganze Teil durch.
#         Am Ende "pde PDE_RC=… protokolle=… bestanden=… durchgefallen=…"; PDE_RC=0 nur, wenn
#         jedes verlangte Protokoll besteht und protokolle>0.
#           pde:<name>  nur das Protokoll <name>, für Jobs unter 5 Minuten
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
NOT_JAVA='^langpack/engines/|^langpack/index/|/delta-change-same/'
# Zellen der Nachfahrt; sie prüft nur der Teil pde, jede Rust-Zelle genau ein Teil.
PDE='^pde/'
PDE_DIR=${SEESAW_PDE_DUMP:-/root/seesaw-java/pde-dump}
PDE_TIMEOUT=${PDE_TIMEOUT:-270}
PDE_EXPECTED=${PDE_EXPECTED:-$J/bench/pde-abweichungen.txt}
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

# Testklassen bauen, Klassenpfad nach /tmp/jgate-$1.cp. Rückgabe ungleich 0 bei Fehler, die
# Zeilen dazu mit dem Präfix $1.
build_suite() {
    cd "$J" || exit 1
    timeout 200 mvn -q -o -B test-compile >/tmp/jgate-$1-build.log 2>&1 \
        && deps_cp /tmp/jgate-$1.cp >>/tmp/jgate-$1-build.log 2>&1
    rc=$?
    if [ $rc -ne 0 ]; then
        grep -E '^\[ERROR\]' /tmp/jgate-$1-build.log | head -10 | sed "s/^/$1   /"
    fi
    return $rc
}

fp() {
    if ! build_suite fp; then
        echo "fp FP_RC=1 BUILD_RC=$rc"
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
    for c in $(cut -f1 /tmp/jgate-fp.list | grep -Ev "$PDE"); do
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
    cat "$REF/uid-cgu1.tsv" "$REF/uid-small-cgu1.tsv" | awk -F'\t' -v failed=$failed -v skip="$NOT_JAVA" -v pde="$PDE" '
        FNR == NR {
            if ($1 == "sel" && !($2 in ref)) {
                ref[$2] = $8 " " $9 " " $10 " " $11
                if ($2 !~ skip && $2 !~ pde) expected[++m] = $2
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

# Teil pde. Argumente: Namen der Protokolle, ohne Argument alle erwarteten.
pde() {
    if ! build_suite pde; then
        echo "pde PDE_RC=1 BUILD_RC=$rc"
        status=1
        return
    fi
    cp="target/classes:target/test-classes:$(cat /tmp/jgate-pde.cp)"
    # Rust-Zellen pde/… mit Label sel: Zelle, Protokoll, Ergebnis.
    awk -F'\t' '$1 == "sel" && $2 ~ /^pde\/[a-z]+\// && !seen[$2]++ {
        name = $2; sub(/^pde\/[a-z]+\//, "", name)
        print $2 "\t" name "\t" $8 " " $9 " " $10 " " $11
    }' "$REF/uid-cgu1.tsv" >/tmp/jgate-pde.ref
    if [ $# -gt 0 ]; then names=$*; else names=$(cut -f2 /tmp/jgate-pde.ref | sort -u); fi
    if ! SEESAW_PDE_DUMP=$PDE_DIR java -cp "$cp" $MAIN list >/tmp/jgate-pde.list 2>/tmp/jgate-pde.err; then
        echo "pde LIST_FAILED"
        head -3 /tmp/jgate-pde.err | sed 's/^/pde   /'
        : >/tmp/jgate-pde.list
    fi
    echo "pde verzeichnis=$PDE_DIR"
    # Bekannte Abweichungen: Name, Zeile, Meldung (Tab-getrennt nach /tmp/jgate-pde.exp).
    if [ ! -f "$PDE_EXPECTED" ]; then
        echo "pde ERWARTUNG_FEHLT $PDE_EXPECTED"
        echo "pde PDE_RC=1"
        status=1
        return
    fi
    if ! awk -v f="$PDE_EXPECTED" '
        /^[ \t]*(#|$)/ { next }
        {
            line = $0
            if (NF < 3 || $2 !~ /^[1-9][0-9]*$/ || ($1 in seen)) {
                print "pde ERWARTUNG_UNGUELTIG " f ":" FNR ": " line > "/dev/stderr"
                bad = 1
                next
            }
            seen[$1] = 1
            msg = line
            sub(/^[^ ]+ +[^ ]+ +/, "", msg)
            print $1 "\t" $2 "\t" msg
        }
        END { exit bad }' "$PDE_EXPECTED" >/tmp/jgate-pde.exp 2>/tmp/jgate-pde.experr; then
        cat /tmp/jgate-pde.experr
        echo "pde PDE_RC=1"
        status=1
        return
    fi
    echo "pde erwartung=$PDE_EXPECTED eintraege=$(wc -l </tmp/jgate-pde.exp | tr -d ' ')"
    tsv=/tmp/jgate-pde.tsv
    records=0; passed=0; failed_records=0
    for name in $names; do
        records=$((records + 1))
        cells=$(awk -F'\t' -v n="$name" '$2 == n { print $1 }' /tmp/jgate-pde.ref)
        exp_line=$(awk -F'\t' -v n="$name" '$1 == n { print $2 }' /tmp/jgate-pde.exp)
        exp_msg=$(awk -F'\t' -v n="$name" '$1 == n { print $3 }' /tmp/jgate-pde.exp)
        n=0; same=0; diff=0; missing=0; failed=0; known=0; other=0; through=0
        if [ -z "$cells" ]; then
            echo "pde $name OHNE_REFERENZ: keine Rust-Zelle pde/<teil>/$name mit Label sel"
        fi
        for c in $cells; do
            n=$((n + 1))
            rust=$(awk -F'\t' -v c="$c" '$1 == c { print $3 }' /tmp/jgate-pde.ref)
            if ! cut -f1 /tmp/jgate-pde.list | grep -qxF "$c"; then
                missing=$((missing + 1))
                echo "pde $c FEHLT rust=$rust (kein Protokoll $name.trace in $PDE_DIR?)"
                continue
            fi
            rm -f $tsv
            t0=$(date +%s%N)
            SEESAW_PDE_DUMP=$PDE_DIR timeout "$PDE_TIMEOUT" java $JVM -cp "$cp" $MAIN run "$c" \
                --label pde --warmup 0 --reps 1 --tsv $tsv >/dev/null 2>/tmp/jgate-pde.err
            rc=$?
            secs=$(awk -v a="$t0" -v b="$(date +%s%N)" 'BEGIN { printf "%.1f", (b - a) / 1e9 }')
            if [ $rc -ne 0 ] || [ ! -s $tsv ]; then
                # Erste Abweichung der Nachfahrt: Zeile und Meldung.
                mm=$(grep -m1 'PdeReplay\$Mismatch: ' /tmp/jgate-pde.err)
                if [ -n "$exp_line" ] && [ -n "$mm" ]; then
                    at=$(printf '%s\n' "$mm" | sed -n 's/.*\.trace:\([0-9][0-9]*\): \(.*\); Aufruf: .*/\1/p')
                    msg=$(printf '%s\n' "$mm" | sed -n 's/.*\.trace:\([0-9][0-9]*\): \(.*\); Aufruf: .*/\2/p')
                    if [ "$at" = "$exp_line" ] && [ "$msg" = "$exp_msg" ]; then
                        known=$((known + 1))
                        echo "pde $c bekannte Abweichung an Zeile $at: $msg sekunden=$secs"
                    else
                        other=$((other + 1))
                        echo "pde $c ANDERE_ABWEICHUNG an Zeile $at: $msg (Eintrag: Zeile $exp_line: $exp_msg) sekunden=$secs"
                        printf '%s\n' "$mm" | cut -c1-400 | sed 's/^/pde   /'
                    fi
                    continue
                fi
                failed=$((failed + 1))
                echo "pde $c RUN_FAILED rc=$rc sekunden=$secs rust=$rust"
                if grep -q -E 'Exception|Error' /tmp/jgate-pde.err; then
                    grep -m1 -E 'Exception|Error' /tmp/jgate-pde.err | cut -c1-400 | sed 's/^/pde   /'
                else
                    head -3 /tmp/jgate-pde.err | cut -c1-400 | sed 's/^/pde   /'
                fi
                continue
            fi
            got=$(awk -F'\t' '{ print $8 " " $9 " " $10 " " $11 }' $tsv)
            cpu=$(awk -F'\t' '{ print $4 }' $tsv)
            if [ -n "$exp_line" ]; then
                through=$((through + 1))
                echo "pde $c OHNE_ABWEICHUNG java=$got rust=$rust (Eintrag: Zeile $exp_line: $exp_msg) sekunden=$secs"
            elif [ "$got" = "$rust" ]; then
                same=$((same + 1))
                echo "pde $c gleich $got sekunden=$secs cpu_ms=$cpu"
            else
                diff=$((diff + 1))
                echo "pde $c ABWEICHUNG java=$got rust=$rust sekunden=$secs cpu_ms=$cpu"
            fi
        done
        why=""
        [ $n -eq 0 ] && why="$why zellen=0"
        [ $diff -ne 0 ] && why="$why abweichend=$diff"
        [ $missing -ne 0 ] && why="$why fehlend=$missing"
        [ $failed -ne 0 ] && why="$why fehlgeschlagen=$failed"
        [ $other -ne 0 ] && why="$why andere_abweichung=$other"
        [ $through -ne 0 ] && why="$why ohne_abweichung=$through"
        if [ -z "$why" ] && [ -n "$exp_line" ]; then
            passed=$((passed + 1))
            echo "pde $name VERDIKT=bestanden bekannte Abweichung an Zeile $exp_line zellen=$n"
        elif [ -z "$why" ]; then
            passed=$((passed + 1))
            echo "pde $name VERDIKT=bestanden zellen=$n"
        else
            failed_records=$((failed_records + 1))
            echo "pde $name VERDIKT=durchgefallen$why zellen=$n"
        fi
    done
    rc=0
    [ $records -eq 0 ] || [ $failed_records -ne 0 ] && rc=1
    echo "pde PDE_RC=$rc protokolle=$records bestanden=$passed durchgefallen=$failed_records"
    [ $rc -eq 0 ] || status=1
}

[ $# -gt 0 ] || set -- mvn fp pde
for part in "$@"; do
    case $part in
        mvn) mvn_one 1; mvn_one 2; mvn_one 3 ;;
        mvn1) mvn_one 1 ;;
        mvn2) mvn_one 2 ;;
        mvn3) mvn_one 3 ;;
        fp) fp ;;
        pde) pde ;;
        pde:*) pde "${part#pde:}" ;;
        *) echo "$part unbekannter Teil"; exit 2 ;;
    esac
done
exit $status
