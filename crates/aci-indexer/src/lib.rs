use aci_adapters::{AdapterRegistry, default_registry};
use aci_core::{
    Diagnostic, GraphPartition, GraphSnapshot, Language, NodeKind, RepositoryId, Result, SourceFile,
};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct IndexOptions {
    pub root: PathBuf,
    pub workers: usize,
}

impl IndexOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            workers: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexReport {
    pub repo_id: RepositoryId,
    pub root: PathBuf,
    pub partitions: Vec<GraphPartition>,
    pub diagnostics: Vec<Diagnostic>,
    pub skipped: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncrementalPlan {
    pub changed_files: Vec<PathBuf>,
    pub reverse_dependencies: Vec<PathBuf>,
    pub files_to_reindex: Vec<PathBuf>,
}

pub struct IndexPipeline {
    registry: AdapterRegistry,
}

impl Default for IndexPipeline {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

impl IndexPipeline {
    pub fn new(registry: AdapterRegistry) -> Self {
        Self { registry }
    }

    pub fn index_path(&self, options: IndexOptions) -> Result<IndexReport> {
        let root = options.root.canonicalize()?;
        let repo_id = RepositoryId::new("repo", &[root.to_string_lossy().as_ref()]);
        let candidates = discover_files(&root)?;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(options.workers.max(1))
            .build()
            .map_err(|error| aci_core::AciError::Message(error.to_string()))?;
        let indexed = pool.install(|| {
            candidates
                .par_iter()
                .map(|path| self.index_file(&repo_id, &root, path))
                .collect::<Vec<_>>()
        });

        let mut partitions = Vec::new();
        let mut diagnostics = Vec::new();
        let mut skipped = Vec::new();
        for item in indexed {
            match item {
                Ok(Some(partition)) => partitions.push(partition),
                Ok(None) => {}
                Err(FileSkip::Skipped(path)) => skipped.push(path),
                Err(FileSkip::Diagnostic(diagnostic)) => diagnostics.push(diagnostic),
            }
        }
        partitions.sort_by(|left, right| left.path.cmp(&right.path));
        skipped.sort();
        Ok(IndexReport {
            repo_id,
            root,
            partitions,
            diagnostics,
            skipped,
        })
    }

    pub fn index_changed_paths(
        &self,
        root: &Path,
        changed_paths: &[PathBuf],
        workers: usize,
    ) -> Result<Vec<GraphPartition>> {
        let root = root.canonicalize()?;
        let repo_id = RepositoryId::new("repo", &[root.to_string_lossy().as_ref()]);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers.max(1))
            .build()
            .map_err(|error| aci_core::AciError::Message(error.to_string()))?;
        let indexed = pool.install(|| {
            changed_paths
                .par_iter()
                .filter(|path| path.exists())
                .map(|path| self.index_file(&repo_id, &root, path))
                .collect::<Vec<_>>()
        });

        let mut partitions = Vec::new();
        for partition in indexed.into_iter().flatten().flatten() {
            partitions.push(partition);
        }
        partitions.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(partitions)
    }

    fn index_file(
        &self,
        repo_id: &RepositoryId,
        root: &Path,
        path: &Path,
    ) -> std::result::Result<Option<GraphPartition>, FileSkip> {
        if is_vendor_or_generated(path) {
            return Err(FileSkip::Skipped(path.to_path_buf()));
        }
        if !self.registry.path_candidate(path) {
            return Ok(None);
        }
        let bytes = fs::read(path).map_err(|error| {
            FileSkip::Diagnostic(Diagnostic::warning(error.to_string(), None, None))
        })?;
        if is_binary(&bytes) {
            return Err(FileSkip::Skipped(path.to_path_buf()));
        }
        let language = self.registry.detect_language(path, &bytes);
        if language == Language::Unknown {
            return Ok(None);
        }
        let text = String::from_utf8(bytes).map_err(|error| {
            FileSkip::Diagnostic(Diagnostic::warning(error.to_string(), None, None))
        })?;
        let source = SourceFile::new(repo_id.clone(), root, path.to_path_buf(), language, text);
        let started = Instant::now();
        let mut partition = self.registry.extract(&source);
        partition.metrics.extraction_time_micros = started.elapsed().as_micros() as u64;
        Ok(Some(partition))
    }
}

enum FileSkip {
    Skipped(PathBuf),
    Diagnostic(Diagnostic),
}

pub fn discover_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkBuilder::new(root).standard_filters(true).build() {
        let entry = entry.map_err(|error| aci_core::AciError::Message(error.to_string()))?;
        let path = entry.path();
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn replace_changed_partitions(snapshot: &mut GraphSnapshot, changed: Vec<GraphPartition>) {
    for partition in changed {
        snapshot.replace_partition(partition);
    }
}

pub fn plan_incremental_reindex(
    snapshot: &GraphSnapshot,
    changed_paths: &[PathBuf],
) -> IncrementalPlan {
    let changed = changed_paths.iter().cloned().collect::<BTreeSet<_>>();
    let changed_stems = changed_paths
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .collect::<BTreeSet<_>>();
    let mut reverse_dependencies = BTreeSet::new();
    for partition in &snapshot.partitions {
        if changed.contains(&partition.path) {
            continue;
        }
        if partition_depends_on_changed_stem(partition, &changed_stems) {
            reverse_dependencies.insert(partition.path.clone());
        }
    }
    let mut files_to_reindex = changed.clone();
    files_to_reindex.extend(reverse_dependencies.iter().cloned());
    IncrementalPlan {
        changed_files: changed.into_iter().collect(),
        reverse_dependencies: reverse_dependencies.into_iter().collect(),
        files_to_reindex: files_to_reindex.into_iter().collect(),
    }
}

pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0) || std::str::from_utf8(bytes).is_err()
}

