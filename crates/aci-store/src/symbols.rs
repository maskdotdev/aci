use crate::{SymbolIndexEntry, shard_cache::ShardWriterCache, tags};
use aci_core::{FileId, GraphNode, NodeKind, Result, SymbolKind, prefer_fact};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub(crate) struct SymbolIndexWriter {
    tmp_root: PathBuf,
    final_root: PathBuf,
    shards: ShardWriterCache,
}

#[derive(Serialize)]
struct BorrowedSymbolIndexEntry<'a> {
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    file_id: Option<&'a FileId>,
    #[serde(rename = "r", skip_serializing_if = "Option::is_none")]
    path: Option<&'a Path>,
    #[serde(rename = "n", skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    qualified_name: Option<&'a str>,
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    symbol_kind: Option<u8>,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    span: Option<&'a aci_core::SourceSpan>,
    #[serde(rename = "p", default, skip_serializing_if = "is_zero")]
    provenance: u8,
    #[serde(rename = "c", default, skip_serializing_if = "is_zero")]
    confidence: u8,
}

#[derive(Deserialize)]
struct CompactSymbolIndexEntry {
    #[serde(rename = "f")]
    file_id: Option<FileId>,
    #[serde(rename = "r", default)]
    path: Option<PathBuf>,
    #[serde(rename = "n")]
    name: Option<String>,
    #[serde(rename = "q")]
    qualified_name: Option<String>,
    #[serde(rename = "t")]
    symbol_kind: Option<u8>,
    #[serde(rename = "s", default)]
    span: Option<aci_core::SourceSpan>,
    #[serde(rename = "p", default)]
    provenance: u8,
    #[serde(rename = "c", default)]
    confidence: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SymbolIndexKey {
    file_id: Option<FileId>,
    name: Option<String>,
    qualified_name: Option<String>,
    kind: Option<SymbolKind>,
}

impl From<&SymbolIndexEntry> for SymbolIndexKey {
    fn from(entry: &SymbolIndexEntry) -> Self {
        Self {
            file_id: entry.file_id.clone(),
            name: entry.name.clone(),
            qualified_name: entry.qualified_name.clone(),
            kind: entry.symbol_kind,
        }
    }
}

impl SymbolIndexWriter {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        let tmp_root = root.join("symbols.tmp");
        let final_root = root.join("symbols");
        if tmp_root.exists() {
            fs::remove_dir_all(&tmp_root)?;
        }
        fs::create_dir_all(&tmp_root)?;
        Ok(Self {
            shards: ShardWriterCache::new(tmp_root.clone(), "jsonl"),
            tmp_root,
            final_root,
        })
    }

    pub(crate) fn write_node(&mut self, node: &GraphNode, path: &Path) -> Result<()> {
        if node.kind != NodeKind::Symbol {
            return Ok(());
        }
        let mut line = Vec::new();
        serde_json::to_writer(
            &mut line,
            &BorrowedSymbolIndexEntry {
                file_id: node.file_id.as_ref(),
                path: Some(path),
                name: node.name.as_deref(),
                qualified_name: node.qualified_name.as_deref(),
                symbol_kind: node.symbol_kind.map(tags::encode_symbol_kind),
                span: node.span.as_ref(),
                provenance: tags::encode_provenance(node.provenance),
                confidence: tags::encode_confidence(node.confidence),
            },
        )?;
        line.push(b'\n');
        self.shards
            .write_all(shard_for_name(node.name.as_deref()), &line)?;
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<()> {
        self.shards.flush()?;
        if self.final_root.exists() {
            fs::remove_dir_all(&self.final_root)?;
        }
        fs::rename(self.tmp_root, self.final_root)?;
        Ok(())
    }
}

pub(crate) fn lookup(root: &Path, name: Option<&str>) -> Result<Option<Vec<SymbolIndexEntry>>> {
    let symbols_root = root.join("symbols");
    if !symbols_root.exists() {
        return Ok(None);
    }
    let mut selected = BTreeMap::<SymbolIndexKey, SymbolIndexEntry>::new();
    if let Some(name) = name {
        read_shard(
            &symbols_root.join(shard_filename(shard_for_name(Some(name)))),
            name,
            &mut selected,
        )?;
    } else {
        for entry in fs::read_dir(symbols_root)? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                read_shard(&path, "", &mut selected)?;
            }
        }
    }
    Ok(Some(selected.into_values().collect()))
}

fn read_shard(
    path: &Path,
    name: &str,
    selected: &mut BTreeMap<SymbolIndexKey, SymbolIndexEntry>,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let reader = BufReader::new(fs::File::open(path)?);
    for line in reader.lines() {
        let entry = decode_entry(&line?)?;
        if !name.is_empty() && entry.name.as_deref() != Some(name) {
            continue;
        }
        let key = SymbolIndexKey::from(&entry);
        match selected.get(&key) {
            Some(existing)
                if prefer_fact(
                    (existing.provenance, existing.confidence),
                    (entry.provenance, entry.confidence),
                ) =>
            {
                selected.insert(key, entry);
            }
            None => {
                selected.insert(key, entry);
            }
            _ => {}
        }
    }
    Ok(())
}

fn decode_entry(line: &str) -> Result<SymbolIndexEntry> {
    let entry: CompactSymbolIndexEntry = serde_json::from_str(line)?;
    Ok(SymbolIndexEntry {
        file_id: entry.file_id,
        path: entry.path,
        name: entry.name,
        qualified_name: entry.qualified_name,
        symbol_kind: entry
            .symbol_kind
            .map(tags::decode_symbol_kind)
            .transpose()?,
        span: entry.span,
        provenance: tags::decode_provenance(entry.provenance)?,
        confidence: tags::decode_confidence(entry.confidence)?,
    })
}

fn shard_for_name(name: Option<&str>) -> u8 {
    blake3::hash(name.unwrap_or_default().as_bytes()).as_bytes()[0]
}

fn shard_filename(shard: u8) -> String {
    format!("{shard:02x}.jsonl")
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}
