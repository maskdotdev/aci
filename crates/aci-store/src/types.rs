use aci_core::{
    Confidence, FactProvenance, FileId, GraphEdge, GraphPartition, NodeId, Result, SymbolKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub partitions: BTreeMap<String, PartitionEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PartitionEntry {
    pub file_id: String,
    pub path: PathBuf,
    pub fingerprint: String,
    pub partition_file: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_index: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum DeltaRecord {
    ReplacePartition { partition: GraphPartition },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolIndexEntry {
    pub file_id: Option<FileId>,
    pub name: Option<String>,
    pub qualified_name: Option<String>,
    pub symbol_kind: Option<SymbolKind>,
    pub provenance: FactProvenance,
    pub confidence: Confidence,
}

#[derive(Clone, Debug, Default)]
pub struct AdjacencyIndex {
    pub outgoing: HashMap<NodeId, Vec<GraphEdge>>,
    pub incoming: HashMap<NodeId, Vec<GraphEdge>>,
}

pub struct GraphStore {
    pub(crate) root: PathBuf,
}

impl GraphStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("partitions"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}
