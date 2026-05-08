use aci_core::{GraphNode, SourceSpan};
use aci_query::QueryEngine;
use aci_store::{GraphStore, SymbolIndexEntry};
use anyhow::Result;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::args::{QueryArgs, QueryCommand, QueryFormat};
use crate::output::{Output, TableStyle, format_duration, format_location, print_table};

pub fn run_query(args: QueryArgs) -> Result<()> {
    let started = Instant::now();
    let display_root = display_root_for_store(&args.store);
    let color = args.color.enabled();
    let out = Output::new(color);
    let store = GraphStore::open(args.store)?;
    let display_root = display_root.as_deref();
    let style = TableStyle::new(args.pretty && color);
    match args.command {
        QueryCommand::Symbols { name } => print_symbols(
            &store,
            name.as_deref(),
            args.pretty,
            style,
            args.format,
            display_root,
        )?,
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
                .map(|node| node_record(&engine, node, display_root))
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
                .map(|node| node_record(&engine, node, display_root))
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
                .map(|node| node_record(&engine, node, display_root))
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
                .map(|node| node_record(&engine, node, display_root))
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
                        .map(|file| vec![display_path_string(&file, display_root)]),
                    style,
                );
            } else {
                for file in impacted {
                    println!("{}", display_path_string(&file, display_root));
                }
            }
        }
    }
    std::io::stdout().flush()?;
    let timing = format!("query completed in {}", format_duration(started.elapsed()));
    if color {
        eprintln!("{}", out.dim(&timing));
    } else {
        eprintln!("{timing}");
    }
    Ok(())
}

fn print_symbols(
    store: &GraphStore,
    name: Option<&str>,
    pretty: bool,
    style: TableStyle,
    format: QueryFormat,
    display_root: Option<&Path>,
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
                    format_human_location(entry.path.as_deref(), entry.span.as_ref(), display_root)
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
                    format_human_location(
                        path.map(PathBuf::as_path),
                        node.span.as_ref(),
                        display_root,
                    )
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

fn display_root_for_store(store: &Path) -> Option<PathBuf> {
    if store.file_name().and_then(|name| name.to_str()) != Some(".aci") {
        return None;
    }
    let parent = store.parent()?;
    Some(fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()))
}

fn format_human_location(
    path: Option<&Path>,
    span: Option<&SourceSpan>,
    display_root: Option<&Path>,
) -> Option<String> {
    let path = path.map(|path| display_path_string(path, display_root));
    match (path, span) {
        (Some(path), Some(span)) => Some(format!(
            "{}:{}:{}",
            path, span.start.line, span.start.column
        )),
        (Some(path), None) => Some(path),
        (None, Some(span)) => Some(format!("{}:{}", span.start.line, span.start.column)),
        (None, None) => None,
    }
}

fn display_path_string(path: &Path, display_root: Option<&Path>) -> String {
    display_path(path, display_root).display().to_string()
}

fn display_path<'a>(path: &'a Path, display_root: Option<&Path>) -> Cow<'a, Path> {
    if let Some(root) = display_root {
        if let Ok(relative) = path.strip_prefix(root) {
            if !relative.as_os_str().is_empty() {
                return Cow::Owned(relative.to_path_buf());
            }
        }
    }
    Cow::Borrowed(path)
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

fn node_record(engine: &QueryEngine, node: &GraphNode, display_root: Option<&Path>) -> QueryNode {
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
    let location =
        format_human_location(path, node.span.as_ref(), display_root).unwrap_or_default();
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
