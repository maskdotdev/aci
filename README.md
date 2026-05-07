# ACI

ACI is a Rust-first codebase indexer. It walks a repository, detects source
languages, extracts a neutral graph of files, symbols, imports, exports, calls,
references, and package dependencies, then stores and queries that graph.

The project is organized as small crates:

- `aci-core`: graph model, IDs, spans, diagnostics, and language types.
- `aci-adapters`: language detection and structural extraction.
- `aci-indexer`: discovery, filtering, fingerprinting, scheduling, and indexing.
- `aci-store`: manifests, snapshots, delta logs, file partitions, and adjacency.
- `aci-query`: lookup, traversal, callers, callees, references, and impact.
- `aci-export`: JSONL, KiteDB-shaped JSONL, SCIP-shaped JSON, and LSIF-shaped JSON.
- `aci-watch`: filesystem watch and debounce helpers.
- `aci-cli`: command-line entry point over the library crates.

## Quickstart

```sh
cargo test --workspace
cargo run -p aci-cli -- index .
cargo run -p aci-cli -- query symbols --store .aci
cargo run -p aci-cli -- export --store .aci --format jsonl
```

Run the local quality gate:

```sh
./scripts/check-loc.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Benchmark scripts are available under `scripts/` for cold indexing, incremental
indexing, query latency, memory reporting, large-repo runs, and budget
validation.
