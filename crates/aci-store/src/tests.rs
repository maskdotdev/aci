use super::*;
use aci_core::{GraphNode, Language, NodeKind, RepositoryId, SourceFile};
use std::fs;
use std::path::{Path, PathBuf};

fn partition(text: &str) -> GraphPartition {
    let repo = RepositoryId::new("repo", &["store-test"]);
    let file = SourceFile::new(
        repo.clone(),
        Path::new("/repo"),
        PathBuf::from("/repo/a.py"),
        Language::Python,
        text.to_string(),
    );
    let mut partition = GraphPartition::empty(&file);
    partition.nodes.push(GraphNode::deterministic(
        &repo,
        Some(&file.file_id),
        NodeKind::File,
        Language::Python,
        Some("a.py".to_string()),
        Some("a.py".to_string()),
        None,
    ));
    partition.nodes.push(GraphNode::deterministic(
        &repo,
        Some(&file.file_id),
        NodeKind::Symbol,
        Language::Python,
        Some("a".to_string()),
        Some("a".to_string()),
        None,
    ));
    partition
}

#[test]
fn rebuilds_from_snapshot_plus_delta_log() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    store
        .write_partition(&partition("one"))
        .expect("write first");
    store.compact().expect("compact");
    let replacement = partition("two");
    store
        .write_partition(&replacement)
        .expect("write replacement");

    let latest = store.load_latest().expect("load latest");
    assert_eq!(latest.partitions.len(), 1);
    assert_eq!(latest.partitions[0].fingerprint, replacement.fingerprint);
}

#[test]
fn replace_all_writer_loads_from_manifest_without_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    store
        .write_partition(&partition("stale"))
        .expect("write stale");
    store.compact().expect("compact stale snapshot");

    let replacement = partition("fresh");
    let mut writer = store.replace_all_writer().expect("open writer");
    writer.write(&replacement).expect("write replacement");
    assert_eq!(writer.finish().expect("finish writer"), 1);

    assert!(!store.root().join("snapshot.json").exists());
    assert!(!store.root().join("manifest.json").exists());
    assert!(store.root().join("manifest.jsonl").exists());
    assert!(store.root().join("partitions/pack-00000.jsonl").exists());
    assert_eq!(
        store
            .read_manifest()
            .expect("manifest")
            .partitions
            .values()
            .next()
            .and_then(|entry| entry.record_index),
        Some(0)
    );
    assert!(store.read_delta_log().expect("read delta").is_empty());
    assert!(
        store
            .partition_file_check()
            .expect("partition file check")
            .is_empty()
    );
    let symbols = store
        .lookup_symbol_index(Some("a"))
        .expect("symbol index")
        .expect("symbol index exists");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].file_id.as_ref(), Some(&replacement.file_id));
    let latest = store.load_latest().expect("load latest");
    assert_eq!(latest.partitions.len(), 1);
    assert_eq!(latest.partitions[0].fingerprint, replacement.fingerprint);
}

#[test]
fn packed_partition_records_use_compact_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    let replacement = partition("compact");
    let mut writer = store.replace_all_writer().expect("open writer");
    writer.write(&replacement).expect("write replacement");
    writer.finish().expect("finish writer");

    let pack = fs::read_to_string(store.root().join("partitions/pack-00000.jsonl")).expect("pack");
    assert!(pack.contains("\"s\":"));
    assert!(pack.contains("\"n\":"));
    assert!(!pack.contains("\"nodes\""));
    assert!(!pack.contains("\"file_id\""));
    let latest = store.load_latest().expect("load latest");
    assert_eq!(latest.partitions, vec![replacement]);
}

#[test]
fn integrity_check_rejects_symbols_without_files() {
    let repo = RepositoryId::new("repo", &["integrity-symbol"]);
    let file = SourceFile::new(
        repo.clone(),
        Path::new("/repo"),
        PathBuf::from("/repo/a.py"),
        Language::Python,
        "def a(): pass\n".to_string(),
    );
    let mut partition = GraphPartition::empty(&file);
    partition.nodes.push(GraphNode::deterministic(
        &repo,
        None,
        NodeKind::Symbol,
        Language::Python,
        Some("a".to_string()),
        Some("a".to_string()),
        None,
    ));

    let problems = check_snapshot_integrity(&GraphSnapshot {
        partitions: vec![partition],
    });
    assert!(problems.iter().any(|problem| problem.contains("no file")));
}

#[test]
fn integrity_check_rejects_missing_edge_targets_and_bad_spans() {
    let repo = RepositoryId::new("repo", &["integrity-edge"]);
    let file = SourceFile::new(
        repo.clone(),
        Path::new("/repo"),
        PathBuf::from("/repo/a.py"),
        Language::Python,
        "def a(): pass\n".to_string(),
    );
    let file_node = GraphNode::deterministic(
        &repo,
        Some(&file.file_id),
        NodeKind::File,
        Language::Python,
        Some("a.py".to_string()),
        Some("a.py".to_string()),
        Some(aci_core::SourceSpan::new(
            10,
            1,
            aci_core::LineColumn::new(2, 1),
            aci_core::LineColumn::new(1, 1),
        )),
    );
    let missing = NodeId::from_raw("node:missing");
    let edge = GraphEdge::deterministic(EdgeKind::References, &file_node.id, &missing, None);
    let mut partition = GraphPartition::empty(&file);
    partition.nodes.push(file_node);
    partition.edges.push(edge);

    let problems = check_snapshot_integrity(&GraphSnapshot {
        partitions: vec![partition],
    });
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("missing target"))
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("invalid byte span"))
    );
}
