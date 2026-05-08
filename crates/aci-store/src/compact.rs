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

const PACK_MAGIC: &[u8] = b"ACIPACK1\n";

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

pub(crate) fn write_pack_header(writer: &mut impl Write) -> Result<()> {
    writer.write_all(PACK_MAGIC)?;
    Ok(())
}

pub(crate) fn read_pack_header(reader: &mut impl Read) -> Result<()> {
    let mut magic = [0; PACK_MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if magic != PACK_MAGIC {
        return Err(AciError::Message(
            "partition pack has invalid header".to_string(),
        ));
    }
    Ok(())
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

fn write_string(writer: &mut impl Write, value: &str) -> Result<()> {
    write_len(writer, value.len(), "string")?;
    writer.write_all(value.as_bytes())?;
    Ok(())
}

fn read_string(reader: &mut impl Read) -> Result<String> {
    let len = read_var_u32(reader, "string length")?;
    let mut bytes = vec![0; capacity(len, "string")?];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes)
        .map_err(|error| AciError::Message(format!("partition pack has invalid utf-8: {error}")))
}

fn write_opt_span(writer: &mut impl Write, span: Option<CompactSpan>) -> Result<()> {
    match span {
        Some(span) => {
            write_u8(writer, 1)?;
            for value in span {
                write_var_u32(writer, value)?;
            }
        }
        None => write_u8(writer, 0)?,
    }
    Ok(())
}

fn read_opt_span(reader: &mut impl Read) -> Result<Option<CompactSpan>> {
    match read_u8(reader, "span presence")? {
        0 => Ok(None),
        1 => Ok(Some([
            read_var_u32(reader, "span byte start")?,
            read_var_u32(reader, "span byte end")?,
            read_var_u32(reader, "span start line")?,
            read_var_u32(reader, "span start column")?,
            read_var_u32(reader, "span end line")?,
            read_var_u32(reader, "span end column")?,
        ])),
        value => Err(AciError::Message(format!(
            "partition pack has invalid span presence tag {value}"
        ))),
    }
}

fn write_opt_u32(writer: &mut impl Write, value: Option<u32>) -> Result<()> {
    write_var_u32(writer, value.map(|value| value + 1).unwrap_or(0))
}

fn read_opt_u32(reader: &mut impl Read, field: &str) -> Result<Option<u32>> {
    Ok(match read_var_u32(reader, field)? {
        0 => None,
        value => Some(value - 1),
    })
}

fn write_opt_u8(writer: &mut impl Write, value: Option<u8>) -> Result<()> {
    write_u8(writer, value.unwrap_or(u8::MAX))
}

fn read_opt_u8(reader: &mut impl Read, field: &str) -> Result<Option<u8>> {
    Ok(match read_u8(reader, field)? {
        u8::MAX => None,
        value => Some(value),
    })
}

fn write_len(writer: &mut impl Write, len: usize, field: &str) -> Result<()> {
    let len = u32::try_from(len)
        .map_err(|_| AciError::Message(format!("partition pack has too many {field}")))?;
    write_var_u32(writer, len)
}

fn capacity(len: u32, field: &str) -> Result<usize> {
    usize::try_from(len).map_err(|_| {
        AciError::Message(format!(
            "partition pack {field} length does not fit this platform"
        ))
    })
}

fn write_u8(writer: &mut impl Write, value: u8) -> Result<()> {
    writer.write_all(&[value])?;
    Ok(())
}

fn read_u8(reader: &mut impl Read, field: &str) -> Result<u8> {
    let mut byte = [0; 1];
    reader
        .read_exact(&mut byte)
        .map_err(|error| truncated(error, field))?;
    Ok(byte[0])
}

fn write_var_u32(writer: &mut impl Write, value: u32) -> Result<()> {
    write_var_u64(writer, u64::from(value))
}

fn read_var_u32(reader: &mut impl Read, field: &str) -> Result<u32> {
    let value = read_var_u64_optional(reader)?
        .ok_or_else(|| AciError::Message(format!("partition pack ended before {field}")))?;
    u32::try_from(value)
        .map_err(|_| AciError::Message(format!("partition pack {field} does not fit u32")))
}

fn write_var_u64(writer: &mut impl Write, mut value: u64) -> Result<()> {
    while value >= 0x80 {
        writer.write_all(&[((value as u8) & 0x7f) | 0x80])?;
        value >>= 7;
    }
    writer.write_all(&[value as u8])?;
    Ok(())
}

fn read_var_u64(reader: &mut impl Read, field: &str) -> Result<u64> {
    read_var_u64_optional(reader)?
        .ok_or_else(|| AciError::Message(format!("partition pack ended before {field}")))
}

fn read_var_u64_optional(reader: &mut impl Read) -> Result<Option<u64>> {
    let mut value = 0_u64;
    let mut shift = 0;
    loop {
        let mut byte = [0; 1];
        match reader.read(&mut byte) {
            Ok(0) if shift == 0 => return Ok(None),
            Ok(0) => {
                return Err(AciError::Message(
                    "partition pack has truncated varint".to_string(),
                ));
            }
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }
        value |= u64::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(Some(value));
        }
        shift += 7;
        if shift >= 64 {
            return Err(AciError::Message(
                "partition pack varint is too large".to_string(),
            ));
        }
    }
}

fn truncated(error: std::io::Error, field: &str) -> AciError {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        AciError::Message(format!("partition pack ended while reading {field}"))
    } else {
        error.into()
    }
}
