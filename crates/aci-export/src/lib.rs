use aci_core::{
    Confidence, EdgeKind, FactProvenance, GraphEdge, GraphNode, GraphPartition, GraphSnapshot,
    Language, LineColumn, NodeKind, PartitionMetrics, RepositoryId, Result, SourceFile, SourceSpan,
    SymbolKind,
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportFormat {
    Jsonl,
    KiteDb,
    Scip,
    Lsif,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "jsonl" => Ok(Self::Jsonl),
            "kitedb" => Ok(Self::KiteDb),
            "scip" => Ok(Self::Scip),
            "lsif" => Ok(Self::Lsif),
            other => Err(format!("unsupported export format: {other}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum JsonlRecord {
    Partition {
        file_id: String,
        path: String,
        language: Language,
        fingerprint: String,
        metrics: PartitionMetrics,
    },
    Node {
        partition: String,
        node: GraphNode,
    },
    Edge {
        partition: String,
        edge: GraphEdge,
    },
    Diagnostic {
        partition: String,
        diagnostic: aci_core::Diagnostic,
    },
}

#[derive(Clone, Debug, Serialize)]
struct KiteRecord<'a> {
    path: String,
    name: Option<&'a str>,
    qualified_name: Option<&'a str>,
    kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct ScipDocument<'a> {
    version: u32,
    documents: Vec<ScipFile<'a>>,
}

#[derive(Clone, Debug, Serialize)]
struct ScipFile<'a> {
    relative_path: String,
    symbols: Vec<&'a GraphNode>,
}

pub fn export_snapshot<W: Write>(
    snapshot: &GraphSnapshot,
    format: ExportFormat,
    mut writer: W,
) -> Result<()> {
    match format {
        ExportFormat::Jsonl => write_jsonl(snapshot, writer),
        ExportFormat::KiteDb => write_kitedb(snapshot, writer),
        ExportFormat::Scip => {
            let document = ScipDocument {
                version: 1,
                documents: snapshot
                    .partitions
                    .iter()
                    .map(|partition| ScipFile {
                        relative_path: partition.path.to_string_lossy().replace('\\', "/"),
                        symbols: partition
                            .nodes
                            .iter()
                            .filter(|node| node.symbol_kind.is_some())
                            .collect(),
                    })
                    .collect(),
            };
            serde_json::to_writer_pretty(&mut writer, &document)?;
            writeln!(writer)?;
            Ok(())
        }
        ExportFormat::Lsif => write_lsif(snapshot, writer),
    }
}

pub fn import_jsonl<R: BufRead>(reader: R) -> Result<GraphSnapshot> {
    let mut partitions: Vec<GraphPartition> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record: JsonlRecord = serde_json::from_str(&line)?;
        match record {
            JsonlRecord::Partition {
                file_id,
                path,
                language,
                fingerprint,
                metrics,
            } => partitions.push(GraphPartition {
                file_id: aci_core::FileId::from_raw(file_id),
                path: path.into(),
                language,
                fingerprint,
                nodes: Vec::new(),
                edges: Vec::new(),
                diagnostics: Vec::new(),
                metrics,
            }),
            JsonlRecord::Node { partition, node } => {
                if let Some(target) = partitions
                    .iter_mut()
                    .find(|item| item.file_id.as_str() == partition)
                {
                    target.nodes.push(node);
                }
            }
            JsonlRecord::Edge { partition, edge } => {
                if let Some(target) = partitions
                    .iter_mut()
                    .find(|item| item.file_id.as_str() == partition)
                {
                    target.edges.push(edge);
                }
            }
            JsonlRecord::Diagnostic {
                partition,
                diagnostic,
            } => {
                if let Some(target) = partitions
                    .iter_mut()
                    .find(|item| item.file_id.as_str() == partition)
                {
                    target.diagnostics.push(diagnostic);
                }
            }
        }
    }
    Ok(GraphSnapshot { partitions })
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScipInput {
    pub documents: Vec<ScipInputDocument>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScipInputDocument {
    #[serde(rename = "relativePath")]
    pub relative_path: String,
    #[serde(default)]
    pub occurrences: Vec<ScipOccurrence>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScipOccurrence {
    pub symbol: String,
    #[serde(default)]
    pub range: Vec<u32>,
    #[serde(default)]
    pub roles: u32,
}

pub fn import_scip_enrichment<R: Read>(
    repo_id: RepositoryId,
    repo_root: &Path,
    reader: R,
) -> Result<GraphSnapshot> {
    let input: ScipInput = serde_json::from_reader(reader)?;
    let mut snapshot = GraphSnapshot::default();
    for document in input.documents {
        let path = repo_root.join(&document.relative_path);
        let source = SourceFile::new(
            repo_id.clone(),
            repo_root,
            path,
            Language::Unknown,
            String::new(),
        );
        let mut partition = GraphPartition::empty(&source);
        let file_node = GraphNode::deterministic(
            &repo_id,
            Some(&source.file_id),
            NodeKind::File,
            Language::Unknown,
            Some(document.relative_path.clone()),
            Some(document.relative_path),
            None,
        )
        .with_fact_quality(FactProvenance::Scip, Confidence::Exact);
        let file_node_id = file_node.id.clone();
        partition.nodes.push(file_node);

        for occurrence in document.occurrences {
            let name = occurrence
                .symbol
                .rsplit(['/', '#', '.'])
                .next()
                .filter(|value| !value.is_empty())
                .unwrap_or(occurrence.symbol.as_str());
            let span = scip_range_to_span(&occurrence.range);
            let target = GraphNode::deterministic(
                &repo_id,
                Some(&source.file_id),
                NodeKind::Symbol,
                Language::Unknown,
                Some(name.to_string()),
                Some(occurrence.symbol.clone()),
                span.clone(),
            )
            .with_symbol_kind(SymbolKind::Unknown)
            .with_fact_quality(FactProvenance::Scip, Confidence::Exact);
            let target_id = target.id.clone();
            partition.nodes.push(target);
            let edge_kind = if occurrence.roles & 1 == 1 {
                EdgeKind::Defines
            } else {
                EdgeKind::References
            };
            partition.edges.push(
                GraphEdge::deterministic(edge_kind, &file_node_id, &target_id, span)
                    .with_fact_quality(FactProvenance::Scip, Confidence::Exact),
            );
        }
        snapshot.replace_partition(partition);
    }
    Ok(snapshot)
}

#[derive(Clone, Debug, Deserialize)]
pub struct LspInput {
    pub documents: Vec<LspInputDocument>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LspInputDocument {
    pub uri: String,
    #[serde(default)]
    pub facts: Vec<LspFact>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LspFact {
    pub symbol: String,
    pub kind: LspFactKind,
    pub range: LspRange,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum LspFactKind {
    Definition,
    Reference,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

pub fn import_lsp_enrichment<R: Read>(
    repo_id: RepositoryId,
    repo_root: &Path,
    reader: R,
) -> Result<GraphSnapshot> {
    let input: LspInput = serde_json::from_reader(reader)?;
    let mut snapshot = GraphSnapshot::default();
    for document in input.documents {
        let path = lsp_uri_to_path(repo_root, &document.uri);
        let source = SourceFile::new(
            repo_id.clone(),
            repo_root,
            path.clone(),
            Language::Unknown,
            String::new(),
        );
        let mut partition = GraphPartition::empty(&source);
        let file_node = GraphNode::deterministic(
            &repo_id,
            Some(&source.file_id),
            NodeKind::File,
            Language::Unknown,
            path.file_name()
                .map(|name| name.to_string_lossy().to_string()),
            Some(path.to_string_lossy().replace('\\', "/")),
            None,
        )
        .with_fact_quality(FactProvenance::Lsp, Confidence::High);
        let file_node_id = file_node.id.clone();
        partition.nodes.push(file_node);

        for fact in document.facts {
            let span = Some(lsp_range_to_span(&fact.range));
            let name = fact
                .symbol
                .rsplit(['/', '#', '.'])
                .next()
                .filter(|value| !value.is_empty())
                .unwrap_or(fact.symbol.as_str());
            let node = GraphNode::deterministic(
                &repo_id,
                Some(&source.file_id),
                NodeKind::Symbol,
                Language::Unknown,
                Some(name.to_string()),
                Some(fact.symbol),
                span.clone(),
            )
            .with_symbol_kind(SymbolKind::Unknown)
            .with_fact_quality(FactProvenance::Lsp, Confidence::High);
            let node_id = node.id.clone();
            partition.nodes.push(node);
            let edge_kind = match fact.kind {
                LspFactKind::Definition => EdgeKind::Defines,
                LspFactKind::Reference => EdgeKind::References,
            };
            partition.edges.push(
                GraphEdge::deterministic(edge_kind, &file_node_id, &node_id, span)
                    .with_fact_quality(FactProvenance::Lsp, Confidence::High),
            );
        }
        snapshot.replace_partition(partition);
    }
    Ok(snapshot)
}

fn lsp_uri_to_path(repo_root: &Path, uri: &str) -> std::path::PathBuf {
    uri.strip_prefix("file://")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| repo_root.join(uri))
}

fn lsp_range_to_span(range: &LspRange) -> SourceSpan {
    SourceSpan::new(
        0,
        0,
        LineColumn::new(range.start.line + 1, range.start.character + 1),
        LineColumn::new(range.end.line + 1, range.end.character + 1),
    )
}

fn scip_range_to_span(range: &[u32]) -> Option<SourceSpan> {
    if range.len() < 3 {
        return None;
    }
    let start_line = range[0] + 1;
    let start_col = range[1] + 1;
    let end_col = range[2] + 1;
    Some(SourceSpan::new(
        0,
        0,
        LineColumn::new(start_line, start_col),
        LineColumn::new(start_line, end_col),
    ))
}

fn write_jsonl<W: Write>(snapshot: &GraphSnapshot, mut writer: W) -> Result<()> {
    for partition in &snapshot.partitions {
        write_record(
            &mut writer,
            &JsonlRecord::Partition {
                file_id: partition.file_id.to_string(),
                path: partition.path.to_string_lossy().to_string(),
                language: partition.language,
                fingerprint: partition.fingerprint.clone(),
                metrics: partition.metrics.clone(),
            },
        )?;
        for node in &partition.nodes {
            write_record(
                &mut writer,
                &JsonlRecord::Node {
                    partition: partition.file_id.to_string(),
                    node: node.clone(),
                },
            )?;
        }
        for edge in &partition.edges {
            write_record(
                &mut writer,
                &JsonlRecord::Edge {
                    partition: partition.file_id.to_string(),
                    edge: edge.clone(),
                },
            )?;
        }
        for diagnostic in &partition.diagnostics {
            write_record(
                &mut writer,
                &JsonlRecord::Diagnostic {
                    partition: partition.file_id.to_string(),
                    diagnostic: diagnostic.clone(),
                },
            )?;
        }
    }
    Ok(())
}

fn write_kitedb<W: Write>(snapshot: &GraphSnapshot, mut writer: W) -> Result<()> {
    for node in snapshot
        .partitions
        .iter()
        .flat_map(|partition| &partition.nodes)
    {
        if node.symbol_kind.is_some() {
            let record = KiteRecord {
                path: node
                    .file_id
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                name: node.name.as_deref(),
                qualified_name: node.qualified_name.as_deref(),
                kind: "symbol",
            };
            serde_json::to_writer(&mut writer, &record)?;
            writeln!(writer)?;
        }
    }
    Ok(())
}

fn write_lsif<W: Write>(snapshot: &GraphSnapshot, mut writer: W) -> Result<()> {
    let mut id = 1_u64;
    for partition in &snapshot.partitions {
        serde_json::to_writer(
            &mut writer,
            &serde_json::json!({
                "id": id,
                "type": "vertex",
                "label": "document",
                "uri": partition.path.to_string_lossy(),
            }),
        )?;
        writeln!(writer)?;
        id += 1;
        for node in partition
            .nodes
            .iter()
            .filter(|node| node.symbol_kind.is_some())
        {
            serde_json::to_writer(
                &mut writer,
                &serde_json::json!({
                    "id": id,
                    "type": "vertex",
                    "label": "resultSet",
                    "symbol": node.qualified_name,
                }),
            )?;
            writeln!(writer)?;
            id += 1;
        }
    }
    Ok(())
}

fn write_record<W: Write, T: Serialize>(writer: &mut W, record: &T) -> Result<()> {
    serde_json::to_writer(&mut *writer, record)?;
    writeln!(writer)?;
    Ok(())
}

#[cfg(test)]
mod tests;
