# Implementation Plan

Status key:

- `[ ]` Not done
- `[x]` Done

Keep this file phase-level. Detailed implementation notes can move into docs,
issues, or design files as the project grows.

## Phase 0: Project Groundwork

- [x] Create `AGENTS.md` with architecture boundaries and file-size rules.
- [x] Create this implementation tracker.
- [x] Initialize the Rust Cargo workspace.
- [x] Add top-level `README.md` with project goals and quickstart.
- [x] Add `docs/architecture.md` for the high-level system design.
- [x] Add `scripts/check-loc.sh` to enforce the 800 LOC rule.
- [x] Add baseline CI checks for format, lint, tests, and LOC limits.

## Phase 1: Core Graph Model

- [x] Create `crates/aci-core`.
- [x] Define deterministic ID types for repositories, files, symbols, spans,
  nodes, and edges.
- [x] Define source span types using byte offsets plus line/column ranges.
- [x] Define graph node kinds: repository, directory, file, module, symbol,
  import, export, package, external symbol, span, and chunk.
- [x] Define graph edge kinds: contains, defines, imports, exports, calls,
  references, extends, implements, overrides, depends on, and tests.
- [x] Define diagnostics and recoverable error reporting.
- [x] Add serialization tests for the graph model.

## Phase 2: Indexing Pipeline

- [x] Create `crates/aci-indexer`.
- [x] Implement gitignore-aware file discovery.
- [x] Add language detection by extension, filename, and shebang.
- [x] Add binary/generated/vendor file filtering.
- [x] Add BLAKE3 file fingerprinting.
- [x] Add per-file indexing jobs and a bounded worker scheduler.
- [x] Add file-level graph partition replacement for changed files.
- [x] Add smoke tests for indexing a small mixed-language fixture.

## Phase 3: Adapter Framework

- [x] Create `crates/aci-adapters`.
- [x] Define the `LanguageAdapter` trait.
- [x] Add a language adapter registry.
- [x] Add shared Tree-sitter parser utilities.
- [x] Add shared extraction helpers for symbols, imports, calls, and comments.
- [x] Define adapter fixture conventions.
- [x] Add adapter conformance tests.

## Phase 4: First Language Adapters

- [x] Implement TypeScript/JavaScript detection.
- [x] Implement TypeScript/JavaScript symbol extraction.
- [x] Implement TypeScript/JavaScript import/export extraction.
- [x] Implement TypeScript/JavaScript rough call/reference extraction.
- [x] Implement Python detection.
- [x] Implement Python symbol extraction.
- [x] Implement Python import extraction.
- [x] Implement Python rough call/reference extraction.
- [x] Add fixtures covering imports, exports, classes, functions, methods,
  nested declarations, syntax errors, and large files.

## Phase 5: Storage Layer

- [x] Create `crates/aci-store`.
- [x] Define on-disk manifest format.
- [x] Implement append-only delta log.
- [x] Implement compacted graph snapshots.
- [x] Implement per-file graph partitions.
- [x] Implement adjacency indexes for fast traversal.
- [x] Add store integrity checks.
- [x] Add tests for crash-safe rebuild from snapshot plus delta log.

## Phase 6: Query Engine

- [x] Create `crates/aci-query`.
- [x] Implement symbol lookup by name, qualified name, file, and kind.
- [x] Implement file dependency queries.
- [x] Implement package dependency queries.
- [x] Implement callers and callees queries.
- [x] Implement reference lookup.
- [x] Implement impact analysis from changed files or symbols.
- [x] Add traversal tests against known graph fixtures.

## Phase 7: CLI

- [x] Create `crates/aci-cli`.
- [x] Add `aci index <path>`.
- [x] Add `aci query symbols`.
- [x] Add `aci query deps`.
- [x] Add `aci query callers`.
- [x] Add `aci query impact`.
- [x] Add `aci export`.
- [x] Keep command handlers thin and delegate behavior to library crates.

## Phase 8: Incremental And Watch Mode

- [x] Create `crates/aci-watch`.
- [x] Add filesystem watcher integration.
- [x] Add debouncing and event coalescing.
- [x] Re-index changed files only.
- [x] Re-resolve affected reverse dependencies.
- [x] Add benchmark for single-file incremental updates.
- [x] Add watch-mode integration tests.

## Phase 9: Export Adapters

- [x] Create `crates/aci-export`.
- [x] Add neutral JSONL graph export.
- [x] Add neutral JSONL graph import for testing round trips.
- [x] Add KiteDB schema mapping.
- [x] Add KiteDB batch export.
- [x] Add SCIP export or import compatibility path.
- [x] Add LSIF export for LSP-style navigation data.
- [x] Add export smoke tests for each supported format.

## Phase 10: Semantic Enrichment

- [x] Evaluate SCIP as the first semantic enrichment source.
- [x] Add optional SCIP ingestion for definitions and references.
- [x] Add optional LSP-based enrichment where practical.
- [x] Track fact provenance: Tree-sitter, SCIP, LSP, compiler, or manual.
- [x] Track confidence levels for structural vs semantic facts.
- [x] Add conflict resolution when multiple sources disagree.

## Phase 11: Performance Hardening

- [x] Add cold-index benchmark.
- [x] Add incremental-index benchmark.
- [x] Add query-latency benchmark.
- [x] Add memory usage reporting.
- [x] Add large-repo benchmark script.
- [x] Optimize string interning and path normalization.
- [x] Optimize adjacency layout and traversal hot paths.
- [x] Validate target performance budgets.

## Phase 12: Documentation And Release Readiness

- [x] Document graph model in `docs/graph-model.md`.
- [x] Document adapter authoring in `docs/adapter-authoring.md`.
- [x] Document storage format in `docs/storage.md`.
- [x] Document export formats in `docs/exports.md`.
- [x] Add examples for indexing and querying a repo.
- [x] Add troubleshooting notes.
- [x] Prepare first tagged release checklist.

## Target Performance Budgets

- [ ] Cold index 100k files in under 60 seconds on a modern laptop.
- [ ] Single-file structural update under 250 ms.
- [ ] Single-file semantic refresh under 1 second where supported.
- [ ] Warm graph queries under 10 ms for common lookups.
- [ ] Keep memory under 1-2 GB for large monorepos.
