# Storage

ACI storage has four durable pieces:

- `manifest.json`: store version and the known per-file partitions.
- `partitions/*.json`: one serialized graph partition per indexed file.
- `delta.jsonl`: append-only partition replacement log.
- `snapshot.json`: compacted graph snapshot used for fast query startup.

Writes use a temporary file followed by rename for snapshot, manifest, and
partition replacement. The store can rebuild the latest graph from a snapshot
plus the delta log.
