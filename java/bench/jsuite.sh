#!/bin/sh
# jsuite.sh: Messstrecke JavaSuite gegen die Hauptklassen mehrerer Stände (Messleiter).
#
# Die Messstrecke (Paket bench, dazu Langpack und LangWorld aus den Testklassen) wird einmal
# aus dem neuesten Stand gebaut, dem Baum, in dem dieses Skript liegt (auf codepit
# /root/seesaw-java/src, per rsync übertragen). Gefahren wird sie gegen das Haupt-JAR jedes
# Stands (java/src/main). Sie nutzt nur öffentliche API der Hauptklassen. Die paket-privaten
# Zähler der Engine liest sie per Reflexion (bench.Counters), -1 wo ein Stand sie nicht hat.
#
# Teilung zwischen Mac und codepit. Der Mac hat git und baut nichts, codepit baut und misst
# und hat für die übertragenen Stände kein git.
#
#   Mac:
#     sh java/bench/jsuite.sh send <label> <commit>
#         java/ des Stands nach codepit (git archive | ssh tar), nach
#         /root/seesaw-java/stands/<label>/java, der volle Commit in .../<label>/COMMIT
#     sh java/bench/jsuite.sh send-ladder
#         send für jede Zeile von ladder.txt
#
#   codepit, über das Board, jeder Aufruf unter 5 Minuten:
#     sh jsuite.sh bench
#         Messstrecke bauen (test-compile im Baum dieses Skripts), Klassen nach
#         /root/seesaw-java/bench/classes
#     sh jsuite.sh build <label> <commit>
#         Haupt-JAR des gesendeten Stands bauen (Commit muss zu COMMIT passen), nach
#         /root/seesaw-java/jars/<label>.jar, Klassenpfad der Abhängigkeiten nach <label>.cp
#     sh jsuite.sh run [--warmup W] [--reps R] <tsv> <label,...> <zelle>...
#         je Zelle jedes Label einmal, im Wechsel, jeder Lauf ein frischer Prozess. Jeder Lauf
#         hängt eine Zeile an die TSV an. Das Aufteilen auf Jobs unter 5 Minuten liegt beim
#         Aufrufer (z. B. eine Zelle je Aufruf). Ein Lauf wird nach LIMIT Sekunden beendet.
#     sh jsuite.sh ladder [--tsv <tsv>] [<zelle>...]
#         alle Stände aus ladder.txt in drei Runden (ROUNDS), Vorgabe die großen Zellen (CELLS),
#         TSV ladder.tsv. Baut fehlende JARs und die fehlende Messstrecke. Setzt fort, wo die
#         TSV aufhört: in Runde r läuft ein Paar (Label, Zelle), wenn die TSV weniger als r
#         Zeilen davon hat. Ein Aufruf bleibt unter LIMIT Sekunden: einen weiteren Lauf startet
#         er nur, wenn die geschätzte Dauer mal 1,25 noch in die Zeit passt. Schätzung aus
#         <tsv>.dauer: die längste Dauer desselben Paars (Label, Zelle), sonst die zuletzt
#         gemessene Dauer der Zelle. Eine Zelle ohne bekannte Dauer läuft nur als erster Lauf
#         des Aufrufs. Ein Lauf, der an der Zeitgrenze endet, läuft im nächsten Aufruf neu. Dann
#         meldet er "ladder offen=<n>". Erneut aufrufen bis "ladder fertig". Vor dem ersten und
#         nach dem letzten Lauf einer Runde in einem Aufruf hängt er die Ausgabe von uptime
#         (Load Average) an <tsv>.uptime an: Zeit (UTC), runde=<r>, vor oder nach (unterbrochen,
#         wenn der Aufruf mitten in der Runde endet), uptime.
#
# Eine Zelle, die ein Stand nicht kann (…/reseed/…/t4 und t8 ohne Engine.seedWithThreads), schreibt
# die Zeile "<label> <zelle> nicht_verfuegbar <grund>" mit Exit-Code 0. Sie zählt als Lauf, die
# Leiter läuft weiter, jreport.py führt sie getrennt auf.
#
# TSV ohne Pfad liegt in /root/seesaw-java/results/. Jeder erfolgreiche Lauf hängt
# "<label> <zelle> <sekunden>" an <tsv>.dauer an. JVM fest: -Xms8g -Xmx8g -XX:+UseParallelGC,
# Vorgabe --warmup 3 --reps 5. Spalten der TSV: siehe JavaSuite.java.
set -u

