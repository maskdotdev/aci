#!/usr/bin/env bash
set -euo pipefail

cold_budget="${ACI_BUDGET_COLD_SECONDS:-60}"
incremental_budget="${ACI_BUDGET_INCREMENTAL_SECONDS:-0.250}"
query_budget="${ACI_BUDGET_QUERY_SECONDS:-0.010}"
semantic_budget="${ACI_BUDGET_SEMANTIC_SECONDS:-1.000}"
memory_budget_kb="${ACI_BUDGET_MEMORY_KB:-2097152}"

cold_output="$(scripts/bench-cold-index.sh)"
incremental_output="$(scripts/bench-incremental-index.sh)"
query_output="$(scripts/bench-query-latency.sh)"
memory_output="$(scripts/bench-memory.sh)"

cold_seconds="$(printf '%s\n' "$cold_output" | awk -F= '/cold_index_seconds/ { print $2 }')"
incremental_seconds="$(printf '%s\n' "$incremental_output" | awk -F= '/incremental_seconds/ { print $2 }')"
query_seconds="$(printf '%s\n' "$query_output" | awk -F= '/query_average_seconds/ { print $2 }')"
rss_kb="$(printf '%s\n' "$memory_output" | awk -F= '/memory_rss_kb/ { print $2 }')"

cargo build --release -p aci-cli --bin aci >/dev/null
semantic_output="$(target/release/aci bench semantic --iterations "${ACI_BENCH_SEMANTIC_ITERATIONS:-1000}")"
semantic_seconds="$(printf '%s\n' "$semantic_output" | awk -F= '/semantic_refresh_seconds/ { print $2 }')"

echo "$cold_output"
echo "$incremental_output"
echo "$query_output"
echo "$memory_output"
echo "$semantic_output"

awk -v value="$cold_seconds" -v budget="$cold_budget" 'BEGIN { exit !(value <= budget) }'
awk -v value="$incremental_seconds" -v budget="$incremental_budget" 'BEGIN { exit !(value <= budget) }'
awk -v value="$query_seconds" -v budget="$query_budget" 'BEGIN { exit !(value <= budget) }'
awk -v value="$semantic_seconds" -v budget="$semantic_budget" 'BEGIN { exit !(value <= budget) }'
awk -v value="$rss_kb" -v budget="$memory_budget_kb" 'BEGIN { exit !(value <= budget) }'

echo "performance_budgets=pass"
