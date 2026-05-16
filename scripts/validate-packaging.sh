#!/usr/bin/env bash
set -euo pipefail

# Full verification works for crates whose dependencies are already available
# from crates.io. Workspace crates with unpublished path dependencies cannot be
# fully packaged until their publish order has run, so list their package
# contents to catch missing files and invalid include/exclude state.
cargo package -p aci-core "$@"
cargo package --workspace --list "$@" >/dev/null
