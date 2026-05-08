use crate::shard_cache::ShardWriterCache;
use aci_core::{GraphNode, GraphPartition, NodeKind, Result};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) struct DependencyIndexWriter {
    tmp_root: PathBuf,
    final_root: PathBuf,
    path_writer: BufWriter<fs::File>,
    shards: ShardWriterCache,
    paths: HashMap<PathBuf, u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreIncrementalPlan {
    pub changed_files: Vec<PathBuf>,
    pub reverse_dependencies: Vec<PathBuf>,
    pub files_to_reindex: Vec<PathBuf>,
}

impl DependencyIndexWriter {
    pub(crate) fn create(root: &Path) -> Result<Self> {
        let tmp_root = root.join("deps.tmp");
        let final_root = root.join("deps");
        if tmp_root.exists() {
            fs::remove_dir_all(&tmp_root)?;
        }
        fs::create_dir_all(&tmp_root)?;
        let path_writer = BufWriter::new(fs::File::create(tmp_root.join("paths.tsv"))?);
        Ok(Self {
            shards: ShardWriterCache::new(tmp_root.clone(), "tsv"),
            tmp_root,
            final_root,
            path_writer,
            paths: HashMap::new(),
        })
    }

    pub(crate) fn write_partition_imports(
        &mut self,
        partition: &GraphPartition,
        mut import_stems: Vec<String>,
    ) -> Result<()> {
        import_stems.sort();
        import_stems.dedup();
        let path_index = self.intern_path(&partition.path)?;
        for stem in import_stems {
            let mut line = Vec::new();
            line.write_all(stem.as_bytes())?;
            line.write_all(b"\t")?;
            write!(line, "{path_index}")?;
            line.write_all(b"\n")?;
            self.shards.write_all(shard_for_stem(&stem), &line)?;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<()> {
        self.path_writer.flush()?;
        self.shards.flush()?;
        drop(self.path_writer);
        if self.final_root.exists() {
            fs::remove_dir_all(&self.final_root)?;
        }
        fs::rename(self.tmp_root, self.final_root)?;
        Ok(())
    }

    fn intern_path(&mut self, path: &Path) -> Result<u32> {
        if let Some(index) = self.paths.get(path) {
            return Ok(*index);
        }
        let index = u32::try_from(self.paths.len()).map_err(|_| {
            aci_core::AciError::Message("dependency index has too many paths".to_string())
        })?;
        writeln!(self.path_writer, "{}", path.display())?;
        self.paths.insert(path.to_path_buf(), index);
        Ok(index)
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
    let index_root = root.join("deps");
    if !index_root.exists() {
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
    let paths = read_paths(&index_root)?;
    let mut reverse_dependencies = BTreeSet::new();
    for stem in changed_stems {
        let shard = index_root.join(shard_filename(shard_for_stem(&stem)));
        if !shard.exists() {
            continue;
        }
        read_reverse_shard(&shard, &stem, &paths, &changed, &mut reverse_dependencies)?;
    }
    let mut files_to_reindex = changed.clone();
    files_to_reindex.extend(reverse_dependencies.iter().cloned());
    Ok(Some(StoreIncrementalPlan {
        changed_files: changed.into_iter().collect(),
        reverse_dependencies: reverse_dependencies.into_iter().collect(),
        files_to_reindex: files_to_reindex.into_iter().collect(),
    }))
}

fn read_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let reader = BufReader::new(fs::File::open(root.join("paths.tsv"))?);
    reader
        .lines()
        .map(|line| Ok(PathBuf::from(line?)))
        .collect()
}

fn read_reverse_shard(
    path: &Path,
    stem: &str,
    paths: &[PathBuf],
    changed: &BTreeSet<PathBuf>,
    reverse_dependencies: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let reader = BufReader::new(fs::File::open(path)?);
    for line in reader.lines() {
        let line = line?;
        let Some((line_stem, path_index)) = parse_reverse_entry(&line) else {
            continue;
        };
        if line_stem != stem {
            continue;
        }
        if let Some(path) = paths.get(path_index)
            && !changed.contains(path)
        {
            reverse_dependencies.insert(path.clone());
        }
    }
    Ok(())
}

fn parse_reverse_entry(line: &str) -> Option<(&str, usize)> {
    let (stem, path_index) = line.split_once('\t')?;
    Some((stem, path_index.parse().ok()?))
}

fn shard_for_stem(stem: &str) -> u8 {
    blake3::hash(stem.as_bytes()).as_bytes()[0]
}

fn shard_filename(shard: u8) -> String {
    format!("{shard:02x}.tsv")
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
