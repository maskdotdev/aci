use crate::{GraphStore, Manifest, PartitionEntry};
use aci_core::Result;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

pub(crate) struct ManifestJsonlWriter {
    pub(crate) writer: BufWriter<fs::File>,
    pub(crate) tmp_path: PathBuf,
    pub(crate) final_path: PathBuf,
}

impl GraphStore {
    pub fn read_manifest(&self) -> Result<Manifest> {
        let mut manifest = Manifest::default();
        for entry in self.read_manifest_jsonl()? {
            manifest.partitions.insert(entry.file_id.clone(), entry);
        }
        Ok(manifest)
    }

    pub(crate) fn read_manifest_jsonl(&self) -> Result<Vec<PartitionEntry>> {
        let path = self.root.join("manifest.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_jsonl_lines(&path)
    }

    pub(crate) fn manifest_jsonl_writer(&self) -> Result<ManifestJsonlWriter> {
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
}

pub(crate) fn read_jsonl_lines<T: for<'de> serde::Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<Vec<T>> {
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
