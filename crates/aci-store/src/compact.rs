use crate::pack::{
    CompactSpan, capacity, read_opt_span, read_opt_u8, read_opt_u32, read_string, read_u8,
    read_var_u32, read_var_u64, read_var_u64_optional, write_len, write_opt_span, write_opt_u8,
    write_opt_u32, write_string, write_u8, write_var_u32, write_var_u64,
};
use crate::tags::{
    decode_confidence, decode_edge_kind, decode_language, decode_node_kind, decode_provenance,
    decode_severity, decode_symbol_kind, encode_confidence, encode_edge_kind, encode_language,
    encode_node_kind, encode_provenance, encode_severity, encode_symbol_kind,
};
use aci_core::{
    AciError, Diagnostic, EdgeId, FileId, GraphEdge, GraphNode, GraphPartition, Language,
    LineColumn, NodeId, PartitionMetrics, Result, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
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
    #[serde(rename = "l")]
    language: Option<u8>,
    #[serde(rename = "n")]
    name: Option<u32>,
    #[serde(rename = "q")]
    qualified_name: Option<u32>,
    #[serde(rename = "t")]
    symbol_kind: Option<u8>,
    #[serde(rename = "f")]
    file_id: Option<u32>,
    #[serde(rename = "s")]
    span: Option<CompactSpan>,
    #[serde(rename = "p")]
    provenance: u8,
    #[serde(rename = "c")]
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
    #[serde(rename = "s")]
    span: Option<CompactSpan>,
    #[serde(rename = "p")]
    provenance: u8,
    #[serde(rename = "c")]
    confidence: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompactDiagnostic {
    #[serde(rename = "s")]
    severity: u8,
    #[serde(rename = "m")]
    message: u32,
    #[serde(rename = "f")]
    file_id: Option<u32>,
    #[serde(rename = "r")]
    span: Option<CompactSpan>,
}

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

pub(crate) fn write_pack_header(writer: &mut impl Write) -> Result<()> {
    crate::pack::write_pack_header(writer)
}

pub(crate) fn read_pack_header(reader: &mut impl Read) -> Result<()> {
    crate::pack::read_pack_header(reader)
}

pub(crate) fn write_partition_binary(
    writer: &mut impl Write,
    partition: &GraphPartition,
) -> Result<()> {
    CompactPartition::from_partition(partition).write_binary(writer)
}

pub(crate) fn read_partition_binary(reader: &mut impl Read) -> Result<Option<GraphPartition>> {
    let Some(compact) = CompactPartition::read_binary(reader)? else {
        return Ok(None);
    };
    compact.into_partition().map(Some)
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

    fn write_binary(self, writer: &mut impl Write) -> Result<()> {
        write_len(writer, self.strings.len(), "strings")?;
        for value in self.strings {
            write_string(writer, &value)?;
        }
        write_var_u32(writer, self.file_id)?;
        write_var_u32(writer, self.path)?;
        write_u8(writer, self.language)?;
        write_var_u32(writer, self.fingerprint)?;
        for metric in self.metrics {
            write_var_u64(writer, metric)?;
        }
        write_len(writer, self.nodes.len(), "nodes")?;
        for node in self.nodes {
            node.write_binary(writer)?;
        }
        write_len(writer, self.edges.len(), "edges")?;
        for edge in self.edges {
            edge.write_binary(writer)?;
        }
        write_len(writer, self.diagnostics.len(), "diagnostics")?;
        for diagnostic in self.diagnostics {
            diagnostic.write_binary(writer)?;
        }
        Ok(())
    }

    fn read_binary(reader: &mut impl Read) -> Result<Option<Self>> {
        let Some(string_count) = read_var_u64_optional(reader)? else {
            return Ok(None);
        };
        let string_count = u32::try_from(string_count).map_err(|_| {
            AciError::Message("partition pack string count does not fit u32".to_string())
        })?;
        let mut strings = Vec::with_capacity(capacity(string_count, "strings")?);
        for _ in 0..string_count {
            strings.push(read_string(reader)?);
        }
        let file_id = read_var_u32(reader, "file id")?;
        let path = read_var_u32(reader, "path")?;
        let language = read_u8(reader, "language")?;
        let fingerprint = read_var_u32(reader, "fingerprint")?;
        let metrics = [
            read_var_u64(reader, "parse time")?,
            read_var_u64(reader, "extraction time")?,
            read_var_u64(reader, "query captures")?,
        ];
        let node_count = read_var_u32(reader, "node count")?;
        let mut nodes = Vec::with_capacity(capacity(node_count, "nodes")?);
        for _ in 0..node_count {
            nodes.push(CompactNode::read_binary(reader)?);
        }
        let edge_count = read_var_u32(reader, "edge count")?;
        let mut edges = Vec::with_capacity(capacity(edge_count, "edges")?);
        for _ in 0..edge_count {
            edges.push(CompactEdge::read_binary(reader)?);
        }
        let diagnostic_count = read_var_u32(reader, "diagnostic count")?;
        let mut diagnostics = Vec::with_capacity(capacity(diagnostic_count, "diagnostics")?);
        for _ in 0..diagnostic_count {
            diagnostics.push(CompactDiagnostic::read_binary(reader)?);
        }
        Ok(Some(Self {
            strings,
            file_id,
            path,
            language,
            fingerprint,
            nodes,
            edges,
            diagnostics,
            metrics,
        }))
    }
}

impl CompactNode {
    fn write_binary(self, writer: &mut impl Write) -> Result<()> {
        write_var_u32(writer, self.id)?;
        write_u8(writer, self.kind)?;
        write_opt_u8(writer, self.language)?;
        write_opt_u32(writer, self.name)?;
        write_opt_u32(writer, self.qualified_name)?;
        write_opt_u8(writer, self.symbol_kind)?;
        write_opt_u32(writer, self.file_id)?;
        write_opt_span(writer, self.span)?;
        write_u8(writer, self.provenance)?;
        write_u8(writer, self.confidence)?;
        Ok(())
    }

    fn read_binary(reader: &mut impl Read) -> Result<Self> {
        Ok(Self {
            id: read_var_u32(reader, "node id")?,
            kind: read_u8(reader, "node kind")?,
            language: read_opt_u8(reader, "node language")?,
            name: read_opt_u32(reader, "node name")?,
            qualified_name: read_opt_u32(reader, "qualified name")?,
            symbol_kind: read_opt_u8(reader, "symbol kind")?,
            file_id: read_opt_u32(reader, "node file id")?,
            span: read_opt_span(reader)?,
            provenance: read_u8(reader, "node provenance")?,
            confidence: read_u8(reader, "node confidence")?,
        })
    }
}

impl CompactEdge {
    fn write_binary(self, writer: &mut impl Write) -> Result<()> {
        write_var_u32(writer, self.id)?;
        write_u8(writer, self.kind)?;
        write_var_u32(writer, self.from)?;
        write_var_u32(writer, self.to)?;
        write_opt_span(writer, self.span)?;
        write_u8(writer, self.provenance)?;
        write_u8(writer, self.confidence)?;
        Ok(())
    }

    fn read_binary(reader: &mut impl Read) -> Result<Self> {
        Ok(Self {
            id: read_var_u32(reader, "edge id")?,
            kind: read_u8(reader, "edge kind")?,
            from: read_var_u32(reader, "edge source")?,
            to: read_var_u32(reader, "edge target")?,
            span: read_opt_span(reader)?,
            provenance: read_u8(reader, "edge provenance")?,
            confidence: read_u8(reader, "edge confidence")?,
        })
    }
}

impl CompactDiagnostic {
    fn write_binary(self, writer: &mut impl Write) -> Result<()> {
        write_u8(writer, self.severity)?;
        write_var_u32(writer, self.message)?;
        write_opt_u32(writer, self.file_id)?;
        write_opt_span(writer, self.span)?;
        Ok(())
    }

    fn read_binary(reader: &mut impl Read) -> Result<Self> {
        Ok(Self {
            severity: read_u8(reader, "diagnostic severity")?,
            message: read_var_u32(reader, "diagnostic message")?,
            file_id: read_opt_u32(reader, "diagnostic file id")?,
            span: read_opt_span(reader)?,
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
