use aci_core::{
    EdgeKind, FileId, GraphEdge, GraphNode, GraphSnapshot, NodeId, NodeKind, SymbolKind,
    prefer_fact,
};
use aci_store::{AdjacencyIndex, build_adjacency};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

pub struct QueryEngine {
    snapshot: GraphSnapshot,
    adjacency: AdjacencyIndex,
    nodes: BTreeMap<NodeId, GraphNode>,
}

impl QueryEngine {
    pub fn new(snapshot: GraphSnapshot) -> Self {
        let adjacency = build_adjacency(&snapshot);
        let nodes = snapshot
            .partitions
            .iter()
            .flat_map(|partition| partition.nodes.iter().cloned())
            .map(|node| (node.id.clone(), node))
            .collect();
        Self {
            snapshot,
            adjacency,
            nodes,
        }
    }

    pub fn symbols(&self) -> Vec<&GraphNode> {
        self.nodes
            .values()
            .filter(|node| node.kind == NodeKind::Symbol)
            .collect()
    }

    pub fn lookup_symbols(
        &self,
        name: Option<&str>,
        qualified_name: Option<&str>,
        file: Option<&Path>,
        kind: Option<SymbolKind>,
    ) -> Vec<&GraphNode> {
        let mut selected = BTreeMap::<SymbolKey, &GraphNode>::new();
        for node in self
            .symbols()
            .into_iter()
            .filter(|node| name.is_none_or(|name| node.name.as_deref() == Some(name)))
            .filter(|node| {
                qualified_name
                    .is_none_or(|qualified| node.qualified_name.as_deref() == Some(qualified))
            })
            .filter(|node| kind.is_none_or(|kind| node.symbol_kind == Some(kind)))
            .filter(|node| {
                file.is_none_or(|file| {
                    node.file_id
                        .as_ref()
                        .and_then(|file_id| self.path_for_file(file_id))
                        .is_some_and(|path| path == file)
                })
            })
        {
            let key = SymbolKey::from_node(node);
            match selected.get(&key) {
                Some(existing)
                    if prefer_fact(
                        (existing.provenance, existing.confidence),
                        (node.provenance, node.confidence),
                    ) =>
                {
                    selected.insert(key, node);
                }
                None => {
                    selected.insert(key, node);
                }
                _ => {}
            }
        }
        selected.into_values().collect()
    }

    pub fn matching_symbols(&self, symbol_name: &str) -> Vec<&GraphNode> {
        self.symbols()
            .into_iter()
            .filter(|node| {
                node.name.as_deref() == Some(symbol_name)
                    || node.qualified_name.as_deref() == Some(symbol_name)
            })
            .collect()
    }

