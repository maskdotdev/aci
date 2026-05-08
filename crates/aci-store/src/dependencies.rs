use aci_core::{GraphNode, GraphPartition, NodeKind, Result};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) struct DependencyIndexWriter {
    mode: DependencyWriterMode,
    shards: HashMap<u8, DependencyShardWriter>,
}

enum DependencyWriterMode {
    ReplaceAll {
        tmp_root: PathBuf,
        final_root: PathBuf,
    },
    Append {
        root: PathBuf,
    },
}

struct DependencyShardWriter {
    writer: BufWriter<fs::File>,
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
        Ok(Self {
            mode: DependencyWriterMode::ReplaceAll {
                tmp_root,
                final_root,
            },
            shards: HashMap::new(),
        })
    }

    pub(crate) fn copy_existing(root: &Path) -> Result<Self> {
        let final_root = root.join("deps");
        fs::create_dir_all(&final_root)?;
        Ok(Self {
            mode: DependencyWriterMode::Append { root: final_root },
            shards: HashMap::new(),
        })
    }

    pub(crate) fn write_partition_imports(
        &mut self,
        partition: &GraphPartition,
        mut import_stems: Vec<String>,
    ) -> Result<()> {
        import_stems.sort();
        import_stems.dedup();
        for stem in import_stems {
            let writer = self.open_shard(shard_for_stem(&stem))?;
            writer.writer.write_all(stem.as_bytes())?;
            writer.writer.write_all(b"\t")?;
            write!(writer.writer, "{}", partition.path.display())?;
            writeln!(writer.writer)?;
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<()> {
        for shard in self.shards.into_values() {
            let mut writer = shard.writer;
            writer.flush()?;
        }
        match self.mode {
            DependencyWriterMode::ReplaceAll {
                tmp_root,
                final_root,
            } => {
                if final_root.exists() {
                    fs::remove_dir_all(&final_root)?;
                }
                let legacy = final_root.with_file_name("deps.tsv");
                if legacy.exists() {
                    fs::remove_file(legacy)?;
                }
                let jsonl_legacy = final_root.with_file_name("deps.jsonl");
                if jsonl_legacy.exists() {
                    fs::remove_file(jsonl_legacy)?;
                }
                fs::rename(tmp_root, final_root)?;
            }
            DependencyWriterMode::Append { .. } => {}
        }
        Ok(())
    }

    fn open_shard(&mut self, shard: u8) -> Result<&mut DependencyShardWriter> {
        if !self.shards.contains_key(&shard) {
            let path = match &self.mode {
                DependencyWriterMode::ReplaceAll { tmp_root, .. } => {
                    tmp_root.join(shard_filename(shard))
                }
                DependencyWriterMode::Append { root } => root.join(shard_filename(shard)),
            };
            let file = match self.mode {
                DependencyWriterMode::ReplaceAll { .. } => fs::File::create(path)?,
                DependencyWriterMode::Append { .. } => {
                    OpenOptions::new().create(true).append(true).open(path)?
                }
            };
            self.shards.insert(
                shard,
                DependencyShardWriter {
                    writer: BufWriter::new(file),
                },
            );
        }
        Ok(self.shards.get_mut(&shard).expect("shard just inserted"))
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
    let mut reverse_dependencies = BTreeSet::new();
    for stem in changed_stems {
        let shard = index_root.join(shard_filename(shard_for_stem(&stem)));
        if !shard.exists() {
            continue;
        }
        read_reverse_shard(&shard, &stem, &changed, &mut reverse_dependencies)?;
    }
    let mut files_to_reindex = changed.clone();
    files_to_reindex.extend(reverse_dependencies.iter().cloned());
    Ok(Some(StoreIncrementalPlan {
        changed_files: changed.into_iter().collect(),
        reverse_dependencies: reverse_dependencies.into_iter().collect(),
        files_to_reindex: files_to_reindex.into_iter().collect(),
    }))
}

fn read_reverse_shard(
    path: &Path,
    stem: &str,
    changed: &BTreeSet<PathBuf>,
    reverse_dependencies: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let reader = BufReader::new(fs::File::open(path)?);
    for line in reader.lines() {
        let line = line?;
        let Some((line_stem, path)) = parse_reverse_entry(&line) else {
            continue;
        };
        if line_stem == stem && !changed.contains(&path) {
            reverse_dependencies.insert(path);
        }
    }
    Ok(())
}

fn parse_reverse_entry(line: &str) -> Option<(&str, PathBuf)> {
    let (stem, path) = line.split_once('\t')?;
    Some((stem, PathBuf::from(path)))
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
