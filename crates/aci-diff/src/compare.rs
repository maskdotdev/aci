use crate::{
    ChangeKind, ChangedSymbol, DependencyChange, DiffDiagnostic, DiffReport, FileChange,
    ImpactedFile, IndexedRef, RefSide, RefSummary, RiskLevel, SymbolSummary,
    stats::{StatsInput, stats},
};
use aci_core::{
    EdgeKind, FileId, GraphNode, GraphPartition, GraphSnapshot, Language, NodeId, NodeKind,
    SourceSpan, SymbolKind, normalize_path,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub(crate) fn compare_refs(
    base: IndexedRef,
    head: IndexedRef,
    changed_files: Vec<FileChange>,
) -> aci_core::Result<DiffReport> {
    let rename_map = rename_map(&changed_files);
    let base_index = SnapshotIndex::new(&base.root, &base.snapshot, &rename_map);
    let head_index = SnapshotIndex::new(&head.root, &head.snapshot, &BTreeMap::new());
    let changed_symbols = compare_symbols(&base_index, &head_index);
    let public_api_changes = changed_symbols
        .iter()
        .filter(|change| change.public_api)
        .cloned()
        .collect::<Vec<_>>();
    let dependency_changes = compare_dependencies(&base_index, &head_index);
    let impacted_files = impacted_files(
        &base_index,
        &head_index,
        &changed_files,
        &changed_symbols,
        &dependency_changes,
    );
    let diagnostics = collect_diagnostics(&base, RefSide::Base)
        .into_iter()
        .chain(collect_diagnostics(&head, RefSide::Head))
        .collect::<Vec<_>>();
    let stats = stats(StatsInput {
        files: &changed_files,
        symbols: &changed_symbols,
        public_api_changes: public_api_changes.len(),
        dependency_changes: dependency_changes.len(),
        impacted_files: impacted_files.len(),
        diagnostics: diagnostics.len(),
        base: &base,
        head: &head,
    });
    Ok(DiffReport {
        base: RefSummary {
            name: base.label,
            commit: base.commit,
        },
        head: RefSummary {
            name: head.label,
            commit: head.commit,
        },
        changed_files,
        changed_symbols,
        public_api_changes,
        dependency_changes,
        impacted_files,
        diagnostics,
        stats,
    })
}

struct SnapshotIndex<'a> {
    root: &'a Path,
    partitions: &'a [GraphPartition],
    nodes_by_id: BTreeMap<NodeId, &'a GraphNode>,
    file_paths: BTreeMap<FileId, String>,
    symbols: BTreeMap<SymbolKey, SymbolEntry>,
    dependencies: BTreeMap<DependencyKey, DependencyEntry>,
}

