use aci_core::{EdgeKind, GraphEdge, GraphPartition, NodeId, NodeKind};
use std::collections::BTreeMap;

pub(super) fn partition(mut partition: GraphPartition) -> GraphPartition {
    let mut local_symbols = BTreeMap::<String, NodeId>::new();
    let mut external_names = BTreeMap::<NodeId, String>::new();
    for node in &partition.nodes {
        match node.kind {
            NodeKind::Symbol => {
                if let Some(name) = &node.name {
                    local_symbols
                        .entry(name.clone())
                        .or_insert_with(|| node.id.clone());
                }
                if let Some(qualified) = &node.qualified_name {
                    local_symbols
                        .entry(qualified.clone())
                        .or_insert_with(|| node.id.clone());
                }
            }
            NodeKind::ExternalSymbol => {
                if let Some(name) = node.name.clone() {
                    external_names.insert(node.id.clone(), name);
                }
            }
            _ => {}
        }
    }

    for edge in &mut partition.edges {
        if !matches!(edge.kind, EdgeKind::Calls | EdgeKind::References) {
            continue;
        }
        let Some(name) = external_names.get(&edge.to) else {
            continue;
        };
        let Some(target) = local_symbols.get(name) else {
            continue;
        };
        *edge = GraphEdge::deterministic(edge.kind, &edge.from, target, edge.span.clone())
            .with_fact_quality(edge.provenance, edge.confidence);
    }
    partition
}
