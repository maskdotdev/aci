use anyhow::Result;

mod args;
mod bench;
mod export;
mod index;
mod output;
mod query;
mod watch;

pub use args::{
    BenchArgs, BenchCommand, BenchExtractionVariant, Cli, ColorChoice, ExportArgs, IndexArgs,
    QueryArgs, QueryCommand, QueryFormat,
};
pub use bench::run_bench;
pub use export::run_export;
pub use index::run_index;
pub(crate) use index::{normalize_changed_paths, reindex_changed, run_index_command};
pub use query::run_query;

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            args::Command::Index(args) => index::run_index(args),
            args::Command::Query(args) => query::run_query(args),
            args::Command::Export(args) => export::run_export(args),
            args::Command::Bench(args) => bench::run_bench(args),
            args::Command::Watch(args) => watch::run_watch(args),
        }
    }
}
