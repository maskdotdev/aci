use aci_core::{
    AciError, Confidence, EdgeKind, FactProvenance, Language, NodeKind, Result, Severity,
    SymbolKind,
};

pub(crate) fn encode_language(language: Language) -> u8 {
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

pub(crate) fn decode_language(value: u8) -> Result<Language> {
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

pub(crate) fn encode_symbol_kind(kind: SymbolKind) -> u8 {
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

pub(crate) fn decode_symbol_kind(value: u8) -> Result<SymbolKind> {
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

pub(crate) fn encode_provenance(provenance: FactProvenance) -> u8 {
    match provenance {
        FactProvenance::StructuralScanner => 0,
        FactProvenance::TreeSitter => 1,
        FactProvenance::Scip => 2,
        FactProvenance::Lsp => 3,
        FactProvenance::Compiler => 4,
        FactProvenance::Manual => 5,
    }
}

pub(crate) fn decode_provenance(value: u8) -> Result<FactProvenance> {
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

pub(crate) fn encode_confidence(confidence: Confidence) -> u8 {
    match confidence {
        Confidence::Medium => 0,
        Confidence::Low => 1,
        Confidence::High => 2,
        Confidence::Exact => 3,
    }
}

pub(crate) fn decode_confidence(value: u8) -> Result<Confidence> {
    match value {
        0 => Ok(Confidence::Medium),
        1 => Ok(Confidence::Low),
        2 => Ok(Confidence::High),
        3 => Ok(Confidence::Exact),
        _ => Err(invalid_tag("confidence", value)),
    }
}

pub(crate) fn encode_node_kind(kind: NodeKind) -> u8 {
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

pub(crate) fn decode_node_kind(value: u8) -> Result<NodeKind> {
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

pub(crate) fn encode_edge_kind(kind: EdgeKind) -> u8 {
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

pub(crate) fn decode_edge_kind(value: u8) -> Result<EdgeKind> {
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

pub(crate) fn encode_severity(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Error => 2,
    }
}

pub(crate) fn decode_severity(value: u8) -> Result<Severity> {
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
