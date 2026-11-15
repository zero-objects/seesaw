#!/bin/sh
# Hot paths of the measured section of one cell (Linux, perf).
#
#   hot.sh <cell> [lines] [event]
#
# Samples one run with call graphs (event: instructions:u by default,
# cycles:u for time, cache-misses:u for the memory accesses that miss
# the last-level cache) and keeps only the samples whose call chain passes
# through `suite::measure::section`. Two tables, in percent of those
# samples: inclusive per function (the function is somewhere on the
# chain below the section), and self per function (the function is the
# top of the chain). Model construction, rule loading and the
# fingerprint are not in the tables.
#
# `SUITE_FEATURES` (a comma list) builds the suite with those features.
set -eu
cell=$1
lines=${2:-40}
event=${3:-instructions:u}
cd "$(dirname "$0")/../.."
CARGO_PROFILE_BENCH_DEBUG=1 CARGO_TARGET_DIR=target/prof \
    cargo bench -q --bench suite ${SUITE_FEATURES:+--features "$SUITE_FEATURES"} --no-run 2>/dev/null
bin=$(ls -t target/prof/release/deps/suite-* | grep -v '\.d$' | head -1)
data=$(mktemp /tmp/suite-perf.XXXXXX)
case $event in
instructions*) rate="-c 1000003" ;;
cache-misses*) rate="-c 10007" ;;
*) rate="-F 4000" ;;
esac
perf record -q --call-graph dwarf,16384 $rate -o "$data" -e "$event" \
    "$bin" run "$cell" --samples 1 >/dev/null 2>&1
perf script -i "$data" --no-inline -F ip,sym 2>/dev/null |
    awk -v lines="$lines" -v cell="$cell" -v event="$event" '
    function flush(   i, hit, seen, f) {
        if (n == 0) return
        total++
        hit = 0
        for (i = 1; i <= n; i++) if (fr[i] ~ /measure::section/) { hit = i; break }
        if (hit) {
            insec++
            self[fr[1]]++
            for (i = 1; i < hit; i++) {
                f = fr[i]
                if (!(f in seen)) { seen[f] = 1; incl[f]++ }
            }
        }
        n = 0
    }
    /^[ \t]*$/ { flush(); next }
    {
        line = $0
        sub(/^[ \t]+/, "", line)
        sub(/^[0-9a-f]+[ \t]+/, "", line)
        sub(/[ \t]+\([^)]*\)[ \t]*$/, "", line)
        sub(/\+0x[0-9a-f]+$/, "", line)
        gsub(/<impl [^>]*>/, "", line)
        sub(/::h[0-9a-f]{16}$/, "", line)
        sub(/\.llvm\.[0-9]+$/, "", line)
        if (line == "") line = "[unknown]"
        fr[++n] = substr(line, 1, 100)
    }
    END {
        flush()
        printf "== %s, %s: %d of %d samples in the section\n", cell, event, insec, total
        if (insec == 0) exit
        cmd = "sort -t\"\t\" -k1,1nr | head -" lines
        print "-- inclusive, % of section"
        for (f in incl) printf "%6.2f\t%s\n", 100 * incl[f] / insec, f | cmd
        close(cmd)
        print "-- self, % of section"
        for (f in self) printf "%6.2f\t%s\n", 100 * self[f] / insec, f | cmd
        close(cmd)
    }'
rm -f "$data"
