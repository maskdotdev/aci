use crate::{ChangeKind, ChangedSymbol, DiffStats, FileChange, IndexedRef};

pub(crate) struct StatsInput<'a> {
    pub(crate) files: &'a [FileChange],
    pub(crate) symbols: &'a [ChangedSymbol],
    pub(crate) public_api_changes: usize,
    pub(crate) dependency_changes: usize,
    pub(crate) impacted_files: usize,
    pub(crate) diagnostics: usize,
    pub(crate) base: &'a IndexedRef,
    pub(crate) head: &'a IndexedRef,
}

pub(crate) fn stats(input: StatsInput<'_>) -> DiffStats {
    let mut stats = DiffStats {
        public_api_changes: input.public_api_changes,
        dependency_changes: input.dependency_changes,
        impacted_files: input.impacted_files,
        diagnostics: input.diagnostics,
        base_indexed_files: input.base.snapshot.partitions.len(),
        head_indexed_files: input.head.snapshot.partitions.len(),
        base_skipped_files: input.base.skipped.len(),
        head_skipped_files: input.head.skipped.len(),
        ..DiffStats::default()
    };
    for file in input.files {
        match file.change {
            ChangeKind::Added | ChangeKind::Copied => stats.files_added += 1,
            ChangeKind::Removed => stats.files_removed += 1,
            ChangeKind::Modified | ChangeKind::TypeChanged => stats.files_modified += 1,
            ChangeKind::Renamed => stats.files_renamed += 1,
        }
    }
    for symbol in input.symbols {
        match symbol.change {
            ChangeKind::Added | ChangeKind::Copied => stats.symbols_added += 1,
            ChangeKind::Removed => stats.symbols_removed += 1,
            ChangeKind::Modified | ChangeKind::Renamed | ChangeKind::TypeChanged => {
                stats.symbols_modified += 1;
            }
        }
    }
    stats
}
