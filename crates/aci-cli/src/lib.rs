use aci_adapters::tree_sitter::{ExtractionMode, set_extraction_mode};
use aci_core::{GraphNode, RepositoryId, SourceSpan};
use aci_export::{ExportFormat, export_snapshot, import_scip_enrichment};
use aci_indexer::{IndexOptions, IndexPipeline, plan_incremental_reindex};
use aci_query::QueryEngine;
use aci_store::{GraphStore, SymbolIndexEntry, check_partition_integrity};
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::fs;
use std::io::{Cursor, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod output;
mod watch;

use output::{TableStyle, format_location, print_table};
use watch::{WatchArgs, run_watch};

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
            Command::Watch(args) => run_watch(args),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    Index(IndexArgs),
    Query(QueryArgs),
    Export(ExportArgs),
    Bench(BenchArgs),
    Watch(WatchArgs),
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
    #[arg(long, help = "Render query results as aligned tables")]
    pretty: bool,
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
    #[arg(long, value_enum, default_value_t = QueryFormat::Text)]
    format: QueryFormat,
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
    Callees {
        symbol: String,
    },
    Refs {
        symbol: String,
    },
    Packages,
    DepsTree {
        symbol: String,
        #[arg(long, default_value_t = 3)]
        depth: usize,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum QueryFormat {
    Text,
    Json,
}

impl ColorChoice {
    fn enabled(self) -> bool {
        match self {
            Self::Auto => std::io::stdout().is_terminal(),
            Self::Always => true,
            Self::Never => false,
        }
    }
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
    run_index_command(args.path, args.store, args.workers, args.changed)
}

pub(crate) fn run_index_command(
    path: PathBuf,
    store: PathBuf,
    workers: Option<usize>,
    changed: Vec<PathBuf>,
) -> Result<()> {
    let mut options = IndexOptions::new(&path);
    if let Some(workers) = workers {
        options.workers = workers;
    }
    let pipeline = IndexPipeline::default();
    let store = GraphStore::open(store)?;
    let integrity = if changed.is_empty() {
        let mut writer = store.replace_all_writer()?;
        let summary = pipeline
            .stream_path(options, |partition| writer.write(partition))
            .with_context(|| format!("indexing {}", path.display()))?;
        writer.finish()?;
        println!(
            "indexed {} files, skipped {}, diagnostics {}",
            summary.indexed_files, summary.skipped_files, summary.diagnostics
        );
        store.partition_file_check()?
    } else {
        let root = fs::canonicalize(&path)
            .with_context(|| format!("canonicalizing {}", path.display()))?;
        reindex_changed(&store, &pipeline, &root, options.workers, &changed, None)?
    };
    for problem in integrity {
        eprintln!("integrity: {problem}");
    }
    Ok(())
}

pub fn run_query(args: QueryArgs) -> Result<()> {
    let store = GraphStore::open(args.store)?;
    let style = TableStyle::new(args.pretty && args.color.enabled());
    match args.command {
        QueryCommand::Symbols { name } => {
            print_symbols(&store, name.as_deref(), args.pretty, style, args.format)?
        }
        QueryCommand::Deps { file } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let file = fs::canonicalize(&file).unwrap_or(file);
            let deps = engine.file_dependencies(&file);
            if args.format == QueryFormat::Json {
                print_query_json(
                    "deps",
                    json!({
                        "file": file,
                        "dependencies": deps,
                    }),
                )?;
            } else if args.pretty {
                print_table(
                    &["Dependency"],
                    deps.into_iter().map(|dep| vec![dep]),
                    style,
                );
            } else {
                for dep in deps {
                    println!("{dep}");
                }
            }
        }
        QueryCommand::Callers { symbol } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let records = engine
                .callers(&symbol)
                .into_iter()
                .map(|node| node_record(&engine, node))
                .collect::<Vec<_>>();
            if args.format == QueryFormat::Json {
                print_query_json(
                    "callers",
                    Value::Array(records.iter().map(QueryNode::to_json).collect()),
                )?;
            } else if args.pretty {
                let rows = records.iter().map(QueryNode::text_row).collect::<Vec<_>>();
                print_table(&["Caller", "Kind", "Location"], rows, style);
            } else {
                for record in records {
                    print_node_row(record.text_row());
                }
            }
        }
        QueryCommand::Callees { symbol } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let records = engine
                .matching_symbols(&symbol)
                .into_iter()
                .flat_map(|node| engine.callees(&node.id))
                .map(|node| node_record(&engine, node))
                .collect::<Vec<_>>();
            if args.format == QueryFormat::Json {
                print_query_json(
                    "callees",
                    Value::Array(records.iter().map(QueryNode::to_json).collect()),
                )?;
            } else if args.pretty {
                let rows = records.iter().map(QueryNode::text_row).collect::<Vec<_>>();
                print_table(&["Callee", "Kind", "Location"], rows, style);
            } else {
                for record in records {
                    print_node_row(record.text_row());
                }
            }
        }
        QueryCommand::Refs { symbol } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let records = engine
                .references(&symbol)
                .into_iter()
                .map(|node| node_record(&engine, node))
                .collect::<Vec<_>>();
            if args.format == QueryFormat::Json {
                print_query_json(
                    "refs",
                    Value::Array(records.iter().map(QueryNode::to_json).collect()),
                )?;
            } else if args.pretty {
                let rows = records.iter().map(QueryNode::text_row).collect::<Vec<_>>();
                print_table(&["Reference", "Kind", "Location"], rows, style);
            } else {
                for record in records {
                    print_node_row(record.text_row());
                }
            }
        }
        QueryCommand::Packages => {
            let engine = QueryEngine::new(store.load_latest()?);
            let packages = engine.package_dependencies();
            if args.format == QueryFormat::Json {
                print_query_json("packages", json!(packages))?;
            } else if args.pretty {
                print_table(
                    &["Package"],
                    packages.into_iter().map(|package| vec![package]),
                    style,
                );
            } else {
                for package in packages {
                    println!("{package}");
                }
            }
        }
        QueryCommand::DepsTree { symbol, depth } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let records = engine
                .matching_symbols(&symbol)
                .into_iter()
                .flat_map(|node| engine.traverse_dependencies(&node.id, depth))
                .map(|node| node_record(&engine, node))
                .collect::<Vec<_>>();
            if args.format == QueryFormat::Json {
                print_query_json(
                    "deps-tree",
                    json!({
                        "symbol": symbol,
                        "depth": depth,
                        "dependencies": records.iter().map(QueryNode::to_json).collect::<Vec<_>>(),
                    }),
                )?;
            } else if args.pretty {
                let rows = records.iter().map(QueryNode::text_row).collect::<Vec<_>>();
                print_table(&["Dependency", "Kind", "Location"], rows, style);
            } else {
                for record in records {
                    print_node_row(record.text_row());
                }
            }
        }
        QueryCommand::Impact { files } => {
            let engine = QueryEngine::new(store.load_latest()?);
            let files = files
                .into_iter()
                .map(|file| fs::canonicalize(&file).unwrap_or(file))
                .collect::<Vec<_>>();
            let impacted = engine.impact_from_files(&files);
            if args.format == QueryFormat::Json {
                print_query_json(
                    "impact",
                    json!({
                        "files": files,
                        "impacted": impacted,
                    }),
                )?;
            } else if args.pretty {
                print_table(
                    &["Impacted file"],
                    impacted
                        .into_iter()
                        .map(|file| vec![file.display().to_string()]),
                    style,
                );
            } else {
                for file in impacted {
                    println!("{}", file.display());
                }
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

pub(crate) fn normalize_changed_paths(
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

pub(crate) fn reindex_changed(
    store: &GraphStore,
    pipeline: &IndexPipeline,
    root: &std::path::Path,
    workers: usize,
    changed: &[PathBuf],
    ignored_root: Option<&std::path::Path>,
) -> Result<Vec<String>> {
    let mut changed = normalize_changed_paths(root, changed, pipeline);
    if let Some(ignored_root) = ignored_root {
        changed.retain(|path| !path.starts_with(ignored_root));
    }
    if changed.is_empty() {
        println!("re-indexed 0 changed/dependent files (0 direct, 0 reverse dependencies)");
        return Ok(Vec::new());
    }
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
    let partitions = pipeline.index_changed_paths(root, &plan.files_to_reindex, workers)?;
    let mut integrity = Vec::new();
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
    Ok(integrity)
}

fn print_symbols(
    store: &GraphStore,
    name: Option<&str>,
    pretty: bool,
    style: TableStyle,
    format: QueryFormat,
) -> Result<()> {
    let mut rows = Vec::new();
    let mut records = Vec::new();
    if let Some(entries) = store.lookup_symbol_index(name)? {
        for entry in entries {
            if format == QueryFormat::Json {
                records.push(symbol_index_entry_json(&entry));
            } else {
                rows.push(vec![
                    entry.qualified_name.unwrap_or_default(),
                    entry
                        .symbol_kind
                        .map(|kind| format!("{kind:?}"))
                        .unwrap_or_default(),
                    format_location(entry.path.as_deref(), entry.span.as_ref())
                        .or_else(|| entry.file_id.map(|file_id| file_id.to_string()))
                        .unwrap_or_default(),
                ]);
            }
        }
    } else {
        let snapshot = store.load_latest()?;
        let file_paths = snapshot
            .partitions
            .iter()
            .map(|partition| (partition.file_id.clone(), partition.path.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let engine = QueryEngine::new(snapshot);
        for node in engine.lookup_symbols(name, None, None, None) {
            let path = node
                .file_id
                .as_ref()
                .and_then(|file_id| file_paths.get(file_id));
            if format == QueryFormat::Json {
                records.push(node_json(node, path.map(PathBuf::as_path)));
            } else {
                rows.push(vec![
                    node.qualified_name.clone().unwrap_or_default(),
                    node.symbol_kind
                        .map(|kind| format!("{kind:?}"))
                        .unwrap_or_default(),
                    format_location(path.map(PathBuf::as_path), node.span.as_ref())
                        .or_else(|| node.file_id.as_ref().map(ToString::to_string))
                        .unwrap_or_default(),
                ]);
            }
        }
    }
    if format == QueryFormat::Json {
        print_query_json("symbols", Value::Array(records))?;
    } else if pretty {
        print_table(&["Symbol", "Kind", "Location"], rows, style);
    } else {
        for row in rows {
            println!("{}\t{}\t{}", row[0], row[1], row[2]);
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
    Ok(())
}

#[derive(Clone, Debug)]
struct QueryNode {
    value: Value,
    row: Vec<String>,
}

impl QueryNode {
    fn text_row(&self) -> Vec<String> {
        self.row.clone()
    }

    fn to_json(&self) -> Value {
        self.value.clone()
    }
}

fn node_record(engine: &QueryEngine, node: &GraphNode) -> QueryNode {
    let path = engine.path_for_node(node);
    let name = node
        .qualified_name
        .clone()
        .or_else(|| node.name.clone())
        .unwrap_or_default();
    let kind = node
        .symbol_kind
        .map(|kind| format!("{kind:?}"))
        .unwrap_or_else(|| format!("{:?}", node.kind));
    let location = format_location(path, node.span.as_ref()).unwrap_or_default();
    QueryNode {
        value: node_json(node, path),
        row: vec![name, kind, location],
    }
}

fn print_node_row(row: Vec<String>) {
    println!("{}\t{}\t{}", row[0], row[1], row[2]);
}

fn print_query_json(query: &str, results: Value) -> Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(
        &mut handle,
        &json!({
            "query": query,
            "results": results,
        }),
    )?;
    writeln!(handle)?;
    Ok(())
}

fn symbol_index_entry_json(entry: &SymbolIndexEntry) -> Value {
    json!({
        "name": entry.name,
        "qualified_name": entry.qualified_name,
        "symbol_kind": entry.symbol_kind,
        "file_id": entry.file_id.as_ref().map(ToString::to_string),
        "path": entry.path,
        "location": format_location(entry.path.as_deref(), entry.span.as_ref()),
        "span": span_json(entry.span.as_ref()),
        "provenance": entry.provenance,
        "confidence": entry.confidence,
    })
}

fn node_json(node: &GraphNode, path: Option<&Path>) -> Value {
    json!({
        "id": node.id.to_string(),
        "node_kind": node.kind,
        "language": node.language,
        "name": node.name,
        "qualified_name": node.qualified_name,
        "symbol_kind": node.symbol_kind,
        "file_id": node.file_id.as_ref().map(ToString::to_string),
        "path": path,
        "location": format_location(path, node.span.as_ref()),
        "span": span_json(node.span.as_ref()),
        "provenance": node.provenance,
        "confidence": node.confidence,
    })
}

fn span_json(span: Option<&SourceSpan>) -> Value {
    span.map(|span| {
        json!({
            "byte_start": span.byte_start,
            "byte_end": span.byte_end,
            "start": {
                "line": span.start.line,
                "column": span.start.column,
            },
            "end": {
                "line": span.end.line,
                "column": span.end.column,
            },
        })
    })
    .unwrap_or(Value::Null)
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