HOST=codepit
ROOT=/root/seesaw-java
J=$(cd "$(dirname "$0")/.." && pwd)
LADDER=$J/bench/ladder.txt
JVM="-Xms8g -Xmx8g -XX:+UseParallelGC"
MAIN=net.sandrakessler.seesaw.bench.JavaSuite
ROUNDS=${ROUNDS:-3}
LIMIT=${LIMIT:-270}
BUILD_TIMEOUT=${BUILD_TIMEOUT:-100}
CELLS="langpack/add/k700/n2000 langpack/seed/k700/n2000 langpack/backward/k700/n2000
langpack/add/k700/n20000 langpack/delta-add/k700/n20000/d200
langpack/delta-change/k700/n20000/d200 langpack/delta-delete/k700/n20000/d200
langpack/delta-delete/k700/n20000/d2000 f2p/batch/f20000 f2p/add/f20000
f2p/delta-add/f20000/d1000 f2p/delta-delete/f20000/d1000
langpack/engines/k700/n200/e34 langpack/index/k700/e100
langpack/reseed/k700/n20000/t1 langpack/reseed/k700/n20000/t4 langpack/reseed/k700/n20000/t8
f2p/reseed/f20000/t1 f2p/reseed/f20000/t4 f2p/reseed/f20000/t8"

die() { echo "$*" >&2; exit 1; }

usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0" >&2
    exit 2
}

check_label() {
    case $1 in
        ''|*[!A-Za-z0-9._-]*) die "ungültiges Label: '$1'" ;;
    esac
}

ladder_lines() { grep -v '^[[:space:]]*#' "$LADDER" | grep -v '^[[:space:]]*$'; }

