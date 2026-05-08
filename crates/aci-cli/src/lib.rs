use aci_adapters::tree_sitter::{ExtractionMode, set_extraction_mode};
use aci_core::RepositoryId;
use aci_export::{ExportFormat, export_snapshot, import_scip_enrichment};
use aci_indexer::{IndexOptions, IndexPipeline, plan_incremental_reindex};
use aci_query::QueryEngine;
use aci_store::{GraphStore, check_partition_integrity};
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "aci", about = "Index and query code graphs")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Index(args) => run_index(args),
            Command::Query(args) => run_query(args),
            Command::Export(args) => run_export(args),
            Command::Bench(args) => run_bench(args),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    Index(IndexArgs),
    Query(QueryArgs),
    Export(ExportArgs),
    Bench(BenchArgs),
}

#[derive(Args)]
pub struct IndexArgs {
    path: PathBuf,
    #[arg(long, default_value = ".aci")]
    store: PathBuf,
    #[arg(long)]
    workers: Option<usize>,
    #[arg(long = "changed")]
    changed: Vec<PathBuf>,
}

#[derive(Args)]
pub struct QueryArgs {
    #[arg(long, default_value = ".aci")]
    store: PathBuf,
    #[command(subcommand)]
    command: QueryCommand,
}

#[derive(Subcommand)]
pub enum QueryCommand {
    Symbols {
        #[arg(long)]
        name: Option<String>,
    },
    Deps {
        #[arg(long)]
        file: PathBuf,
    },
    Callers {
        symbol: String,
    },
    Impact {
        files: Vec<PathBuf>,
    },
}

#[derive(Args)]
pub struct ExportArgs {
    #[arg(long, default_value = ".aci")]
    store: PathBuf,
    #[arg(long, default_value = "jsonl")]
    format: ExportFormat,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
pub struct BenchArgs {
    #[command(subcommand)]
    command: BenchCommand,
}

#[derive(Subcommand)]
pub enum BenchCommand {
    Cold {
        path: PathBuf,
        #[arg(long)]
        workers: Option<usize>,
        #[arg(long, value_enum, default_value_t = BenchExtractionVariant::TreeSitterFallback)]
        variant: BenchExtractionVariant,
    },
    Query {
        #[arg(long, default_value = ".aci")]
        store: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 1000)]
        iterations: usize,
    },
    QueryPath {
        path: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 1000)]
        iterations: usize,
        #[arg(long)]
        workers: Option<usize>,
    },
    Semantic {
        #[arg(long, default_value_t = 1000)]
        iterations: usize,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BenchExtractionVariant {
    ScannerOnly,
    TreeSitterOnly,
    TreeSitterFallback,
    TreeSitterEnrichment,
}

impl BenchExtractionVariant {
    fn mode(self) -> ExtractionMode {
        match self {
            Self::ScannerOnly => ExtractionMode::ScannerOnly,
            Self::TreeSitterOnly => ExtractionMode::TreeSitterOnly,
            Self::TreeSitterFallback => ExtractionMode::TreeSitterWithFallback,
            Self::TreeSitterEnrichment => ExtractionMode::TreeSitterWithEnrichment,
        }
    }
}

pub fn run_index(args: IndexArgs) -> Result<()> {
    let mut options = IndexOptions::new(&args.path);
    if let Some(workers) = args.workers {
        options.workers = workers;
    }
    let pipeline = IndexPipeline::default();
    let store = GraphStore::open(args.store)?;
    let mut integrity = Vec::new();
    if args.changed.is_empty() {
        let mut writer = store.replace_all_writer()?;
        let summary = pipeline
            .stream_path(options, |partition| writer.write(partition))
            .with_context(|| format!("indexing {}", args.path.display()))?;
        writer.finish()?;
        println!(
            "indexed {} files, skipped {}, diagnostics {}",
            summary.indexed_files, summary.skipped_files, summary.diagnostics
        );
        integrity = store.partition_file_check()?;
    } else {
        let root = fs::canonicalize(&args.path)
            .with_context(|| format!("canonicalizing {}", args.path.display()))?;
        let changed = normalize_changed_paths(&root, &args.changed, &pipeline);
        let plan = match store.plan_incremental_reindex(&changed)? {
            Some(plan) => plan,
            None => {
                let snapshot = store.load_latest().unwrap_or_default();
                let plan = plan_incremental_reindex(&snapshot, &changed);
                aci_store::StoreIncrementalPlan {
                    changed_files: plan.changed_files,
                    reverse_dependencies: plan.reverse_dependencies,
                    files_to_reindex: plan.files_to_reindex,
                }
            }
        };
        let partitions =
            pipeline.index_changed_paths(&root, &plan.files_to_reindex, options.workers)?;
        for partition in &partitions {
            integrity.extend(check_partition_integrity(partition));
        }
        if !partitions.is_empty() {
            store.replace_partitions(&partitions)?;
        }
        println!(
            "re-indexed {} changed/dependent files ({} direct, {} reverse dependencies)",
            partitions.len(),
            plan.changed_files.len(),
            plan.reverse_dependencies.len()
        );
    }
    for problem in integrity {
        eprintln!("integrity: {problem}");
    }
    Ok(())
}

pub fn run_query(args: QueryArgs) -> Result<()> {
    let store = GraphStore::open(args.store)?;
    match args.command {
        QueryCommand::Symbols { name } => print_symbols(&store, name.as_deref())?,
        QueryCommand::Deps { file } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let file = fs::canonicalize(&file).unwrap_or(file);
            for dep in engine.file_dependencies(&file) {
                println!("{dep}");
            }
        }
        QueryCommand::Callers { symbol } => {
            let engine = QueryEngine::new(store.load_latest()?);
            for node in engine.callers(&symbol) {
                println!("{}", node.qualified_name.as_deref().unwrap_or_default());
            }
        }
        QueryCommand::Impact { files } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let files = files
                .into_iter()
                .map(|file| fs::canonicalize(&file).unwrap_or(file))
                .collect::<Vec<_>>();
            for file in engine.impact_from_files(&files) {
                println!("{}", file.display());
            }
        }
    }
    Ok(())
}

