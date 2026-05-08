use aci_core::{GraphNode, GraphPartition, NodeKind, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) struct DependencyIndexWriter {
    tmp_path: PathBuf,
    final_path: PathBuf,
    reverse: BTreeMap<String, BTreeSet<PathBuf>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreIncrementalPlan {
    pub changed_files: Vec<PathBuf>,
    pub reverse_dependencies: Vec<PathBuf>,
    pub files_to_reindex: Vec<PathBuf>,
}

impl DependencyIndexWriter {
    pub(crate) fn create(root: &Path) -> Result<Self> {
        let final_path = root.join("deps.tsv");
        let tmp_path = final_path.with_extension("tsv.tmp");
        Ok(Self {
            tmp_path,
            final_path,
            reverse: BTreeMap::new(),
        })
    }

    pub(crate) fn copy_existing(root: &Path) -> Result<Self> {
        let mut writer = Self::create(root)?;
        for (stem, paths) in read_reverse(root)? {
            writer.reverse.entry(stem).or_default().extend(paths);
        }
        Ok(writer)
    }

    pub(crate) fn write_partition_imports(
        &mut self,
        partition: &GraphPartition,
        mut import_stems: Vec<String>,
    ) -> Result<()> {
        import_stems.sort();
        import_stems.dedup();
        for stem in import_stems {
            self.reverse
                .entry(stem)
                .or_default()
                .insert(partition.path.clone());
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<()> {
        let mut writer = BufWriter::new(fs::File::create(&self.tmp_path)?);
        for (stem, paths) in self.reverse {
            writer.write_all(stem.as_bytes())?;
            writer.write_all(b"\t")?;
            for (index, path) in paths.iter().enumerate() {
                if index > 0 {
                    writer.write_all(b"\x1f")?;
                }
                write!(writer, "{}", path.display())?;
            }
            writeln!(writer)?;
        }
        writer.flush()?;
        drop(writer);
        let legacy = self.final_path.with_file_name("deps.jsonl");
        if legacy.exists() {
            fs::remove_file(legacy)?;
        }
        fs::rename(self.tmp_path, self.final_path)?;
        Ok(())
    }
}

pub(crate) fn import_stem_for_node(node: &GraphNode) -> Option<String> {
    if node.kind != NodeKind::Import {
        return None;
    }
    node.qualified_name
        .as_deref()
        .or(node.name.as_deref())
        .and_then(module_stem)
        .map(str::to_string)
}

pub(crate) fn plan(root: &Path, changed_paths: &[PathBuf]) -> Result<Option<StoreIncrementalPlan>> {
    if changed_paths.is_empty() {
        return Ok(Some(StoreIncrementalPlan {
            changed_files: Vec::new(),
            reverse_dependencies: Vec::new(),
            files_to_reindex: Vec::new(),
        }));
    }
    let index_path = root.join("deps.tsv");
    if !index_path.exists() {
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
    for (stem, paths) in read_reverse(root)? {
        if !changed_stems.contains(&stem) {
            continue;
        }
        for path in paths {
            if !changed.contains(&path) {
                reverse_dependencies.insert(path);
            }
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

fn read_reverse(root: &Path) -> Result<BTreeMap<String, BTreeSet<PathBuf>>> {
    let path = root.join("deps.tsv");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let reader = BufReader::new(fs::File::open(path)?);
    let mut reverse = BTreeMap::<String, BTreeSet<PathBuf>>::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Some((stem, paths)) = parse_reverse_entry(&line) else {
            continue;
        };
        reverse.entry(stem).or_default().extend(paths);
    }
    Ok(reverse)
}

fn parse_reverse_entry(line: &str) -> Option<(String, BTreeSet<PathBuf>)> {
    let mut parts = line.splitn(2, '\t');
    let stem = parts.next()?.to_string();
    let paths = parts
        .next()
        .unwrap_or_default()
        .split('\x1f')
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect();
    Some((stem, paths))
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
