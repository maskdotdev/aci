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
bench_output="$("$aci_bin" bench query --store "$store" --name f_1 --iterations "$queries")"
average="$(printf '%s\n' "$bench_output" | awk -F= '/query_average_seconds/ { print $2 }')"

echo "query_files=$files"
echo "query_count=$queries"
echo "query_average_seconds=$average"
