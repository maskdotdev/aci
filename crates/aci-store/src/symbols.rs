use crate::{SymbolIndexEntry, tags};
use aci_core::{FileId, GraphNode, NodeKind, Result, SymbolKind, prefer_fact};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) struct SymbolIndexWriter {
    tmp_root: PathBuf,
    final_root: PathBuf,
    shards: HashMap<u8, SymbolShardWriter>,
}

struct SymbolShardWriter {
    writer: BufWriter<fs::File>,
}

#[derive(Serialize)]
struct BorrowedSymbolIndexEntry<'a> {
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    file_id: Option<&'a FileId>,
    #[serde(rename = "n", skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    qualified_name: Option<&'a str>,
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    symbol_kind: Option<u8>,
    #[serde(rename = "p", default, skip_serializing_if = "is_zero")]
    provenance: u8,
    #[serde(rename = "c", default, skip_serializing_if = "is_zero")]
    confidence: u8,
}

#[derive(Deserialize)]
struct CompactSymbolIndexEntry {
    #[serde(rename = "f")]
    file_id: Option<FileId>,
    #[serde(rename = "n")]
    name: Option<String>,
    #[serde(rename = "q")]
    qualified_name: Option<String>,
    #[serde(rename = "t")]
    symbol_kind: Option<u8>,
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
            tmp_root,
            final_root,
            shards: HashMap::new(),
        })
    }

    pub(crate) fn write_node(&mut self, node: &GraphNode) -> Result<()> {
        if node.kind != NodeKind::Symbol {
            return Ok(());
        }
        let shard = shard_for_name(node.name.as_deref());
        let writer = self.open_shard(shard)?;
        serde_json::to_writer(
            &mut writer.writer,
            &BorrowedSymbolIndexEntry {
                file_id: node.file_id.as_ref(),
                name: node.name.as_deref(),
                qualified_name: node.qualified_name.as_deref(),
                symbol_kind: node.symbol_kind.map(tags::encode_symbol_kind),
                provenance: tags::encode_provenance(node.provenance),
                confidence: tags::encode_confidence(node.confidence),
            },
        )?;
        writeln!(writer.writer)?;
        Ok(())
    }

    pub(crate) fn finish(self, store_root: &Path) -> Result<()> {
        for shard in self.shards.into_values() {
            let mut writer = shard.writer;
            writer.flush()?;
        }
        if self.final_root.exists() {
            fs::remove_dir_all(&self.final_root)?;
        }
        let legacy = store_root.join("symbols.jsonl");
        if legacy.exists() {
            fs::remove_file(legacy)?;
        }
        fs::rename(self.tmp_root, self.final_root)?;
        Ok(())
    }

    fn open_shard(&mut self, shard: u8) -> Result<&mut SymbolShardWriter> {
        if !self.shards.contains_key(&shard) {
            let path = self.tmp_root.join(shard_filename(shard));
            let writer = BufWriter::new(fs::File::create(path)?);
            self.shards.insert(shard, SymbolShardWriter { writer });
        }
        Ok(self.shards.get_mut(&shard).expect("shard just inserted"))
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
        name: entry.name,
        qualified_name: entry.qualified_name,
        symbol_kind: entry
            .symbol_kind
            .map(tags::decode_symbol_kind)
            .transpose()?,
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
