use crate::ExtractionOptions;
use crate::helpers::{PartitionBuilder, first_identifier_after, quoted_module, read_identifier};
use crate::languages::typescript::scanner::scanner_extract_typescript;
use crate::tree_sitter::{
    ExtractionMode, ParseLimits, ParseSkip, ParserPool, child_by_field_name, javascript_language,
    node_span, node_text, tsx_language, typescript_language,
};
use aci_core::{
    Confidence, Diagnostic, FactProvenance, GraphPartition, Language, NodeId, SourceFile,
    SymbolKind,
};
use std::path::Path;
use std::sync::OnceLock;

static JAVASCRIPT_POOL: OnceLock<ParserPool> = OnceLock::new();
static TYPESCRIPT_POOL: OnceLock<ParserPool> = OnceLock::new();
static TSX_POOL: OnceLock<ParserPool> = OnceLock::new();

pub fn extract_typescript(file: &SourceFile) -> GraphPartition {
    extract_typescript_with_options(file, ExtractionOptions::default())
}

pub fn extract_typescript_with_options(
    file: &SourceFile,
    options: ExtractionOptions,
) -> GraphPartition {
    match ExtractionMode::current() {
        ExtractionMode::ScannerOnly => scanner_extract_typescript(file),
        ExtractionMode::TreeSitterOnly => {
            tree_sitter_extract_typescript(file, options.parse_limits, false)
        }
        ExtractionMode::TreeSitterWithFallback | ExtractionMode::TreeSitterWithEnrichment => {
            tree_sitter_extract_typescript(file, options.parse_limits, true)
        }
    }
}

