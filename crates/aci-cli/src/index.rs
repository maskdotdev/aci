use aci_indexer::{IndexOptions, IndexPipeline, plan_incremental_reindex};
use aci_store::{GraphStore, check_partition_integrity};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::args::IndexArgs;
use crate::output::{Output, format_duration};

pub fn run_index(args: IndexArgs) -> Result<()> {
    let color = args.color.enabled();
    run_index_command(args.path, args.store, args.workers, args.changed, color)
}

pub(crate) fn run_index_command(
    path: PathBuf,
    store: PathBuf,
    workers: Option<usize>,
    changed: Vec<PathBuf>,
    color: bool,
) -> Result<()> {
    let started = Instant::now();
    let mut options = IndexOptions::new(&path);
    if let Some(workers) = workers {
        options.workers = workers;
    }
    let pipeline = IndexPipeline::default();
    let store = GraphStore::open(store)?;
    let out = Output::new(color);
    let integrity = if changed.is_empty() {
        let mut writer = store.replace_all_writer()?;
        let summary = pipeline
            .stream_path(options, |partition| writer.write(partition))
            .with_context(|| format!("indexing {}", path.display()))?;
        writer.finish()?;
        if color {
            println!(
                "{} {} files  {} {}  {} {}  {}",
                out.label("indexed"),
                out.value(summary.indexed_files),
                out.dim("skipped"),
                out.dim(&summary.skipped_files.to_string()),
                out.dim("diagnostics"),
                out.dim(&summary.diagnostics.to_string()),
                out.dim(&format_duration(started.elapsed()))
            );
        } else {
            println!(
                "indexed {} files, skipped {}, diagnostics {} in {}",
                summary.indexed_files,
                summary.skipped_files,
                summary.diagnostics,
                format_duration(started.elapsed())
            );
        }
        store.partition_file_check()?
    } else {
        let root = fs::canonicalize(&path)
            .with_context(|| format!("canonicalizing {}", path.display()))?;
        reindex_changed(
            &store,
            &pipeline,
            &root,
            options.workers,
            &changed,
            None,
            color,
        )?
    };
    for problem in integrity {
        eprintln!("integrity: {problem}");
    }
    Ok(())
}

pub(crate) fn normalize_changed_paths(
    root: &Path,
    changed: &[PathBuf],
    pipeline: &IndexPipeline,
) -> Vec<PathBuf> {
    changed
        .iter()
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| root.join(path)))
        .filter(|path| pipeline.path_candidate(path))
        .collect()
}

pub(crate) fn reindex_changed(
    store: &GraphStore,
    pipeline: &IndexPipeline,
    root: &Path,
    workers: usize,
    changed: &[PathBuf],
    ignored_root: Option<&Path>,
    color: bool,
) -> Result<Vec<String>> {
    let started = Instant::now();
    let out = Output::new(color);
    let mut changed = normalize_changed_paths(root, changed, pipeline);
    if let Some(ignored_root) = ignored_root {
        changed.retain(|path| !path.starts_with(ignored_root));
    }
    if changed.is_empty() {
        if color {
            println!(
                "{} {} files  {}",
                out.label("re-indexed"),
                out.value(0),
                out.dim(&format_duration(started.elapsed()))
            );
        } else {
            println!(
                "re-indexed 0 changed/dependent files (0 direct, 0 reverse dependencies) in {}",
                format_duration(started.elapsed())
            );
        }
        return Ok(Vec::new());
    }
    let plan = match store.plan_incremental_reindex(&changed)? {
        Some(plan) => plan,
        None => {
            let snapshot = store
                .load_latest()
                .context("loading latest snapshot for incremental reindex fallback")?;
            let plan = plan_incremental_reindex(&snapshot, &changed);
            aci_store::StoreIncrementalPlan {
                changed_files: plan.changed_files,
                reverse_dependencies: plan.reverse_dependencies,
                files_to_reindex: plan.files_to_reindex,
            }
        }
    };
    let partitions = pipeline.index_changed_paths(root, &plan.files_to_reindex, workers)?;
    let mut integrity = Vec::new();
    for partition in &partitions {
        integrity.extend(check_partition_integrity(partition));
    }
    if !partitions.is_empty() {
        store.replace_partitions(&partitions)?;
    }
    if color {
        println!(
            "{} {} files  {} {} direct  {} {} deps  {}",
            out.label("re-indexed"),
            out.value(partitions.len()),
            out.dim("→"),
            out.dim(&plan.changed_files.len().to_string()),
            out.dim("←"),
            out.dim(&plan.reverse_dependencies.len().to_string()),
            out.dim(&format_duration(started.elapsed()))
        );
    } else {
        println!(
            "re-indexed {} changed/dependent files ({} direct, {} reverse dependencies) in {}",
            partitions.len(),
            plan.changed_files.len(),
            plan.reverse_dependencies.len(),
            format_duration(started.elapsed())
        );
    }
    Ok(integrity)
}
