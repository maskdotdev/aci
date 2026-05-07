#!/usr/bin/env bash
set -euo pipefail

variants=(
  scanner-only
  tree-sitter-only
  tree-sitter-fallback
  tree-sitter-enrichment
)

for variant in "${variants[@]}"; do
  echo "variant=$variant"
  ACI_BENCH_VARIANT="$variant" "$(dirname "$0")/bench-cold-index.sh"
done
