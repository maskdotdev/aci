# ACI

ACI is a Rust-first codebase indexer. It walks a repository, detects source
languages, extracts a neutral graph of files, symbols, imports, exports, calls,
references, and package dependencies, then stores that graph for fast queries
and export.

The goal is a small, deterministic indexing engine that can grow from structural
Tree-sitter extraction into richer semantic enrichment without changing the
internal graph model.

## What It Does

- Indexes code into per-file graph partitions.
- Emits stable IDs from repository, path, language, symbol kind, names, and
  source spans.
- Tracks fact provenance and confidence so structural, SCIP, LSP, and compiler
  facts can coexist.
- Stores full indexes as compact binary packs with JSONL manifests.
- Supports incremental replacement of changed files and reverse dependencies.
- Queries symbols, dependencies, callers, callees, references, packages, and
  impact sets.
- Exports graph data as JSONL, KiteDB-shaped JSONL, SCIP-shaped JSON, and
  LSIF-shaped JSON.

## Crates

| Crate | Responsibility |
| --- | --- |
| `aci-core` | Graph model, IDs, spans, diagnostics, language types, fact quality. |
| `aci-adapters` | Language detection, Tree-sitter extraction, scanner fallback, fixtures. |
| `aci-indexer` | Discovery, filtering, fingerprinting, parallel indexing, incremental planning. |
| `aci-diff` | Branch-to-branch semantic diffing over indexed graph snapshots. |
| `aci-store` | Manifests, packed partitions, delta logs, snapshots, symbol/dependency indexes. |
| `aci-query` | Symbol lookup, dependency traversal, callers, callees, references, impact. |
| `aci-export` | JSONL, KiteDB, SCIP, and LSIF export shapes. |
| `aci-watch` | Filesystem watch and debounce helpers. |
| `aci-cli` | Thin command-line entry point over the library crates. |

## Supported Inputs

Current adapters cover:

| Language | Detection | Extraction |
| --- | --- | --- |
| C, C++, Objective-C | Extensions and parser support | Tree-sitter with scanner fallback |
| Go, Java, Rust | Extensions and parser support | Tree-sitter with scanner fallback |
| JavaScript, TypeScript, TSX | Extensions and parser support | Tree-sitter with scanner fallback |
| Python | Extension and shebang support | Tree-sitter with scanner fallback |
| JSON / `package.json` | Filename and extension | Package/dependency extraction |

Unsupported, binary, generated, and vendor paths are skipped before parsing.

## Quickstart

Install the latest release:

```sh
curl -fsSL https://aci.mask.dev/install.sh | sh
```

Run the tests:

```sh
cargo test --workspace
```

Index the current repository into `.aci`:

```sh
cargo run -p aci-cli -- index .
cargo run -p aci-cli -- index . --max-parse-bytes 10485760
```

Query symbols:

```sh
cargo run -p aci-cli -- query symbols
cargo run -p aci-cli -- query symbols --name main
cargo run -p aci-cli -- query --pretty symbols --name main
cargo run -p aci-cli -- query --pretty --color always symbols --name main
cargo run -p aci-cli -- query --format json symbols --name main
```

Symbol queries include jump locations in `path:line:column` form after the
store is indexed with the current binary. Query commands default to text output;
use `--format json` before the query subcommand for machine-readable output.
Query and export commands use `.aci` by default; pass `--store` only when
reading a different store path.

Query dependencies and impact:

```sh
cargo run -p aci-cli -- query deps --file src/lib.rs
cargo run -p aci-cli -- query packages
cargo run -p aci-cli -- query callers main
cargo run -p aci-cli -- query callees main
cargo run -p aci-cli -- query refs main
cargo run -p aci-cli -- query deps-tree main --depth 2
cargo run -p aci-cli -- query impact src/lib.rs
cargo run -p aci-cli -- query --pretty impact src/lib.rs
```

Compare two Git refs semantically:

```sh
cargo run -p aci-cli -- diff main feature
cargo run -p aci-cli -- diff main feature --agent
cargo run -p aci-cli -- diff main feature --pretty
cargo run -p aci-cli -- diff main feature --format json --pretty
cargo run -p aci-cli -- diff main feature --agent --format json --pretty
cargo run -p aci-cli -- diff main feature --agent --max-parse-bytes 10485760
```

`aci diff` checks each ref out into an isolated detached worktree, indexes both
trees, then reports file, symbol, public API, dependency, diagnostic, and impact
changes without mutating the current working tree.

Tree-sitter adapters skip files above the parse byte cap and fall back to
structural scanners when fallback mode is enabled. Use `--max-parse-bytes` on
`index`, `watch`, `diff`, `bench cold`, and `bench query-path` when a repository
has large generated-by-hand source files that should still get parser-backed
facts. The default protects indexing latency and memory on mixed repositories.

Keep the store updated while editing:

```sh
cargo run -p aci-cli -- watch .
cargo run -p aci-cli -- watch . --debounce-ms 250
cargo run -p aci-cli -- watch . --once --max-wait-ms 5000
cargo run -p aci-cli -- watch . --max-parse-bytes 10485760
```

Export the graph:

```sh
cargo run -p aci-cli -- export --format jsonl
cargo run -p aci-cli -- export --format scip --output graph.scip.json
```

Run a cold-index benchmark:

```sh
cargo run -p aci-cli -- bench cold . --variant tree-sitter-fallback
```

## Development Workflow

Use this as the local quality gate before committing:

```sh
./scripts/check-loc.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./scripts/validate-packaging.sh
```

Benchmark and budget scripts live under `scripts/`:

- `bench-cold-index.sh`
- `bench-incremental-index.sh`
- `bench-query-latency.sh`
- `bench-memory.sh`
- `bench-real-repo.sh`
- `bench-structural-variants.sh`
- `validate-performance.sh`

## Architecture Notes

ACI keeps parser-specific details behind adapters. Every adapter emits the same
`aci-core` graph model, and storage writes are partitioned by file so changed
files can be replaced without rewriting unrelated graph data.

Full index writes stream compact partition records into
`partitions/pack-00000.bin` and write `manifest.jsonl`. Incremental updates
write changed file partitions and append replacement records to `delta.jsonl`.
Snapshots are optional compaction artifacts for faster query startup.

## Documentation

- [Architecture](docs/architecture.md)
- [Graph model](docs/graph-model.md)
- [Storage](docs/storage.md)
- [Adapter authoring](docs/adapter-authoring.md)
- [Exports](docs/exports.md)
- [Branch diffs](docs/diff.md)
- [Semantic enrichment](docs/semantic-enrichment.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Release checklist](docs/release-checklist.md)
- [Tree-sitter production plan](docs/tree-sitter-production-plan.md)

## Contributing and Security

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, validation, and pull request
expectations. See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## Current Limits

- Extraction is strongest for structural facts. Semantic enrichment exists, but
  compiler/LSP-grade facts are still adapter work.
- Query APIs are library-first; the CLI intentionally stays thin.
- Store compatibility is tested for the current packed layout. Old local `.aci`
  stores should be regenerated after storage format changes.
