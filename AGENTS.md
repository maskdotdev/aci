# Agent Guidelines

## Project Shape

This project is a Rust-first codebase indexer that builds a graph of files,
symbols, functions, imports, exports, calls, references, and package
dependencies.

Keep the architecture modular:

- `aci-core`: shared graph model, IDs, spans, language types, diagnostics.
- `aci-indexer`: discovery, fingerprinting, scheduling, cache invalidation,
  and indexing pipeline orchestration.
- `aci-adapters`: language-specific parsing, extraction, and resolution.
- `aci-store`: snapshots, delta logs, partitions, adjacency indexes, manifests.
- `aci-query`: symbol lookup, dependency traversal, impact analysis, graph
  queries.
- `aci-export`: JSONL, SCIP, LSIF, KiteDB, and future export targets.
- `aci-cli`: thin command-line layer over the library crates.

## File Size

Source files should stay under 800 lines of code.

If a file approaches 600 lines, split it before adding more behavior. Prefer
splitting by responsibility:

- model vs behavior
- parse vs extract vs resolve
- read path vs write path
- CLI args vs command execution
- storage manifest vs storage mutation

Fixtures and generated files may exceed this limit when unavoidable.

## Adapter Layout

Language adapters should follow a consistent shape:

```text
languages/<language>/
  mod.rs
  detect.rs
  extract.rs
  resolve.rs
  queries/
    symbols.scm
    imports.scm
    calls.scm
```

Each adapter should emit the neutral internal graph model from `aci-core`.
Tree-sitter, SCIP, LSP, KiteDB, JSONL, and future systems are adapters around
that model, not the model itself.

## Engineering Defaults

- Prefer deterministic IDs based on repository, path, language, symbol kind,
  qualified name, and source span.
- Keep indexing incremental: hash files, skip unchanged files, and replace graph
  partitions per changed file.
- Batch writes and avoid global locks in hot paths.
- Keep CLI commands thin and put reusable behavior in library crates.
- Add focused tests for every language adapter and graph export format.
