use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id<T> {
    value: String,
    #[serde(skip)]
    marker: PhantomData<T>,
}

impl<T> Id<T> {
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

    pub fn from_raw(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            marker: PhantomData,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RepositoryTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FileTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SymbolTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum NodeTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EdgeTag {}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackageTag {}

pub type RepositoryId = Id<RepositoryTag>;
pub type FileId = Id<FileTag>;
pub type SymbolId = Id<SymbolTag>;
pub type NodeId = Id<NodeTag>;
pub type EdgeId = Id<EdgeTag>;
pub type PackageId = Id<PackageTag>;
