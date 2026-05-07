use aci_export::{ExportFormat, export_snapshot};
use aci_indexer::{IndexOptions, IndexPipeline, plan_incremental_reindex};
use aci_query::QueryEngine;
use aci_store::GraphStore;
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Index(args) => index(args),
        Command::Query(args) => query(args),
        Command::Export(args) => export(args),
    }
}

fn index(args: IndexArgs) -> Result<()> {
    let mut options = IndexOptions::new(&args.path);
    if let Some(workers) = args.workers {
        options.workers = workers;
    }
    let pipeline = IndexPipeline::default();
    let store = GraphStore::open(args.store)?;
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
    store.compact()?;
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
