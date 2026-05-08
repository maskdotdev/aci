use crate::io::write_json_atomic_unsynced;
use crate::manifest::ManifestJsonlWriter;
use crate::{DeltaRecord, GraphStore, PartitionEntry, compact, dependencies, symbols};
use aci_core::{GraphPartition, Result};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

pub struct PartitionWriter<'a> {
    store: &'a GraphStore,
    manifest_jsonl: Option<ManifestJsonlWriter>,
    delta: Option<fs::File>,
    pack: Option<PartitionPack>,
    symbols: Option<symbols::SymbolIndexWriter>,
    dependencies: Option<dependencies::DependencyIndexWriter>,
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

impl GraphStore {
    pub fn write_partition(&self, partition: &GraphPartition) -> Result<()> {
        let mut writer = self.replace_partitions_writer()?;
        writer.write(partition)?;
        writer.finish().map(|_| ())
    }

    pub fn replace_all_writer(&self) -> Result<PartitionWriter<'_>> {
        let final_path = self.root.join("partitions").join("pack-00000.bin");
        let tmp_path = final_path.with_extension("bin.tmp");
        let manifest_final_path = self.root.join("manifest.jsonl");
        let manifest_tmp_path = manifest_final_path.with_extension("jsonl.tmp");
        let mut pack_writer = BufWriter::new(fs::File::create(&tmp_path)?);
        compact::write_pack_header(&mut pack_writer)?;
        Ok(PartitionWriter {
            store: self,
            manifest_jsonl: Some(ManifestJsonlWriter {
                writer: BufWriter::new(fs::File::create(&manifest_tmp_path)?),
                tmp_path: manifest_tmp_path,
                final_path: manifest_final_path,
            }),
            delta: None,
            pack: Some(PartitionPack {
                writer: pack_writer,
                tmp_path,
                final_path,
                manifest_path: PathBuf::from("partitions").join("pack-00000.bin"),
                next_index: 0,
            }),
            symbols: Some(symbols::SymbolIndexWriter::new(&self.root)?),
            dependencies: Some(dependencies::DependencyIndexWriter::create(&self.root)?),
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
            dependencies: None,
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

    pub(crate) fn write_partition_file(&self, partition: &GraphPartition) -> Result<PathBuf> {
        let relative = partition_filename(partition.file_id.as_str());
        let path = self.root.join("partitions").join(&relative);
        write_json_atomic_unsynced(&path, partition)?;
        Ok(relative)
    }
}

impl PartitionWriter<'_> {
    pub fn write(&mut self, partition: &GraphPartition) -> Result<()> {
        let (partition_file, record_index) = if let Some(pack) = &mut self.pack {
            let record_index = pack.next_index;
            compact::write_partition_binary(&mut pack.writer, partition)?;
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
        self.write_manifest_entry(partition, partition_file, record_index)?;
        self.write_auxiliary_indexes(partition)?;
        self.written += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<usize> {
        if let Some(mut pack) = self.pack.take() {
            pack.writer.flush()?;
            drop(pack.writer);
            fs::rename(pack.tmp_path, pack.final_path)?;
        }
        if let Some(symbols) = self.symbols.take() {
            symbols.finish()?;
        }
        if let Some(mut manifest_jsonl) = self.manifest_jsonl.take() {
            manifest_jsonl.writer.flush()?;
            drop(manifest_jsonl.writer);
            fs::rename(manifest_jsonl.tmp_path, manifest_jsonl.final_path)?;
        }
        if let Some(dependencies) = self.dependencies.take() {
            dependencies.finish()?;
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

    fn write_manifest_entry(
        &mut self,
        partition: &GraphPartition,
        partition_file: PathBuf,
        record_index: Option<usize>,
    ) -> Result<()> {
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
        Ok(())
    }

    fn write_auxiliary_indexes(&mut self, partition: &GraphPartition) -> Result<()> {
        let mut import_stems = self.dependencies.as_ref().map(|_| Vec::new());
        if self.symbols.is_some() || import_stems.is_some() {
            for node in &partition.nodes {
                if let Some(symbols) = &mut self.symbols {
                    symbols.write_node(node, &partition.path)?;
                }
                if let Some(import_stems) = &mut import_stems
                    && let Some(stem) = dependencies::import_stem_for_node(node)
                {
                    import_stems.push(stem);
                }
            }
        }
        if let Some(dependencies) = &mut self.dependencies {
            dependencies.write_partition_imports(partition, import_stems.unwrap_or_default())?;
        }
        Ok(())
    }
}

fn partition_filename(file_id: &str) -> PathBuf {
    let digest = blake3::hash(file_id.as_bytes()).to_hex();
    PathBuf::from(format!("{}.json", &digest[..24]))
}
