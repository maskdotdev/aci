use aci_core::{EdgeKind, GraphEdge, GraphPartition, GraphSnapshot, NodeId, NodeKind, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub partitions: BTreeMap<String, PartitionEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: VERSION,
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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum DeltaRecord {
    ReplacePartition { partition: GraphPartition },
}

#[derive(Clone, Debug, Default)]
pub struct AdjacencyIndex {
    pub outgoing: HashMap<NodeId, Vec<GraphEdge>>,
    pub incoming: HashMap<NodeId, Vec<GraphEdge>>,
}

pub struct GraphStore {
    root: PathBuf,
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
        let relative = self.write_partition_file(partition)?;
        self.append_delta(&DeltaRecord::ReplacePartition {
            partition: partition.clone(),
        })?;

        let mut manifest = self.read_manifest().unwrap_or_default();
        manifest.partitions.insert(
            partition.file_id.to_string(),
            PartitionEntry {
                file_id: partition.file_id.to_string(),
                path: partition.path.clone(),
                fingerprint: partition.fingerprint.clone(),
                partition_file: PathBuf::from("partitions").join(relative),
            },
        );
        self.write_manifest(&manifest)
    }

    pub fn replace_partitions(&self, partitions: &[GraphPartition]) -> Result<()> {
        let mut manifest = self.read_manifest().unwrap_or_default();
        let mut delta = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("delta.jsonl"))?;
        for partition in partitions {
            let relative = self.write_partition_file(partition)?;
            serde_json::to_writer(
                &mut delta,
                &DeltaRecord::ReplacePartition {
                    partition: partition.clone(),
                },
            )?;
            writeln!(delta)?;
            manifest.partitions.insert(
                partition.file_id.to_string(),
                PartitionEntry {
                    file_id: partition.file_id.to_string(),
                    path: partition.path.clone(),
                    fingerprint: partition.fingerprint.clone(),
                    partition_file: PathBuf::from("partitions").join(relative),
                },
            );
        }
        self.write_manifest(&manifest)
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
        let path = self.root.join("manifest.json");
        if path.exists() {
            read_json(&path)
        } else {
            Ok(Manifest::default())
        }
    }

    pub fn integrity_check(&self) -> Result<Vec<String>> {
        let snapshot = self.load_latest()?;
        Ok(check_snapshot_integrity(&snapshot))
    }

    fn write_manifest(&self, manifest: &Manifest) -> Result<()> {
        write_json_atomic(&self.root.join("manifest.json"), manifest)
    }

    fn write_partition_file(&self, partition: &GraphPartition) -> Result<PathBuf> {
        let relative = partition_filename(partition.file_id.as_str());
        let path = self.root.join("partitions").join(&relative);
        write_json_atomic(&path, partition)?;
        Ok(relative)
    }

    fn append_delta(&self, record: &DeltaRecord) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("delta.jsonl"))?;
        serde_json::to_writer(&mut file, record)?;
        writeln!(file)?;
        Ok(())
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
        for entry in manifest.partitions.values() {
            let partition: GraphPartition = read_json(&self.root.join(&entry.partition_file))?;
            snapshot.replace_partition(partition);
        }
        Ok(snapshot)
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
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer(&mut file, value)?;
        writeln!(file)?;
        file.sync_all()?;
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
