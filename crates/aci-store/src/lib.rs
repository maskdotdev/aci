use aci_core::{
    Confidence, EdgeKind, FactProvenance, FileId, GraphEdge, GraphPartition, GraphSnapshot, NodeId,
    NodeKind, Result, SymbolKind, prefer_fact,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub partitions: BTreeMap<String, PartitionEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            partitions: BTreeMap::new(),
        }
    }
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
    root: PathBuf,
}

pub struct PartitionWriter<'a> {
    store: &'a GraphStore,
    manifest_jsonl: Option<ManifestJsonlWriter>,
    delta: Option<fs::File>,
    pack: Option<PartitionPack>,
    symbols: Option<SymbolIndexWriter>,
    replace_all: bool,
    written: usize,
}

struct PartitionPack {
    writer: BufWriter<fs::File>,
    tmp_path: PathBuf,
    final_path: PathBuf,
    manifest_path: PathBuf,
    next_index: usize,
}

struct SymbolIndexWriter {
    writer: BufWriter<fs::File>,
    tmp_path: PathBuf,
    final_path: PathBuf,
}

#[derive(Serialize)]
struct BorrowedSymbolIndexEntry<'a> {
    pub file_id: Option<&'a FileId>,
    pub name: Option<&'a str>,
    pub qualified_name: Option<&'a str>,
    pub symbol_kind: Option<SymbolKind>,
    pub provenance: FactProvenance,
    pub confidence: Confidence,
}

struct ManifestJsonlWriter {
    writer: BufWriter<fs::File>,
    tmp_path: PathBuf,
    final_path: PathBuf,
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

    pub fn write_partition(&self, partition: &GraphPartition) -> Result<()> {
        let mut writer = self.replace_partitions_writer()?;
        writer.write(partition)?;
        writer.finish().map(|_| ())
    }

