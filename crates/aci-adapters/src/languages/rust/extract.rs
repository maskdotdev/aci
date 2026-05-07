use crate::helpers::PartitionBuilder;
use crate::tree_sitter::{
    ExtractionMode, ParseLimits, ParseSkip, ParserPool, child_by_field_name, node_span, node_text,
    rust_language,
};
use aci_core::{
    Confidence, Diagnostic, FactProvenance, GraphPartition, NodeId, SourceFile, SymbolKind,
};
use std::sync::OnceLock;

static RUST_POOL: OnceLock<ParserPool> = OnceLock::new();

pub fn extract_rust(file: &SourceFile) -> GraphPartition {
    match ExtractionMode::current() {
        ExtractionMode::ScannerOnly => scanner_extract_rust(file),
        ExtractionMode::TreeSitterOnly => tree_sitter_extract_rust(file, false),
        ExtractionMode::TreeSitterWithFallback | ExtractionMode::TreeSitterWithEnrichment => {
            tree_sitter_extract_rust(file, true)
        }
    }
}

fn tree_sitter_extract_rust(file: &SourceFile, fallback: bool) -> GraphPartition {
    let limits = ParseLimits::default();
    let pool = RUST_POOL.get_or_init(|| ParserPool::new(rust_language()));
    let report = match pool.parse(&file.text, &file.file_id, limits) {
        Ok(report) => report,
        Err(skip) if fallback => {
            let mut partition = scanner_extract_rust(file);
            partition.diagnostics.push(skip_diagnostic(skip, file));
            return partition;
        }
        Err(skip) => {
            let mut partition = GraphPartition::empty(file);
            partition.diagnostics.push(skip_diagnostic(skip, file));
            return partition;
        }
    };

    let mut builder =
        PartitionBuilder::new_with_quality(file, FactProvenance::TreeSitter, Confidence::High);
    builder.add_diagnostics(report.diagnostics);
    let module_name = file
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("module");
    let module = builder.add_symbol(
        module_name,
        module_name,
        SymbolKind::Module,
        node_span(report.tree.root_node()),
    );
    let mut scopes = vec![Scope {
        node: module,
        qualified_name: module_name.to_string(),
        kind: SymbolKind::Module,
    }];
    visit_node(
        report.tree.root_node(),
        &file.text,
        &mut builder,
        &mut scopes,
    );
    let mut partition = crate::languages::rust::resolve_partition(builder.finish());
    partition.metrics.parse_time_micros = report.parse_time.as_micros() as u64;
    partition
}

fn scanner_extract_rust(file: &SourceFile) -> GraphPartition {
    let mut builder = PartitionBuilder::new(file);
    for (line_index, line) in file.text.lines().enumerate() {
        let trimmed = line.trim();
        let span = crate::helpers::line_span(&file.text, line_index);
        if let Some(rest) = trimmed.strip_prefix("use ") {
            builder.add_import(rest.trim_end_matches(';'), span.clone());
        }
        for prefix in ["fn ", "pub fn ", "async fn ", "pub async fn "] {
            if let Some(name) = crate::helpers::first_identifier_after(trimmed, prefix) {
                builder.add_symbol(name, name, SymbolKind::Function, span.clone());
            }
        }
    }
    builder.finish()
}

#[derive(Clone)]
struct Scope {
    node: NodeId,
    qualified_name: String,
    kind: SymbolKind,
}

