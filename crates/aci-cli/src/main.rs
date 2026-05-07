use aci_adapters::tree_sitter::{ExtractionMode, set_extraction_mode};
use aci_core::RepositoryId;
use aci_export::{ExportFormat, export_snapshot, import_scip_enrichment};
use aci_indexer::{IndexOptions, IndexPipeline, plan_incremental_reindex};
use aci_query::QueryEngine;
use aci_store::GraphStore;
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "aci", about = "Index and query code graphs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Index(IndexArgs),
    Query(QueryArgs),
    Export(ExportArgs),
    Bench(BenchArgs),
}

#[derive(Args)]
struct IndexArgs {
    path: PathBuf,
    #[arg(long, default_value = ".aci")]
    store: PathBuf,
    #[arg(long)]
    workers: Option<usize>,
    #[arg(long = "changed")]
    changed: Vec<PathBuf>,
}

#[derive(Args)]
struct QueryArgs {
    #[arg(long, default_value = ".aci")]
    store: PathBuf,
    #[command(subcommand)]
    command: QueryCommand,
}

#[derive(Subcommand)]
enum QueryCommand {
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
struct ExportArgs {
    #[arg(long, default_value = ".aci")]
    store: PathBuf,
    #[arg(long, default_value = "jsonl")]
    format: ExportFormat,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct BenchArgs {
    #[command(subcommand)]
    command: BenchCommand,
}

#[derive(Subcommand)]
enum BenchCommand {
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
enum BenchExtractionVariant {
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Index(args) => index(args),
        Command::Query(args) => query(args),
        Command::Export(args) => export(args),
        Command::Bench(args) => bench(args),
    }
}

fn index(args: IndexArgs) -> Result<()> {
    let mut options = IndexOptions::new(&args.path);
    if let Some(workers) = args.workers {
        options.workers = workers;
    }
    let pipeline = IndexPipeline::default();
    let store = GraphStore::open(args.store)?;
    let mut should_compact = true;
    if args.changed.is_empty() {
        let report = pipeline
            .index_path(options)
            .with_context(|| format!("indexing {}", args.path.display()))?;
        store.replace_partitions(&report.partitions)?;
        println!(
            "indexed {} files, skipped {}, diagnostics {}",
            report.partitions.len(),
            report.skipped.len(),
            report.diagnostics.len()
        );
    } else {
        should_compact = false;
        let root = fs::canonicalize(&args.path)
            .with_context(|| format!("canonicalizing {}", args.path.display()))?;
        let changed = args
            .changed
            .iter()
            .map(|path| fs::canonicalize(path).unwrap_or_else(|_| root.join(path)))
            .collect::<Vec<_>>();
        let snapshot = store.load_latest().unwrap_or_default();
        let plan = plan_incremental_reindex(&snapshot, &changed);
        let partitions =
            pipeline.index_changed_paths(&root, &plan.files_to_reindex, options.workers)?;
        store.replace_partitions(&partitions)?;
        println!(
            "re-indexed {} changed/dependent files ({} direct, {} reverse dependencies)",
            partitions.len(),
            plan.changed_files.len(),
            plan.reverse_dependencies.len()
        );
    }
    if should_compact {
        store.compact()?;
    }
    let integrity = store.integrity_check()?;
    for problem in integrity {
        eprintln!("integrity: {problem}");
    }
    Ok(())
}

fn query(args: QueryArgs) -> Result<()> {
    let store = GraphStore::open(args.store)?;
    let engine = QueryEngine::new(store.load_latest()?);
    match args.command {
        QueryCommand::Symbols { name } => {
            for node in engine.lookup_symbols(name.as_deref(), None, None, None) {
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
        QueryCommand::Deps { file } => {
            let file = fs::canonicalize(&file).unwrap_or(file);
            for dep in engine.file_dependencies(&file) {
                println!("{dep}");
            }
        }
        QueryCommand::Callers { symbol } => {
            for node in engine.callers(&symbol) {
                println!("{}", node.qualified_name.as_deref().unwrap_or_default());
            }
        }
        QueryCommand::Impact { files } => {
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

fn export(args: ExportArgs) -> Result<()> {
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

fn bench(args: BenchArgs) -> Result<()> {
    match args.command {
        BenchCommand::Cold {
            path,
            workers,
            variant,
        } => {
            let mode = variant.mode();
            set_extraction_mode(mode);
            let mut options = IndexOptions::new(&path);
            if let Some(workers) = workers {
                options.workers = workers;
            }
            let start = Instant::now();
            let report = IndexPipeline::default().index_path(options)?;
            let elapsed = start.elapsed().as_secs_f64();
            let files = report.partitions.len().max(1);
            let parse_micros = report
                .partitions
                .iter()
                .map(|partition| partition.metrics.parse_time_micros)
                .sum::<u64>();
            let extraction_micros = report
                .partitions
                .iter()
                .map(|partition| partition.metrics.extraction_time_micros)
                .sum::<u64>();
            let query_captures = report
                .partitions
                .iter()
                .map(|partition| partition.metrics.query_captures)
                .sum::<u64>();
            let mut language_counts = BTreeMap::new();
            for partition in &report.partitions {
                *language_counts
                    .entry(partition.language.as_str())
                    .or_insert(0_usize) += 1;
            }
            println!("cold_index_variant={}", mode.as_str());
            println!("cold_index_files={}", report.partitions.len());
            println!("cold_skipped_files={}", report.skipped.len());
            println!("cold_diagnostics={}", report.diagnostics.len());
            println!("cold_index_seconds={elapsed:.6}");
            for (language, count) in language_counts {
                println!("cold_language_{language}_files={count}");
            }
            println!(
                "cold_parse_seconds_per_file={:.9}",
                parse_micros as f64 / 1_000_000.0 / files as f64
            );
            println!(
                "cold_extraction_seconds_per_file={:.9}",
                extraction_micros as f64 / 1_000_000.0 / files as f64
            );
            println!(
                "cold_query_captures_per_file={:.3}",
                query_captures as f64 / files as f64
            );
        }
        BenchCommand::Query {
            store,
            name,
            iterations,
        } => {
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
        }
        BenchCommand::QueryPath {
            path,
            name,
            iterations,
            workers,
        } => {
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
        }
        BenchCommand::Semantic { iterations } => {
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
        }
    }
    Ok(())
}