    pub fn file_dependencies(&self, file: &Path) -> Vec<String> {
        self.partition_for_path(file)
            .map(|partition| {
                partition
                    .edges
                    .iter()
                    .filter(|edge| {
                        edge.kind == EdgeKind::Imports || edge.kind == EdgeKind::DependsOn
                    })
                    .filter_map(|edge| self.nodes.get(&edge.to))
                    .filter_map(|node| node.qualified_name.clone().or_else(|| node.name.clone()))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn package_dependencies(&self) -> Vec<String> {
        self.snapshot
            .partitions
            .iter()
            .flat_map(|partition| &partition.nodes)
            .filter(|node| node.kind == NodeKind::Package)
            .filter_map(|node| node.name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn callees(&self, symbol: &NodeId) -> Vec<&GraphNode> {
        self.edges_from(symbol, EdgeKind::Calls)
            .into_iter()
            .filter_map(|edge| self.nodes.get(&edge.to))
            .collect()
    }

    pub fn callers(&self, symbol_name: &str) -> Vec<&GraphNode> {
        let targets: BTreeSet<NodeId> = self
            .nodes
            .values()
            .filter(|node| {
                node.name.as_deref() == Some(symbol_name)
                    || node.qualified_name.as_deref() == Some(symbol_name)
            })
            .map(|node| node.id.clone())
            .collect();
        targets
            .iter()
            .flat_map(|target| self.edges_to(target, EdgeKind::Calls))
            .filter_map(|edge| self.nodes.get(&edge.from))
            .collect()
    }

    pub fn references(&self, target_name: &str) -> Vec<&GraphNode> {
        let targets: BTreeSet<NodeId> = self
            .nodes
            .values()
            .filter(|node| {
                node.name.as_deref() == Some(target_name)
                    || node.qualified_name.as_deref() == Some(target_name)
            })
            .map(|node| node.id.clone())
            .collect();
        targets
            .iter()
            .flat_map(|target| self.edges_to(target, EdgeKind::References))
            .filter_map(|edge| self.nodes.get(&edge.from))
            .collect()
    }

    pub fn impact_from_files(&self, files: &[PathBuf]) -> Vec<PathBuf> {
        let changed: BTreeSet<FileId> = files
            .iter()
            .filter_map(|file| {
                self.partition_for_path(file)
                    .map(|partition| partition.file_id.clone())
            })
            .collect();
        let mut impacted = BTreeSet::new();
        for partition in &self.snapshot.partitions {
            let depends_on_changed = partition.edges.iter().any(|edge| {
                matches!(edge.kind, EdgeKind::Imports | EdgeKind::DependsOn)
                    && self
                        .nodes
                        .get(&edge.to)
                        .and_then(|node| node.file_id.clone())
                        .is_some_and(|file_id| changed.contains(&file_id))
            });
            if changed.contains(&partition.file_id) || depends_on_changed {
                impacted.insert(partition.path.clone());
            }
        }
        impacted.into_iter().collect()
    }

    pub fn traverse_dependencies(&self, start: &NodeId, max_depth: usize) -> Vec<&GraphNode> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([(start.clone(), 0_usize)]);
        while let Some((node, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            for edge in self.edges_from(&node, EdgeKind::DependsOn) {
                if seen.insert(edge.to.clone()) {
                    queue.push_back((edge.to.clone(), depth + 1));
                }
            }
        }
        seen.iter().filter_map(|id| self.nodes.get(id)).collect()
    }

    pub fn path_for_node(&self, node: &GraphNode) -> Option<&Path> {
        node.file_id
            .as_ref()
            .and_then(|file_id| self.path_for_file(file_id))
    }

    fn edges_from(&self, node: &NodeId, kind: EdgeKind) -> Vec<&GraphEdge> {
        self.adjacency
            .outgoing
            .get(node)
            .into_iter()
            .flatten()
            .filter(|edge| edge.kind == kind)
            .collect()
    }

    fn edges_to(&self, node: &NodeId, kind: EdgeKind) -> Vec<&GraphEdge> {
        self.adjacency
            .incoming
            .get(node)
            .into_iter()
            .flatten()
            .filter(|edge| edge.kind == kind)
            .collect()
    }

    fn partition_for_path(&self, file: &Path) -> Option<&aci_core::GraphPartition> {
        self.snapshot
            .partitions
            .iter()
            .find(|partition| partition.path == file)
    }

    fn path_for_file(&self, file_id: &FileId) -> Option<&Path> {
        self.snapshot
            .partitions
            .iter()
            .find(|partition| &partition.file_id == file_id)
            .map(|partition| partition.path.as_path())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SymbolKey {
    file_id: Option<FileId>,
    name: Option<String>,
    qualified_name: Option<String>,
    kind: Option<SymbolKind>,
}

impl SymbolKey {
    fn from_node(node: &GraphNode) -> Self {
        Self {
            file_id: node.file_id.clone(),
            name: node.name.clone(),
            qualified_name: node.qualified_name.clone(),
            kind: node.symbol_kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aci_core::{
        Confidence, EdgeKind, FactProvenance, GraphEdge, GraphNode, GraphPartition, Language,
        NodeKind, RepositoryId, SourceFile, SymbolKind,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn finds_symbols_and_callees() {
        let repo = RepositoryId::new("repo", &["query"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/app.py"),
            Language::Python,
            "def a():\n    b()\n".to_string(),
        );
        let a = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("a".to_string()),
            Some("a".to_string()),
            Some(aci_core::SourceSpan::new(
                0,
                5,
                aci_core::LineColumn::new(1, 1),
                aci_core::LineColumn::new(1, 6),
            )),
        )
        .with_symbol_kind(SymbolKind::Function);
        let b = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("b".to_string()),
            Some("b".to_string()),
            None,
        )
        .with_symbol_kind(SymbolKind::Function);
        let edge = GraphEdge::deterministic(EdgeKind::Calls, &a.id, &b.id, None);
        let mut partition = GraphPartition::empty(&file);
        partition.nodes = vec![a.clone(), b.clone()];
        partition.edges = vec![edge];

        let engine = QueryEngine::new(GraphSnapshot {
            partitions: vec![partition],
        });
        assert_eq!(engine.lookup_symbols(Some("a"), None, None, None), vec![&a]);
        assert_eq!(engine.callees(&a.id), vec![&b]);
        assert_eq!(engine.matching_symbols("a"), vec![&a]);
        assert_eq!(engine.path_for_node(&a), Some(Path::new("/repo/app.py")));
    }

    #[test]
    fn finds_packages_references_and_dependency_traversal() {
        let repo = RepositoryId::new("repo", &["query-relationships"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/app.py"),
            Language::Python,
            "import requests\n".to_string(),
        );
        let app = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("app".to_string()),
            Some("app".to_string()),
            None,
        )
        .with_symbol_kind(SymbolKind::Module);
        let package = GraphNode::deterministic(
            &repo,
            None,
            NodeKind::Package,
            Language::Unknown,
            Some("requests".to_string()),
            Some("requests".to_string()),
            None,
        );
        let reference = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("use_app".to_string()),
            Some("use_app".to_string()),
            None,
        )
        .with_symbol_kind(SymbolKind::Function);
        let mut partition = GraphPartition::empty(&file);
        partition.nodes = vec![app.clone(), package.clone(), reference.clone()];
        partition.edges = vec![
            GraphEdge::deterministic(EdgeKind::DependsOn, &app.id, &package.id, None),
            GraphEdge::deterministic(EdgeKind::References, &reference.id, &app.id, None),
        ];

        let engine = QueryEngine::new(GraphSnapshot {
            partitions: vec![partition],
        });

        assert_eq!(engine.package_dependencies(), vec!["requests".to_string()]);
        assert_eq!(engine.references("app"), vec![&reference]);
        let dependencies = engine.traverse_dependencies(&app.id, 2);
        assert_eq!(dependencies, vec![&package]);
    }

    #[test]
    fn lookup_prefers_higher_provenance_duplicate_symbols() {
        let repo = RepositoryId::new("repo", &["query-quality"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/app.py"),
            Language::Python,
            "def a(): pass\n".to_string(),
        );
        let scanner = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("a".to_string()),
            Some("a".to_string()),
            None,
        )
        .with_symbol_kind(SymbolKind::Function)
        .with_fact_quality(FactProvenance::StructuralScanner, Confidence::Medium);
        let semantic = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("a".to_string()),
            Some("a".to_string()),
            Some(aci_core::SourceSpan::new(
                0,
                5,
                aci_core::LineColumn::new(1, 1),
                aci_core::LineColumn::new(1, 6),
            )),
        )
        .with_symbol_kind(SymbolKind::Function)
        .with_fact_quality(FactProvenance::Scip, Confidence::Exact);
        let mut partition = GraphPartition::empty(&file);
        partition.nodes = vec![scanner, semantic.clone()];

        let engine = QueryEngine::new(GraphSnapshot {
            partitions: vec![partition],
        });
        assert_eq!(
            engine.lookup_symbols(Some("a"), None, None, None),
            vec![&semantic]
        );
    }
}
