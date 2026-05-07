#!/usr/bin/env bash
set -euo pipefail

files="${ACI_BENCH_FILES:-100000}"
tmp="$(mktemp -d)"
repo="$tmp/repo"
store="$tmp/store"
aci_bin="target/release/aci"
mkdir -p "$repo/src"

cargo build --release -p aci-cli --bin aci >/dev/null

for i in $(seq 1 "$files"); do
  printf 'def f_%s():\n    return %s\n' "$i" "$i" > "$repo/src/file_$i.py"
done

start="$(perl -MTime::HiRes=time -e 'printf "%.9f\n", time')"
"$aci_bin" index "$repo" --store "$store" >/dev/null
end="$(perl -MTime::HiRes=time -e 'printf "%.9f\n", time')"
seconds="$(awk -v start="$start" -v end="$end" 'BEGIN { printf "%.6f", end - start }')"

echo "cold_index_files=$files"
echo "cold_index_seconds=$seconds"
echo "store=$store"