impl<'a> SnapshotIndex<'a> {
    fn new(
        root: &'a Path,
        snapshot: &'a GraphSnapshot,
        path_rewrites: &BTreeMap<String, String>,
    ) -> Self {
        let nodes_by_id = snapshot
            .partitions
            .iter()
            .flat_map(|partition| partition.nodes.iter())
            .map(|node| (node.id.clone(), node))
            .collect::<BTreeMap<_, _>>();
        let file_paths = snapshot
            .partitions
            .iter()
            .map(|partition| {
                (
                    partition.file_id.clone(),
                    relative_path(root, &partition.path),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut index = Self {
            root,
            partitions: &snapshot.partitions,
            nodes_by_id,
            file_paths,
            symbols: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        };
        index.symbols = build_symbols(&index, path_rewrites);
        index.dependencies = build_dependencies(&index, path_rewrites);
        index
    }

    fn node_label(&self, id: &NodeId) -> Option<String> {
        self.nodes_by_id
            .get(id)
            .and_then(|node| node.qualified_name.clone().or_else(|| node.name.clone()))
    }

    fn file_for_node(&self, id: &NodeId) -> Option<String> {
        self.nodes_by_id
            .get(id)
            .and_then(|node| node.file_id.as_ref())
            .and_then(|file_id| self.file_paths.get(file_id))
            .cloned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SymbolKey {
    file: String,
    language: Language,
    kind: Option<SymbolKind>,
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SymbolEntry {
    summary: SymbolSummary,
    fingerprint: SymbolFingerprint,
    public_api: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SymbolFingerprint {
    span: Option<SourceSpan>,
    source_hash: Option<String>,
    provenance: String,
    confidence: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct DependencyKey {
    file: String,
    dependency: String,
    edge_kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DependencyEntry {
    file: String,
    dependency: String,
    edge_kind: EdgeKind,
}

fn build_symbols(
    index: &SnapshotIndex<'_>,
    path_rewrites: &BTreeMap<String, String>,
) -> BTreeMap<SymbolKey, SymbolEntry> {
    let mut symbols = BTreeMap::new();
    for partition in index.partitions {
        let file = relative_path(index.root, &partition.path);
        let comparison_file = path_rewrites.get(&file).cloned().unwrap_or(file.clone());
        let exports = exported_names(partition);
        let source = fs::read_to_string(&partition.path).ok();
        for node in partition
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Symbol)
        {
            let display_name = node
                .qualified_name
                .clone()
                .or_else(|| node.name.clone())
                .unwrap_or_default();
            let key_name = node.name.clone().unwrap_or_else(|| display_name.clone());
            let source_hash = source
                .as_deref()
                .and_then(|text| node.span.as_ref().and_then(|span| span_hash(text, span)));
            let snippet = source
                .as_deref()
                .and_then(|text| node.span.as_ref().and_then(|span| span_text(text, span)));
            let public_api = is_public_api(node, &exports, snippet);
            let key = SymbolKey {
                file: comparison_file.clone(),
                language: node.language,
                kind: node.symbol_kind,
                name: key_name,
            };
            let summary = SymbolSummary {
                name: node.name.clone().unwrap_or_else(|| display_name.clone()),
                qualified_name: node.qualified_name.clone(),
                kind: node.symbol_kind,
                language: node.language,
                file: file.clone(),
                span: node.span.clone(),
            };
            let entry = SymbolEntry {
                summary,
                fingerprint: SymbolFingerprint {
                    span: node.span.clone(),
                    source_hash,
                    provenance: format!("{:?}", node.provenance),
                    confidence: format!("{:?}", node.confidence),
                },
                public_api,
            };
            symbols.insert(key, entry);
        }
    }
    symbols
}

fn build_dependencies(
    index: &SnapshotIndex<'_>,
    path_rewrites: &BTreeMap<String, String>,
) -> BTreeMap<DependencyKey, DependencyEntry> {
    let mut dependencies = BTreeMap::new();
    for partition in index.partitions {
        let file = relative_path(index.root, &partition.path);
        let comparison_file = path_rewrites.get(&file).cloned().unwrap_or(file);
        for edge in &partition.edges {
            if !matches!(edge.kind, EdgeKind::Imports | EdgeKind::DependsOn) {
                continue;
            }
            if let Some(dependency) = index.node_label(&edge.to) {
                let key = DependencyKey {
                    file: comparison_file.clone(),
                    dependency: dependency.clone(),
                    edge_kind: format!("{:?}", edge.kind),
                };
                dependencies.insert(
                    key,
                    DependencyEntry {
                        file: comparison_file.clone(),
                        dependency,
                        edge_kind: edge.kind,
                    },
                );
            }
        }
    }
    dependencies
}

fn compare_symbols(base: &SnapshotIndex<'_>, head: &SnapshotIndex<'_>) -> Vec<ChangedSymbol> {
    let keys = base
        .symbols
        .keys()
        .chain(head.symbols.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for key in keys {
        match (base.symbols.get(&key), head.symbols.get(&key)) {
            (None, Some(after)) => changes.push(symbol_change(
                ChangeKind::Added,
                None,
                Some(after),
                "symbol added",
            )),
            (Some(before), None) => changes.push(symbol_change(
                ChangeKind::Removed,
                Some(before),
                None,
                "symbol removed",
            )),
            (Some(before), Some(after)) if before.fingerprint != after.fingerprint => {
                changes.push(symbol_change(
                    ChangeKind::Modified,
                    Some(before),
                    Some(after),
                    "symbol body or span changed",
                ));
            }
            _ => {}
        }
    }
    changes
}

fn symbol_change(
    change: ChangeKind,
    before: Option<&SymbolEntry>,
    after: Option<&SymbolEntry>,
    reason: &str,
) -> ChangedSymbol {
    let public_api = before.map(|entry| entry.public_api).unwrap_or(false)
        || after.map(|entry| entry.public_api).unwrap_or(false);
    ChangedSymbol {
        change,
        before: before.map(|entry| entry.summary.clone()),
        after: after.map(|entry| entry.summary.clone()),
        public_api,
        risk: risk_for(change, public_api),
        reason: reason.to_string(),
    }
}

fn compare_dependencies(
    base: &SnapshotIndex<'_>,
    head: &SnapshotIndex<'_>,
) -> Vec<DependencyChange> {
    let keys = base
        .dependencies
        .keys()
        .chain(head.dependencies.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for key in keys {
        match (base.dependencies.get(&key), head.dependencies.get(&key)) {
            (None, Some(entry)) => changes.push(dependency_change(ChangeKind::Added, entry)),
            (Some(entry), None) => changes.push(dependency_change(ChangeKind::Removed, entry)),
            _ => {}
        }
    }
    changes
}

fn dependency_change(change: ChangeKind, entry: &DependencyEntry) -> DependencyChange {
    DependencyChange {
        change,
        file: entry.file.clone(),
        dependency: entry.dependency.clone(),
        edge_kind: entry.edge_kind,
    }
}

fn impacted_files(
    base: &SnapshotIndex<'_>,
    head: &SnapshotIndex<'_>,
    changed_files: &[FileChange],
    changed_symbols: &[ChangedSymbol],
    dependency_changes: &[DependencyChange],
) -> Vec<ImpactedFile> {
    let mut impacted = BTreeMap::<String, BTreeSet<String>>::new();
    for file in changed_files {
        impacted
            .entry(file.path.clone())
            .or_default()
            .insert(format!("file {}", change_label(file.change)));
    }

    let mut labels = BTreeSet::new();
    for symbol in changed_symbols {
        for summary in symbol.before.iter().chain(symbol.after.iter()) {
            labels.insert(summary.name.clone());
            if let Some(qualified) = &summary.qualified_name {
                labels.insert(qualified.clone());
            }
        }
    }
    for dependency in dependency_changes {
        labels.insert(dependency.dependency.clone());
    }
    add_impacts_from_edges(base, &labels, &mut impacted);
    add_impacts_from_edges(head, &labels, &mut impacted);

    impacted
        .into_iter()
        .map(|(path, reasons)| ImpactedFile {
            path,
            reasons: reasons.into_iter().collect(),
        })
        .collect()
}

fn add_impacts_from_edges(
    index: &SnapshotIndex<'_>,
    labels: &BTreeSet<String>,
    impacted: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for partition in index.partitions {
        for edge in &partition.edges {
            if !matches!(
                edge.kind,
                EdgeKind::Calls | EdgeKind::References | EdgeKind::Imports | EdgeKind::DependsOn
            ) {
                continue;
            }
            let Some(label) = index.node_label(&edge.to) else {
                continue;
            };
            if !labels.contains(&label) {
                continue;
            }
            let file = index
                .file_for_node(&edge.from)
                .unwrap_or_else(|| relative_path(index.root, &partition.path));
            impacted.entry(file).or_default().insert(format!(
                "{} {}",
                edge_label(edge.kind),
                label
            ));
        }
    }
}

fn collect_diagnostics(indexed: &IndexedRef, side: RefSide) -> Vec<DiffDiagnostic> {
    let file_paths = indexed
        .snapshot
        .partitions
        .iter()
        .map(|partition| {
            (
                partition.file_id.clone(),
                relative_path(&indexed.root, &partition.path),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = indexed
        .diagnostics
        .iter()
        .map(|diagnostic| DiffDiagnostic {
            reference: side,
            file: diagnostic
                .file_id
                .as_ref()
                .and_then(|file_id| file_paths.get(file_id))
                .cloned(),
            severity: diagnostic.severity,
            message: diagnostic.message.clone(),
            span: diagnostic.span.clone(),
        })
        .collect::<Vec<_>>();
    for partition in &indexed.snapshot.partitions {
        let file = relative_path(&indexed.root, &partition.path);
        diagnostics.extend(
            partition
                .diagnostics
                .iter()
                .map(|diagnostic| DiffDiagnostic {
                    reference: side,
                    file: Some(file.clone()),
                    severity: diagnostic.severity,
                    message: diagnostic.message.clone(),
                    span: diagnostic.span.clone(),
                }),
        );
    }
    diagnostics
}

fn rename_map(changes: &[FileChange]) -> BTreeMap<String, String> {
    changes
        .iter()
        .filter(|change| change.change == ChangeKind::Renamed)
        .filter_map(|change| {
            change
                .old_path
                .as_ref()
                .map(|old_path| (old_path.clone(), change.path.clone()))
        })
        .collect()
}

fn exported_names(partition: &GraphPartition) -> BTreeSet<String> {
    partition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Export)
        .filter_map(|node| node.name.clone().or_else(|| node.qualified_name.clone()))
        .collect()
}

fn is_public_api(
    node: &GraphNode,
    exported_names: &BTreeSet<String>,
    snippet: Option<&str>,
) -> bool {
    let name_matches_export = node
        .name
        .as_ref()
        .is_some_and(|name| exported_names.contains(name))
        || node
            .qualified_name
            .as_ref()
            .is_some_and(|name| exported_names.contains(name));
    if name_matches_export {
        return true;
    }
    let Some(snippet) = snippet else {
        return false;
    };
    let first_code = snippet
        .lines()
        .map(str::trim_start)
        .find(|line| !line.is_empty() && !line.starts_with("#["));
    first_code.is_some_and(|line| {
        line.starts_with("pub ") || line.starts_with("pub(") || line.starts_with("export ")
    })
}

fn span_hash(text: &str, span: &SourceSpan) -> Option<String> {
    span_text(text, span).map(|snippet| blake3::hash(snippet.as_bytes()).to_hex().to_string())
}

fn span_text<'a>(text: &'a str, span: &SourceSpan) -> Option<&'a str> {
    text.get(span.byte_start as usize..span.byte_end as usize)
}

fn relative_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    normalize_path(relative)
}

fn risk_for(change: ChangeKind, public_api: bool) -> RiskLevel {
    match (change, public_api) {
        (ChangeKind::Removed, true) => RiskLevel::High,
        (ChangeKind::Modified | ChangeKind::TypeChanged, true) => RiskLevel::Medium,
        (_, true) => RiskLevel::Medium,
        (ChangeKind::Removed, false) => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}

fn change_label(change: ChangeKind) -> &'static str {
    match change {
        ChangeKind::Added => "added",
        ChangeKind::Removed => "removed",
        ChangeKind::Modified => "modified",
        ChangeKind::Renamed => "renamed",
        ChangeKind::TypeChanged => "type-changed",
        ChangeKind::Copied => "copied",
    }
}

fn edge_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Imports => "imports",
        EdgeKind::DependsOn => "depends on",
        EdgeKind::Calls => "calls",
        EdgeKind::References => "references",
        _ => "uses",
    }
}