fn tree_sitter_extract_typescript(
    file: &SourceFile,
    limits: ParseLimits,
    fallback: bool,
) -> GraphPartition {
    let grammar = grammar_for_path(&file.path, file.language);
    let pool = match grammar {
        Grammar::JavaScript => {
            JAVASCRIPT_POOL.get_or_init(|| ParserPool::new(javascript_language()))
        }
        Grammar::TypeScript => {
            TYPESCRIPT_POOL.get_or_init(|| ParserPool::new(typescript_language()))
        }
        Grammar::Tsx => TSX_POOL.get_or_init(|| ParserPool::new(tsx_language())),
    };
    let report = match pool.parse(&file.text, &file.file_id, limits) {
        Ok(report) => report,
        Err(skip) if fallback => {
            let mut partition = scanner_extract_typescript(file);
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
    let mut partition = crate::languages::typescript::resolve_partition(builder.finish());
    partition.metrics.parse_time_micros = report.parse_time.as_micros() as u64;
    partition
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Grammar {
    JavaScript,
    TypeScript,
    Tsx,
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
        "program" => visit_children(node, source, builder, scopes),
        "import_statement" | "export_statement" => {
            if let Some(text) = node_text(node, source) {
                add_import_or_export(text, node_span(node), builder);
            }
            visit_children(node, source, builder, scopes);
        }
        "class_declaration" => {
            if let Some(name) =
                child_by_field_name(node, "name").and_then(|name| node_text(name, source))
            {
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
        }
        "function_declaration" | "generator_function_declaration" => {
            add_named_scope(node, source, builder, scopes, SymbolKind::Function);
        }
        "method_definition" | "method_signature" => {
            add_named_scope(node, source, builder, scopes, SymbolKind::Method);
        }
        "interface_declaration" => {
            add_named_scope(node, source, builder, scopes, SymbolKind::Interface);
        }
        "type_alias_declaration" => {
            add_named_scope(node, source, builder, scopes, SymbolKind::TypeAlias);
        }
        "enum_declaration" => {
            add_named_scope(node, source, builder, scopes, SymbolKind::Enum);
        }
        "public_field_definition" | "field_definition" => {
            if let Some(name) =
                child_by_field_name(node, "name").and_then(|name| node_text(name, source))
            {
                let qualified = qualify(&current_scope(scopes).qualified_name, name);
                let kind = if current_scope(scopes).kind == SymbolKind::Class {
                    SymbolKind::Field
                } else {
                    SymbolKind::Variable
                };
                builder.add_symbol(name, &qualified, kind, node_span(node));
            }
            visit_children(node, source, builder, scopes);
        }
        "variable_declarator" => {
            add_variable_declarator(node, source, builder, scopes);
            visit_children(node, source, builder, scopes);
        }
        "call_expression" => {
            if let Some(callee) = child_by_field_name(node, "function")
                .and_then(|function| call_name(function, source))
            {
                if callee == "import" {
                    if let Some(specifier) = literal_argument(node, source) {
                        builder.add_import(&specifier, node_span(node));
                    }
                } else {
                    builder.add_call(current_scope(scopes).node.clone(), &callee, node_span(node));
                }
            }
            visit_children(node, source, builder, scopes);
        }
        "new_expression" => {
            if let Some(callee) = child_by_field_name(node, "constructor")
                .or_else(|| child_by_field_name(node, "function"))
                .and_then(|function| call_name(function, source))
            {
                builder.add_call(current_scope(scopes).node.clone(), &callee, node_span(node));
            }
            visit_children(node, source, builder, scopes);
        }
        "jsx_opening_element" | "jsx_self_closing_element" => {
            if let Some(name) =
                child_by_field_name(node, "name").and_then(|name| node_text(name, source))
                && name
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
            {
                builder.add_reference(current_scope(scopes).node.clone(), name, node_span(node));
            }
            visit_children(node, source, builder, scopes);
        }
        "decorator" => {
            if let Some(name) = node_text(node, source).and_then(decorator_name) {
                builder.add_reference(current_scope(scopes).node.clone(), name, node_span(node));
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
    if let Some(name) = child_by_field_name(node, "name").and_then(|name| node_text(name, source)) {
        let qualified = qualify(&current_scope(scopes).qualified_name, name);
        let id = builder.add_symbol(name, &qualified, kind, node_span(node));
        scopes.push(Scope {
            node: id,
            qualified_name: qualified,
            kind,
        });
        visit_children(node, source, builder, scopes);
        scopes.pop();
    }
}

fn add_variable_declarator(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &[Scope],
) {
    let Some(name_node) = child_by_field_name(node, "name") else {
        return;
    };
    for identifier in identifiers(name_node) {
        if let Some(name) = node_text(identifier, source) {
            let value_text =
                child_by_field_name(node, "value").and_then(|value| node_text(value, source));
            let kind = if value_text
                .is_some_and(|value| value.contains("=>") || value.starts_with("function"))
            {
                SymbolKind::Function
            } else {
                SymbolKind::Variable
            };
            let qualified = qualify(&current_scope(scopes).qualified_name, name);
            builder.add_symbol(name, &qualified, kind, node_span(identifier));
        }
    }
}

fn identifiers(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut found = Vec::new();
    if matches!(
        node.kind(),
        "identifier" | "shorthand_property_identifier_pattern"
    ) {
        found.push(node);
        return found;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        found.extend(identifiers(child));
    }
    found
}

fn add_import_or_export(
    text: &str,
    span: aci_core::SourceSpan,
    builder: &mut PartitionBuilder<'_>,
) {
    let trimmed = text.trim();
    if trimmed.starts_with("import ") {
        if let Some(module) = module_specifier(trimmed) {
            for alias in import_aliases(trimmed) {
                builder.add_import_alias(module, &alias, span.clone());
            }
            if import_aliases(trimmed).is_empty() {
                builder.add_import(module, span);
            }
        }
    } else if trimmed.starts_with("export ") {
        if let Some(module) = module_specifier(trimmed) {
            builder.add_import(module, span.clone());
        }
        for name in export_names(trimmed) {
            builder.add_export(&name, span.clone());
        }
    }
}

fn import_aliases(line: &str) -> Vec<String> {
    if let Some((head, _module)) = line.split_once(" from ") {
        return head
            .trim_start_matches("import")
            .trim()
            .trim_matches(['{', '}'])
            .split(',')
            .filter_map(|item| {
                let item = item.trim();
                if item.is_empty() || item.starts_with("type ") {
                    return None;
                }
                if let Some(alias) = item.strip_prefix("* as ") {
                    return Some(alias.trim().to_string());
                }
                Some(
                    item.split_once(" as ")
                        .map(|(_, alias)| alias.trim())
                        .unwrap_or_else(|| item.split_whitespace().last().unwrap_or(item))
                        .to_string(),
                )
            })
            .collect();
    }
    Vec::new()
}

fn export_names(line: &str) -> Vec<String> {
    export_name(line)
        .map(|name| vec![name.to_string()])
        .unwrap_or_else(|| {
            line.trim_start_matches("export")
                .trim()
                .trim_matches(['{', '}'])
                .split(',')
                .filter_map(|item| {
                    let name = item
                        .trim()
                        .split_once(" as ")
                        .map(|(_, alias)| alias.trim())
                        .unwrap_or_else(|| item.trim());
                    (!name.is_empty() && !name.starts_with("from ")).then(|| name.to_string())
                })
                .collect()
        })
}

fn export_name(line: &str) -> Option<&str> {
    first_identifier_after(line, "export default function ")
        .or_else(|| first_identifier_after(line, "export default class "))
        .or_else(|| first_identifier_after(line, "export function "))
        .or_else(|| first_identifier_after(line, "export class "))
        .or_else(|| first_identifier_after(line, "export interface "))
        .or_else(|| first_identifier_after(line, "export type "))
        .or_else(|| first_identifier_after(line, "export enum "))
        .or_else(|| first_identifier_after(line, "export const "))
        .or_else(|| first_identifier_after(line, "export let "))
        .or_else(|| first_identifier_after(line, "export var "))
}

fn call_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => node_text(node, source).map(ToOwned::to_owned),
        "member_expression" | "subscript_expression" => child_by_field_name(node, "property")
            .and_then(|property| node_text(property, source))
            .map(ToOwned::to_owned),
        _ => node_text(node, source)
            .map(|text| text.rsplit('.').next().unwrap_or(text).trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn literal_argument(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "arguments")
        .and_then(|arguments| {
            let mut arg_cursor = arguments.walk();
            arguments
                .children(&mut arg_cursor)
                .find(|child| child.kind() == "string")
        })
        .and_then(|string| node_text(string, source))
        .map(|value| value.trim_matches(['"', '\'']).to_string())
}

fn current_scope(scopes: &[Scope]) -> &Scope {
    scopes
        .last()
        .expect("typescript extractor always has a scope")
}

fn decorator_name(text: &str) -> Option<&str> {
    read_identifier(text.trim_start_matches('@').trim())
}

fn skip_diagnostic(skip: ParseSkip, file: &SourceFile) -> Diagnostic {
    let message = match skip {
        ParseSkip::TooLarge { bytes, limit } => {
            format!("tree-sitter skipped large JS/TS file: {bytes} bytes > {limit}")
        }
        ParseSkip::Timeout => "tree-sitter JS/TS parse timed out".to_string(),
        ParseSkip::Parser(error) => format!("tree-sitter JS/TS parser failed: {error}"),
    };
    Diagnostic::warning(message, Some(file.file_id.clone()), None)
}

fn grammar_for_path(path: &Path, language: Language) -> Grammar {
    if language == Language::JavaScript {
        return Grammar::JavaScript;
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("tsx" | "jsx") => Grammar::Tsx,
        _ => Grammar::TypeScript,
    }
}

fn module_specifier(line: &str) -> Option<&str> {
    line.split(" from ")
        .nth(1)
        .and_then(quoted_module)
        .or_else(|| quoted_module(line))
}

fn qualify(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}.{name}")
    }
}
