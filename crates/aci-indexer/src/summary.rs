use aci_core::{GraphPartition, Language};
use std::collections::BTreeMap;

/// Aggregate indexing metrics that can be computed without retaining partitions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexSummary {
    pub indexed_files: usize,
    pub skipped_files: usize,
    pub diagnostics: usize,
    pub language_counts: BTreeMap<Language, usize>,
    pub nodes: usize,
    pub edges: usize,
    pub max_nodes_per_file: usize,
    pub max_edges_per_file: usize,
    pub parse_time_micros: u64,
    pub extraction_time_micros: u64,
}

impl From<GraphPartition> for IndexSummary {
    fn from(partition: GraphPartition) -> Self {
        let mut language_counts = BTreeMap::new();
        language_counts.insert(partition.language, 1);
        Self {
            indexed_files: 1,
            skipped_files: 0,
            diagnostics: partition.diagnostics.len(),
            language_counts,
            nodes: partition.nodes.len(),
            edges: partition.edges.len(),
            max_nodes_per_file: partition.nodes.len(),
            max_edges_per_file: partition.edges.len(),
            parse_time_micros: partition.metrics.parse_time_micros,
            extraction_time_micros: partition.metrics.extraction_time_micros,
        }
    }
}

impl IndexSummary {
    pub(crate) fn merge_partition(&mut self, partition: &GraphPartition) {
        self.indexed_files += 1;
        self.diagnostics += partition.diagnostics.len();
        self.nodes += partition.nodes.len();
        self.edges += partition.edges.len();
        self.max_nodes_per_file = self.max_nodes_per_file.max(partition.nodes.len());
        self.max_edges_per_file = self.max_edges_per_file.max(partition.edges.len());
        self.parse_time_micros += partition.metrics.parse_time_micros;
        self.extraction_time_micros += partition.metrics.extraction_time_micros;
        *self.language_counts.entry(partition.language).or_insert(0) += 1;
    }

    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.indexed_files += other.indexed_files;
        self.skipped_files += other.skipped_files;
        self.diagnostics += other.diagnostics;
        self.nodes += other.nodes;
        self.edges += other.edges;
        self.max_nodes_per_file = self.max_nodes_per_file.max(other.max_nodes_per_file);
        self.max_edges_per_file = self.max_edges_per_file.max(other.max_edges_per_file);
        self.parse_time_micros += other.parse_time_micros;
        self.extraction_time_micros += other.extraction_time_micros;
        for (language, count) in other.language_counts {
            *self.language_counts.entry(language).or_insert(0) += count;
        }
        self
    }
}
