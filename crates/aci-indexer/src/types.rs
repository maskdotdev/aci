use aci_core::{Diagnostic, GraphPartition, RepositoryId};
use std::path::PathBuf;

/// Options for indexing a repository root.
#[derive(Clone, Debug)]
pub struct IndexOptions {
    pub root: PathBuf,
    pub workers: usize,
    pub max_parse_bytes: Option<usize>,
}

impl IndexOptions {
    /// Creates options using all available parallel workers.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            workers: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
            max_parse_bytes: None,
        }
    }
}

/// Full indexing output retained in memory.
#[derive(Clone, Debug)]
pub struct IndexReport {
    pub repo_id: RepositoryId,
    pub root: PathBuf,
    pub partitions: Vec<GraphPartition>,
    pub diagnostics: Vec<Diagnostic>,
    pub skipped: Vec<PathBuf>,
}

/// Files that should be refreshed after a set of direct changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncrementalPlan {
    pub changed_files: Vec<PathBuf>,
    pub reverse_dependencies: Vec<PathBuf>,
    pub files_to_reindex: Vec<PathBuf>,
}
