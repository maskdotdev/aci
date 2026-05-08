use aci_core::{GraphPartition, NodeKind, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) struct DependencyIndexWriter {
    writer: BufWriter<fs::File>,
    tmp_path: PathBuf,
    final_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreIncrementalPlan {
    pub changed_files: Vec<PathBuf>,
    pub reverse_dependencies: Vec<PathBuf>,
    pub files_to_reindex: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct DependencyIndexEntry {
    file_id: String,
    path: PathBuf,
    import_stems: Vec<String>,
}

impl DependencyIndexWriter {
    pub(crate) fn create(root: &Path) -> Result<Self> {
        let final_path = root.join("deps.jsonl");
        let tmp_path = final_path.with_extension("jsonl.tmp");
        Ok(Self {
            writer: BufWriter::new(fs::File::create(&tmp_path)?),
            tmp_path,
            final_path,
        })
    }

    pub(crate) fn copy_existing(root: &Path) -> Result<Self> {
        let mut writer = Self::create(root)?;
        for entry in read(root)? {
            writer.write_entry(&entry)?;
        }
        Ok(writer)
    }

    pub(crate) fn write_partition(&mut self, partition: &GraphPartition) -> Result<()> {
        self.write_entry(&DependencyIndexEntry::from_partition(partition))
    }

    pub(crate) fn finish(mut self) -> Result<()> {
        self.writer.flush()?;
        drop(self.writer);
        fs::rename(self.tmp_path, self.final_path)?;
        Ok(())
    }

    fn write_entry(&mut self, entry: &DependencyIndexEntry) -> Result<()> {
        serde_json::to_writer(&mut self.writer, entry)?;
        writeln!(self.writer)?;
        Ok(())
    }
}

pub(crate) fn plan(root: &Path, changed_paths: &[PathBuf]) -> Result<Option<StoreIncrementalPlan>> {
    let entries = read(root)?;
    if entries.is_empty() {
        return Ok(None);
    }
    let changed = changed_paths.iter().cloned().collect::<BTreeSet<_>>();
    let changed_stems = changed_paths
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .collect::<BTreeSet<_>>();
    let mut reverse_dependencies = BTreeSet::new();
    for entry in entries {
        if changed.contains(&entry.path) {
            continue;
        }
        if entry
            .import_stems
            .iter()
            .any(|stem| changed_stems.contains(stem))
        {
            reverse_dependencies.insert(entry.path);
        }
    }
    let mut files_to_reindex = changed.clone();
    files_to_reindex.extend(reverse_dependencies.iter().cloned());
    Ok(Some(StoreIncrementalPlan {
        changed_files: changed.into_iter().collect(),
        reverse_dependencies: reverse_dependencies.into_iter().collect(),
        files_to_reindex: files_to_reindex.into_iter().collect(),
    }))
}

fn read(root: &Path) -> Result<Vec<DependencyIndexEntry>> {
    let path = root.join("deps.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let reader = BufReader::new(fs::File::open(path)?);
    let mut entries = BTreeMap::<String, DependencyIndexEntry>::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: DependencyIndexEntry = serde_json::from_str(&line)?;
        entries.insert(entry.file_id.clone(), entry);
    }
    Ok(entries.into_values().collect())
}

impl DependencyIndexEntry {
    fn from_partition(partition: &GraphPartition) -> Self {
        let import_stems = partition
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Import)
            .filter_map(|node| node.qualified_name.as_deref().or(node.name.as_deref()))
            .filter_map(module_stem)
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            file_id: partition.file_id.to_string(),
            path: partition.path.clone(),
            import_stems,
        }
    }
}

fn module_stem(module: &str) -> Option<&str> {
    module
        .rsplit('/')
        .next()
        .and_then(|value| value.strip_suffix(".js").or(Some(value)))
        .and_then(|value| value.strip_suffix(".ts").or(Some(value)))
        .and_then(|value| value.strip_suffix(".py").or(Some(value)))
        .filter(|value| !value.is_empty())
}
