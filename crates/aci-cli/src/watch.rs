use crate::args::ColorChoice;
use crate::output::Output;
use crate::{ReindexOptions, normalize_changed_paths, reindex_changed, run_index_command};
use aci_indexer::{IndexOptions, IndexPipeline};
use aci_store::GraphStore;
use aci_watch::{WatchOptions, watch_until_quiet};
use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Args)]
pub struct WatchArgs {
    pub(crate) path: PathBuf,
    #[arg(long, default_value = ".aci")]
    pub(crate) store: PathBuf,
    #[arg(long)]
    pub(crate) workers: Option<usize>,
    #[arg(long)]
    pub(crate) max_parse_bytes: Option<usize>,
    #[arg(long, default_value_t = 150)]
    pub(crate) debounce_ms: u64,
    #[arg(long, default_value_t = 86_400_000)]
    pub(crate) max_wait_ms: u64,
    #[arg(long, help = "Wait for one debounced change batch and exit")]
    pub(crate) once: bool,
    #[arg(long, help = "Skip the initial full index before watching")]
    pub(crate) no_initial: bool,
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub(crate) color: ColorChoice,
}

pub fn run_watch(args: WatchArgs) -> Result<()> {
    let color = args.color.enabled();
    let out = Output::new(color);
    let path = fs::canonicalize(&args.path)
        .with_context(|| format!("canonicalizing {}", args.path.display()))?;
    if !args.no_initial {
        run_index_command(
            path.clone(),
            args.store.clone(),
            args.workers,
            args.max_parse_bytes,
            Vec::new(),
            color,
        )?;
    }
    let mut index_options = IndexOptions::new(&path);
    if let Some(workers) = args.workers {
        index_options.workers = workers;
    }
    index_options.max_parse_bytes = args.max_parse_bytes;
    let store_path = absolute_store_path(&args.store)?;
    let pipeline = IndexPipeline::default();
    if color {
        println!(
            "{} {}",
            out.label("watching"),
            out.path(&path.display().to_string())
        );
    } else {
        println!("watching {}", path.display());
    }
    loop {
        let mut options = WatchOptions::new(&path);
        options.debounce = Duration::from_millis(args.debounce_ms);
        let changes = watch_until_quiet(options, Duration::from_millis(args.max_wait_ms))?;
        if changes.paths.is_empty() {
            if args.once {
                if color {
                    println!("{}", out.dim("no changes observed"));
                } else {
                    println!("no changes observed");
                }
                return Ok(());
            }
            continue;
        }
        let mut changed_paths = normalize_changed_paths(&path, &changes.paths, &pipeline);
        changed_paths.retain(|path| path.is_file() && !path.starts_with(&store_path));
        if changed_paths.is_empty() {
            continue;
        }
        let store = GraphStore::open(&args.store)?;
        let integrity = reindex_changed(
            &store,
            &pipeline,
            &path,
            &changed_paths,
            ReindexOptions {
                workers: index_options.workers,
                max_parse_bytes: index_options.max_parse_bytes,
                ignored_root: None,
                color,
            },
        )?;
        for problem in integrity {
            eprintln!("integrity: {problem}");
        }
        if args.once {
            return Ok(());
        }
    }
}

fn absolute_store_path(store: &Path) -> Result<PathBuf> {
    if store.exists() {
        return fs::canonicalize(store)
            .with_context(|| format!("canonicalizing {}", store.display()));
    }
    let current_dir = std::env::current_dir().context("reading current directory")?;
    Ok(if store.is_absolute() {
        store.to_path_buf()
    } else {
        current_dir.join(store)
    })
}
