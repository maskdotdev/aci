use aci_core::{EdgeKind, Language, Severity, SourceSpan, SymbolKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
    Renamed,
    TypeChanged,
    Copied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefSide {
    Base,
    Head,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefSummary {
    pub name: String,
    pub commit: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileChange {
    pub change: ChangeKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<SymbolKind>,
    pub language: Language,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangedSymbol {
    pub change: ChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<SymbolSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<SymbolSummary>,
    pub public_api: bool,
    pub risk: RiskLevel,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyChange {
    pub change: ChangeKind,
    pub file: String,
    pub dependency: String,
    pub edge_kind: EdgeKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImpactedFile {
    pub path: String,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffDiagnostic {
    pub reference: RefSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffStats {
    pub files_added: usize,
    pub files_removed: usize,
    pub files_modified: usize,
    pub files_renamed: usize,
    pub symbols_added: usize,
    pub symbols_removed: usize,
    pub symbols_modified: usize,
    pub public_api_changes: usize,
    pub dependency_changes: usize,
    pub impacted_files: usize,
    pub diagnostics: usize,
    pub base_indexed_files: usize,
    pub head_indexed_files: usize,
    pub base_skipped_files: usize,
    pub head_skipped_files: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffReport {
    pub base: RefSummary,
    pub head: RefSummary,
    pub changed_files: Vec<FileChange>,
    pub changed_symbols: Vec<ChangedSymbol>,
    pub public_api_changes: Vec<ChangedSymbol>,
    pub dependency_changes: Vec<DependencyChange>,
    pub impacted_files: Vec<ImpactedFile>,
    pub diagnostics: Vec<DiffDiagnostic>,
    pub stats: DiffStats,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentDiffStats {
    pub files_changed: usize,
    pub public_api_changes: usize,
    pub important_symbol_changes: usize,
    pub dependency_changes: usize,
    pub diagnostics: usize,
    pub tests_changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTopChange {
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub risk: RiskLevel,
    pub why_it_matters: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentReviewFocus {
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentDiffReport {
    pub base: RefSummary,
    pub head: RefSummary,
    pub risk: RiskLevel,
    pub summary: AgentDiffStats,
    pub top_changes: Vec<AgentTopChange>,
    pub review_focus: Vec<AgentReviewFocus>,
    pub notes: Vec<String>,
}
