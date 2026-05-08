use crate::io::{read_json, write_json_atomic};
use crate::{DeltaRecord, GraphStore, compact, manifest};
use aci_core::{GraphPartition, GraphSnapshot, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

impl GraphStore {
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

    pub(crate) fn read_delta_log(&self) -> Result<Vec<DeltaRecord>> {
        let path = self.root.join("delta.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        manifest::read_jsonl_lines(&path)
    }

    pub(crate) fn load_partitions_from_manifest(&self) -> Result<GraphSnapshot> {
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
            read_packed_manifest_entries(&self.root, &partition_file, &entries, &mut snapshot)?;
        }
        Ok(snapshot)
    }
}

fn read_packed_manifest_entries(
    root: &Path,
    partition_file: &Path,
    entries: &BTreeSet<usize>,
    snapshot: &mut GraphSnapshot,
) -> Result<()> {
    let mut reader = BufReader::new(fs::File::open(root.join(partition_file))?);
    compact::read_pack_header(&mut reader)?;
    let max_record_index = entries.iter().next_back().copied();
    let mut record_index = 0;
    while max_record_index.is_some_and(|max| record_index <= max) {
        let Some(partition) = compact::read_partition_binary(&mut reader)? else {
            break;
        };
        if entries.contains(&record_index) {
            snapshot.replace_partition(partition);
        }
        record_index += 1;
    }
    Ok(())
}
