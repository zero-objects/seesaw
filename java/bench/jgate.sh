#!/bin/sh
# jgate.sh [teil ...]: Prüfungen des Java-Zweigs, auf codepit über das Board. Jede Ergebniszeile
# beginnt mit dem Namen des Teils. Ohne Argument laufen alle Teile nacheinander (mvn fp pde
# diff pde-voll). Das dauert länger als 5 Minuten, über das Board deshalb jeden Teil als eigenen
# Job fahren (mvn1, mvn2, mvn3, fp, pde:java40, pde:open5, pde:open2 pde:open2-edit, die Stücke
# von diff, siehe dort, und pde-voll:open2-edit).
#
#   mvn   mvn test in drei Stellungen, je Stellung die Zeilen
#         "mvn MVN_RC=… run=… failures=… errors=… skipped=… [argument]" und
#         "mvn VERDIKT=bestanden [argument]" oder "mvn VERDIKT=durchgefallen <gründe> [argument]".
#         Bestanden nur bei MVN_RC=0, run>0, failures=0 und errors=0.
#           mvn1  ohne Argument
#           mvn2  -DargLine=-Dseesaw.anchorIndex=false
#           mvn3  -DargLine=-Dseesaw.checks=true
#   fp    jede Zelle von JavaSuite einmal (--warmup 0 --reps 1) außer den Zellen pde/… (die
#         prüft der Teil pde). Schritte, angewandte Schritte, Knoten und Fingerprint gegen die
#         Rust-Zellen mit Label sel aus benches/suite/results/2026-11-15-auswahl/uid-cgu1.tsv und
#         uid-small-cgu1.tsv und mit Label par aus par.tsv (Seed auf mehreren Threads, …/reseed/…).
#         Erwartet wird jede dieser Rust-Zellen außer denen, die die
#         Java-Seite nicht hat (NOT_JAVA), und außer pde/…. Eine erwartete Zelle ohne
#         Java-Ergebnis meldet "fp <zelle> FEHLT", eine Zelle, die der Stand nicht kann (Zeile
#         nicht_verfuegbar von JavaSuite), "fp <zelle> NICHT_VERFUEGBAR". Eine Zeile je Zelle, dann
#         "fp FP_RC=… zellen=… erwartet=… gleich=… abweichend=… fehlend=… ohne_referenz=…
#         fehlgeschlagen=… nicht_verfuegbar=…". FP_RC=0 nur, wenn jede erwartete Zelle gleich ist,
#         keine Java-Zelle abweicht, ohne Referenz oder nicht verfügbar ist und zellen>0.
#   pde   die Protokolle des Hosts zerodds-pde (PdeReplay) im Verzeichnis SEESAW_PDE_DUMP
#         (Vorgabe /root/seesaw-java/pde-dump). Erwartet wird jedes Protokoll, zu dem uid-cgu1.tsv
#         Rust-Zellen pde/<teil>/<name> mit Label sel hat (java40, open5, open2, open2-edit). Je
#         Rust-Zelle läuft die gleichnamige Java-Zelle einmal (--warmup 0 --reps 1, Zeitgrenze
#         PDE_TIMEOUT, Vorgabe 270 s). Die Nachfahrt prüft jeden Aufruf gegen das Protokoll und
#         bricht bei der ersten Abweichung mit Zeilennummer ab. Eine Zeile je Zelle mit
#         "gleich", "ABWEICHUNG", "FEHLT" oder "RUN_FAILED" (mit der Meldung der Nachfahrt), der
#         Dauer des Prozesses (sekunden=) und der Thread-CPU der Messstrecke (cpu_ms=, Spalte 4
#         der Zeile von JavaSuite, ein Lauf ohne Aufwärmen, kein Messwert), dann je Protokoll "pde <name> VERDIKT=bestanden zellen=…" oder
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
#         Am Ende "pde PDE_RC=… protokolle=… bestanden=… durchgefallen=…". PDE_RC=0 nur, wenn
#         jedes verlangte Protokoll besteht und protokolle>0.
#           pde:<name>  nur das Protokoll <name>, für Jobs unter 5 Minuten
#   diff  [n]  Differenztest gegen Java 2.0.2 (Vorgabe n=1500). Dieselben Szenarien
#         (Worlds.scenario, Seeds 1 bis n) laufen durch die Engine dieses Baums und durch die von
#         v2.0.2. Verglichen werden je Szenario die SHA-256 über alle Abzüge (Worlds.dump) der
#         Wellen, Graph, Kaskade, Rückgaben von step und Zustand der Engine. Die Referenz liegt in
#         SEESAW_DIFF_REF (Vorgabe /root/seesaw-java/ref) und muss vorher dort sein, auf dem Mac:
#           git archive v2.0.2 java | ssh codepit 'rm -rf /root/seesaw-java/ref && mkdir -p
#           /root/seesaw-java/ref && tar -x -C /root/seesaw-java/ref'
#         Dazu kopiert der Teil Worlds.java (ohne Index-Argument und ohne seedFactor, das 2.0.2
#         nicht kennt) und DiffDump.java dieses Baums in die Referenz, setzt in deren pom.xml die
#         jackson-databind-Version dieses Baums (2.17.2 liegt nicht im lokalen Repository) und baut
#         sie dort. Fehlt die
#         Referenz oder enthält sie AnchorIndex, ist das FEHLER (REF_FEHLT, REF_ZU_NEU), kein Erfolg.
#         Ein Stück sind eine Welt und eine Richtung (gerichtet/ungerichtet). Je Stück eine Zeile
#         "diff <welt>/<richtung> DIFF n=… gleich=… erste_abweichung=… wellen=… angewandt=…
#         sekunden=…" (erste_abweichung=welt/richtung/seed=…/welle=… oder keine) und
#         "diff <welt>/<richtung> VERDIKT=bestanden" oder "…VERDIKT=durchgefallen <gründe>".
#         Bestanden nur, wenn beide Seiten n Szenarien lieferten, alle gleich sind, wellen>0 und
#         angewandt>0 (mindestens ein angewandter Schritt) und beide Seiten ohne Fehler liefen. Bei
#         einer Abweichung stehen die erste abweichende Zeile beider Seiten dabei.
#         Gegenprobe (diff:gegenprobe): die Seite dieses Baums verändert mit -Dseesaw.diff.mutate
#         eine Zeile eines Abzugs. Bestanden nur, wenn der Vergleich genau dieses Szenario meldet.
#         Am Ende "diff DIFF_RC=… stuecke=… bestanden=… durchgefallen=…".
#           diff               alle Stücke und die Gegenprobe (länger als 5 Minuten, auf codepit
#                              etwa 20 Minuten), family/ungerichtet läuft darin in Stücken zu
#                              250 Seeds (diff:family/ungerichtet@1-250 … @1251-1500)
#           diff:<welt>        beide Richtungen einer Welt
#           diff:<welt>/<richtung>  ein Stück
#           diff:<welt>/<richtung>@<von>-<bis>  nur die Seeds von bis bis (für Jobs unter
#                      5 Minuten, n=bis-von+1, erste_abweichung nennt den Seed wie sonst)
#           diff:gegenprobe    nur die Gegenprobe
#         Welten: followup family predicates predicates_no_equals. Das n steht hinter dem Teil
#         ("diff 1500", "diff:family/gerichtet 1500").
#   pde-voll  ein Protokoll des Hosts ganz, auch hinter der ersten Abweichung von Java 2.0.2
#         (PdeReplay voll: je Aufruf an einer Engine eine Zeile mit Rückgabe, Länge der Kaskade
#         und Zahl der Knoten, nach consol mit Fingerprint, am Ende "ende …" mit Fingerprint).
#         Der Lauf geht einmal gegen die Hauptklassen von L0 (fa12974, Java 2.0.2 mit Index,
#         JAR aus jsuite.sh build L0 fa12974, das der Teil aufruft, wenn das JAR fehlt oder zu
#         einem anderen Stand gehört, gesendet auf dem Mac mit jsuite.sh send L0 fa12974) und
#         einmal gegen die dieses Baums. Die Nachfahrt kommt beide Male aus diesem Baum.
#         Verglichen werden die beiden Ausgaben Zeile für Zeile (/tmp/jgate-voll-L0.out und
#         /tmp/jgate-voll-HEAD.out). Je Seite eine Zeile "pde-voll <name> <seite> ende=ja|nein
#         zeilen=… sekunden=…" und die Zeile "ende …" oder "abbruch …" der Seite, dann
#         "pde-voll <name> VERDIKT=bestanden zeilen=…" nur, wenn beide Seiten bis zum Ende
#         kamen und die Ausgaben gleich sind, sonst "VERDIKT=durchgefallen <gründe>" mit der
#         ersten abweichenden Zeile beider Seiten. Am Ende "pde-voll VOLL_RC=… protokolle=…
#         bestanden=… durchgefallen=…".
#           pde-voll           jedes Protokoll <name>-edit in SEESAW_PDE_DUMP
#           pde-voll:<name>    nur das Protokoll <name> (für das Gate pde-voll:open2-edit)
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
NOT_JAVA='/delta-change-same/'
# Zellen der Nachfahrt. Sie prüft nur der Teil pde, jede Rust-Zelle genau ein Teil.
PDE='^pde/'
PDE_DIR=${SEESAW_PDE_DUMP:-/root/seesaw-java/pde-dump}
PDE_TIMEOUT=${PDE_TIMEOUT:-270}
PDE_EXPECTED=${PDE_EXPECTED:-$J/bench/pde-abweichungen.txt}
status=0

