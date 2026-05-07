use super::*;
use aci_core::{GraphNode, GraphPartition, NodeKind};
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[test]
fn jsonl_round_trips_partitions() {
    let repo = RepositoryId::new("repo", &["export"]);
    let file = SourceFile::new(
        repo,
        Path::new("/repo"),
        PathBuf::from("/repo/main.ts"),
        Language::TypeScript,
        "export const x = 1;\n".to_string(),
    );
    let mut partition = GraphPartition::empty(&file);
    partition.nodes.push(
        GraphNode::deterministic(
            &file.repo_id,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("main".to_string()),
            Some("main".to_string()),
            None,
        )
        .with_symbol_kind(SymbolKind::Function),
    );
    let snapshot = GraphSnapshot {
        partitions: vec![partition],
    };
    let mut bytes = Vec::new();
    export_snapshot(&snapshot, ExportFormat::Jsonl, &mut bytes).expect("export");
    let imported = import_jsonl(BufReader::new(bytes.as_slice())).expect("import");
    assert_eq!(imported, snapshot);
}

#[test]
fn all_export_formats_emit_output() {
    let repo = RepositoryId::new("repo", &["export-all"]);
    let file = SourceFile::new(
        repo,
        Path::new("/repo"),
        PathBuf::from("/repo/main.py"),
        Language::Python,
        "def main():\n    pass\n".to_string(),
    );
    let mut partition = GraphPartition::empty(&file);
    partition.nodes.push(
        GraphNode::deterministic(
            &file.repo_id,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("main".to_string()),
            Some("main".to_string()),
            None,
        )
        .with_symbol_kind(SymbolKind::Function),
    );
    let snapshot = GraphSnapshot {
        partitions: vec![partition],
    };
    for format in [
        ExportFormat::Jsonl,
        ExportFormat::KiteDb,
        ExportFormat::Scip,
        ExportFormat::Lsif,
    ] {
        let mut bytes = Vec::new();
        export_snapshot(&snapshot, format, &mut bytes).expect("export format");
        assert!(!bytes.is_empty(), "empty output for {format:?}");
    }
}

#[test]
fn imports_scip_definition_and_reference_occurrences() {
    let repo = RepositoryId::new("repo", &["scip-import"]);
    let input = br#"{
      "documents": [{
        "relativePath": "src/main.py",
        "occurrences": [
          { "symbol": "local 0 main().", "range": [0, 4, 8], "roles": 1 },
          { "symbol": "local 0 helper().", "range": [1, 2, 8], "roles": 0 }
        ]
      }]
    }"#;
    let snapshot =
        import_scip_enrichment(repo, Path::new("/repo"), input.as_slice()).expect("import scip");
    assert_eq!(snapshot.partitions.len(), 1);
    assert!(
        snapshot.partitions[0]
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Defines)
    );
    assert!(
        snapshot.partitions[0]
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::References)
    );
    assert!(
        snapshot.partitions[0]
            .nodes
            .iter()
            .all(|node| node.provenance == FactProvenance::Scip)
    );
}

#[test]
fn imports_lsp_definition_and_reference_facts() {
    let repo = RepositoryId::new("repo", &["lsp-import"]);
    let input = br#"{
      "documents": [{
        "uri": "src/main.py",
        "facts": [
          {
            "symbol": "main",
            "kind": "definition",
            "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 8 } }
          },
          {
            "symbol": "helper",
            "kind": "reference",
            "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 8 } }
          }
        ]
      }]
    }"#;
    let snapshot =
        import_lsp_enrichment(repo, Path::new("/repo"), input.as_slice()).expect("import lsp");
    assert_eq!(snapshot.partitions.len(), 1);
    assert!(
        snapshot.partitions[0]
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Defines)
    );
    assert!(
        snapshot.partitions[0]
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::References)
    );
    assert!(
        snapshot.partitions[0]
            .nodes
            .iter()
            .all(|node| node.provenance == FactProvenance::Lsp)
    );
}
