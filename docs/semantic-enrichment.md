# Semantic Enrichment

ACI treats Tree-sitter, SCIP, LSP, compiler, and manual facts as enrichment
sources over the same neutral graph model.

SCIP is the first semantic enrichment source because it is language-neutral,
batch-friendly, and already represents definitions and references. `aci-export`
includes `import_scip_enrichment` to convert SCIP-shaped occurrences into file
partitions with `FactProvenance::Scip` and exact confidence.

LSP enrichment is supported as a practical interchange format for definition
and reference locations through `import_lsp_enrichment`. This keeps editor-driven
facts optional and avoids coupling the indexer to a live language server.

When facts disagree, `aci-core::prefer_fact` ranks candidates by provenance and
confidence. Compiler and manual facts outrank LSP, SCIP, Tree-sitter, and
structural scanner facts; exact confidence outranks high, medium, and low
confidence within the same source tier.
