# Implementation Plan

Status key:

- `[ ]` Not done
- `[x]` Done

Keep this file phase-level. Detailed implementation notes can move into docs,
issues, or design files as the project grows.

## Phase 0: Project Groundwork

- [x] Create `AGENTS.md` with architecture boundaries and file-size rules.
- [x] Create this implementation tracker.
- [ ] Initialize the Rust Cargo workspace.
- [ ] Add top-level `README.md` with project goals and quickstart.
- [ ] Add `docs/architecture.md` for the high-level system design.
- [ ] Add `scripts/check-loc.sh` to enforce the 800 LOC rule.
- [ ] Add baseline CI checks for format, lint, tests, and LOC limits.

## Phase 1: Core Graph Model

- [ ] Create `crates/aci-core`.
- [ ] Define deterministic ID types for repositories, files, symbols, spans,
  nodes, and edges.
- [ ] Define source span types using byte offsets plus line/column ranges.
- [ ] Define graph node kinds: repository, directory, file, module, symbol,
  import, export, package, external symbol, span, and chunk.
- [ ] Define graph edge kinds: contains, defines, imports, exports, calls,
  references, extends, implements, overrides, depends on, and tests.
- [ ] Define diagnostics and recoverable error reporting.
- [ ] Add serialization tests for the graph model.

## Phase 2: Indexing Pipeline

- [ ] Create `crates/aci-indexer`.
- [ ] Implement gitignore-aware file discovery.
- [ ] Add language detection by extension, filename, and shebang.
- [ ] Add binary/generated/vendor file filtering.
- [ ] Add BLAKE3 file fingerprinting.
- [ ] Add per-file indexing jobs and a bounded worker scheduler.
- [ ] Add file-level graph partition replacement for changed files.
- [ ] Add smoke tests for indexing a small mixed-language fixture.

## Phase 3: Adapter Framework

- [ ] Create `crates/aci-adapters`.
- [ ] Define the `LanguageAdapter` trait.
- [ ] Add a language adapter registry.
- [ ] Add shared Tree-sitter parser utilities.
- [ ] Add shared extraction helpers for symbols, imports, calls, and comments.
- [ ] Define adapter fixture conventions.
- [ ] Add adapter conformance tests.

## Phase 4: First Language Adapters

- [ ] Implement TypeScript/JavaScript detection.
- [ ] Implement TypeScript/JavaScript symbol extraction.
- [ ] Implement TypeScript/JavaScript import/export extraction.
- [ ] Implement TypeScript/JavaScript rough call/reference extraction.
- [ ] Implement Python detection.
- [ ] Implement Python symbol extraction.
- [ ] Implement Python import extraction.
- [ ] Implement Python rough call/reference extraction.
- [ ] Add fixtures covering imports, exports, classes, functions, methods,
  nested declarations, syntax errors, and large files.

## Phase 5: Storage Layer

- [ ] Create `crates/aci-store`.
- [ ] Define on-disk manifest format.
- [ ] Implement append-only delta log.
- [ ] Implement compacted graph snapshots.
- [ ] Implement per-file graph partitions.
- [ ] Implement adjacency indexes for fast traversal.
- [ ] Add store integrity checks.
- [ ] Add tests for crash-safe rebuild from snapshot plus delta log.

## Phase 6: Query Engine

- [ ] Create `crates/aci-query`.
- [ ] Implement symbol lookup by name, qualified name, file, and kind.
- [ ] Implement file dependency queries.
- [ ] Implement package dependency queries.
- [ ] Implement callers and callees queries.
- [ ] Implement reference lookup.
- [ ] Implement impact analysis from changed files or symbols.
- [ ] Add traversal tests against known graph fixtures.

## Phase 7: CLI

- [ ] Create `crates/aci-cli`.
- [ ] Add `aci index <path>`.
- [ ] Add `aci query symbols`.
- [ ] Add `aci query deps`.
- [ ] Add `aci query callers`.
- [ ] Add `aci query impact`.
- [ ] Add `aci export`.
- [ ] Keep command handlers thin and delegate behavior to library crates.

## Phase 8: Incremental And Watch Mode

- [ ] Create `crates/aci-watch`.
- [ ] Add filesystem watcher integration.
- [ ] Add debouncing and event coalescing.
- [ ] Re-index changed files only.
- [ ] Re-resolve affected reverse dependencies.
- [ ] Add benchmark for single-file incremental updates.
- [ ] Add watch-mode integration tests.

## Phase 9: Export Adapters

- [ ] Create `crates/aci-export`.
- [ ] Add neutral JSONL graph export.
- [ ] Add neutral JSONL graph import for testing round trips.
- [ ] Add KiteDB schema mapping.
- [ ] Add KiteDB batch export.
- [ ] Add SCIP export or import compatibility path.
- [ ] Add LSIF export for LSP-style navigation data.
- [ ] Add export smoke tests for each supported format.

## Phase 10: Semantic Enrichment

- [ ] Evaluate SCIP as the first semantic enrichment source.
- [ ] Add optional SCIP ingestion for definitions and references.
- [ ] Add optional LSP-based enrichment where practical.
- [ ] Track fact provenance: Tree-sitter, SCIP, LSP, compiler, or manual.
- [ ] Track confidence levels for structural vs semantic facts.
- [ ] Add conflict resolution when multiple sources disagree.

## Phase 11: Performance Hardening

- [ ] Add cold-index benchmark.
- [ ] Add incremental-index benchmark.
- [ ] Add query-latency benchmark.
- [ ] Add memory usage reporting.
- [ ] Add large-repo benchmark script.
- [ ] Optimize string interning and path normalization.
- [ ] Optimize adjacency layout and traversal hot paths.
- [ ] Validate target performance budgets.

## Phase 12: Documentation And Release Readiness

- [ ] Document graph model in `docs/graph-model.md`.
- [ ] Document adapter authoring in `docs/adapter-authoring.md`.
- [ ] Document storage format in `docs/storage.md`.
- [ ] Document export formats in `docs/exports.md`.
- [ ] Add examples for indexing and querying a repo.
- [ ] Add troubleshooting notes.
- [ ] Prepare first tagged release checklist.

## Target Performance Budgets

- [ ] Cold index 100k files in under 60 seconds on a modern laptop.
- [ ] Single-file structural update under 250 ms.
- [ ] Single-file semantic refresh under 1 second where supported.
- [ ] Warm graph queries under 10 ms for common lookups.
- [ ] Keep memory under 1-2 GB for large monorepos.
