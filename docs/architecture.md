# Architecture

ACI builds a repository graph through five layers.

1. `aci-indexer` discovers files with gitignore-aware walking, filters binary,
   generated, and vendor paths, fingerprints content with BLAKE3, and schedules
   per-file indexing jobs.
2. `aci-adapters` owns language detection and extraction. Each adapter emits the
   neutral graph model from `aci-core`; parser-specific facts stay behind the
   adapter boundary.
3. `aci-core` defines deterministic IDs, source spans, graph nodes and edges,
   diagnostics, and graph partitions.
4. `aci-store` persists per-file partitions, append-only deltas, compacted
   snapshots, manifests, and adjacency indexes.
5. `aci-query` and `aci-export` consume stored graph data for lookup, traversal,
   impact analysis, and interchange formats.

The graph is partitioned by file. When a file changes, the indexer replaces that
file's partition and keeps the rest of the graph intact. Higher-confidence
semantic sources such as SCIP or LSP can enrich the same model later by adding
facts with provenance and confidence.

Adapters follow this shape:

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

The current adapters use deterministic line scanners with shared helper
utilities. The module layout leaves a direct path for Tree-sitter queries without
changing the graph model.
