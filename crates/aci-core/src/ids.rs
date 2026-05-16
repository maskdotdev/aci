use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

/// Stable typed identifier stored as a prefixed hash string.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id<T> {
    value: String,
    #[serde(skip)]
    marker: PhantomData<T>,
}

impl<T> Id<T> {
    /// Builds a deterministic id from ordered string parts.
    pub fn new(prefix: &str, parts: &[impl AsRef<str>]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(part.as_ref().as_bytes());
            hasher.update(b"\0");
        }
        let digest = hasher.finalize().to_hex();
        Self {
            value: format!("{prefix}:{}", &digest[..24]),
            marker: PhantomData,
        }
    }

    /// Wraps a serialized id value without rehashing it.
    pub fn from_raw(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            marker: PhantomData,
        }
    }

    /// Returns the serialized id value.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

/// Marker type for repository ids.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RepositoryTag {}
/// Marker type for file ids.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FileTag {}
/// Marker type for symbol ids.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SymbolTag {}
/// Marker type for graph node ids.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum NodeTag {}
/// Marker type for graph edge ids.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EdgeTag {}
/// Marker type for package ids.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackageTag {}

/// Repository identifier.
pub type RepositoryId = Id<RepositoryTag>;
/// Source file identifier.
pub type FileId = Id<FileTag>;
/// Symbol identifier.
pub type SymbolId = Id<SymbolTag>;
/// Graph node identifier.
pub type NodeId = Id<NodeTag>;
/// Graph edge identifier.
pub type EdgeId = Id<EdgeTag>;
/// Package identifier.
pub type PackageId = Id<PackageTag>;
