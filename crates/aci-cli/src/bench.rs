use aci_adapters::tree_sitter::set_extraction_mode;
use aci_core::RepositoryId;
use aci_export::import_scip_enrichment;
use aci_indexer::{IndexOptions, IndexPipeline};
use aci_query::QueryEngine;
use aci_store::GraphStore;
use anyhow::Result;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Instant;

use crate::args::{BenchArgs, BenchCommand, BenchExtractionVariant};

pub fn run_bench(args: BenchArgs) -> Result<()> {
    match args.command {
        BenchCommand::Cold {
            path,
            workers,
            variant,
        } => bench_cold(path, workers, variant),
        BenchCommand::Query {
            store,
            name,
            iterations,
        } => bench_query(store, name, iterations),
        BenchCommand::QueryPath {
            path,
            name,
            iterations,
            workers,
        } => bench_query_path(path, name, iterations, workers),
        BenchCommand::Semantic { iterations } => bench_semantic(iterations),
    }
}

fn bench_cold(
    path: PathBuf,
    workers: Option<usize>,
    variant: BenchExtractionVariant,
) -> Result<()> {
    let mode = variant.mode();
    set_extraction_mode(mode);
    let mut options = IndexOptions::new(&path);
    if let Some(workers) = workers {
        options.workers = workers;
    }
    let start = Instant::now();
    let summary = IndexPipeline::default().summarize_path(options)?;
    let elapsed = start.elapsed().as_secs_f64();
    let files = summary.indexed_files.max(1);
    println!("cold_index_variant={}", mode.as_str());
    println!("cold_index_files={}", summary.indexed_files);
    println!("cold_skipped_files={}", summary.skipped_files);
    println!("cold_diagnostics={}", summary.diagnostics);
    println!("cold_nodes={}", summary.nodes);
    println!("cold_edges={}", summary.edges);
    println!("cold_max_nodes_per_file={}", summary.max_nodes_per_file);
    println!("cold_max_edges_per_file={}", summary.max_edges_per_file);
    println!("cold_index_seconds={elapsed:.6}");
    for (language, count) in summary.language_counts {
        println!("cold_language_{}_files={count}", language.as_str());
    }
    println!(
        "cold_parse_seconds_per_file={:.9}",
        summary.parse_time_micros as f64 / 1_000_000.0 / files as f64
    );
    println!(
        "cold_extraction_seconds_per_file={:.9}",
        summary.extraction_time_micros as f64 / 1_000_000.0 / files as f64
    );
    Ok(())
}

fn bench_query(store: PathBuf, name: String, iterations: usize) -> Result<()> {
    let store = GraphStore::open(store)?;
    let engine = QueryEngine::new(store.load_latest()?);
    let iterations = iterations.max(1);
    let start = Instant::now();
    let mut hits = 0_usize;
    for _ in 0..iterations {
        hits += engine.lookup_symbols(Some(&name), None, None, None).len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("query_iterations={iterations}");
    println!("query_hits={hits}");
    println!("query_average_seconds={:.9}", elapsed / iterations as f64);
    Ok(())
}

fn bench_query_path(
    path: PathBuf,
    name: String,
    iterations: usize,
    workers: Option<usize>,
) -> Result<()> {
    let mut options = IndexOptions::new(&path);
    if let Some(workers) = workers {
        options.workers = workers;
    }
    let report = IndexPipeline::default().index_path(options)?;
    let engine = QueryEngine::new(aci_core::GraphSnapshot {
        partitions: report.partitions,
    });
    let iterations = iterations.max(1);
    let start = Instant::now();
    let mut hits = 0_usize;
    for _ in 0..iterations {
        hits += engine.lookup_symbols(Some(&name), None, None, None).len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("query_files={}", engine.symbols().len());
    println!("query_iterations={iterations}");
    println!("query_hits={hits}");
    println!("query_average_seconds={:.9}", elapsed / iterations as f64);
    Ok(())
}

fn bench_semantic(iterations: usize) -> Result<()> {
    let iterations = iterations.max(1);
    let input = br#"{
      "documents": [{
        "relativePath": "src/main.py",
        "occurrences": [
          { "symbol": "local 0 main().", "range": [0, 4, 8], "roles": 1 },
          { "symbol": "local 0 helper().", "range": [1, 2, 8], "roles": 0 }
        ]
      }]
    }"#;
    let repo = RepositoryId::new("repo", &["semantic-bench"]);
    let start = Instant::now();
    let mut partitions = 0_usize;
    for _ in 0..iterations {
        let snapshot = import_scip_enrichment(
            repo.clone(),
            std::path::Path::new("/repo"),
            Cursor::new(input),
        )?;
        partitions += snapshot.partitions.len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("semantic_iterations={iterations}");
    println!("semantic_partitions={partitions}");
    println!(
        "semantic_refresh_seconds={:.9}",
        elapsed / iterations as f64
    );
    Ok(())
}
