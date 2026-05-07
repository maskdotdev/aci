# Graph Model

The graph model is neutral: it does not expose Tree-sitter, LSP, SCIP, LSIF, or
KiteDB-specific types. Source adapters and export adapters translate at the
boundary.

IDs are deterministic BLAKE3-derived strings built from repository, path,
language, symbol kind, qualified name, and source span. Spans store byte offsets
plus one-based line and column ranges.

Nodes represent repositories, directories, files, modules, symbols, imports,
exports, packages, external symbols, spans, and chunks. Edges represent
contains, defines, imports, exports, calls, references, extends, implements,
overrides, depends-on, and tests relationships.

Each file contributes a `GraphPartition`. Re-indexing a file replaces only that
partition.
