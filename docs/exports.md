# Exports

`aci-export` supports:

- `jsonl`: neutral graph records for repositories, nodes, edges, diagnostics,
  and partitions.
- `kitedb`: a simple KiteDB-shaped JSONL projection for symbol and relation
  consumers.
- `scip`: a SCIP-shaped JSON compatibility document for future semantic export.
- `lsif`: an LSIF-shaped JSONL compatibility stream for navigation tooling.

JSONL import is available for test round trips and migration checks.
