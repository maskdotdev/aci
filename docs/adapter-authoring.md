# Adapter Authoring

Adapters implement `LanguageAdapter` from `aci-adapters`.

An adapter should:

- Detect files by extension, well-known filename, and shebang where applicable.
- Emit `GraphPartition` values using `aci-core` types only.
- Recover from syntax errors with diagnostics instead of failing the whole index.
- Honor shared `ExtractionOptions` for parser limits such as
  `max_parse_bytes`.
- Keep extraction deterministic and stable across machines.
- Add fixtures for imports, exports, classes, functions, methods, nested
  declarations, syntax errors, and large files.

The shared `helpers` module provides line/byte span conversion, identifier
scanning, and partition construction.
