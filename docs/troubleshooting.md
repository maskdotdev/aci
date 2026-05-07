# Troubleshooting

## No Files Indexed

Check whether the target path is ignored by `.gitignore`, inside a vendor
directory, generated, or detected as binary.

## Missing Symbols

The first adapters are structural scanners. They intentionally recover from
partial or invalid source and may miss complex language-specific constructs until
semantic enrichment is enabled.

## Slow Queries

Run compaction so queries can load `snapshot.json` instead of replaying the full
delta log:

```sh
cargo run -p aci-cli -- index .
```
