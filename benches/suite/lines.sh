#!/bin/sh
# Where the self time of one function goes (Linux, perf, addr2line).
#
#   lines.sh <cell> <function pattern> [lines] [event]
#
# `hot.sh` gives the self share per function as the binary has it: what
# the compiler inlined into a function counts as that function. This
# script takes the samples of the measured section whose innermost
# frame matches the pattern (an awk regular expression on the symbol,
# e.g. `step_with_limit`) and resolves their addresses with the inline
# information of the debug data. Three tables, in percent of the
# samples of the section: per inlined function (the innermost one at
# the address), per source line of that function, and per source line
# of the function the code was inlined into (the statement that caused
# the work). The event is instructions:u by default; cache-misses:u
# shows which lines wait for main memory.
#
# The run has address space randomisation switched off (`setarch -R`),
# so that a sample address minus the load address is the address in
# the file.
#
# `SUITE_FEATURES` (a comma list) builds the suite with those features.
set -eu
cell=$1
pattern=$2
lines=${3:-30}
event=${4:-instructions:u}
case $event in
instructions*) rate="-c 1000003" ;;
cache-misses*) rate="-c 10007" ;;
*) rate="-F 4000" ;;
esac
cd "$(dirname "$0")/../.."
# The binary of this set of features, from cargo's own output: a set
# that is built already is not linked again, so the newest file in the
# directory may belong to another set.
bin=$(CARGO_PROFILE_BENCH_DEBUG=1 CARGO_TARGET_DIR=target/prof \
    cargo bench -q --bench suite ${SUITE_FEATURES:+--features "$SUITE_FEATURES"} --no-run \
    --message-format=json 2>/dev/null |
    grep -o '"executable":"[^"]*/suite-[^"]*"' | tail -1 | sed 's/^"executable":"//; s/"$//')
[ -x "$bin" ] || { echo "the suite did not build" >&2; exit 1; }
tmp=$(mktemp -d /tmp/suite-lines.XXXXXX)
perf record -q --call-graph dwarf,16384 $rate -o "$tmp/data" -e "$event" \
    setarch "$(uname -m)" -R "$bin" run "$cell" --samples 1 >/dev/null 2>&1
# Load address of the binary in that run.
base=$(perf script -i "$tmp/data" --show-mmap-events 2>/dev/null |
    awk -v bin="$(basename "$bin")" '
        /PERF_RECORD_MMAP/ && index($0, bin) {
            if (match($0, /\[0x[0-9a-f]+\(0x[0-9a-f]+\) @ (0x)?[0-9a-f]+/)) {
                s = substr($0, RSTART + 1, RLENGTH - 1)
                split(s, a, /[(@ ]+/)
                if (a[3] == "0" || a[3] == "0x0") { print a[1]; exit }
            }
        }')
[ -n "$base" ] || { echo "load address not found" >&2; exit 1; }
# Addresses of the innermost frames that match, with their counts.
perf script -i "$tmp/data" --no-inline -F ip,sym 2>/dev/null |
    awk -v pat="$pattern" -v total="$tmp/total" '
    function flush(   i, hit) {
        if (n == 0) return
        hit = 0
        for (i = 1; i <= n; i++) if (fr[i] ~ /measure::section/) { hit = 1; break }
        if (hit) {
            insec++
            if (fr[1] ~ pat) print ip
        }
        n = 0
    }
    /^[ \t]*$/ { flush(); next }
    { n++; fr[n] = $0; if (n == 1) ip = $1 }
    END { flush(); print insec + 0 > total }' | sort | uniq -c >"$tmp/ips"
insec=$(cat "$tmp/total")
: >"$tmp/addrs"
while read -r count ip; do
    printf '%s 0x%x\n' "$count" $((0x$ip - base)) >>"$tmp/addrs"
done <"$tmp/ips"
cut -d' ' -f2 "$tmp/addrs" | addr2line -e "$bin" -a -i -f -C >"$tmp/resolved"
awk -v lines="$lines" -v cell="$cell" -v pat="$pattern" -v insec="$insec" -v event="$event" '
    FNR == NR { count[$2] = $1; next }
    function clean(loc) {
        sub(/ \(discriminator [0-9]+\)$/, "", loc)
        sub(/^.*\/src\//, "src/", loc)
        return loc
    }
    function outer() {
        if (have) byouter[last] += count[addr]
        have = 0
    }
    /^0x/ {
        outer()
        addr = $1
        sub(/^0x0*/, "0x", addr)
        state = 1
        next
    }
    # The frames of one address: function and location in turn, the
    # innermost inlined function first, the function of the binary last.
    state == 1 { fn = $0; state = 2; next }
    state == 2 {
        gsub(/<impl [^>]*>/, "", fn)
        c = count[addr]
        byfn[substr(fn, 1, 90)] += c
        byline[clean($0)] += c
        matched += c
        last = clean($0)
        have = 1
        state = 3
        next
    }
    state == 3 { state = 4; next }
    state == 4 { last = clean($0); state = 3; next }
    END {
        outer()
        if (insec == 0) { print "no samples in the section"; exit }
        printf "== %s, %s, self of /%s/: %.2f %% of the section (%d of %d samples)\n",
            cell, event, pat, 100 * matched / insec, matched, insec
        cmd = "sort -t\"\t\" -k1,1nr | head -" lines
        print "-- innermost inlined function, % of section"
        for (f in byfn) printf "%6.2f\t%s\n", 100 * byfn[f] / insec, f | cmd
        close(cmd)
        print "-- source line, % of section"
        for (l in byline) printf "%6.2f\t%s\n", 100 * byline[l] / insec, l | cmd
        close(cmd)
        print "-- source line of the function it was inlined into, % of section"
        for (l in byouter) printf "%6.2f\t%s\n", 100 * byouter[l] / insec, l | cmd
        close(cmd)
    }' "$tmp/addrs" "$tmp/resolved"
rm -rf "$tmp"
