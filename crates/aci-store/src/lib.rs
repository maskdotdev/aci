mod compact;
mod dependencies;
mod graph;
mod integrity;
mod io;
mod load;
mod manifest;
mod pack;
mod shard_cache;
mod symbols;
mod tags;
mod types;
mod write;

pub use dependencies::StoreIncrementalPlan;
pub use graph::build_adjacency;
pub use integrity::{
    check_manifest_partition_files, check_partition_integrity, check_snapshot_integrity,
};
pub use types::{
    AdjacencyIndex, DeltaRecord, GraphStore, Manifest, PartitionEntry, SymbolIndexEntry,
};
pub use write::PartitionWriter;

impl GraphStore {
    pub fn lookup_symbol_index(
        &self,
        name: Option<&str>,
    ) -> aci_core::Result<Option<Vec<SymbolIndexEntry>>> {
        if !self.read_delta_log()?.is_empty() {
            return Ok(None);
        }
        symbols::lookup(&self.root, name)
    }

    pub fn plan_incremental_reindex(
        &self,
        changed_paths: &[std::path::PathBuf],
    ) -> aci_core::Result<Option<StoreIncrementalPlan>> {
        dependencies::plan(&self.root, changed_paths)
    }
}

#[cfg(test)]
mod tests;
