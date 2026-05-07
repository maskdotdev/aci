#!/usr/bin/env bash
set -euo pipefail

files="${ACI_BENCH_FILES:-1000}"
queries="${ACI_BENCH_QUERIES:-100}"
tmp="$(mktemp -d)"
repo="$tmp/repo"
store="$tmp/store"
aci_bin="target/release/aci"
mkdir -p "$repo/src"

cargo build --release -p aci-cli --bin aci >/dev/null

for i in $(seq 1 "$files"); do
  printf 'def f_%s():\n    return %s\n' "$i" "$i" > "$repo/src/file_$i.py"
done

"$aci_bin" index "$repo" --store "$store" >/dev/null
start="$(perl -MTime::HiRes=time -e 'printf "%.9f\n", time')"
for _ in $(seq 1 "$queries"); do
  "$aci_bin" query --store "$store" symbols --name f_1 >/dev/null
done
end="$(perl -MTime::HiRes=time -e 'printf "%.9f\n", time')"
total="$(awk -v start="$start" -v end="$end" 'BEGIN { printf "%.6f", end - start }')"
average="$(awk -v total="$total" -v queries="$queries" 'BEGIN { printf "%.6f", total / queries }')"

echo "query_files=$files"
echo "query_count=$queries"
echo "query_total_seconds=$total"
echo "query_average_seconds=$average"
