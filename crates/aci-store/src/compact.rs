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
    language: u8,
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
    kind: u8,
    #[serde(rename = "l", skip_serializing_if = "Option::is_none")]
    language: Option<u8>,
    #[serde(rename = "n", skip_serializing_if = "Option::is_none")]
    name: Option<u32>,
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    qualified_name: Option<u32>,
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    symbol_kind: Option<u8>,
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    file_id: Option<u32>,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    span: Option<CompactSpan>,
    #[serde(rename = "p", default, skip_serializing_if = "is_default_provenance")]
    provenance: u8,
    #[serde(rename = "c", default, skip_serializing_if = "is_zero")]
    confidence: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactEdge {
    #[serde(rename = "i")]
    id: u32,
    #[serde(rename = "k")]
    kind: u8,
    #[serde(rename = "f")]
    from: u32,
    #[serde(rename = "t")]
    to: u32,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    span: Option<CompactSpan>,
    #[serde(rename = "p", default, skip_serializing_if = "is_default_provenance")]
    provenance: u8,
    #[serde(rename = "c", default, skip_serializing_if = "is_zero")]
    confidence: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactDiagnostic {
    #[serde(rename = "s")]
    severity: u8,
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
                kind: encode_node_kind(node.kind),
                language: (node.language != partition.language)
                    .then_some(encode_language(node.language)),
                name: intern_option(&mut strings, node.name.as_deref()),
                qualified_name: intern_option(&mut strings, node.qualified_name.as_deref()),
                symbol_kind: node.symbol_kind.map(encode_symbol_kind),
                file_id: node
                    .file_id
                    .as_ref()
                    .map(|file_id| strings.intern(file_id.as_str())),
                span: node.span.as_ref().map(compact_span),
                provenance: encode_provenance(node.provenance),
                confidence: encode_confidence(node.confidence),
            })
            .collect();
        let edges = partition
            .edges
            .iter()
            .map(|edge| CompactEdge {
                id: strings.intern(edge.id.as_str()),
                kind: encode_edge_kind(edge.kind),
                from: strings.intern(edge.from.as_str()),
                to: strings.intern(edge.to.as_str()),
                span: edge.span.as_ref().map(compact_span),
                provenance: encode_provenance(edge.provenance),
                confidence: encode_confidence(edge.confidence),
            })
            .collect();
        let diagnostics = partition
            .diagnostics
            .iter()
            .map(|diagnostic| CompactDiagnostic {
                severity: encode_severity(diagnostic.severity),
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
            language: encode_language(partition.language),
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
        let language = decode_language(language)?;
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
            kind: decode_node_kind(node.kind)?,
            language: node
                .language
                .map(decode_language)
                .transpose()?
                .unwrap_or(self.language),
            name: self.optional_string(node.name, "node name")?,
            qualified_name: self.optional_string(node.qualified_name, "qualified name")?,
            symbol_kind: node.symbol_kind.map(decode_symbol_kind).transpose()?,
            file_id: self.optional_file_id(node.file_id, "node file id")?,
            span: node.span.map(expand_span),
            provenance: decode_provenance(node.provenance)?,
            confidence: decode_confidence(node.confidence)?,
        })
    }

    fn expand_edge(&self, edge: CompactEdge) -> Result<GraphEdge> {
        Ok(GraphEdge {
            id: EdgeId::from_raw(self.string(edge.id, "edge id")?),
            kind: decode_edge_kind(edge.kind)?,
            from: NodeId::from_raw(self.string(edge.from, "edge source")?),
            to: NodeId::from_raw(self.string(edge.to, "edge target")?),
            span: edge.span.map(expand_span),
            provenance: decode_provenance(edge.provenance)?,
            confidence: decode_confidence(edge.confidence)?,
        })
    }

    fn expand_diagnostic(&self, diagnostic: CompactDiagnostic) -> Result<Diagnostic> {
        Ok(Diagnostic {
            severity: decode_severity(diagnostic.severity)?,
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

fn encode_language(language: Language) -> u8 {
    match language {
        Language::C => 0,
        Language::Cpp => 1,
        Language::Go => 2,
        Language::JavaScript => 3,
        Language::Json => 4,
        Language::Java => 5,
        Language::ObjectiveC => 6,
        Language::TypeScript => 7,
        Language::Python => 8,
        Language::Rust => 9,
        Language::Unknown => 10,
    }
}

fn decode_language(value: u8) -> Result<Language> {
    match value {
        0 => Ok(Language::C),
        1 => Ok(Language::Cpp),
        2 => Ok(Language::Go),
        3 => Ok(Language::JavaScript),
        4 => Ok(Language::Json),
        5 => Ok(Language::Java),
        6 => Ok(Language::ObjectiveC),
        7 => Ok(Language::TypeScript),
        8 => Ok(Language::Python),
        9 => Ok(Language::Rust),
        10 => Ok(Language::Unknown),
        _ => Err(invalid_tag("language", value)),
    }
}

fn encode_symbol_kind(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::Function => 0,
        SymbolKind::Method => 1,
        SymbolKind::Class => 2,
        SymbolKind::Interface => 3,
        SymbolKind::TypeAlias => 4,
        SymbolKind::Enum => 5,
        SymbolKind::Variable => 6,
        SymbolKind::Module => 7,
        SymbolKind::Field => 8,
        SymbolKind::Unknown => 9,
    }
}

fn decode_symbol_kind(value: u8) -> Result<SymbolKind> {
    match value {
        0 => Ok(SymbolKind::Function),
        1 => Ok(SymbolKind::Method),
        2 => Ok(SymbolKind::Class),
        3 => Ok(SymbolKind::Interface),
        4 => Ok(SymbolKind::TypeAlias),
        5 => Ok(SymbolKind::Enum),
        6 => Ok(SymbolKind::Variable),
        7 => Ok(SymbolKind::Module),
        8 => Ok(SymbolKind::Field),
        9 => Ok(SymbolKind::Unknown),
        _ => Err(invalid_tag("symbol kind", value)),
    }
}

fn encode_provenance(provenance: FactProvenance) -> u8 {
    match provenance {
        FactProvenance::StructuralScanner => 0,
        FactProvenance::TreeSitter => 1,
        FactProvenance::Scip => 2,
        FactProvenance::Lsp => 3,
        FactProvenance::Compiler => 4,
        FactProvenance::Manual => 5,
    }
}

fn decode_provenance(value: u8) -> Result<FactProvenance> {
    match value {
        0 => Ok(FactProvenance::StructuralScanner),
        1 => Ok(FactProvenance::TreeSitter),
        2 => Ok(FactProvenance::Scip),
        3 => Ok(FactProvenance::Lsp),
        4 => Ok(FactProvenance::Compiler),
        5 => Ok(FactProvenance::Manual),
        _ => Err(invalid_tag("provenance", value)),
    }
}

fn encode_confidence(confidence: Confidence) -> u8 {
    match confidence {
        Confidence::Medium => 0,
        Confidence::Low => 1,
        Confidence::High => 2,
        Confidence::Exact => 3,
    }
}

fn decode_confidence(value: u8) -> Result<Confidence> {
    match value {
        0 => Ok(Confidence::Medium),
        1 => Ok(Confidence::Low),
        2 => Ok(Confidence::High),
        3 => Ok(Confidence::Exact),
        _ => Err(invalid_tag("confidence", value)),
    }
}

fn encode_node_kind(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::Repository => 0,
        NodeKind::Directory => 1,
        NodeKind::File => 2,
        NodeKind::Module => 3,
        NodeKind::Symbol => 4,
        NodeKind::Import => 5,
        NodeKind::Export => 6,
        NodeKind::Package => 7,
        NodeKind::ExternalSymbol => 8,
        NodeKind::Span => 9,
        NodeKind::Chunk => 10,
    }
}

fn decode_node_kind(value: u8) -> Result<NodeKind> {
    match value {
        0 => Ok(NodeKind::Repository),
        1 => Ok(NodeKind::Directory),
        2 => Ok(NodeKind::File),
        3 => Ok(NodeKind::Module),
        4 => Ok(NodeKind::Symbol),
        5 => Ok(NodeKind::Import),
        6 => Ok(NodeKind::Export),
        7 => Ok(NodeKind::Package),
        8 => Ok(NodeKind::ExternalSymbol),
        9 => Ok(NodeKind::Span),
        10 => Ok(NodeKind::Chunk),
        _ => Err(invalid_tag("node kind", value)),
    }
}

fn encode_edge_kind(kind: EdgeKind) -> u8 {
    match kind {
        EdgeKind::Contains => 0,
        EdgeKind::Defines => 1,
        EdgeKind::Imports => 2,
        EdgeKind::Exports => 3,
        EdgeKind::Calls => 4,
        EdgeKind::References => 5,
        EdgeKind::Extends => 6,
        EdgeKind::Implements => 7,
        EdgeKind::Overrides => 8,
        EdgeKind::DependsOn => 9,
        EdgeKind::Tests => 10,
    }
}

fn decode_edge_kind(value: u8) -> Result<EdgeKind> {
    match value {
        0 => Ok(EdgeKind::Contains),
        1 => Ok(EdgeKind::Defines),
        2 => Ok(EdgeKind::Imports),
        3 => Ok(EdgeKind::Exports),
        4 => Ok(EdgeKind::Calls),
        5 => Ok(EdgeKind::References),
        6 => Ok(EdgeKind::Extends),
        7 => Ok(EdgeKind::Implements),
        8 => Ok(EdgeKind::Overrides),
        9 => Ok(EdgeKind::DependsOn),
        10 => Ok(EdgeKind::Tests),
        _ => Err(invalid_tag("edge kind", value)),
    }
}

fn encode_severity(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Error => 2,
    }
}

fn decode_severity(value: u8) -> Result<Severity> {
    match value {
        0 => Ok(Severity::Info),
        1 => Ok(Severity::Warning),
        2 => Ok(Severity::Error),
        _ => Err(invalid_tag("severity", value)),
    }
}

fn invalid_tag(field: &str, value: u8) -> AciError {
    AciError::Message(format!("compact partition has invalid {field} tag {value}"))
}

fn is_default_provenance(provenance: &u8) -> bool {
    *provenance == 0
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}
