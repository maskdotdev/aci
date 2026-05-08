mod diagnostics;
mod ids;
mod language;
mod model;
mod quality;
mod span;

pub use diagnostics::{AciError, Diagnostic, Result, Severity};
pub use ids::{
    EdgeId, EdgeTag, FileId, FileTag, Id, NodeId, NodeTag, PackageId, PackageTag, RepositoryId,
    RepositoryTag, SymbolId, SymbolTag,
};
pub use language::Language;
pub use model::{
    EdgeKind, GraphEdge, GraphNode, GraphPartition, GraphSnapshot, NodeKind, PartitionMetrics,
    SourceFile, SymbolKind, normalize_path,
};
pub use quality::{Confidence, FactProvenance, prefer_fact};
pub use span::{LineColumn, SourceSpan};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn ids_are_deterministic() {
        let left = RepositoryId::new("repo", &["/tmp/project"]);
        let right = RepositoryId::new("repo", &["/tmp/project"]);
        assert_eq!(left, right);
    }

    #[test]
    fn graph_partition_serializes() {
        let repo = RepositoryId::new("repo", &["example"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/src/main.py"),
            Language::Python,
            "def main():\n    pass\n".to_string(),
        );
        let span = SourceSpan::new(0, 10, LineColumn::new(1, 1), LineColumn::new(1, 11));
        let node = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("main".to_string()),
            Some("main".to_string()),
            Some(span),
        )
        .with_symbol_kind(SymbolKind::Function);
        let mut partition = GraphPartition::empty(&file);
        partition.nodes.push(node);

        let json = serde_json::to_string(&partition).expect("serialize partition");
        let round_trip: GraphPartition =
            serde_json::from_str(&json).expect("deserialize partition");
        assert_eq!(round_trip, partition);
    }

    #[test]
    fn conflict_resolution_prefers_higher_quality_facts() {
        assert!(prefer_fact(
            (FactProvenance::TreeSitter, Confidence::High),
            (FactProvenance::Scip, Confidence::Exact)
        ));
        assert!(!prefer_fact(
            (FactProvenance::Compiler, Confidence::High),
            (FactProvenance::Lsp, Confidence::Exact)
        ));
    }

    #[test]
    fn path_normalization_uses_forward_slashes() {
        assert_eq!(normalize_path(Path::new("./src/lib.rs")), "src/lib.rs");
    }
}
