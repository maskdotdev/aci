use aci_core::{Diagnostic, GraphPartition, RepositoryId};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct IndexOptions {
    pub root: PathBuf,
    pub workers: usize,
}

impl IndexOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            workers: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexReport {
    pub repo_id: RepositoryId,
    pub root: PathBuf,
    pub partitions: Vec<GraphPartition>,
    pub diagnostics: Vec<Diagnostic>,
    pub skipped: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncrementalPlan {
    pub changed_files: Vec<PathBuf>,
    pub reverse_dependencies: Vec<PathBuf>,
    pub files_to_reindex: Vec<PathBuf>,
}