# Klassenpfad der Laufzeit-Abhängigkeiten nach $1. Offline. Fehlt dem lokalen Repository das
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
    cat "$REF/uid-cgu1.tsv" "$REF/uid-small-cgu1.tsv" "$REF/par.tsv" | awk -F'\t' -v failed=$failed -v skip="$NOT_JAVA" -v pde="$PDE" '
        FNR == NR {
            if (($1 == "sel" || $1 == "par") && !($2 in ref)) {
                ref[$2] = $8 " " $9 " " $10 " " $11
                if ($2 !~ skip && $2 !~ pde) expected[++m] = $2
            }
            next
        }
        {
            n++
            seen[$2] = 1
            got = $8 " " $9 " " $10 " " $11
            if ($3 == "nicht_verfuegbar") { unav++; print "fp " $2 " NICHT_VERFUEGBAR " $4; next }
            if (!($2 in ref)) { none++; print "fp " $2 " OHNE_REFERENZ java=" got; next }
            if (got == ref[$2]) { same++; print "fp " $2 " gleich " got }
            else { diff++; print "fp " $2 " ABWEICHUNG java=" got " rust=" ref[$2] }
        }
        END {
            for (i = 1; i <= m; i++) {
                if (!(expected[i] in seen)) { missing++; print "fp " expected[i] " FEHLT rust=" ref[expected[i]] }
            }
            rc = (diff + none + failed + missing + unav > 0 || n == 0 || m == 0) ? 1 : 0
            print "fp FP_RC=" rc " zellen=" n + 0 " erwartet=" m + 0 " gleich=" same + 0 \
                " abweichend=" diff + 0 " fehlend=" missing + 0 " ohne_referenz=" none + 0 \
                " fehlgeschlagen=" failed " nicht_verfuegbar=" unav + 0
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

# Teil pde-voll. Argumente: Namen der Protokolle, ohne Argument jedes <name>-edit.
VOLL_JARS=${SEESAW_JARS:-/root/seesaw-java/jars}
VOLL_L0=fa12974
VOLL_TIMEOUT=${VOLL_TIMEOUT:-120}
pde_full() {
    if ! build_suite voll; then
        echo "pde-voll VOLL_RC=1 BUILD_RC=$rc"
        status=1
        return
    fi
    # Die Hauptklassen von L0, gebaut von jsuite.sh, wenn sie fehlen oder zu einem anderen
    # Stand gehören.
    case $(cat "$VOLL_JARS/L0.commit" 2>/dev/null) in
        "$VOLL_L0"*) ;;
        *)
            if ! sh "$J/bench/jsuite.sh" build L0 "$VOLL_L0" >/tmp/jgate-voll-l0build.log 2>&1; then
                sed 's/^/pde-voll   /' /tmp/jgate-voll-l0build.log | head -10
                echo "pde-voll VOLL_RC=1 L0_FEHLT (auf dem Mac: sh java/bench/jsuite.sh send L0 $VOLL_L0)"
                status=1
                return
            fi
            sed 's/^/pde-voll   /' /tmp/jgate-voll-l0build.log | tail -1
            ;;
    esac
    head_cp="target/classes:target/test-classes:$(cat /tmp/jgate-voll.cp)"
    l0_cp="$VOLL_JARS/L0.jar:target/test-classes:$(cat "$VOLL_JARS/L0.cp")"
    if [ $# -gt 0 ]; then
        names=$*
    else
        names=$(ls "$PDE_DIR" 2>/dev/null | sed -n 's/^\(.*-edit\)\.trace$/\1/p')
    fi
    echo "pde-voll verzeichnis=$PDE_DIR L0=$(cat "$VOLL_JARS/L0.commit")"
    records=0; passed=0; failed_records=0
    for name in $names; do
        records=$((records + 1))
        trace=$PDE_DIR/$name.trace
        why=""
        if [ ! -f "$trace" ]; then
            echo "pde-voll $name VERDIKT=durchgefallen PROTOKOLL_FEHLT $trace"
            failed_records=$((failed_records + 1))
            continue
        fi
        for side in L0 HEAD; do
            if [ $side = L0 ]; then cp=$l0_cp; else cp=$head_cp; fi
            out=/tmp/jgate-voll-$side.out
            t0=$(date +%s%N)
            timeout "$VOLL_TIMEOUT" java $JVM -cp "$cp" net.sandrakessler.seesaw.bench.PdeReplay \
                voll "$trace" >"$out" 2>/tmp/jgate-voll-$side.err
            rc=$?
            secs=$(awk -v a="$t0" -v b="$(date +%s%N)" 'BEGIN { printf "%.1f", (b - a) / 1e9 }')
            last=$(tail -1 "$out")
            case $last in ende\ *) fin=ja ;; *) fin=nein ;; esac
            echo "pde-voll $name $side ende=$fin rc=$rc zeilen=$(wc -l <"$out" | tr -d ' ') sekunden=$secs"
            echo "pde-voll   $(printf '%s\n' "$last" | cut -c1-400)"
            if [ $fin != ja ]; then
                why="$why ${side}_ohne_ende"
                head -3 /tmp/jgate-voll-$side.err | cut -c1-300 | sed 's/^/pde-voll   /'
            fi
        done
        n=$(wc -l </tmp/jgate-voll-HEAD.out | tr -d ' ')
        if ! cmp -s /tmp/jgate-voll-L0.out /tmp/jgate-voll-HEAD.out; then
            at=$(awk 'FNR == NR { a[FNR] = $0; n = FNR; next }
                { if (!(FNR in a) || a[FNR] != $0) { print FNR; found = 1; exit } }
                END { if (!found && FNR < n) print FNR + 1 }' \
                /tmp/jgate-voll-L0.out /tmp/jgate-voll-HEAD.out)
            [ -n "$at" ] || at=1
            why="$why ungleich_ab_zeile=$at"
            echo "pde-voll   L0:   $(sed -n "${at}p" /tmp/jgate-voll-L0.out | cut -c1-300)"
            echo "pde-voll   HEAD: $(sed -n "${at}p" /tmp/jgate-voll-HEAD.out | cut -c1-300)"
        fi
        if [ -z "$why" ]; then
            passed=$((passed + 1))
            echo "pde-voll $name VERDIKT=bestanden zeilen=$n"
        else
            failed_records=$((failed_records + 1))
            echo "pde-voll $name VERDIKT=durchgefallen$why zeilen=$n"
        fi
    done
    rc=0
    [ $records -eq 0 ] || [ $failed_records -ne 0 ] && rc=1
    echo "pde-voll VOLL_RC=$rc protokolle=$records bestanden=$passed durchgefallen=$failed_records"
    [ $rc -eq 0 ] || status=1
}

