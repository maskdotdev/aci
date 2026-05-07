use crate::helpers::{
    PartitionBuilder, call_identifiers, first_identifier_after, line_span, read_identifier,
};
use crate::tree_sitter::{
    ExtractionMode, ParseLimits, ParseSkip, ParserPool, child_by_field_name, node_span, node_text,
    python_language,
};
use aci_core::{
    Confidence, Diagnostic, FactProvenance, GraphPartition, NodeId, SourceFile, SymbolKind,
};
use std::sync::OnceLock;

static PYTHON_POOL: OnceLock<ParserPool> = OnceLock::new();

pub fn extract_python(file: &SourceFile) -> GraphPartition {
    match ExtractionMode::current() {
        ExtractionMode::ScannerOnly => scanner_extract_python(file),
        ExtractionMode::TreeSitterOnly => tree_sitter_extract_python(file, false),
        ExtractionMode::TreeSitterWithFallback | ExtractionMode::TreeSitterWithEnrichment => {
            tree_sitter_extract_python(file, true)
        }
    }
}

fn tree_sitter_extract_python(file: &SourceFile, fallback: bool) -> GraphPartition {
    let limits = ParseLimits::default();
    let pool = PYTHON_POOL.get_or_init(|| ParserPool::new(python_language()));
    let report = match pool.parse(&file.text, &file.file_id, limits) {
        Ok(report) => report,
        Err(skip) if fallback => {
            let mut partition = scanner_extract_python(file);
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
        .unwrap_or("__main__");
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
    let mut partition = crate::languages::python::resolve_partition(builder.finish());
    partition.metrics.parse_time_micros = report.parse_time.as_micros() as u64;
    partition
}

fn scanner_extract_python(file: &SourceFile) -> GraphPartition {
    let mut builder = PartitionBuilder::new(file);
    let mut scopes: Vec<(usize, String, NodeId)> = vec![(0, String::new(), builder.file_node())];

    for (line_index, line) in file.text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();
        while scopes.len() > 1 && indent <= scopes.last().map(|scope| scope.0).unwrap_or(0) {
            scopes.pop();
        }

        let span = line_span(&file.text, line_index);
        if let Some((module, alias)) = import_module(trimmed) {
            let import_id = builder.add_import_alias(module, alias, span.clone());
            let caller = scopes
                .last()
                .map(|scope| scope.2.clone())
                .unwrap_or_else(|| builder.file_node());
            builder.add_reference(caller, alias, span.clone());
            scopes.push((indent + 1, module.to_string(), import_id));
            continue;
        }
        if let Some((name, kind)) = symbol_declaration(trimmed) {
            let qualified = qualify(
                scopes.last().map(|scope| scope.1.as_str()).unwrap_or(""),
                name,
            );
            let node_id = builder.add_symbol(name, &qualified, kind, span.clone());
            scopes.push((indent, qualified, node_id));
            continue;
        }
        let caller = scopes
            .last()
            .map(|scope| scope.2.clone())
            .unwrap_or_else(|| builder.file_node());
        for call in call_identifiers(trimmed) {
            builder.add_call(caller.clone(), call, span.clone());
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
        "module" => visit_children(node, source, builder, scopes),
        "class_definition" => {
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
        "function_definition" => {
            if let Some(name) =
                child_by_field_name(node, "name").and_then(|name| node_text(name, source))
            {
                let parent = current_scope(scopes);
                let kind = if parent.kind == SymbolKind::Class {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let qualified = qualify(&parent.qualified_name, name);
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
        "assignment" => {
            add_assignment_symbols(node, source, builder, scopes);
            visit_children(node, source, builder, scopes);
        }
        "import_statement" | "import_from_statement" => {
            if let Some(text) = node_text(node, source) {
                add_imports_from_text(
                    text,
                    node_span(node),
                    builder,
                    current_scope(scopes).node.clone(),
                );
            }
        }
        "call" => {
            if let Some(callee) = child_by_field_name(node, "function")
                .and_then(|function| call_name(function, source))
            {
                builder.add_call(current_scope(scopes).node.clone(), &callee, node_span(node));
            }
            visit_children(node, source, builder, scopes);
        }
        "decorator" => {
            if let Some(text) = node_text(node, source).and_then(decorator_name) {
                builder.add_reference(current_scope(scopes).node.clone(), text, node_span(node));
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

fn add_assignment_symbols(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &[Scope],
) {
    let Some(left) = child_by_field_name(node, "left") else {
        return;
    };
    for identifier in identifiers(left) {
        if let Some(name) = node_text(identifier, source) {
            let parent = current_scope(scopes);
            let kind = if parent.kind == SymbolKind::Class {
                SymbolKind::Field
            } else {
                SymbolKind::Variable
            };
            let qualified = qualify(&parent.qualified_name, name);
            builder.add_symbol(name, &qualified, kind, node_span(identifier));
        }
    }
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
        "attribute" => child_by_field_name(node, "attribute")
            .and_then(|attribute| node_text(attribute, source))
            .map(ToOwned::to_owned),
        _ => node_text(node, source)
            .and_then(|text| text.rsplit('.').next())
            .map(|value| value.trim_matches(['(', ')']).to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn add_imports_from_text(
    text: &str,
    span: aci_core::SourceSpan,
    builder: &mut PartitionBuilder<'_>,
    scope: NodeId,
) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("import ") {
        for item in rest
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let (specifier, alias) = split_alias(item);
            builder.add_import_alias(specifier, alias, span.clone());
            builder.add_reference(scope.clone(), alias, span.clone());
        }
    } else if let Some(rest) = trimmed.strip_prefix("from ") {
        let Some((module, names)) = rest.split_once(" import ") else {
            return;
        };
        for item in names
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let (name, alias) = split_alias(item);
            let specifier = if name == "*" {
                module.to_string()
            } else {
                format!("{module}.{name}")
            };
            builder.add_import_alias(&specifier, alias, span.clone());
            builder.add_reference(scope.clone(), alias, span.clone());
        }
    }
}

fn split_alias(value: &str) -> (&str, &str) {
    value
        .split_once(" as ")
        .map(|(specifier, alias)| (specifier.trim(), alias.trim()))
        .unwrap_or_else(|| {
            let specifier = value.trim();
            let alias = specifier.rsplit('.').next().unwrap_or(specifier);
            (specifier, alias)
        })
}

fn current_scope(scopes: &[Scope]) -> &Scope {
    scopes.last().expect("python extractor always has a scope")
}

fn decorator_name(text: &str) -> Option<&str> {
    read_identifier(text.trim_start_matches('@').trim())
}

fn skip_diagnostic(skip: ParseSkip, file: &SourceFile) -> Diagnostic {
    let message = match skip {
        ParseSkip::TooLarge { bytes, limit } => {
            format!("tree-sitter skipped large Python file: {bytes} bytes > {limit}")
        }
        ParseSkip::Timeout => "tree-sitter Python parse timed out".to_string(),
        ParseSkip::Parser(error) => format!("tree-sitter Python parser failed: {error}"),
    };
    Diagnostic::warning(message, Some(file.file_id.clone()), None)
}

fn import_module(line: &str) -> Option<(&str, &str)> {
    first_identifier_after(line, "import ")
        .map(|module| (module, module.rsplit('.').next().unwrap_or(module)))
        .or_else(|| {
            first_identifier_after(line, "from ")
                .map(|module| (module, module.rsplit('.').next().unwrap_or(module)))
        })
}

fn symbol_declaration(line: &str) -> Option<(&str, SymbolKind)> {
    first_identifier_after(line, "async def ")
        .or_else(|| first_identifier_after(line, "def "))
        .map(|name| (name, SymbolKind::Function))
        .or_else(|| first_identifier_after(line, "class ").map(|name| (name, SymbolKind::Class)))
        .or_else(|| assignment_symbol(line))
}

fn assignment_symbol(line: &str) -> Option<(&str, SymbolKind)> {
    let (name, _) = line.split_once('=')?;
    let name = read_identifier(name.trim())?;
    Some((name, SymbolKind::Variable))
}

fn qualify(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}.{name}")
    }
}
