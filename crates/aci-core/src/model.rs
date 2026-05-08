use crate::{
    Confidence, Diagnostic, EdgeId, FactProvenance, FileId, Language, NodeId, RepositoryId,
    SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Variable,
    Module,
    Field,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Repository,
    Directory,
    File,
    Module,
    Symbol,
    Import,
    Export,
    Package,
    ExternalSymbol,
    Span,
    Chunk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    Contains,
    Defines,
    Imports,
    Exports,
    Calls,
    References,
    Extends,
    Implements,
    Overrides,
    DependsOn,
    Tests,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub language: Language,
    pub name: Option<String>,
    pub qualified_name: Option<String>,
    pub symbol_kind: Option<SymbolKind>,
    pub file_id: Option<FileId>,
    pub span: Option<SourceSpan>,
    #[serde(default)]
    pub provenance: FactProvenance,
    #[serde(default)]
    pub confidence: Confidence,
}

impl GraphNode {
    pub fn deterministic(
        repo_id: &RepositoryId,
        file_id: Option<&FileId>,
        kind: NodeKind,
        language: Language,
        name: Option<String>,
        qualified_name: Option<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        let span_key = span
            .as_ref()
            .map(|span| format!("{}:{}", span.byte_start, span.byte_end))
            .unwrap_or_default();
        let file_key = file_id.map(ToString::to_string).unwrap_or_default();
        let name_key = qualified_name.as_deref().or(name.as_deref()).unwrap_or("");
        let id = NodeId::new(
            "node",
            &[
                repo_id.as_str(),
                &file_key,
                kind.key(),
                language.as_str(),
                name_key,
                &span_key,
            ],
        );
        Self {
            id,
            kind,
            language,
            name,
            qualified_name,
            symbol_kind: None,
            file_id: file_id.cloned(),
            span,
            provenance: FactProvenance::default(),
            confidence: Confidence::default(),
        }
    }

    pub fn with_symbol_kind(mut self, symbol_kind: SymbolKind) -> Self {
        self.symbol_kind = Some(symbol_kind);
        self
    }

    pub fn with_fact_quality(mut self, provenance: FactProvenance, confidence: Confidence) -> Self {
        self.provenance = provenance;
        self.confidence = confidence;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: EdgeId,
    pub kind: EdgeKind,
    pub from: NodeId,
    pub to: NodeId,
    pub span: Option<SourceSpan>,
    #[serde(default)]
    pub provenance: FactProvenance,
    #[serde(default)]
    pub confidence: Confidence,
}

impl GraphEdge {
    pub fn deterministic(
        kind: EdgeKind,
        from: &NodeId,
        to: &NodeId,
        span: Option<SourceSpan>,
    ) -> Self {
        let span_key = span
            .as_ref()
            .map(|span| format!("{}:{}", span.byte_start, span.byte_end))
            .unwrap_or_default();
        Self {
            id: EdgeId::new("edge", &[kind.key(), from.as_str(), to.as_str(), &span_key]),
            kind,
            from: from.clone(),
            to: to.clone(),
            span,
            provenance: FactProvenance::default(),
            confidence: Confidence::default(),
        }
    }

    pub fn with_fact_quality(mut self, provenance: FactProvenance, confidence: Confidence) -> Self {
        self.provenance = provenance;
        self.confidence = confidence;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceFile {
    pub repo_id: RepositoryId,
    pub file_id: FileId,
    pub path: PathBuf,
    pub language: Language,
    pub fingerprint: String,
    pub text: String,
}

impl SourceFile {
    pub fn new(
        repo_id: RepositoryId,
        repo_root: &Path,
        path: PathBuf,
        language: Language,
        text: String,
    ) -> Self {
        let relative = path.strip_prefix(repo_root).unwrap_or(&path).to_path_buf();
        let relative = normalize_path(&relative);
        let fingerprint = blake3::hash(text.as_bytes()).to_hex().to_string();
        let file_id = FileId::new("file", &[repo_id.as_str(), &relative, language.as_str()]);
        Self {
            repo_id,
            file_id,
            path,
            language,
            fingerprint,
            text,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphPartition {
    pub file_id: FileId,
    pub path: PathBuf,
    pub language: Language,
    pub fingerprint: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    pub metrics: PartitionMetrics,
}

impl GraphPartition {
    pub fn empty(file: &SourceFile) -> Self {
        Self {
            file_id: file.file_id.clone(),
            path: file.path.clone(),
            language: file.language,
            fingerprint: file.fingerprint.clone(),
            nodes: Vec::new(),
            edges: Vec::new(),
            diagnostics: Vec::new(),
            metrics: PartitionMetrics::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PartitionMetrics {
    pub parse_time_micros: u64,
    pub extraction_time_micros: u64,
    pub query_captures: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub partitions: Vec<GraphPartition>,
}

impl GraphSnapshot {
    pub fn replace_partition(&mut self, replacement: GraphPartition) {
        if let Some(existing) = self
            .partitions
            .iter_mut()
            .find(|partition| partition.file_id == replacement.file_id)
        {
            *existing = replacement;
        } else {
            self.partitions.push(replacement);
        }
    }
}

pub fn normalize_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| {
            let part = component.as_os_str().to_string_lossy();
            match part.as_ref() {
                "" | "." => None,
                _ => Some(part.into_owned()),
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

trait KindKey {
    fn key(self) -> &'static str;
}

impl KindKey for NodeKind {
    fn key(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Directory => "directory",
            Self::File => "file",
            Self::Module => "module",
            Self::Symbol => "symbol",
            Self::Import => "import",
            Self::Export => "export",
            Self::Package => "package",
            Self::ExternalSymbol => "external-symbol",
            Self::Span => "span",
            Self::Chunk => "chunk",
        }
    }
}

impl KindKey for EdgeKind {
    fn key(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Defines => "defines",
            Self::Imports => "imports",
            Self::Exports => "exports",
            Self::Calls => "calls",
            Self::References => "references",
            Self::Extends => "extends",
            Self::Implements => "implements",
            Self::Overrides => "overrides",
            Self::DependsOn => "depends-on",
            Self::Tests => "tests",
        }
    }
}
