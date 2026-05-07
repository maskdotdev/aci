#!/usr/bin/env bash
set -euo pipefail

files="${ACI_BENCH_FILES:-100000}"
variant="${ACI_BENCH_VARIANT:-tree-sitter-fallback}"
tmp="$(mktemp -d)"
repo="$tmp/repo"
store="$tmp/store"
aci_bin="target/release/aci"
mkdir -p "$repo/src"

cargo build --release -p aci-cli --bin aci >/dev/null

for i in $(seq 1 "$files"); do
  printf 'def f_%s():\n    return %s\n' "$i" "$i" > "$repo/src/file_$i.py"
done

bench_output="$("$aci_bin" bench cold "$repo" --variant "$variant")"

echo "$bench_output"
echo "store=$store"
