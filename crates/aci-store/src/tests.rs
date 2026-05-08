use super::*;
use aci_core::{
    Confidence, Diagnostic, EdgeKind, FactProvenance, GraphEdge, GraphNode, GraphPartition,
    GraphSnapshot, Language, LineColumn, NodeId, NodeKind, PartitionMetrics, RepositoryId,
    Severity, SourceFile, SourceSpan,
};
use std::fs;
use std::path::{Path, PathBuf};

fn partition(text: &str) -> GraphPartition {
    partition_at("/repo/a.py", text)
}

fn partition_at(path: &str, text: &str) -> GraphPartition {
    let repo = RepositoryId::new("repo", &["store-test"]);
    let file = SourceFile::new(
        repo.clone(),
        Path::new("/repo"),
        PathBuf::from(path),
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
    assert!(store.root().join("manifest.jsonl").exists());
    assert!(store.root().join("partitions/pack-00000.bin").exists());
    assert!(store.root().join("symbols").is_dir());
    assert!(store.root().join("deps").is_dir());
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
    assert_eq!(symbols[0].path.as_ref(), Some(&replacement.path));
    let all_symbols = store
        .lookup_symbol_index(None)
        .expect("symbol index")
        .expect("symbol index exists");
    assert_eq!(all_symbols.len(), 1);
    let latest = store.load_latest().expect("load latest");
    assert_eq!(latest.partitions.len(), 1);
    assert_eq!(latest.partitions[0].fingerprint, replacement.fingerprint);
}

#[test]
fn symbol_index_lookup_falls_back_after_delta_updates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    let original = partition("original");
    let mut writer = store.replace_all_writer().expect("open writer");
    writer.write(&original).expect("write original");
    writer.finish().expect("finish writer");
    assert!(
        store
            .lookup_symbol_index(Some("a"))
            .expect("lookup before delta")
            .is_some()
    );

    let replacement = partition("replacement");
    store
        .replace_partitions(&[replacement])
        .expect("replace partition");
    assert!(
        store
            .lookup_symbol_index(Some("a"))
            .expect("lookup after delta")
            .is_none()
    );
}

#[test]
fn dependency_index_plans_incremental_reverse_dependencies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    let lib = partition_at("/repo/lib.py", "def run(): pass\n");
    let mut app = partition_at("/repo/app.py", "from lib import run\n");
    let repo = RepositoryId::new("repo", &["store-test"]);
    app.nodes.push(GraphNode::deterministic(
        &repo,
        Some(&app.file_id),
        NodeKind::Import,
        Language::Python,
        Some("lib".to_string()),
        Some("lib".to_string()),
        None,
    ));
    let mut writer = store.replace_all_writer().expect("open writer");
    writer.write(&lib).expect("write lib");
    writer.write(&app).expect("write app");
    writer.finish().expect("finish writer");

    let plan = store
        .plan_incremental_reindex(std::slice::from_ref(&lib.path))
        .expect("plan")
        .expect("dependency index exists");
    assert_eq!(plan.changed_files, vec![lib.path]);
    assert_eq!(plan.reverse_dependencies, vec![app.path]);
    assert_eq!(plan.files_to_reindex.len(), 2);
}

#[test]
fn packed_partition_records_use_compact_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    let replacement = partition("compact");
    let mut writer = store.replace_all_writer().expect("open writer");
    writer.write(&replacement).expect("write replacement");
    writer.finish().expect("finish writer");

    let pack = fs::read(store.root().join("partitions/pack-00000.bin")).expect("pack");
    assert!(pack.starts_with(b"ACIPACK1\n"));
    assert!(!String::from_utf8_lossy(&pack).contains("\"nodes\""));
    assert!(!String::from_utf8_lossy(&pack).contains("\"file_id\""));
    let latest = store.load_latest().expect("load latest");
    assert_eq!(latest.partitions, vec![replacement]);
}

#[test]
fn packed_partition_binary_preserves_rich_partition_shape() {
    let mut replacement = partition("rich");
    replacement.metrics = PartitionMetrics {
        parse_time_micros: 11,
        extraction_time_micros: 22,
        query_captures: 33,
    };
    replacement.nodes[1] = replacement.nodes[1]
        .clone()
        .with_fact_quality(FactProvenance::TreeSitter, Confidence::Exact);
    replacement.diagnostics.push(Diagnostic {
        severity: Severity::Warning,
        message: "syntax recovered".to_string(),
        file_id: Some(replacement.file_id.clone()),
        span: Some(SourceSpan::new(
            1,
            3,
            LineColumn::new(1, 2),
            LineColumn::new(1, 4),
        )),
    });
    replacement.edges.push(GraphEdge::deterministic(
        EdgeKind::References,
        &replacement.nodes[0].id,
        &replacement.nodes[1].id,
        Some(SourceSpan::new(
            4,
            5,
            LineColumn::new(1, 5),
            LineColumn::new(1, 6),
        )),
    ));

    let mut encoded = Vec::new();
    compact::write_partition_binary(&mut encoded, &replacement).expect("encode partition");
    let decoded = compact::read_partition_binary(&mut encoded.as_slice())
        .expect("decode partition")
        .expect("partition record");

    assert_eq!(decoded, replacement);
}

#[test]
fn partition_file_check_reports_missing_packed_record() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    let replacement = partition("compact");
    let mut writer = store.replace_all_writer().expect("open writer");
    writer.write(&replacement).expect("write replacement");
    writer.finish().expect("finish writer");

    let mut manifest = store.read_manifest().expect("manifest");
    let entry = manifest
        .partitions
        .values_mut()
        .next()
        .expect("manifest entry");
    entry.record_index = Some(1);
    let mut manifest_jsonl = String::new();
    for entry in manifest.partitions.values() {
        manifest_jsonl.push_str(&serde_json::to_string(entry).expect("serialize entry"));
        manifest_jsonl.push('\n');
    }
    fs::write(store.root().join("manifest.jsonl"), manifest_jsonl).expect("rewrite manifest");

    let problems = store
        .partition_file_check()
        .expect("partition file check should complete");
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("missing packed record 1"))
    );
}

#[test]
fn load_latest_rejects_invalid_pack_header() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(dir.path()).expect("open store");
    let replacement = partition("compact");
    let entry = PartitionEntry {
        file_id: replacement.file_id.to_string(),
        path: replacement.path.clone(),
        fingerprint: replacement.fingerprint.clone(),
        partition_file: PathBuf::from("partitions").join("pack-00000.bin"),
        record_index: Some(0),
    };
    fs::write(
        store.root().join("manifest.jsonl"),
        format!("{}\n", serde_json::to_string(&entry).expect("entry json")),
    )
    .expect("write manifest");
    fs::write(
        store.root().join("partitions/pack-00000.bin"),
        b"not-an-aci-pack",
    )
    .expect("write invalid pack");

    let error = store
        .load_latest()
        .expect_err("invalid pack header should fail");
    assert!(error.to_string().contains("invalid header"));
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