tsv_path() {
    case $1 in
        /*) echo "$1" ;;
        *) mkdir -p $ROOT/results && echo "$ROOT/results/$1" ;;
    esac
}

# --- Mac ---

send() {
    label=$1; commit=$2
    check_label "$label"
    repo=$(git -C "$J" rev-parse --show-toplevel 2>/dev/null) || die "send: kein git-Repository (auf dem Mac aufrufen)"
    sha=$(git -C "$repo" rev-parse --verify --quiet "$commit^{commit}") || die "send: unbekannter Stand $commit"
    git -C "$repo" archive "$sha" java | ssh $HOST "rm -rf $ROOT/stands/$label && mkdir -p $ROOT/stands/$label && tar -x -C $ROOT/stands/$label && echo $sha > $ROOT/stands/$label/COMMIT" \
        || die "send $label: Übertragung fehlgeschlagen"
    echo "gesendet $label $sha"
}

# --- codepit ---

bench() {
    (
        cd "$J" || exit 1
        timeout 200 mvn -q -o -B test-compile >/tmp/jsuite-bench.log 2>&1
        rc=$?
        if [ $rc -ne 0 ]; then
            echo "BENCH_BUILD_FAILED rc=$rc"
            grep -E '^\[ERROR\]' /tmp/jsuite-bench.log | head -20
            exit 1
        fi
        mkdir -p $ROOT/bench && rm -rf $ROOT/bench/classes && cp -r target/test-classes $ROOT/bench/classes || exit 1
        echo "bench gebaut aus $J, $(find $ROOT/bench/classes/net/sandrakessler/seesaw/bench -name '*.class' | wc -l) Klassen im Paket bench"
    )
}

# Klassenpfad der Laufzeit-Abhängigkeiten nach $1 (im Verzeichnis eines pom.xml). Offline.
# Fehlt dem lokalen Repository das Plugin, einmal ohne -o.
deps_cp() {
    timeout 60 mvn -q -o -B dependency:build-classpath -DincludeScope=runtime -Dmdep.outputFile="$1" \
        || timeout 60 mvn -q -B dependency:build-classpath -DincludeScope=runtime -Dmdep.outputFile="$1"
}

build() {
    label=$1; commit=$2
    check_label "$label"
    dir=$ROOT/stands/$label
    [ -f "$dir/COMMIT" ] || { echo "BUILD_FAILED $label: $dir fehlt, erst auf dem Mac: sh java/bench/jsuite.sh send $label $commit"; return 1; }
    sha=$(cat "$dir/COMMIT")
    case $sha in
        "$commit"*) ;;
        *) echo "BUILD_FAILED $label: gesendet ist $sha, nicht $commit"; return 1 ;;
    esac
    (
        cd "$dir/java" || exit 1
        rm -rf target
        timeout $BUILD_TIMEOUT mvn -q -o -B -Dmaven.test.skip=true package >/tmp/jsuite-build-$label.log 2>&1 \
            && deps_cp $dir/deps.cp >>/tmp/jsuite-build-$label.log 2>&1
        rc=$?
        if [ $rc -ne 0 ]; then
            echo "BUILD_FAILED $label rc=$rc"
            grep -E '^\[ERROR\]' /tmp/jsuite-build-$label.log | head -20
            exit 1
        fi
        jar=$(ls target/*.jar 2>/dev/null | head -1)
        [ -n "$jar" ] || { echo "BUILD_FAILED $label: kein JAR in target/"; exit 1; }
        mkdir -p $ROOT/jars
        cp "$jar" $ROOT/jars/$label.jar && cp $dir/deps.cp $ROOT/jars/$label.cp && echo "$sha" > $ROOT/jars/$label.commit || exit 1
        echo "gebaut $label $sha $(basename "$jar")"
    )
}

# Ein Lauf: tsv label zelle warmup reps sekunden. Rückgabe wie java, 124 bei Zeitgrenze.
run_one() {
    jar=$ROOT/jars/$2.jar
    [ -f "$jar" ] || { echo "RUN_FAILED $2 $3: $jar fehlt, erst build"; return 1; }
    t0=$(date +%s)
    timeout "$6" java $JVM -cp "$jar:$ROOT/bench/classes:$(cat $ROOT/jars/$2.cp)" $MAIN \
        run "$3" --label "$2" --warmup "$4" --reps "$5" --tsv "$1" 2>/tmp/jsuite-run.err
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "RUN_FAILED $2 $3 rc=$rc"
        head -5 /tmp/jsuite-run.err
        return $rc
    fi
    printf '%s\t%s\t%s\n' "$2" "$3" $(( $(date +%s) - t0 )) >> "$1.dauer"
}

cmd_run() {
    w=3; r=5
    while [ $# -gt 0 ]; do
        case $1 in
            --warmup) w=$2; shift 2 ;;
            --reps) r=$2; shift 2 ;;
            *) break ;;
        esac
    done
    [ $# -ge 3 ] || usage
    tsv=$(tsv_path "$1"); labels=$(echo "$2" | tr ',' ' '); shift 2
    [ -d $ROOT/bench/classes ] || die "run: $ROOT/bench/classes fehlt, erst sh jsuite.sh bench"
    failed=0
    for cell in "$@"; do
        for label in $labels; do
            run_one "$tsv" "$label" "$cell" "$w" "$r" "$LIMIT" || failed=$((failed + 1))
        done
    done
    echo "run fertig, fehlgeschlagen=$failed, Zeilen in $(basename "$tsv"): $(wc -l < "$tsv" 2>/dev/null || echo 0)"
    [ $failed -eq 0 ]
}

# Zahl der Zeilen von (Label, Zelle) in der TSV.
lines_of() {
    [ -f "$1" ] || { echo 0; return; }
    awk -F'\t' -v l="$2" -v c="$3" '$1 == l && $2 == c { n++ } END { print n + 0 }' "$1"
}

# Geschätzte Dauer (s) eines Laufs (Label, Zelle): die längste Dauer desselben Paars, sonst
# die zuletzt gemessene Dauer der Zelle, 0 wenn die Zelle keine Dauer hat.
estimate() {
    [ -f "$1.dauer" ] || { echo 0; return; }
    awk -F'\t' -v l="$2" -v c="$3" '$2 == c { last = $3; if ($1 == l && $3 > m) m = $3 }
        END { print (m > 0 ? m : last) + 0 }' "$1.dauer"
}

# Zahl der noch fehlenden Läufe der Leiter: tsv, Zellen, Labels.
open_runs() {
    open=0
    r=1
    while [ $r -le "$ROUNDS" ]; do
        for c in $2; do
            for l in $3; do
                [ "$(lines_of "$1" "$l" "$c")" -lt $r ] && open=$((open + 1))
            done
        done
        r=$((r + 1))
    done
    echo $open
}

cmd_ladder() {
    start=$(date +%s)
    tsv=$(tsv_path ladder.tsv)
    if [ "${1:-}" = "--tsv" ]; then
        [ $# -ge 2 ] || usage
        tsv=$(tsv_path "$2"); shift 2
    fi
    if [ $# -gt 0 ]; then cells=$*; else cells=$CELLS; fi
    labels=$(ladder_lines | awk '{ print $1 }')
    [ -n "$labels" ] || die "ladder: keine Stände in $LADDER"
    elapsed() { echo $(( $(date +%s) - start )); }
    # Load Average zu einer Runde: runde, vor|nach|unterbrochen.
    load() {
        printf '%s\trunde=%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" "$2" "$(uptime)" >> "$tsv.uptime"
    }
    stop() {
        [ "${began:-0}" -eq 1 ] && load "$round" unterbrochen
        echo "ladder offen=$(open_runs "$tsv" "$cells" "$labels") Zeilen in $(basename "$tsv"): $(wc -l < "$tsv" 2>/dev/null || echo 0)"
        exit 0
    }
    # Arbeit in diesem Aufruf. Danach startet nur, was nach Schätzung noch passt.
    worked=0

    if [ ! -d $ROOT/bench/classes ]; then
        bench || exit 1
        worked=1
    fi
    for label in $labels; do
        commit=$(ladder_lines | awk -v l="$label" '$1 == l { print $2 }')
        [ -f $ROOT/stands/$label/COMMIT ] || die "ladder: Stand $label fehlt, erst auf dem Mac: sh java/bench/jsuite.sh send-ladder"
        if [ ! -f $ROOT/jars/$label.jar ] || ! cmp -s $ROOT/jars/$label.commit $ROOT/stands/$label/COMMIT; then
            [ $worked -eq 1 ] && [ $(( $(elapsed) + BUILD_TIMEOUT )) -gt "$LIMIT" ] && stop
            build "$label" "$commit" || exit 1
            worked=1
        fi
    done

    round=1
    while [ $round -le "$ROUNDS" ]; do
        began=0
        for cell in $cells; do
            for label in $labels; do
                [ "$(lines_of "$tsv" "$label" "$cell")" -lt $round ] || continue
                if [ $worked -eq 1 ]; then
                    est=$(estimate "$tsv" "$label" "$cell")
                    [ "$est" -eq 0 ] && stop
                    [ $(( $(elapsed) + est * 5 / 4 )) -gt "$LIMIT" ] && stop
                fi
                if [ $began -eq 0 ]; then
                    load "$round" vor
                    began=1
                fi
                run_one "$tsv" "$label" "$cell" 3 5 $(( LIMIT - $(elapsed) ))
                rc=$?
                if [ $rc -eq 124 ] && [ $worked -eq 1 ]; then
                    echo "ladder: $label $cell an der Zeitgrenze beendet, läuft im nächsten Aufruf neu"
                    stop
                fi
                [ $rc -eq 0 ] || exit 1
                worked=1
            done
        done
        [ $began -eq 1 ] && load "$round" nach
        round=$((round + 1))
    done
    echo "ladder fertig, Zeilen in $(basename "$tsv"): $(wc -l < "$tsv")"
}

[ $# -ge 1 ] || usage
cmd=$1; shift
case $cmd in
    send) [ $# -eq 2 ] || usage; send "$1" "$2" ;;
    send-ladder)
        ladder_lines | while read -r label commit; do send "$label" "$commit" || exit 1; done ;;
    bench) bench ;;
    build) [ $# -eq 2 ] || usage; build "$1" "$2" ;;
    run) cmd_run "$@" ;;
    ladder) cmd_ladder "$@" ;;
    *) usage ;;
esac
