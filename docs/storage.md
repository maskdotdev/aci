# Storage

ACI storage has four durable pieces:

- `manifest.jsonl`: one partition manifest entry per indexed file.
- `partitions/pack-00000.bin`: compact binary partition records for full writes.
- `delta.jsonl`: append-only partition replacement log.
- `snapshot.json`: compacted graph snapshot used for fast query startup.

Full index writes stream partition records into the pack and atomically replace
the manifest. Incremental replacement writes per-file JSON partitions and appends
to `delta.jsonl`. The store can rebuild the latest graph from a snapshot plus
the delta log, or from the manifest and partition files when no snapshot exists.
