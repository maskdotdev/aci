use aci_core::{
    AciError, Confidence, Diagnostic, EdgeId, EdgeKind, FactProvenance, FileId, GraphEdge,
    GraphNode, GraphPartition, Language, LineColumn, NodeId, NodeKind, PartitionMetrics, Result,
    Severity, SourceSpan, SymbolKind,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct CompactPartition {
    #[serde(rename = "s")]
    strings: Vec<String>,
    #[serde(rename = "f")]
    file_id: u32,
    #[serde(rename = "p")]
    path: u32,
    #[serde(rename = "l")]
    language: Language,
    #[serde(rename = "h")]
    fingerprint: u32,
    #[serde(rename = "n")]
    nodes: Vec<CompactNode>,
    #[serde(rename = "e")]
    edges: Vec<CompactEdge>,
    #[serde(rename = "d")]
    diagnostics: Vec<CompactDiagnostic>,
    #[serde(rename = "m")]
    metrics: [u64; 3],
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactNode {
    #[serde(rename = "i")]
    id: u32,
    #[serde(rename = "k")]
    kind: NodeKind,
    #[serde(rename = "l", skip_serializing_if = "Option::is_none")]
    language: Option<Language>,
    #[serde(rename = "n", skip_serializing_if = "Option::is_none")]
    name: Option<u32>,
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    qualified_name: Option<u32>,
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    symbol_kind: Option<SymbolKind>,
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    file_id: Option<u32>,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    span: Option<CompactSpan>,
    #[serde(rename = "p", default, skip_serializing_if = "is_default_provenance")]
    provenance: FactProvenance,
    #[serde(rename = "c", default, skip_serializing_if = "is_default_confidence")]
    confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactEdge {
    #[serde(rename = "i")]
    id: u32,
    #[serde(rename = "k")]
    kind: EdgeKind,
    #[serde(rename = "f")]
    from: u32,
    #[serde(rename = "t")]
    to: u32,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    span: Option<CompactSpan>,
    #[serde(rename = "p", default, skip_serializing_if = "is_default_provenance")]
    provenance: FactProvenance,
    #[serde(rename = "c", default, skip_serializing_if = "is_default_confidence")]
    confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactDiagnostic {
    #[serde(rename = "s")]
    severity: Severity,
    #[serde(rename = "m")]
    message: u32,
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    file_id: Option<u32>,
    #[serde(rename = "r", skip_serializing_if = "Option::is_none")]
    span: Option<CompactSpan>,
}

type CompactSpan = [u32; 6];

#[derive(Default)]
struct StringTable {
    values: Vec<String>,
    indexes: HashMap<String, u32>,
}

impl StringTable {
    fn intern(&mut self, value: impl AsRef<str>) -> u32 {
        let value = value.as_ref();
        if let Some(index) = self.indexes.get(value) {
            return *index;
        }
        let index = self.values.len() as u32;
        self.values.push(value.to_string());
        self.indexes.insert(value.to_string(), index);
        index
    }
}

pub(crate) fn write_partition<W: Write>(writer: W, partition: &GraphPartition) -> Result<()> {
    Ok(serde_json::to_writer(
        writer,
        &CompactPartition::from_partition(partition),
    )?)
}

pub(crate) fn read_partition_line(line: &str) -> Result<GraphPartition> {
    let compact: CompactPartition = serde_json::from_str(line)?;
    compact.into_partition()
}

impl CompactPartition {
    fn from_partition(partition: &GraphPartition) -> Self {
        let mut strings = StringTable::default();
        let file_id = strings.intern(partition.file_id.as_str());
        let path = strings.intern(partition.path.to_string_lossy());
        let fingerprint = strings.intern(&partition.fingerprint);
        let nodes = partition
            .nodes
            .iter()
            .map(|node| CompactNode {
                id: strings.intern(node.id.as_str()),
                kind: node.kind,
                language: (node.language != partition.language).then_some(node.language),
                name: intern_option(&mut strings, node.name.as_deref()),
                qualified_name: intern_option(&mut strings, node.qualified_name.as_deref()),
                symbol_kind: node.symbol_kind,
                file_id: node
                    .file_id
                    .as_ref()
                    .map(|file_id| strings.intern(file_id.as_str())),
                span: node.span.as_ref().map(compact_span),
                provenance: node.provenance,
                confidence: node.confidence,
            })
            .collect();
        let edges = partition
            .edges
            .iter()
            .map(|edge| CompactEdge {
                id: strings.intern(edge.id.as_str()),
                kind: edge.kind,
                from: strings.intern(edge.from.as_str()),
                to: strings.intern(edge.to.as_str()),
                span: edge.span.as_ref().map(compact_span),
                provenance: edge.provenance,
                confidence: edge.confidence,
            })
            .collect();
        let diagnostics = partition
            .diagnostics
            .iter()
            .map(|diagnostic| CompactDiagnostic {
                severity: diagnostic.severity,
                message: strings.intern(&diagnostic.message),
                file_id: diagnostic
                    .file_id
                    .as_ref()
                    .map(|file_id| strings.intern(file_id.as_str())),
                span: diagnostic.span.as_ref().map(compact_span),
            })
            .collect();
        Self {
            strings: strings.values,
            file_id,
            path,
            language: partition.language,
            fingerprint,
            nodes,
            edges,
            diagnostics,
            metrics: [
                partition.metrics.parse_time_micros,
                partition.metrics.extraction_time_micros,
                partition.metrics.query_captures,
            ],
        }
    }

    fn into_partition(self) -> Result<GraphPartition> {
        let CompactPartition {
            strings,
            file_id,
            path,
            language,
            fingerprint,
            nodes,
            edges,
            diagnostics,
            metrics,
        } = self;
        let decoder = CompactDecoder { strings, language };
        Ok(GraphPartition {
            file_id: FileId::from_raw(decoder.string(file_id, "file id")?),
            path: PathBuf::from(decoder.string(path, "path")?),
            language,
            fingerprint: decoder.string(fingerprint, "fingerprint")?,
            nodes: nodes
                .into_iter()
                .map(|node| decoder.expand_node(node))
                .collect::<Result<_>>()?,
            edges: edges
                .into_iter()
                .map(|edge| decoder.expand_edge(edge))
                .collect::<Result<_>>()?,
            diagnostics: diagnostics
                .into_iter()
                .map(|diagnostic| decoder.expand_diagnostic(diagnostic))
                .collect::<Result<_>>()?,
            metrics: PartitionMetrics {
                parse_time_micros: metrics[0],
                extraction_time_micros: metrics[1],
                query_captures: metrics[2],
            },
        })
    }
}

struct CompactDecoder {
    strings: Vec<String>,
    language: Language,
}

impl CompactDecoder {
    fn expand_node(&self, node: CompactNode) -> Result<GraphNode> {
        Ok(GraphNode {
            id: NodeId::from_raw(self.string(node.id, "node id")?),
            kind: node.kind,
            language: node.language.unwrap_or(self.language),
            name: self.optional_string(node.name, "node name")?,
            qualified_name: self.optional_string(node.qualified_name, "qualified name")?,
            symbol_kind: node.symbol_kind,
            file_id: self.optional_file_id(node.file_id, "node file id")?,
            span: node.span.map(expand_span),
            provenance: node.provenance,
            confidence: node.confidence,
        })
    }

    fn expand_edge(&self, edge: CompactEdge) -> Result<GraphEdge> {
        Ok(GraphEdge {
            id: EdgeId::from_raw(self.string(edge.id, "edge id")?),
            kind: edge.kind,
            from: NodeId::from_raw(self.string(edge.from, "edge source")?),
            to: NodeId::from_raw(self.string(edge.to, "edge target")?),
            span: edge.span.map(expand_span),
            provenance: edge.provenance,
            confidence: edge.confidence,
        })
    }

    fn expand_diagnostic(&self, diagnostic: CompactDiagnostic) -> Result<Diagnostic> {
        Ok(Diagnostic {
            severity: diagnostic.severity,
            message: self.string(diagnostic.message, "diagnostic message")?,
            file_id: self.optional_file_id(diagnostic.file_id, "diagnostic file id")?,
            span: diagnostic.span.map(expand_span),
        })
    }

    fn optional_file_id(&self, index: Option<u32>, field: &str) -> Result<Option<FileId>> {
        index
            .map(|index| self.string(index, field).map(FileId::from_raw))
            .transpose()
    }

    fn optional_string(&self, index: Option<u32>, field: &str) -> Result<Option<String>> {
        index.map(|index| self.string(index, field)).transpose()
    }

    fn string(&self, index: u32, field: &str) -> Result<String> {
        self.strings
            .get(index as usize)
            .cloned()
            .ok_or_else(|| AciError::Message(format!("compact partition has invalid {field}")))
    }
}

fn intern_option(strings: &mut StringTable, value: Option<&str>) -> Option<u32> {
    value.map(|value| strings.intern(value))
}

fn compact_span(span: &SourceSpan) -> CompactSpan {
    [
        span.byte_start,
        span.byte_end,
        span.start.line,
        span.start.column,
        span.end.line,
        span.end.column,
    ]
}

fn expand_span(span: CompactSpan) -> SourceSpan {
    SourceSpan {
        byte_start: span[0],
        byte_end: span[1],
        start: LineColumn {
            line: span[2],
            column: span[3],
        },
        end: LineColumn {
            line: span[4],
            column: span[5],
        },
    }
}

fn is_default_provenance(provenance: &FactProvenance) -> bool {
    *provenance == FactProvenance::default()
}

fn is_default_confidence(confidence: &Confidence) -> bool {
    *confidence == Confidence::default()
}
