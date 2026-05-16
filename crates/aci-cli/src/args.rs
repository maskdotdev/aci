use aci_adapters::tree_sitter::ExtractionMode;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use std::path::PathBuf;

use crate::watch::WatchArgs;

#[derive(Parser)]
#[command(name = "aci", about = "Index and query code graphs")]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    Index(IndexArgs),
    Diff(DiffArgs),
    Query(QueryArgs),
    Export(ExportArgs),
    Bench(BenchArgs),
    Watch(WatchArgs),
}

#[derive(Args)]
pub struct IndexArgs {
    pub(crate) path: PathBuf,
    #[arg(long, default_value = ".aci")]
    pub(crate) store: PathBuf,
    #[arg(long)]
    pub(crate) workers: Option<usize>,
    #[arg(long = "changed")]
    pub(crate) changed: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub(crate) color: ColorChoice,
}

#[derive(Args)]
pub struct DiffArgs {
    pub(crate) base: String,
    pub(crate) head: String,
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,
    #[arg(long)]
    pub(crate) workers: Option<usize>,
    #[arg(long, help = "Render diff results as aligned tables")]
    pub(crate) pretty: bool,
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub(crate) color: ColorChoice,
    #[arg(long, value_enum, default_value_t = QueryFormat::Text)]
    pub(crate) format: QueryFormat,
}

#[derive(Args)]
pub struct QueryArgs {
    #[arg(long, default_value = ".aci", global = true)]
    pub(crate) store: PathBuf,
    #[arg(long, global = true, help = "Render query results as aligned tables")]
    pub(crate) pretty: bool,
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    pub(crate) color: ColorChoice,
    #[arg(long, value_enum, default_value_t = QueryFormat::Text, global = true)]
    pub(crate) format: QueryFormat,
    #[command(subcommand)]
    pub(crate) command: QueryCommand,
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
    pub(crate) store: PathBuf,
    #[arg(long, default_value = "jsonl")]
    pub(crate) format: aci_export::ExportFormat,
    #[arg(long)]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub struct BenchArgs {
    #[command(subcommand)]
    pub(crate) command: BenchCommand,
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
    pub(crate) fn enabled(self) -> bool {
        match self {
            Self::Auto => std::io::stdout().is_terminal(),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

impl BenchExtractionVariant {
    pub(crate) fn mode(self) -> ExtractionMode {
        match self {
            Self::ScannerOnly => ExtractionMode::ScannerOnly,
            Self::TreeSitterOnly => ExtractionMode::TreeSitterOnly,
            Self::TreeSitterFallback => ExtractionMode::TreeSitterWithFallback,
            Self::TreeSitterEnrichment => ExtractionMode::TreeSitterWithEnrichment,
        }
    }
}