pub fn run_export(args: ExportArgs) -> Result<()> {
    let store = GraphStore::open(args.store)?;
    let snapshot = store.load_latest()?;
    if let Some(output) = args.output {
        let file = fs::File::create(output)?;
        export_snapshot(&snapshot, args.format, file)?;
    } else {
        let stdout = std::io::stdout();
        let handle = stdout.lock();
        export_snapshot(&snapshot, args.format, handle)?;
    }
    Ok(())
}

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

fn normalize_changed_paths(
    root: &std::path::Path,
    changed: &[PathBuf],
    pipeline: &IndexPipeline,
) -> Vec<PathBuf> {
    changed
        .iter()
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| root.join(path)))
        .filter(|path| pipeline.path_candidate(path))
        .collect()
}

fn print_symbols(store: &GraphStore, name: Option<&str>) -> Result<()> {
    if let Some(entries) = store.lookup_symbol_index(name)? {
        for entry in entries {
            println!(
                "{}\t{}\t{}",
                entry.qualified_name.as_deref().unwrap_or_default(),
                entry
                    .symbol_kind
                    .map(|kind| format!("{kind:?}"))
                    .unwrap_or_default(),
                entry
                    .file_id
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
        }
    } else {
        let engine = QueryEngine::new(store.load_latest()?);
        for node in engine.lookup_symbols(name, None, None, None) {
            println!(
                "{}\t{}\t{}",
                node.qualified_name.as_deref().unwrap_or_default(),
                node.symbol_kind
                    .map(|kind| format!("{kind:?}"))
                    .unwrap_or_default(),
                node.file_id
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
        }
    }
    Ok(())
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
    println!(
        "cold_query_captures_per_file={:.3}",
        summary.query_captures as f64 / files as f64
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
