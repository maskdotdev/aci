use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id<T> {
    value: String,
    #[serde(skip)]
    marker: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(prefix: &str, parts: &[impl AsRef<str>]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(part.as_ref().as_bytes());
            hasher.update(b"\0");
        }
        let digest = hasher.finalize().to_hex();
        Self {
            value: format!("{prefix}:{}", &digest[..24]),
            marker: PhantomData,
        }
    }

    pub fn from_raw(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            marker: PhantomData,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RepositoryTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FileTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SymbolTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum NodeTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EdgeTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackageTag {}

pub type RepositoryId = Id<RepositoryTag>;
pub type FileId = Id<FileTag>;
pub type SymbolId = Id<SymbolTag>;
pub type NodeId = Id<NodeTag>;
pub type EdgeId = Id<EdgeTag>;
pub type PackageId = Id<PackageTag>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    JavaScript,
    TypeScript,
    Python,
    Rust,
    Unknown,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LineColumn {
    pub line: u32,
    pub column: u32,
}

impl LineColumn {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub byte_start: u32,
    pub byte_end: u32,
    pub start: LineColumn,
    pub end: LineColumn,
}

impl SourceSpan {
    pub fn new(byte_start: u32, byte_end: u32, start: LineColumn, end: LineColumn) -> Self {
        Self {
            byte_start,
            byte_end,
            start,
            end,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactProvenance {
    StructuralScanner,
    TreeSitter,
    Scip,
    Lsp,
    Compiler,
    Manual,
}

impl FactProvenance {
    pub fn rank(self) -> u8 {
        match self {
            Self::StructuralScanner => 1,
            Self::TreeSitter => 2,
            Self::Lsp => 3,
            Self::Scip => 4,
            Self::Compiler => 5,
            Self::Manual => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Low,
    Medium,
    High,
    Exact,
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
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub file_id: Option<FileId>,
    pub span: Option<SourceSpan>,
}

impl Diagnostic {
    pub fn warning(
        message: impl Into<String>,
        file_id: Option<FileId>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            file_id,
            span,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Error,
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
        }
    }
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

#[derive(Debug, thiserror::Error)]
pub enum AciError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, AciError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct InternedString(u32);

impl InternedString {
    pub fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StringInterner {
    values: Vec<String>,
    indexes: BTreeMap<String, InternedString>,
}

impl StringInterner {
    pub fn intern(&mut self, value: impl AsRef<str>) -> InternedString {
        let value = value.as_ref();
        if let Some(existing) = self.indexes.get(value) {
            return *existing;
        }
        let index = InternedString(self.values.len() as u32);
        self.values.push(value.to_string());
        self.indexes.insert(value.to_string(), index);
        index
    }

    pub fn resolve(&self, value: InternedString) -> Option<&str> {
        self.values.get(value.0 as usize).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
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

impl Default for FactProvenance {
    fn default() -> Self {
        Self::StructuralScanner
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::Medium
    }
}

pub fn prefer_fact(
    existing: (FactProvenance, Confidence),
    candidate: (FactProvenance, Confidence),
) -> bool {
    let existing_score = fact_score(existing.0, existing.1);
    let candidate_score = fact_score(candidate.0, candidate.1);
    candidate_score > existing_score
}

fn fact_score(provenance: FactProvenance, confidence: Confidence) -> u16 {
    let confidence_score = match confidence {
        Confidence::Low => 1,
        Confidence::Medium => 2,
        Confidence::High => 3,
        Confidence::Exact => 4,
    };
    u16::from(provenance.rank()) * 10 + confidence_score
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn string_interner_reuses_existing_values() {
        let mut interner = StringInterner::default();
        let left = interner.intern("src/main.rs");
        let right = interner.intern("src/main.rs");
        assert_eq!(left, right);
        assert_eq!(interner.len(), 1);
        assert_eq!(interner.resolve(left), Some("src/main.rs"));
    }

    #[test]
    fn path_normalization_uses_forward_slashes() {
        assert_eq!(normalize_path(Path::new("./src/lib.rs")), "src/lib.rs");
    }
}
