use crate::AdjacencyIndex;
use aci_core::GraphSnapshot;

pub fn build_adjacency(snapshot: &GraphSnapshot) -> AdjacencyIndex {
    let mut index = AdjacencyIndex::default();
    for edge in snapshot
        .partitions
        .iter()
        .flat_map(|partition| &partition.edges)
    {
        index
            .outgoing
            .entry(edge.from.clone())
            .or_default()
            .push(edge.clone());
        index
            .incoming
            .entry(edge.to.clone())
            .or_default()
            .push(edge.clone());
    }
    index
}
