//! Branch-to-branch semantic diff support for ACI.
//!
//! This crate checks out two Git references into isolated worktrees, indexes
//! both trees with `aci-indexer`, and compares the resulting graph snapshots.

mod agent;
mod compare;
mod git;
mod labels;
mod report;
mod stats;
mod symbol_identity;

use aci_core::{GraphSnapshot, Result};
use aci_indexer::{IndexOptions, IndexPipeline};
use std::path::PathBuf;

pub use agent::summarize_for_agent;
pub use labels::{edge_kind_label, ref_side_label, risk_label, symbol_kind_label};
pub use report::{
    AgentDiffReport, AgentDiffStats, AgentReviewFocus, AgentTopChange, ChangeKind, ChangedSymbol,
    DependencyChange, DiffDiagnostic, DiffReport, DiffStats, FileChange, ImpactedFile, RefSide,
    RefSummary, RiskLevel, SymbolSummary,
};

/// Options for comparing two Git references.
#[derive(Clone, Debug)]
pub struct DiffOptions {
    pub repo_root: PathBuf,
    pub base_ref: String,
    pub head_ref: String,
    pub workers: usize,
    pub max_parse_bytes: Option<usize>,
}

impl DiffOptions {
    /// Creates diff options rooted at the current directory.
    pub fn new(base_ref: impl Into<String>, head_ref: impl Into<String>) -> Self {
        Self {
            repo_root: PathBuf::from("."),
            base_ref: base_ref.into(),
            head_ref: head_ref.into(),
            workers: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
            max_parse_bytes: None,
        }
    }

    /// Sets the repository root used for Git operations.
    pub fn with_repo_root(mut self, repo_root: impl Into<PathBuf>) -> Self {
        self.repo_root = repo_root.into();
        self
    }

    /// Sets the maximum indexing worker count.
    pub fn with_workers(mut self, workers: usize) -> Self {
        self.workers = workers.max(1);
        self
    }

    /// Sets the maximum source bytes a Tree-sitter adapter may parse per file.
    pub fn with_max_parse_bytes(mut self, max_parse_bytes: Option<usize>) -> Self {
        self.max_parse_bytes = max_parse_bytes;
        self
    }
}

/// Compares two Git references and returns a stable semantic diff report.
pub fn diff_refs(options: DiffOptions) -> Result<DiffReport> {
    let repository = git::GitRepository::open(&options.repo_root)?;
    let base_commit = repository.resolve_ref(&options.base_ref)?;
    let head_commit = repository.resolve_ref(&options.head_ref)?;
    let changed_files = repository.diff_name_status(&base_commit, &head_commit)?;
    let worktrees = repository.checkout_pair(&base_commit, &head_commit)?;
    let base = index_ref(
        &options.base_ref,
        &base_commit,
        worktrees.base_root.clone(),
        options.workers,
        options.max_parse_bytes,
    )?;
    let head = index_ref(
        &options.head_ref,
        &head_commit,
        worktrees.head_root.clone(),
        options.workers,
        options.max_parse_bytes,
    )?;
    compare::compare_refs(base, head, changed_files)
}

pub(crate) struct IndexedRef {
    label: String,
    commit: String,
    root: PathBuf,
    snapshot: GraphSnapshot,
    diagnostics: Vec<aci_core::Diagnostic>,
    skipped: Vec<PathBuf>,
}

fn index_ref(
    label: &str,
    commit: &str,
    root: PathBuf,
    workers: usize,
    max_parse_bytes: Option<usize>,
) -> Result<IndexedRef> {
    let mut options = IndexOptions::new(&root);
    options.workers = workers;
    options.max_parse_bytes = max_parse_bytes;
    let report = IndexPipeline::default().index_path(options)?;
    Ok(IndexedRef {
        label: label.to_string(),
        commit: commit.to_string(),
        root: report.root,
        snapshot: GraphSnapshot {
            partitions: report.partitions,
        },
        diagnostics: report.diagnostics,
        skipped: report.skipped,
    })
}
