use crate::io::read_json;
use crate::{GraphStore, Manifest, PartitionEntry, compact};
use aci_core::{EdgeKind, GraphPartition, GraphSnapshot, NodeId, NodeKind, Result, SourceSpan};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufReader;
use std::path::PathBuf;

impl GraphStore {
    pub fn integrity_check(&self) -> Result<Vec<String>> {
        let snapshot = self.load_latest()?;
        Ok(check_snapshot_integrity(&snapshot))
    }

    pub fn partition_file_check(&self) -> Result<Vec<String>> {
        check_manifest_partition_files(&self.root, &self.read_manifest()?)
    }
}

pub fn check_manifest_partition_files(
    root: &std::path::Path,
    manifest: &Manifest,
) -> Result<Vec<String>> {
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
        let path = root.join(&entry.partition_file);
        if !path.exists() {
            problems.push(format!("partition file {} is missing", path.display()));
            continue;
        }
        let partition: GraphPartition = read_json(&path)?;
        check_manifest_partition(entry, &partition, &mut problems);
    }
    for (partition_file, entries) in packed_entries {
        check_packed_entries(root, &partition_file, entries, &mut problems)?;
    }
    Ok(problems)
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
            check_node_integrity(partition, node, &mut problems);
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

pub fn check_partition_integrity(partition: &GraphPartition) -> Vec<String> {
    let mut problems = Vec::new();
    check_partition_file_integrity(partition, &mut problems);
    problems
}

fn check_packed_entries(
    root: &std::path::Path,
    partition_file: &std::path::Path,
    mut entries: BTreeMap<usize, PartitionEntry>,
    problems: &mut Vec<String>,
) -> Result<()> {
    let path = root.join(partition_file);
    if !path.exists() {
        problems.push(format!("partition file {} is missing", path.display()));
        return Ok(());
    }
    let mut reader = BufReader::new(fs::File::open(path)?);
    compact::read_pack_header(&mut reader)?;
    let mut record_index = 0;
    while !entries.is_empty() {
        let Some(partition) = compact::read_partition_binary(&mut reader)? else {
            break;
        };
        let Some(entry) = entries.remove(&record_index) else {
            record_index += 1;
            continue;
        };
        check_manifest_partition(&entry, &partition, problems);
        record_index += 1;
    }
    for (record_index, entry) in entries {
        problems.push(format!(
            "partition {} is missing packed record {}",
            entry.file_id, record_index
        ));
    }
    Ok(())
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

fn check_partition_file_integrity(partition: &GraphPartition, problems: &mut Vec<String>) {
    for node in &partition.nodes {
        check_node_integrity(partition, node, problems);
    }
    for edge in &partition.edges {
        if let Some(span) = &edge.span {
            validate_span(problems, &format!("edge {}", edge.id), span);
        }
    }
}

fn check_node_integrity(
    partition: &GraphPartition,
    node: &aci_core::GraphNode,
    problems: &mut Vec<String>,
) {
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

fn validate_span(problems: &mut Vec<String>, owner: &str, span: &SourceSpan) {
    if span.byte_start > span.byte_end {
        problems.push(format!("{owner} has an invalid byte span"));
    }
    if span.start.line > span.end.line
        || (span.start.line == span.end.line && span.start.column > span.end.column)
    {
        problems.push(format!("{owner} has an invalid line/column span"));
    }
}
