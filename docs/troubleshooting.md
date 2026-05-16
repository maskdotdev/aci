# Troubleshooting

## No Files Indexed

Check whether the target path is ignored by `.gitignore`, inside a vendor
directory, generated, or detected as binary.

## Missing Symbols

Adapters prefer Tree-sitter extraction and fall back to structural scanners when
parsing fails, times out, or exceeds size limits. They intentionally recover from
partial or invalid source and may miss complex language-specific constructs until
semantic enrichment is enabled.

## Parser Skipped a Large File

Tree-sitter extraction has a byte cap per file so a single unusually large
source file cannot dominate indexing latency or memory. When fallback extraction
is enabled, ACI still emits scanner-derived symbols and records a diagnostic such
as `tree-sitter skipped large JS/TS file`.

Raise the cap for repositories with legitimate large source files:

```sh
aci index . --max-parse-bytes 10485760
aci diff main feature --agent --max-parse-bytes 10485760
aci watch . --max-parse-bytes 10485760
```

If the file is generated, prefer excluding it through normal repository hygiene
instead of raising the cap.

## Slow Queries

Run compaction so queries can load `snapshot.json` instead of replaying the full
delta log:

```sh
cargo run -p aci-cli -- index .
```