fn visit_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &mut Vec<Scope>,
) {
    match node.kind() {
        "source_file" => visit_children(node, source, builder, scopes),
        "mod_item" => {
            if let Some(name) = name_text(node, source) {
                let qualified = qualify(&current_scope(scopes).qualified_name, name);
                let id = builder.add_symbol(name, &qualified, SymbolKind::Module, node_span(node));
                scopes.push(Scope {
                    node: id,
                    qualified_name: qualified,
                    kind: SymbolKind::Module,
                });
                visit_children(node, source, builder, scopes);
                scopes.pop();
            }
        }
        "function_item" => add_named_scope(node, source, builder, scopes, SymbolKind::Function),
        "struct_item" => add_named_scope(node, source, builder, scopes, SymbolKind::Class),
        "enum_item" => add_named_scope(node, source, builder, scopes, SymbolKind::Enum),
        "trait_item" => add_named_scope(node, source, builder, scopes, SymbolKind::Interface),
        "type_item" => add_named_scope(node, source, builder, scopes, SymbolKind::TypeAlias),
        "impl_item" => {
            let name = impl_name(node, source).unwrap_or("impl");
            let qualified = qualify(&current_scope(scopes).qualified_name, name);
            let id = builder.add_symbol(name, &qualified, SymbolKind::Class, node_span(node));
            scopes.push(Scope {
                node: id,
                qualified_name: qualified,
                kind: SymbolKind::Class,
            });
            visit_children(node, source, builder, scopes);
            scopes.pop();
        }
        "let_declaration" => {
            if let Some(pattern) = child_by_field_name(node, "pattern") {
                for identifier in identifiers(pattern) {
                    if let Some(name) = node_text(identifier, source) {
                        let qualified = qualify(&current_scope(scopes).qualified_name, name);
                        builder.add_symbol(
                            name,
                            &qualified,
                            SymbolKind::Variable,
                            node_span(identifier),
                        );
                    }
                }
            }
            visit_children(node, source, builder, scopes);
        }
        "use_declaration" => {
            if let Some(text) = node_text(node, source) {
                let specifier = text
                    .trim()
                    .trim_start_matches("use")
                    .trim()
                    .trim_end_matches(';')
                    .trim();
                if !specifier.is_empty() {
                    builder.add_import(specifier, node_span(node));
                }
            }
        }
        "call_expression" => {
            if let Some(function) = child_by_field_name(node, "function")
                && let Some(name) = call_name(function, source)
            {
                builder.add_call(current_scope(scopes).node.clone(), &name, node_span(node));
            }
            visit_children(node, source, builder, scopes);
        }
        "macro_invocation" => {
            if let Some(macro_node) = child_by_field_name(node, "macro")
                && let Some(name) = node_text(macro_node, source)
            {
                builder.add_call(
                    current_scope(scopes).node.clone(),
                    name.trim_end_matches('!'),
                    node_span(node),
                );
            }
            visit_children(node, source, builder, scopes);
        }
        _ => visit_children(node, source, builder, scopes),
    }
}

fn visit_children(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &mut Vec<Scope>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        visit_node(child, source, builder, scopes);
    }
}

fn add_named_scope(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &mut Vec<Scope>,
    kind: SymbolKind,
) {
    if let Some(name) = name_text(node, source) {
        let actual_kind =
            if kind == SymbolKind::Function && current_scope(scopes).kind == SymbolKind::Class {
                SymbolKind::Method
            } else {
                kind
            };
        let qualified = qualify(&current_scope(scopes).qualified_name, name);
        let id = builder.add_symbol(name, &qualified, actual_kind, node_span(node));
        scopes.push(Scope {
            node: id,
            qualified_name: qualified,
            kind: actual_kind,
        });
        visit_children(node, source, builder, scopes);
        scopes.pop();
    }
}

fn name_text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> Option<&'a str> {
    child_by_field_name(node, "name").and_then(|name| node_text(name, source))
}

fn impl_name<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> Option<&'a str> {
    child_by_field_name(node, "type")
        .and_then(|type_node| node_text(type_node, source))
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn identifiers(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut found = Vec::new();
    if node.kind() == "identifier" {
        found.push(node);
        return found;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        found.extend(identifiers(child));
    }
    found
}

fn call_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => node_text(node, source).map(ToOwned::to_owned),
        "field_expression" => child_by_field_name(node, "field")
            .and_then(|field| node_text(field, source))
            .map(ToOwned::to_owned),
        "scoped_identifier" => child_by_field_name(node, "name")
            .and_then(|name| node_text(name, source))
            .map(ToOwned::to_owned),
        _ => node_text(node, source)
            .map(str::trim)
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty()),
    }
}

fn current_scope(scopes: &[Scope]) -> &Scope {
    scopes.last().expect("rust extractor always has a scope")
}

fn qualify(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}::{name}")
    }
}

fn skip_diagnostic(skip: ParseSkip, file: &SourceFile) -> Diagnostic {
    let message = match skip {
        ParseSkip::TooLarge { bytes, limit } => {
            format!("tree-sitter skipped large Rust file: {bytes} bytes > {limit}")
        }
        ParseSkip::Timeout => "tree-sitter Rust parse timed out".to_string(),
        ParseSkip::Parser(error) => format!("tree-sitter Rust parser failed: {error}"),
    };
    Diagnostic::warning(message, Some(file.file_id.clone()), None)
}
