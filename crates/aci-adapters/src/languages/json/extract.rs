use crate::helpers::PartitionBuilder;
use aci_core::{
    Confidence, Diagnostic, FactProvenance, GraphPartition, NodeId, SourceFile, SourceSpan,
    SymbolKind,
};
use serde_json::Value;
use std::time::Instant;

const DEPENDENCY_KEYS: &[&str] = &[
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
    "bundledDependencies",
    "bundleDependencies",
];

pub fn extract_json(file: &SourceFile) -> GraphPartition {
    let started = Instant::now();
    let mut builder = PartitionBuilder::new_with_quality(
        file,
        FactProvenance::StructuralScanner,
        Confidence::High,
    );
    let document_name = file
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("json");
    let root_span = SourceSpan::new(
        0,
        file.text.len() as u32,
        aci_core::LineColumn::new(1, 1),
        aci_core::LineColumn::new(file.text.lines().count().max(1) as u32, 1),
    );
    let document = builder.add_symbol(
        document_name,
        document_name,
        SymbolKind::Module,
        root_span.clone(),
    );

    match serde_json::from_str::<Value>(&file.text) {
        Ok(value) => extract_value(&value, document, &mut builder, &root_span),
        Err(error) => builder.add_diagnostic(Diagnostic::warning(
            format!("json parse failed: {error}"),
            Some(file.file_id.clone()),
            None,
        )),
    }

    let mut partition = builder.finish();
    partition.metrics.extraction_time_micros = started.elapsed().as_micros() as u64;
    partition
}

fn extract_value(
    value: &Value,
    document: NodeId,
    builder: &mut PartitionBuilder<'_>,
    span: &SourceSpan,
) {
    let Some(object) = value.as_object() else {
        return;
    };

    if let Some(name) = object.get("name").and_then(Value::as_str) {
        builder.add_symbol(name, name, SymbolKind::Module, span.clone());
    }

    for key in DEPENDENCY_KEYS {
        if let Some(dependencies) = object.get(*key) {
            extract_dependencies(dependencies, document.clone(), builder, span);
        }
    }

    if let Some(imports) = object.get("imports").and_then(Value::as_object) {
        for target in imports.values() {
            extract_json_string_dependency(target, document.clone(), builder, span);
        }
    }
    if let Some(exports) = object.get("exports") {
        extract_json_string_dependency(exports, document, builder, span);
    }
}

fn extract_dependencies(
    value: &Value,
    document: NodeId,
    builder: &mut PartitionBuilder<'_>,
    span: &SourceSpan,
) {
    match value {
        Value::Object(dependencies) => {
            for package in dependencies.keys() {
                builder.add_import(package, span.clone());
                builder.add_dependency(document.clone(), package, span.clone());
            }
        }
        Value::Array(dependencies) => {
            for package in dependencies.iter().filter_map(Value::as_str) {
                builder.add_import(package, span.clone());
                builder.add_dependency(document.clone(), package, span.clone());
            }
        }
        _ => {}
    }
}

fn extract_json_string_dependency(
    value: &Value,
    document: NodeId,
    builder: &mut PartitionBuilder<'_>,
    span: &SourceSpan,
) {
    match value {
        Value::String(target) if target.starts_with('.') || target.starts_with('/') => {
            builder.add_reference(document, target, span.clone());
        }
        Value::Array(items) => {
            for item in items {
                extract_json_string_dependency(item, document.clone(), builder, span);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                extract_json_string_dependency(item, document.clone(), builder, span);
            }
        }
        _ => {}
    }
}
