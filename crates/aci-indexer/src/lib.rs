//! Repository discovery and indexing pipeline orchestration.
//!
//! `aci-indexer` walks repositories, filters files, fingerprints source bytes,
//! invokes language adapters, and streams per-file graph partitions into a
//! caller-provided sink or store. The pipeline keeps indexing incremental by
//! planning direct changes plus reverse dependencies.

mod discover;
mod incremental;
mod pipeline;
mod summary;
mod types;

pub use discover::{discover_files, fingerprint_bytes, is_binary, is_vendor_or_generated};
pub use incremental::{plan_incremental_reindex, replace_changed_partitions};
pub use pipeline::IndexPipeline;
pub use summary::IndexSummary;
pub use types::{IncrementalPlan, IndexOptions, IndexReport};

#[cfg(test)]
mod tests {
    use super::*;
    use aci_core::{GraphSnapshot, Language};
    use std::fs;
    use std::path::Path;

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
    fn summarizes_without_retaining_partitions() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("app.ts"), "export function main() {}\n").expect("write ts");
        fs::write(dir.path().join("tool.py"), "def build():\n    return 1\n").expect("write py");
        fs::write(dir.path().join("bundle.min.js"), "minified();\n").expect("write generated");

        let summary = IndexPipeline::default()
            .summarize_path(IndexOptions::new(dir.path()))
            .expect("summarize fixture");

        assert_eq!(summary.indexed_files, 2);
        assert_eq!(summary.skipped_files, 1);
        assert_eq!(summary.language_counts.get(&Language::TypeScript), Some(&1));
        assert_eq!(summary.language_counts.get(&Language::Python), Some(&1));
        assert!(summary.nodes > 0);
        assert!(summary.edges > 0);
    }

    #[test]
    fn streams_partitions_to_sink() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("app.ts"), "export function main() {}\n").expect("write ts");
        fs::write(dir.path().join("tool.py"), "def build():\n    return 1\n").expect("write py");
        let mut paths = Vec::new();

        let summary = IndexPipeline::default()
            .stream_path(IndexOptions::new(dir.path()), |partition| {
                paths.push(partition.path.clone());
                Ok(())
            })
            .expect("stream fixture");

        assert_eq!(summary.indexed_files, 2);
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|path| path.ends_with("app.ts")));
        assert!(paths.iter().any(|path| path.ends_with("tool.py")));
    }

    #[test]
    fn max_parse_bytes_skips_tree_sitter_and_keeps_fallback_facts() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("app.ts"),
            "export function main() {\n  helper();\n}\nfunction helper() {}\n",
        )
        .expect("write ts");
        let mut options = IndexOptions::new(dir.path());
        options.max_parse_bytes = Some(8);

        let report = IndexPipeline::default()
            .index_path(options)
            .expect("index oversized fixture");

        let partition = report
            .partitions
            .iter()
            .find(|partition| partition.path.ends_with("app.ts"))
            .expect("typescript partition");
        assert!(
            partition
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("skipped large JS/TS file"))
        );
        assert!(partition.nodes.iter().any(|node| {
            node.name.as_deref() == Some("main") || node.name.as_deref() == Some("helper")
        }));
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