DIFF_REF=${SEESAW_DIFF_REF:-/root/seesaw-java/ref}
DIFF_N=1500
DIFF_BARGS=""
DIFF_WORLDS="followup family predicates predicates_no_equals"
DIFF_DIRS="gerichtet ungerichtet"
DIFF_MAIN=net.sandrakessler.seesaw.engine.DiffDump
DIFF_PKG=src/test/java/net/sandrakessler/seesaw/engine
diff_ready=""
diff_total=0; diff_passed=0; diff_failed=0

# Baut beide Seiten einmal je Aufruf: diese Seite per test-compile (Klassenpfad
# /tmp/jgate-diff.cp), die Referenz per compile plus javac der beiden Testdateien (Klassenpfad
# /tmp/jgate-diffref.cp). Rückgabe ungleich 0 bei Fehler, mit Zeilen "diff …".
diff_build() {
    [ "$diff_ready" = ok ] && return 0
    if ! build_suite diff; then
        echo "diff BUILD_FEHLER Seite=dieser Baum"
        return 1
    fi
    rj=$DIFF_REF/java
    if [ ! -f "$rj/pom.xml" ] || [ ! -d "$rj/src/main" ]; then
        echo "diff REF_FEHLT $rj (git archive v2.0.2 java | ssh codepit tar -x -C $DIFF_REF)"
        return 1
    fi
    if [ -e "$rj/src/main/java/net/sandrakessler/seesaw/engine/AnchorIndex.java" ]; then
        echo "diff REF_ZU_NEU $rj enthält AnchorIndex, ist nicht v2.0.2"
        return 1
    fi
    mkdir -p "$rj/$DIFF_PKG"
    # Die angewandten Schlüssel liest Worlds.dump über e.appliedKeys(), v2.0.2 führt sie als Menge
    # e.applied.
    sed -e 's/new Engine(rules, useIndex)/new Engine(rules)/' -e '/if (indexSeed) e.seedFactor = 0;/d' \
        -e 's/e\.appliedKeys()/e.applied/' \
        "$J/$DIFF_PKG/Worlds.java" >"$rj/$DIFF_PKG/Worlds.java"
    if grep -q -e 'seedFactor' -e 'new Engine(rules, ' "$rj/$DIFF_PKG/Worlds.java"; then
        echo "diff BUILD_FEHLER Worlds.java der Referenz: Index-Argument nicht entfernt"
        return 1
    fi
    if grep -q 'appliedKeys' "$rj/$DIFF_PKG/Worlds.java"; then
        echo "diff BUILD_FEHLER Worlds.java der Referenz: appliedKeys() nicht ersetzt"
        return 1
    fi
    cp "$J/$DIFF_PKG/DiffDump.java" "$rj/$DIFF_PKG/DiffDump.java"
    # jackson-databind wie in diesem Baum (2.17.2 liegt nicht im lokalen Repository). Die Engine
    # selbst bleibt der Stand v2.0.2.
    jv=$(sed -n '/<artifactId>jackson-databind<\/artifactId>/{n;s/.*<version>\(.*\)<\/version>.*/\1/p;}' "$J/pom.xml")
    sed -i "/<artifactId>jackson-databind<\/artifactId>/{n;s|<version>.*</version>|<version>$jv</version>|;}" "$rj/pom.xml"
    cd "$rj" || return 1
    if ! { timeout 200 mvn -q -o -B compile && deps_cp /tmp/jgate-diffref.cp; } >/tmp/jgate-diff-refbuild.log 2>&1; then
        grep -E '^\[ERROR\]' /tmp/jgate-diff-refbuild.log | head -10 | sed 's/^/diff   /'
        echo "diff BUILD_FEHLER Seite=Referenz"
        return 1
    fi
    rm -rf target/diff-classes && mkdir -p target/diff-classes
    if ! javac -d target/diff-classes -cp "target/classes:$(cat /tmp/jgate-diffref.cp)" \
            "$DIFF_PKG/Worlds.java" "$DIFF_PKG/DiffDump.java" >/tmp/jgate-diff-refjavac.log 2>&1; then
        head -10 /tmp/jgate-diff-refjavac.log | sed 's/^/diff   /'
        echo "diff BUILD_FEHLER Seite=Referenz (javac)"
        return 1
    fi
    cd "$J" || return 1
    diff_ready=ok
}

