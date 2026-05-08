use crate::IncrementalPlan;
use aci_core::{GraphPartition, GraphSnapshot, NodeKind};
use std::collections::BTreeSet;
use std::path::PathBuf;

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
