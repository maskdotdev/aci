#!/usr/bin/env bash
set -euo pipefail

dependents="${ACI_BENCH_DEPENDENTS:-0}"
variant="${ACI_BENCH_VARIANT:-tree-sitter-fallback}"
tmp="$(mktemp -d)"
repo="$tmp/repo"
store="$tmp/store"
aci_bin="target/release/aci"
mkdir -p "$repo/src"

cargo build --release -p aci-cli --bin aci >/dev/null

printf 'export function run() { return 1; }\n' > "$repo/src/lib.ts"
if [ "$dependents" -gt 0 ]; then
  for i in $(seq 1 "$dependents"); do
    printf 'import { run } from "./lib";\nexport function f_%s() { return run(); }\n' "$i" > "$repo/src/app_$i.ts"
  done
fi

ACI_EXTRACTION_MODE="$variant" "$aci_bin" index "$repo" --store "$store" >/dev/null
printf 'export function run() { return 2; }\n' > "$repo/src/lib.ts"

start="$(perl -MTime::HiRes=time -e 'printf "%.9f\n", time')"
ACI_EXTRACTION_MODE="$variant" "$aci_bin" index "$repo" --store "$store" --changed "$repo/src/lib.ts" >/dev/null
end="$(perl -MTime::HiRes=time -e 'printf "%.9f\n", time')"
seconds="$(awk -v start="$start" -v end="$end" 'BEGIN { printf "%.6f", end - start }')"

echo "incremental_dependents=$dependents"
echo "incremental_variant=$variant"
echo "incremental_seconds=$seconds"
echo "store=$store"