# diff_side <b|r> <ausgabe> <fehlerausgabe> <java-argument ...>: ein Lauf von DiffDump.
diff_side() {
    side=$1; o=$2; e=$3; shift 3
    if [ "$side" = b ]; then
        (cd "$J" && timeout 280 java $DIFF_BARGS -cp "target/classes:target/test-classes:$(cat /tmp/jgate-diff.cp)" \
            $DIFF_MAIN "$@") >"$o" 2>"$e"
    else
        (cd "$DIFF_REF/java" && timeout 280 java -cp "target/diff-classes:target/classes:$(cat /tmp/jgate-diffref.cp)" \
            $DIFF_MAIN "$@") >"$o" 2>"$e"
    fi
}

# Zeile, Welle und Seed der ersten Abweichung zweier Abzüge im Modus voll.
diff_first_line() {
    cmpout=$(cmp "$1" "$2" 2>&1)
    L=$(printf '%s\n' "$cmpout" | sed -n 's/.*line \([0-9][0-9]*\).*/\1/p' | head -1)
    [ -n "$L" ] || { echo "welle=? (cmp: $cmpout)"; return; }
    case $cmpout in *EOF*) L=$((L + 1)) ;; esac
    f=$1; [ "$(wc -l <"$1")" -ge "$L" ] || f=$2
    awk -v L="$L" 'NR <= L && /^@@ / { h = $0 } NR == L { print h; exit }' "$f" | sed 's/^@@ //'
    echo "    dieser Baum: $(sed -n "${L}p" "$1" | cut -c1-300)"
    echo "    v2.0.2:      $(sed -n "${L}p" "$2" | cut -c1-300)"
}

