use aci_export::export_snapshot;
use aci_store::GraphStore;
use anyhow::Result;

use crate::args::ExportArgs;

pub fn run_export(args: ExportArgs) -> Result<()> {
    let store = GraphStore::open(args.store)?;
    let snapshot = store.load_latest()?;
    if let Some(output) = args.output {
        let file = std::fs::File::create(output)?;
        export_snapshot(&snapshot, args.format, file)?;
    } else {
        let stdout = std::io::stdout();
        let handle = stdout.lock();
        export_snapshot(&snapshot, args.format, handle)?;
    }
    Ok(())
}