    pub fn replace_all_writer(&self) -> Result<PartitionWriter<'_>> {
        let final_path = self.root.join("partitions").join("pack-00000.jsonl");
        let tmp_path = final_path.with_extension("jsonl.tmp");
        let symbols_final_path = self.root.join("symbols.jsonl");
        let symbols_tmp_path = symbols_final_path.with_extension("jsonl.tmp");
        let manifest_final_path = self.root.join("manifest.jsonl");
        let manifest_tmp_path = manifest_final_path.with_extension("jsonl.tmp");
        Ok(PartitionWriter {
            store: self,
            manifest_jsonl: Some(ManifestJsonlWriter {
                writer: BufWriter::new(fs::File::create(&manifest_tmp_path)?),
                tmp_path: manifest_tmp_path,
                final_path: manifest_final_path,
            }),
            delta: None,
            pack: Some(PartitionPack {
                writer: BufWriter::new(fs::File::create(&tmp_path)?),
                tmp_path,
                final_path,
                manifest_path: PathBuf::from("partitions").join("pack-00000.jsonl"),
                next_index: 0,
            }),
            symbols: Some(SymbolIndexWriter {
                writer: BufWriter::new(fs::File::create(&symbols_tmp_path)?),
                tmp_path: symbols_tmp_path,
                final_path: symbols_final_path,
            }),
            replace_all: true,
            written: 0,
        })
    }

    pub fn replace_partitions_writer(&self) -> Result<PartitionWriter<'_>> {
        Ok(PartitionWriter {
            store: self,
            manifest_jsonl: Some(self.manifest_jsonl_writer()?),
            delta: Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.root.join("delta.jsonl"))?,
            ),
            pack: None,
            symbols: None,
            replace_all: false,
            written: 0,
        })
    }

    pub fn replace_partitions(&self, partitions: &[GraphPartition]) -> Result<()> {
        let mut writer = self.replace_partitions_writer()?;
        for partition in partitions {
            writer.write(partition)?;
        }
        writer.finish().map(|_| ())
    }

    pub fn compact(&self) -> Result<GraphSnapshot> {
        let snapshot = self.load_latest()?;
        write_json_atomic(&self.root.join("snapshot.json"), &snapshot)?;
        fs::File::create(self.root.join("delta.jsonl"))?;
        Ok(snapshot)
    }

    pub fn load_latest(&self) -> Result<GraphSnapshot> {
        let snapshot = if self.root.join("snapshot.json").exists() {
            read_json(&self.root.join("snapshot.json"))?
        } else {
            GraphSnapshot::default()
        };
        let mut partitions = snapshot
            .partitions
            .into_iter()
            .map(|partition| (partition.file_id.to_string(), partition))
            .collect::<BTreeMap<_, _>>();
        for record in self.read_delta_log()? {
            match record {
                DeltaRecord::ReplacePartition { partition } => {
                    partitions.insert(partition.file_id.to_string(), partition);
                }
            }
        }
        if partitions.is_empty() {
            return self.load_partitions_from_manifest();
        }
        Ok(GraphSnapshot {
            partitions: partitions.into_values().collect(),
        })
    }

    pub fn read_manifest(&self) -> Result<Manifest> {
        let mut manifest = Manifest::default();
        for entry in self.read_manifest_jsonl()? {
            manifest.partitions.insert(entry.file_id.clone(), entry);
        }
        Ok(manifest)
    }

    pub fn integrity_check(&self) -> Result<Vec<String>> {
        let snapshot = self.load_latest()?;
        Ok(check_snapshot_integrity(&snapshot))
    }

    pub fn partition_file_check(&self) -> Result<Vec<String>> {
        let manifest = self.read_manifest()?;
        let mut problems = Vec::new();
        let mut packed_entries = BTreeMap::<PathBuf, BTreeMap<usize, PartitionEntry>>::new();
        for entry in manifest.partitions.values() {
            if let Some(record_index) = entry.record_index {
                packed_entries
                    .entry(entry.partition_file.clone())
                    .or_default()
                    .insert(record_index, entry.clone());
                continue;
            }
            let path = self.root.join(&entry.partition_file);
            if !path.exists() {
                problems.push(format!("partition file {} is missing", path.display()));
                continue;
            }
            let partition: GraphPartition = read_json(&path)?;
            check_manifest_partition(entry, &partition, &mut problems);
        }
        for (partition_file, mut entries) in packed_entries {
            let path = self.root.join(&partition_file);
            if !path.exists() {
                problems.push(format!("partition file {} is missing", path.display()));
                continue;
            }
            let reader = BufReader::new(fs::File::open(path)?);
            for (record_index, line) in reader.lines().enumerate() {
                let Some(entry) = entries.remove(&record_index) else {
                    continue;
                };
                let partition: GraphPartition = serde_json::from_str(&line?)?;
                check_manifest_partition(&entry, &partition, &mut problems);
            }
            for (record_index, entry) in entries {
                problems.push(format!(
                    "partition {} is missing packed record {}",
                    entry.file_id, record_index
                ));
            }
        }
        Ok(problems)
    }

    pub fn lookup_symbol_index(&self, name: Option<&str>) -> Result<Option<Vec<SymbolIndexEntry>>> {
        let path = self.root.join("symbols.jsonl");
        if !path.exists() {
            return Ok(None);
        }
        let reader = BufReader::new(fs::File::open(path)?);
        let mut selected = BTreeMap::<SymbolIndexKey, SymbolIndexEntry>::new();
        for line in reader.lines() {
            let entry: SymbolIndexEntry = serde_json::from_str(&line?)?;
            if name.is_some_and(|name| entry.name.as_deref() != Some(name)) {
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
        Ok(Some(selected.into_values().collect()))
    }

    fn read_manifest_jsonl(&self) -> Result<Vec<PartitionEntry>> {
        let path = self.root.join("manifest.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let reader = BufReader::new(fs::File::open(path)?);
        reader
            .lines()
            .filter(|line| {
                line.as_ref()
                    .map(|line| !line.trim().is_empty())
                    .unwrap_or(true)
            })
            .map(|line| {
                let line = line?;
                Ok(serde_json::from_str(&line)?)
            })
            .collect()
    }

    fn manifest_jsonl_writer(&self) -> Result<ManifestJsonlWriter> {
        let final_path = self.root.join("manifest.jsonl");
        let tmp_path = final_path.with_extension("jsonl.tmp");
        let mut writer = BufWriter::new(fs::File::create(&tmp_path)?);
        for entry in self.read_manifest()?.partitions.into_values() {
            serde_json::to_writer(&mut writer, &entry)?;
            writeln!(writer)?;
        }
        Ok(ManifestJsonlWriter {
            writer,
            tmp_path,
            final_path,
        })
    }

    fn write_partition_file(&self, partition: &GraphPartition) -> Result<PathBuf> {
        let relative = partition_filename(partition.file_id.as_str());
        let path = self.root.join("partitions").join(&relative);
        write_json_atomic_unsynced(&path, partition)?;
        Ok(relative)
    }

    fn read_delta_log(&self) -> Result<Vec<DeltaRecord>> {
        let path = self.root.join("delta.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let reader = BufReader::new(fs::File::open(path)?);
        reader
            .lines()
            .filter(|line| {
                line.as_ref()
                    .map(|line| !line.trim().is_empty())
                    .unwrap_or(true)
            })
            .map(|line| {
                let line = line?;
                Ok(serde_json::from_str(&line)?)
            })
            .collect()
    }

    fn load_partitions_from_manifest(&self) -> Result<GraphSnapshot> {
        let manifest = self.read_manifest()?;
        let mut snapshot = GraphSnapshot::default();
        let mut packed_entries = BTreeMap::<PathBuf, BTreeSet<usize>>::new();
        for entry in manifest.partitions.values() {
            if let Some(record_index) = entry.record_index {
                packed_entries
                    .entry(entry.partition_file.clone())
                    .or_default()
                    .insert(record_index);
            } else {
                let partition: GraphPartition = read_json(&self.root.join(&entry.partition_file))?;
                snapshot.replace_partition(partition);
            }
        }
        for (partition_file, entries) in packed_entries {
            let reader = BufReader::new(fs::File::open(self.root.join(partition_file))?);
            for (record_index, line) in reader.lines().enumerate() {
                if entries.contains(&record_index) {
                    let partition: GraphPartition = serde_json::from_str(&line?)?;
                    snapshot.replace_partition(partition);
                }
            }
        }
        Ok(snapshot)
    }
}

impl PartitionWriter<'_> {
    pub fn write(&mut self, partition: &GraphPartition) -> Result<()> {
        let (partition_file, record_index) = if let Some(pack) = &mut self.pack {
            let record_index = pack.next_index;
            serde_json::to_writer(&mut pack.writer, partition)?;
            writeln!(pack.writer)?;
            pack.next_index += 1;
            (pack.manifest_path.clone(), Some(record_index))
        } else {
            let relative = self.store.write_partition_file(partition)?;
            (PathBuf::from("partitions").join(relative), None)
        };
        if let Some(delta) = &mut self.delta {
            serde_json::to_writer(
                &mut *delta,
                &DeltaRecord::ReplacePartition {
                    partition: partition.clone(),
                },
            )?;
            writeln!(delta)?;
        }
        let entry = PartitionEntry {
            file_id: partition.file_id.to_string(),
            path: partition.path.clone(),
            fingerprint: partition.fingerprint.clone(),
            partition_file,
            record_index,
        };
        if let Some(manifest_jsonl) = &mut self.manifest_jsonl {
            serde_json::to_writer(&mut manifest_jsonl.writer, &entry)?;
            writeln!(manifest_jsonl.writer)?;
        }
        if let Some(symbols) = &mut self.symbols {
            for node in partition
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Symbol)
            {
                serde_json::to_writer(
                    &mut symbols.writer,
                    &BorrowedSymbolIndexEntry {
                        file_id: node.file_id.as_ref(),
                        name: node.name.as_deref(),
                        qualified_name: node.qualified_name.as_deref(),
                        symbol_kind: node.symbol_kind,
                        provenance: node.provenance,
                        confidence: node.confidence,
                    },
                )?;
                writeln!(symbols.writer)?;
            }
        }
        self.written += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<usize> {
        if let Some(mut pack) = self.pack.take() {
            pack.writer.flush()?;
            drop(pack.writer);
            fs::rename(pack.tmp_path, pack.final_path)?;
        }
        if let Some(mut symbols) = self.symbols.take() {
            symbols.writer.flush()?;
            drop(symbols.writer);
            fs::rename(symbols.tmp_path, symbols.final_path)?;
        }
        if let Some(mut manifest_jsonl) = self.manifest_jsonl.take() {
            manifest_jsonl.writer.flush()?;
            drop(manifest_jsonl.writer);
            fs::rename(manifest_jsonl.tmp_path, manifest_jsonl.final_path)?;
        }
        if self.replace_all {
            let snapshot = self.store.root.join("snapshot.json");
            if snapshot.exists() {
                fs::remove_file(snapshot)?;
            }
            fs::File::create(self.store.root.join("delta.jsonl"))?;
        }
        Ok(self.written)
    }
}

pub fn build_adjacency(snapshot: &GraphSnapshot) -> AdjacencyIndex {
    let mut index = AdjacencyIndex::default();
    for edge in snapshot
        .partitions
        .iter()
        .flat_map(|partition| &partition.edges)
    {
        index
            .outgoing
            .entry(edge.from.clone())
            .or_default()
            .push(edge.clone());
        index
            .incoming
            .entry(edge.to.clone())
            .or_default()
            .push(edge.clone());
    }
    index
}

pub fn check_snapshot_integrity(snapshot: &GraphSnapshot) -> Vec<String> {
    let nodes: BTreeSet<NodeId> = snapshot
        .partitions
        .iter()
        .flat_map(|partition| partition.nodes.iter().map(|node| node.id.clone()))
        .collect();
    let mut problems = Vec::new();
    for partition in &snapshot.partitions {
        for node in &partition.nodes {
            if node.kind == NodeKind::Symbol && node.file_id.is_none() {
                problems.push(format!("symbol {} has no file", node.id));
            }
            if let Some(file_id) = &node.file_id
                && file_id != &partition.file_id
            {
                problems.push(format!(
                    "node {} belongs to file {} but is stored in partition {}",
                    node.id, file_id, partition.file_id
                ));
            }
            if let Some(span) = &node.span {
                validate_span(&mut problems, &format!("node {}", node.id), span);
            }
        }
        for edge in &partition.edges {
            if !nodes.contains(&edge.from) {
                problems.push(format!("edge {} has missing source {}", edge.id, edge.from));
            }
            if !nodes.contains(&edge.to) && edge.kind != EdgeKind::DependsOn {
                problems.push(format!("edge {} has missing target {}", edge.id, edge.to));
            }
            if let Some(span) = &edge.span {
                validate_span(&mut problems, &format!("edge {}", edge.id), span);
            }
        }
    }
    problems
}

fn check_manifest_partition(
    entry: &PartitionEntry,
    partition: &GraphPartition,
    problems: &mut Vec<String>,
) {
    if partition.file_id.to_string() != entry.file_id {
        problems.push(format!(
            "manifest entry {} points to partition {}",
            entry.file_id, partition.file_id
        ));
    }
    if partition.path != entry.path {
        problems.push(format!(
            "partition {} has stale manifest path",
            entry.file_id
        ));
    }
    if partition.fingerprint != entry.fingerprint {
        problems.push(format!(
            "partition {} has stale manifest fingerprint",
            entry.file_id
        ));
    }
    check_partition_file_integrity(partition, problems);
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

fn check_partition_file_integrity(partition: &GraphPartition, problems: &mut Vec<String>) {
    for node in &partition.nodes {
        if node.kind == NodeKind::Symbol && node.file_id.is_none() {
            problems.push(format!("symbol {} has no file", node.id));
        }
        if let Some(file_id) = &node.file_id
            && file_id != &partition.file_id
        {
            problems.push(format!(
                "node {} belongs to file {} but is stored in partition {}",
                node.id, file_id, partition.file_id
            ));
        }
        if let Some(span) = &node.span {
            validate_span(problems, &format!("node {}", node.id), span);
        }
    }
    for edge in &partition.edges {
        if let Some(span) = &edge.span {
            validate_span(problems, &format!("edge {}", edge.id), span);
        }
    }
}

fn validate_span(problems: &mut Vec<String>, owner: &str, span: &aci_core::SourceSpan) {
    if span.byte_start > span.byte_end {
        problems.push(format!("{owner} has an invalid byte span"));
    }
    if span.start.line > span.end.line
        || (span.start.line == span.end.line && span.start.column > span.end.column)
    {
        problems.push(format!("{owner} has an invalid line/column span"));
    }
}

fn partition_filename(file_id: &str) -> PathBuf {
    let digest = blake3::hash(file_id.as_bytes()).to_hex();
    PathBuf::from(format!("{}.json", &digest[..24]))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_json_atomic_with_sync(path, value, true)
}

fn write_json_atomic_unsynced<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_json_atomic_with_sync(path, value, false)
}

fn write_json_atomic_with_sync<T: Serialize>(path: &Path, value: &T, sync: bool) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer(&mut file, value)?;
        writeln!(file)?;
        if sync {
            file.sync_all()?;
        }
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aci_core::{GraphNode, Language, NodeKind, RepositoryId, SourceFile};
    use std::path::{Path, PathBuf};

    fn partition(text: &str) -> GraphPartition {
        let repo = RepositoryId::new("repo", &["store-test"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/a.py"),
            Language::Python,
            text.to_string(),
        );
        let mut partition = GraphPartition::empty(&file);
        partition.nodes.push(GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::File,
            Language::Python,
            Some("a.py".to_string()),
            Some("a.py".to_string()),
            None,
        ));
        partition.nodes.push(GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::Symbol,
            Language::Python,
            Some("a".to_string()),
            Some("a".to_string()),
            None,
        ));
        partition
    }

    #[test]
    fn rebuilds_from_snapshot_plus_delta_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = GraphStore::open(dir.path()).expect("open store");
        store
            .write_partition(&partition("one"))
            .expect("write first");
        store.compact().expect("compact");
        let replacement = partition("two");
        store
            .write_partition(&replacement)
            .expect("write replacement");

        let latest = store.load_latest().expect("load latest");
        assert_eq!(latest.partitions.len(), 1);
        assert_eq!(latest.partitions[0].fingerprint, replacement.fingerprint);
    }

    #[test]
    fn replace_all_writer_loads_from_manifest_without_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = GraphStore::open(dir.path()).expect("open store");
        store
            .write_partition(&partition("stale"))
            .expect("write stale");
        store.compact().expect("compact stale snapshot");

        let replacement = partition("fresh");
        let mut writer = store.replace_all_writer().expect("open writer");
        writer.write(&replacement).expect("write replacement");
        assert_eq!(writer.finish().expect("finish writer"), 1);

        assert!(!store.root().join("snapshot.json").exists());
        assert!(!store.root().join("manifest.json").exists());
        assert!(store.root().join("manifest.jsonl").exists());
        assert!(store.root().join("partitions/pack-00000.jsonl").exists());
        assert_eq!(
            store
                .read_manifest()
                .expect("manifest")
                .partitions
                .values()
                .next()
                .and_then(|entry| entry.record_index),
            Some(0)
        );
        assert!(store.read_delta_log().expect("read delta").is_empty());
        assert!(
            store
                .partition_file_check()
                .expect("partition file check")
                .is_empty()
        );
        let symbols = store
            .lookup_symbol_index(Some("a"))
            .expect("symbol index")
            .expect("symbol index exists");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].file_id.as_ref(), Some(&replacement.file_id));
        let latest = store.load_latest().expect("load latest");
        assert_eq!(latest.partitions.len(), 1);
        assert_eq!(latest.partitions[0].fingerprint, replacement.fingerprint);
    }

    #[test]
    fn integrity_check_rejects_symbols_without_files() {
        let repo = RepositoryId::new("repo", &["integrity-symbol"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/a.py"),
            Language::Python,
            "def a(): pass\n".to_string(),
        );
        let mut partition = GraphPartition::empty(&file);
        partition.nodes.push(GraphNode::deterministic(
            &repo,
            None,
            NodeKind::Symbol,
            Language::Python,
            Some("a".to_string()),
            Some("a".to_string()),
            None,
        ));

        let problems = check_snapshot_integrity(&GraphSnapshot {
            partitions: vec![partition],
        });
        assert!(problems.iter().any(|problem| problem.contains("no file")));
    }

    #[test]
    fn integrity_check_rejects_missing_edge_targets_and_bad_spans() {
        let repo = RepositoryId::new("repo", &["integrity-edge"]);
        let file = SourceFile::new(
            repo.clone(),
            Path::new("/repo"),
            PathBuf::from("/repo/a.py"),
            Language::Python,
            "def a(): pass\n".to_string(),
        );
        let file_node = GraphNode::deterministic(
            &repo,
            Some(&file.file_id),
            NodeKind::File,
            Language::Python,
            Some("a.py".to_string()),
            Some("a.py".to_string()),
            Some(aci_core::SourceSpan::new(
                10,
                1,
                aci_core::LineColumn::new(2, 1),
                aci_core::LineColumn::new(1, 1),
            )),
        );
        let missing = NodeId::from_raw("node:missing");
        let edge = GraphEdge::deterministic(EdgeKind::References, &file_node.id, &missing, None);
        let mut partition = GraphPartition::empty(&file);
        partition.nodes.push(file_node);
        partition.edges.push(edge);

        let problems = check_snapshot_integrity(&GraphSnapshot {
            partitions: vec![partition],
        });
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("missing target"))
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("invalid byte span"))
        );
    }
}
