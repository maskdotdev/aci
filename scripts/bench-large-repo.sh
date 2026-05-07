#!/usr/bin/env bash
set -euo pipefail

ACI_BENCH_FILES="${ACI_BENCH_FILES:-100000}" "$(dirname "$0")/bench-cold-index.sh"