# diff_piece <welt> <richtung> <n>: setzt P_SAME, P_FIRST (Seed oder leer), P_WHY, P_INFO.
diff_piece() {
    w=$1; d=$2; n=$3; seeds=${4:-$3}
    P_SAME=0; P_FIRST=""; P_WHY=""; P_INFO=""; P_WAVES=0; P_APPLIED=0
    t0=$(date +%s%N)
    diff_side b /tmp/jgate-diff-b.out /tmp/jgate-diff-b.err "$w" "$d" "$seeds"; brc=$?
    diff_side r /tmp/jgate-diff-r.out /tmp/jgate-diff-r.err "$w" "$d" "$seeds"; rrc=$?
    P_SECS=$(awk -v a="$t0" -v b="$(date +%s%N)" 'BEGIN { printf "%.1f", (b - a) / 1e9 }')
    [ $brc -ne 0 ] && { P_WHY="$P_WHY lauf_dieser_baum_rc=$brc"; head -3 /tmp/jgate-diff-b.err | cut -c1-300; }
    [ $rrc -ne 0 ] && { P_WHY="$P_WHY lauf_referenz_rc=$rrc"; head -3 /tmp/jgate-diff-r.err | cut -c1-300; }
    # Zeilen szenario=… beider Seiten nach Zeilennummer vergleichen.
    res=$(awk -v n="$n" '
        FNR == NR { if ($1 == "szenario") r[++nr] = $0; else if ($1 == "summe") rs = $0; next }
        $1 == "szenario" {
            nb++
            if (nb <= nr && $0 == r[nb]) same++
            else if (first == "") { first = $2; sub(/^seed=/, "", first) }
        }
        $1 == "summe" { bs = $0 }
        END {
            w = bs; sub(/.*wellen=/, "", w); sub(/ .*/, "", w)
            a = bs; sub(/.*angewandt=/, "", a)
            print nb + 0, nr + 0, same + 0, first, w + 0, a + 0, (bs == rs && bs != "") ? "gleich" : "ungleich"
        }' /tmp/jgate-diff-r.out /tmp/jgate-diff-b.out)
    set -- $res
    nb=$1; nr=$2; P_SAME=$3
    if [ $# -eq 7 ]; then P_FIRST=$4; P_WAVES=$5; P_APPLIED=$6; sums=$7; else P_WAVES=$4; P_APPLIED=$5; sums=$6; fi
    [ "$nb" -ne "$n" ] && P_WHY="$P_WHY szenarien_dieser_baum=$nb"
    [ "$nr" -ne "$n" ] && P_WHY="$P_WHY szenarien_referenz=$nr"
    [ "$P_SAME" -ne "$n" ] && P_WHY="$P_WHY abweichend=$((n - P_SAME))"
    [ "$sums" != gleich ] && P_WHY="$P_WHY summe_$sums"
    [ "$P_WAVES" -eq 0 ] && P_WHY="$P_WHY wellen=0"
    [ "$P_APPLIED" -eq 0 ] && P_WHY="$P_WHY angewandt=0"
    # Ohne Abweichung in den Szenarien, aber ungleich oder zu wenig: ohne erste Abweichung melden.
    if [ -n "$P_FIRST" ]; then
        diff_side b /tmp/jgate-diff-bv.out /tmp/jgate-diff-bv.err "$w" "$d" "$seeds" voll "$P_FIRST"
        diff_side r /tmp/jgate-diff-rv.out /tmp/jgate-diff-rv.err "$w" "$d" "$seeds" voll "$P_FIRST"
        P_INFO=$(diff_first_line /tmp/jgate-diff-bv.out /tmp/jgate-diff-rv.out)
    fi
}

# Ein Stück: Zeilen DIFF und VERDIKT.
diff_one() {
    w=$1; d=$2; seeds=$DIFF_N; n=$DIFF_N
    case $d in
        *@*)
            seeds=${d#*@}; d=${d%%@*}
            n=$(printf '%s' "$seeds" | awk -F- '$1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ && $2 >= $1 { print $2 - $1 + 1 }')
            if [ -z "$n" ]; then echo "diff $w/$d VERDIKT=durchgefallen Bereich=$seeds ungültig"; diff_total=$((diff_total + 1)); diff_failed=$((diff_failed + 1)); return; fi
            ;;
    esac
    diff_total=$((diff_total + 1))
    diff_piece "$w" "$d" "$n" "$seeds"
    where=keine
    [ -n "$P_FIRST" ] && where="$w/$d/seed=$P_FIRST/$(printf '%s\n' "$P_INFO" | head -1 | sed 's/^seed=[0-9]* //')"
    echo "diff $w/$d DIFF n=$n seeds=$seeds gleich=$P_SAME erste_abweichung=$where wellen=$P_WAVES angewandt=$P_APPLIED sekunden=$P_SECS"
    [ -n "$P_FIRST" ] && printf '%s\n' "$P_INFO" | sed 1d
    if [ -z "$P_WHY" ]; then
        diff_passed=$((diff_passed + 1))
        echo "diff $w/$d VERDIKT=bestanden"
    else
        diff_failed=$((diff_failed + 1))
        echo "diff $w/$d VERDIKT=durchgefallen$P_WHY"
    fi
}

# Alle Seeds einer Welt und Richtung. family/ungerichtet läuft in Stücken zu 250 Seeds, ein Lauf
# mit allen 1500 Seeds braucht auf codepit mehr als 10 Minuten (und überschreitet die Zeitgrenze
# je Seite von diff_side).
diff_all_seeds() {
    if [ "$1/$2" = family/ungerichtet ] && [ "$DIFF_N" -gt 250 ]; then
        lo=1
        while [ $lo -le "$DIFF_N" ]; do
            hi=$((lo + 249)); [ $hi -gt "$DIFF_N" ] && hi=$DIFF_N
            diff_one "$1" "$2@$lo-$hi"
            lo=$((hi + 1))
        done
    else
        diff_one "$1" "$2"
    fi
}

# Gegenprobe: dieser Baum verändert eine Zeile, der Vergleich muss genau dieses Szenario melden.
diff_probe() {
    diff_total=$((diff_total + 1))
    DIFF_BARGS="-Dseesaw.diff.mutate=7"
    diff_piece followup gerichtet 40
    DIFF_BARGS=""
    mut=$(sed -n 's/^mutiert seed=\([0-9][0-9]*\)$/\1/p' /tmp/jgate-diff-b.err | head -1)
    where=keine
    [ -n "$P_FIRST" ] && where="followup/gerichtet/seed=$P_FIRST/$(printf '%s\n' "$P_INFO" | head -1 | sed 's/^seed=[0-9]* //')"
    echo "diff gegenprobe DIFF n=40 gleich=$P_SAME mutiert_seed=${mut:-keiner} erste_abweichung=$where"
    [ -n "$P_FIRST" ] && printf '%s\n' "$P_INFO" | sed 1d
    if [ -n "$mut" ] && [ "$P_FIRST" = "$mut" ] && [ "$P_SAME" -eq 39 ]; then
        diff_passed=$((diff_passed + 1))
        echo "diff gegenprobe VERDIKT=bestanden (die veränderte Zeile wurde gemeldet)"
    else
        diff_failed=$((diff_failed + 1))
        echo "diff gegenprobe VERDIKT=durchgefallen (veränderte Zeile nicht genau gemeldet: mutiert=${mut:-keiner} gemeldet=${P_FIRST:-keine} gleich=$P_SAME)"
    fi
}

# diff [<stück>]: leer = alle Stücke und die Gegenprobe.
diff_part() {
    spec=$1
    diff_total=0; diff_passed=0; diff_failed=0
    if [ "$DIFF_N" -le 0 ]; then
        echo "diff DIFF_RC=1 n=$DIFF_N ungültig"
        status=1
        return
    fi
    if ! diff_build; then
        echo "diff DIFF_RC=1"
        status=1
        return
    fi
    case $spec in
        "") for w in $DIFF_WORLDS; do for d in $DIFF_DIRS; do diff_all_seeds $w $d; done; done; diff_probe ;;
        gegenprobe) diff_probe ;;
        */*@*) diff_one "${spec%%/*}" "${spec#*/}" ;;
        */*) diff_all_seeds "${spec%%/*}" "${spec#*/}" ;;
        *) for d in $DIFF_DIRS; do diff_all_seeds "$spec" $d; done ;;
    esac
    rc=0
    [ $diff_total -eq 0 ] || [ $diff_failed -ne 0 ] && rc=1
    echo "diff DIFF_RC=$rc stuecke=$diff_total bestanden=$diff_passed durchgefallen=$diff_failed"
    [ $rc -eq 0 ] || status=1
}

[ $# -gt 0 ] || set -- mvn fp pde diff pde-voll
while [ $# -gt 0 ]; do
    part=$1; shift
    case $part in
        mvn) mvn_one 1; mvn_one 2; mvn_one 3 ;;
        mvn1) mvn_one 1 ;;
        mvn2) mvn_one 2 ;;
        mvn3) mvn_one 3 ;;
        fp) fp ;;
        pde) pde ;;
        pde:*) pde "${part#pde:}" ;;
        pde-voll) pde_full ;;
        pde-voll:*) pde_full "${part#pde-voll:}" ;;
        diff|diff:*)
            # Eine Zahl dahinter ist das n, sonst gilt 1500.
            DIFF_N=1500
            if [ $# -gt 0 ]; then case $1 in ''|*[!0-9]*) ;; *) DIFF_N=$1; shift ;; esac; fi
            diff_part "$(case $part in diff) ;; *) printf '%s' "${part#diff:}" ;; esac)" ;;
        *) echo "$part unbekannter Teil"; exit 2 ;;
    esac
done
exit $status
