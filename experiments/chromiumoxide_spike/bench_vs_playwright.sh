#!/usr/bin/env bash
# Parity head-to-head: chromiumoxide_spike vs playwright-python.
# Both point at the same chrome-headless-shell binary.
# Same viewport, same timeout, same URL list, same concurrency.
# 3-run median for each config.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CS="$HERE/target/release/chromiumoxide_spike"
PW="$HERE/playwright_bench.py"
VPY=/home/aus/PycharmProjects/blazeweb/.venv/bin/python
HS=/home/aus/PycharmProjects/blazeweb/experiments/cef_spike/.cache/chrome-headless-shell-linux64/chrome-headless-shell
URL_FILE="${1:-/tmp/urls_stable.txt}"
OUT_BASE="$HERE/bench_vs_playwright"
rm -rf "$OUT_BASE" && mkdir -p "$OUT_BASE"

[[ -x "$CS" ]] || { echo "FAIL: $CS not found" >&2; exit 1; }
[[ -x "$HS" ]] || { echo "FAIL: $HS not found" >&2; exit 1; }
[[ -f "$PW" ]] || { echo "FAIL: $PW not found" >&2; exit 1; }

N=$(grep -cvE '^(#|$)' "$URL_FILE")
echo ">> Parity bench: $N URLs, chrome-headless-shell 148.0.7778.56"

run_cs() {
    local par="$1" mode="$2"
    local walls=()
    for run in 1 2 3; do
        local out="$OUT_BASE/cs_P${par}_${mode}_r${run}"
        rm -rf "$out"
        local tlog="$out.time"
        /usr/bin/time -o "$tlog" -f "%e" $CS --chrome $HS --out-dir "$out" --concurrency $par --mode $mode --timeout-secs 20 < "$URL_FILE" > "$out.json" 2> "$out.stderr"
        walls+=("$(cat $tlog)")
    done
    local med=$(python3 -c "import statistics; print(f'{statistics.median([${walls[0]},${walls[1]},${walls[2]}]):.2f}')")
    local mn=$(python3 -c "print(f'{min([${walls[0]},${walls[1]},${walls[2]}]):.2f}')")
    local ok=$(grep -c '"ok":true' "$OUT_BASE/cs_P${par}_${mode}_r1.json")
    local rate=$(python3 -c "print(f'{$ok/$med:.2f}')")
    printf "  %-40s  wall med=%5ss min=%5ss  ok=%s/%s  rate=%5s URL/s\n" "chromiumoxide P=$par mode=$mode" "$med" "$mn" "$ok" "$N" "$rate"
}

run_pw() {
    local par="$1" mode="$2"
    local walls=()
    for run in 1 2 3; do
        local out="$OUT_BASE/pw_P${par}_${mode}_r${run}"
        rm -rf "$out"
        local tlog="$out.time"
        /usr/bin/time -o "$tlog" -f "%e" $VPY $PW --chrome $HS --out-dir "$out" --concurrency $par --mode $mode --timeout-secs 20 < "$URL_FILE" > "$out.json" 2> "$out.stderr"
        walls+=("$(cat $tlog)")
    done
    local med=$(python3 -c "import statistics; print(f'{statistics.median([${walls[0]},${walls[1]},${walls[2]}]):.2f}')")
    local mn=$(python3 -c "print(f'{min([${walls[0]},${walls[1]},${walls[2]}]):.2f}')")
    local ok=$(grep -c '"ok":true' "$OUT_BASE/pw_P${par}_${mode}_r1.json")
    local rate=$(python3 -c "print(f'{$ok/$med:.2f}')")
    printf "  %-40s  wall med=%5ss min=%5ss  ok=%s/%s  rate=%5s URL/s\n" "playwright    P=$par mode=$mode" "$med" "$mn" "$ok" "$N" "$rate"
}

echo ""; echo ">> PNG-only"
run_cs 8  png;  run_pw 8  png
run_cs 16 png;  run_pw 16 png
echo ""; echo ">> PNG + HTML (mode=both)"
run_cs 8  both; run_pw 8  both
run_cs 16 both; run_pw 16 both