pub fn is_vendor_or_generated(path: &Path) -> bool {
    let vendor_dir = path.components().any(|component| {
        let value = component.as_os_str();
        matches!(
            value.to_str(),
            Some(
                "node_modules"
                    | ".git"
                    | "target"
                    | "dist"
                    | "build"
                    | "third_party"
                    | "vendor"
                    | ".venv"
                    | "__pycache__"
            )
        )
    });
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
    vendor_dir
        || name.ends_with(".min.js")
        || name.ends_with(".generated.ts")
        || name.ends_with(".generated.py")
        || name.ends_with(".pb.go")
}

fn partition_depends_on_changed_stem(
    partition: &GraphPartition,
    changed_stems: &BTreeSet<String>,
) -> bool {
    partition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Import)
        .filter_map(|node| node.qualified_name.as_deref().or(node.name.as_deref()))
        .filter_map(module_stem)
        .any(|stem| changed_stems.contains(stem))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn indexes_small_mixed_language_fixture() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("app.ts"),
            "import { run } from './lib';\nexport function main() { run(); }\n",
        )
        .expect("write ts");
        fs::write(
            dir.path().join("tool.py"),
            "import os\n\ndef build():\n    print(os.getcwd())\n",
        )
        .expect("write py");

        let report = IndexPipeline::default()
            .index_path(IndexOptions::new(dir.path()))
            .expect("index fixture");

        assert_eq!(report.partitions.len(), 2);
        assert!(
            report
                .partitions
                .iter()
                .any(|partition| partition.language == Language::TypeScript)
        );
        assert!(
            report
                .partitions
                .iter()
                .any(|partition| partition.language == Language::Python)
        );
        assert!(
            report
                .partitions
                .iter()
                .flat_map(|partition| &partition.nodes)
                .any(|node| {
                    node.name.as_deref() == Some("main") || node.name.as_deref() == Some("build")
                })
        );
    }

    #[test]
    fn skips_binary_and_vendor_files() {
        assert!(is_binary(b"a\0b"));
        assert!(is_vendor_or_generated(Path::new(
            "repo/node_modules/pkg/index.js"
        )));
        assert!(is_vendor_or_generated(Path::new(
            "repo/third_party/pkg/index.js"
        )));
        assert!(is_vendor_or_generated(Path::new("repo/src/app.min.js")));
    }

    #[test]
    fn incremental_plan_includes_changed_files_and_reverse_dependencies() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("lib.ts"), "export function run() {}\n").expect("write lib");
        fs::write(
            dir.path().join("app.ts"),
            "import { run } from './lib';\nexport function main() { run(); }\n",
        )
        .expect("write app");
        let report = IndexPipeline::default()
            .index_path(IndexOptions::new(dir.path()))
            .expect("index fixture");
        let snapshot = GraphSnapshot {
            partitions: report.partitions,
        };
        let changed = vec![dir.path().join("lib.ts").canonicalize().expect("canonical")];
        let plan = plan_incremental_reindex(&snapshot, &changed);
        assert!(
            plan.changed_files
                .iter()
                .any(|path| path.ends_with("lib.ts"))
        );
        assert!(
            plan.reverse_dependencies
                .iter()
                .any(|path| path.ends_with("app.ts"))
        );
        assert_eq!(plan.files_to_reindex.len(), 2);
    }
}
