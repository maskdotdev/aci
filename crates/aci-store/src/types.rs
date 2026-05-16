use aci_core::{
    Confidence, FactProvenance, FileId, GraphEdge, GraphPartition, NodeId, Result, SourceSpan,
    SymbolKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Store manifest mapping file ids to packed partition records.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub partitions: BTreeMap<String, PartitionEntry>,
}

/// Location and fingerprint metadata for one stored partition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PartitionEntry {
    pub file_id: String,
    pub path: PathBuf,
    pub fingerprint: String,
    pub partition_file: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_index: Option<usize>,
}

/// Append-only mutation record used for incremental updates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum DeltaRecord {
    ReplacePartition { partition: GraphPartition },
}

/// Compact symbol lookup entry persisted beside partition data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolIndexEntry {
    pub file_id: Option<FileId>,
    pub path: Option<PathBuf>,
    pub name: Option<String>,
    pub qualified_name: Option<String>,
    pub symbol_kind: Option<SymbolKind>,
    pub span: Option<SourceSpan>,
    pub provenance: FactProvenance,
    pub confidence: Confidence,
}

/// Incoming and outgoing adjacency lists built from graph edges.
#[derive(Clone, Debug, Default)]
pub struct AdjacencyIndex {
    pub outgoing: HashMap<NodeId, Vec<GraphEdge>>,
    pub incoming: HashMap<NodeId, Vec<GraphEdge>>,
}

/// Filesystem-backed graph store rooted at a `.aci` directory.
pub struct GraphStore {
    pub(crate) root: PathBuf,
}

impl GraphStore {
    /// Opens or creates a graph store at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("partitions"))?;
        Ok(Self { root })
    }

    /// Returns the store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
