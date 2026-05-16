use crate::discover::{discover_files, is_binary, is_vendor_or_generated};
use crate::{IndexOptions, IndexReport, IndexSummary};
use aci_adapters::{AdapterRegistry, ExtractionOptions, default_registry};
use aci_core::{Diagnostic, GraphPartition, Language, RepositoryId, Result, SourceFile};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

pub struct IndexPipeline {
    registry: AdapterRegistry,
}

impl Default for IndexPipeline {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

impl IndexPipeline {
    pub fn new(registry: AdapterRegistry) -> Self {
        Self { registry }
    }

    pub fn index_path(&self, options: IndexOptions) -> Result<IndexReport> {
        let root = options.root.canonicalize()?;
        let extraction_options =
            ExtractionOptions::default().with_max_parse_bytes(options.max_parse_bytes);
        let repo_id = RepositoryId::new("repo", &[root.to_string_lossy().as_ref()]);
        let candidates = discover_files(&root)?;
        let pool = thread_pool(options.workers)?;
        let indexed = pool.install(|| {
            candidates
                .par_iter()
                .map(|path| self.index_file(&repo_id, &root, path, extraction_options))
                .collect::<Vec<_>>()
        });

        let mut partitions = Vec::new();
        let mut diagnostics = Vec::new();
        let mut skipped = Vec::new();
        for item in indexed {
            match item {
                Ok(Some(partition)) => partitions.push(partition),
                Ok(None) => {}
                Err(FileSkip::Skipped(path)) => skipped.push(path),
                Err(FileSkip::Diagnostic(diagnostic)) => diagnostics.push(diagnostic),
            }
        }
        partitions.sort_by(|left, right| left.path.cmp(&right.path));
        skipped.sort();
        Ok(IndexReport {
            repo_id,
            root,
            partitions,
            diagnostics,
            skipped,
        })
    }

    pub fn summarize_path(&self, options: IndexOptions) -> Result<IndexSummary> {
        let root = options.root.canonicalize()?;
        let extraction_options =
            ExtractionOptions::default().with_max_parse_bytes(options.max_parse_bytes);
        let repo_id = RepositoryId::new("repo", &[root.to_string_lossy().as_ref()]);
        let candidates = discover_files(&root)?;
        let pool = thread_pool(options.workers)?;
        Ok(pool.install(|| {
            candidates
                .par_iter()
                .map(
                    |path| match self.index_file(&repo_id, &root, path, extraction_options) {
                        Ok(Some(partition)) => IndexSummary::from(partition),
                        Ok(None) => IndexSummary::default(),
                        Err(FileSkip::Skipped(_)) => IndexSummary {
                            skipped_files: 1,
                            ..IndexSummary::default()
                        },
                        Err(FileSkip::Diagnostic(_)) => IndexSummary {
                            diagnostics: 1,
                            ..IndexSummary::default()
                        },
                    },
                )
                .reduce(IndexSummary::default, IndexSummary::merge)
        }))
    }

    pub fn stream_path<F>(&self, options: IndexOptions, mut on_partition: F) -> Result<IndexSummary>
    where
        F: FnMut(&GraphPartition) -> Result<()>,
    {
        let root = options.root.canonicalize()?;
        let extraction_options =
            ExtractionOptions::default().with_max_parse_bytes(options.max_parse_bytes);
        let repo_id = RepositoryId::new("repo", &[root.to_string_lossy().as_ref()]);
        let candidates = discover_files(&root)?;
        let pool = thread_pool(options.workers)?;
        let (tx, rx) = mpsc::sync_channel(options.workers.max(1) * 2);
        let mut summary = IndexSummary::default();
        let mut sink_error = None;

        let producer_result = std::thread::scope(|scope| {
            let producer = scope.spawn(|| {
                pool.install(|| {
                    candidates.par_iter().try_for_each_with(tx, |tx, path| {
                        tx.send(self.index_file(&repo_id, &root, path, extraction_options))
                            .map_err(|_| {
                                aci_core::AciError::Message(
                                    "index stream receiver closed".to_string(),
                                )
                            })
                    })
                })
            });

            while let Ok(item) = rx.recv() {
                match item {
                    Ok(Some(partition)) => {
                        if let Err(error) = on_partition(&partition) {
                            sink_error = Some(error);
                            break;
                        }
                        summary.merge_partition(&partition);
                    }
                    Ok(None) => {}
                    Err(FileSkip::Skipped(_)) => summary.skipped_files += 1,
                    Err(FileSkip::Diagnostic(_)) => summary.diagnostics += 1,
                }
            }
            drop(rx);
            producer
                .join()
                .map_err(|_| aci_core::AciError::Message("index producer panicked".to_string()))?
        });

        if let Some(error) = sink_error {
            return Err(error);
        }
        producer_result?;
        Ok(summary)
    }

    pub fn index_changed_paths(
        &self,
        root: &Path,
        changed_paths: &[PathBuf],
        workers: usize,
        max_parse_bytes: Option<usize>,
    ) -> Result<Vec<GraphPartition>> {
        let root = root.canonicalize()?;
        let extraction_options = ExtractionOptions::default().with_max_parse_bytes(max_parse_bytes);
        let repo_id = RepositoryId::new("repo", &[root.to_string_lossy().as_ref()]);
        let pool = thread_pool(workers)?;
        let indexed = pool.install(|| {
            changed_paths
                .par_iter()
                .filter(|path| path.exists())
                .map(|path| self.index_file(&repo_id, &root, path, extraction_options))
                .collect::<Vec<_>>()
        });

        let mut partitions = Vec::new();
        for partition in indexed.into_iter().flatten().flatten() {
            partitions.push(partition);
        }
        partitions.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(partitions)
    }

    pub fn path_candidate(&self, path: &Path) -> bool {
        !is_vendor_or_generated(path) && self.registry.path_candidate(path)
    }

    fn index_file(
        &self,
        repo_id: &RepositoryId,
        root: &Path,
        path: &Path,
        extraction_options: ExtractionOptions,
    ) -> std::result::Result<Option<GraphPartition>, FileSkip> {
        if is_vendor_or_generated(path) {
            return Err(FileSkip::Skipped(path.to_path_buf()));
        }
        if !self.registry.path_candidate(path) {
            return Ok(None);
        }
        let bytes = fs::read(path).map_err(|error| {
            FileSkip::Diagnostic(Diagnostic::warning(error.to_string(), None, None))
        })?;
        if is_binary(&bytes) {
            return Err(FileSkip::Skipped(path.to_path_buf()));
        }
        let language = self.registry.detect_language(path, &bytes);
        if language == Language::Unknown {
            return Ok(None);
        }
        let text = String::from_utf8(bytes).map_err(|error| {
            FileSkip::Diagnostic(Diagnostic::warning(error.to_string(), None, None))
        })?;
        let source = SourceFile::new(repo_id.clone(), root, path.to_path_buf(), language, text);
        let started = Instant::now();
        let mut partition = self
            .registry
            .extract_with_options(&source, extraction_options);
        partition.metrics.extraction_time_micros = started.elapsed().as_micros() as u64;
        Ok(Some(partition))
    }
}

enum FileSkip {
    Skipped(PathBuf),
    Diagnostic(Diagnostic),
}

fn thread_pool(workers: usize) -> Result<rayon::ThreadPool> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers.max(1))
        .build()
        .map_err(|error| aci_core::AciError::Message(error.to_string()))
}
