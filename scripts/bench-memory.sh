#!/usr/bin/env bash
set -euo pipefail

files="${ACI_BENCH_FILES:-100000}"
variant="${ACI_BENCH_VARIANT:-tree-sitter-fallback}"
tmp="$(mktemp -d)"
repo="$tmp/repo"
store="$tmp/store"
aci_bin="target/release/aci"
time_log="$tmp/time.log"
mkdir -p "$repo/src"

cargo build --release -p aci-cli --bin aci >/dev/null

for i in $(seq 1 "$files"); do
  printf 'def f_%s():\n    return %s\n' "$i" "$i" > "$repo/src/file_$i.py"
done

/usr/bin/time -l "$aci_bin" bench cold "$repo" --variant "$variant" >/dev/null 2>"$time_log"
rss_kb="$(awk '/maximum resident set size/ { print int($1 / 1024) }' "$time_log")"
rss_kb="${rss_kb:-0}"

echo "memory_files=$files"
echo "memory_variant=$variant"
echo "memory_rss_kb=$rss_kb"
echo "store=$store"
