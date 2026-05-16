use crate::ExtractionOptions;
use crate::helpers::PartitionBuilder;
use crate::tree_sitter::{
    ExtractionMode, ParseLimits, ParseSkip, ParserPool, child_by_field_name, node_span, node_text,
};
use aci_core::{
    Confidence, Diagnostic, FactProvenance, GraphPartition, NodeId, SourceFile, SymbolKind,
};

mod resolve;
mod scanner;

pub struct BraceLanguage {
    pub name: &'static str,
    pub scope_separator: &'static str,
    pub module_fallback: &'static str,
    pub imports: &'static [&'static str],
    pub functions: &'static [&'static str],
    pub methods: &'static [&'static str],
    pub classes: &'static [&'static str],
    pub interfaces: &'static [&'static str],
    pub enums: &'static [&'static str],
    pub type_aliases: &'static [&'static str],
    pub variables: &'static [&'static str],
    pub scopes: &'static [&'static str],
    pub calls: &'static [&'static str],
}

pub fn extract(file: &SourceFile, pool: &ParserPool, config: &BraceLanguage) -> GraphPartition {
    extract_with_options(file, pool, config, ExtractionOptions::default())
}

pub fn extract_with_options(
    file: &SourceFile,
    pool: &ParserPool,
    config: &BraceLanguage,
    options: ExtractionOptions,
) -> GraphPartition {
    match ExtractionMode::current() {
        ExtractionMode::ScannerOnly => scanner::extract(file, config),
        ExtractionMode::TreeSitterOnly => {
            tree_sitter_extract(file, pool, config, options.parse_limits, false)
        }
        ExtractionMode::TreeSitterWithFallback | ExtractionMode::TreeSitterWithEnrichment => {
            tree_sitter_extract(file, pool, config, options.parse_limits, true)
        }
    }
}

pub fn resolve_partition(partition: GraphPartition) -> GraphPartition {
    resolve::partition(partition)
}

fn tree_sitter_extract(
    file: &SourceFile,
    pool: &ParserPool,
    config: &BraceLanguage,
    limits: ParseLimits,
    fallback: bool,
) -> GraphPartition {
    let report = match pool.parse(&file.text, &file.file_id, limits) {
        Ok(report) => report,
        Err(skip) if fallback => {
            let mut partition = scanner::extract(file, config);
            partition
                .diagnostics
                .push(skip_diagnostic(skip, file, config.name));
            return partition;
        }
        Err(skip) => {
            let mut partition = GraphPartition::empty(file);
            partition
                .diagnostics
                .push(skip_diagnostic(skip, file, config.name));
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
        .unwrap_or(config.module_fallback);
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
        config,
    );
    let mut partition = resolve_partition(builder.finish());
    partition.metrics.parse_time_micros = report.parse_time.as_micros() as u64;
    partition
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
    config: &BraceLanguage,
) {
    let kind = node.kind();
    if contains(config.imports, kind) {
        add_import(node, source, builder);
        visit_children(node, source, builder, scopes, config);
        return;
    }
    if contains(config.scopes, kind)
        && let Some(name) = name_text(node, source)
    {
        add_scope(
            node,
            source,
            builder,
            scopes,
            config,
            name,
            SymbolKind::Module,
        );
        return;
    }
    if contains(config.functions, kind)
        && let Some(name) = function_name(node, source)
    {
        add_scope(
            node,
            source,
            builder,
            scopes,
            config,
            &name,
            SymbolKind::Function,
        );
        return;
    }
    if contains(config.methods, kind)
        && let Some(name) = name_text(node, source)
    {
        add_scope(
            node,
            source,
            builder,
            scopes,
            config,
            name,
            SymbolKind::Method,
        );
        return;
    }
    if contains(config.classes, kind)
        && let Some(name) = name_text(node, source)
    {
        add_scope(
            node,
            source,
            builder,
            scopes,
            config,
            name,
            SymbolKind::Class,
        );
        return;
    }
    if contains(config.interfaces, kind)
        && let Some(name) = name_text(node, source)
    {
        add_scope(
            node,
            source,
            builder,
            scopes,
            config,
            name,
            SymbolKind::Interface,
        );
        return;
    }
    if contains(config.enums, kind)
        && let Some(name) = name_text(node, source)
    {
        add_scope(
            node,
            source,
            builder,
            scopes,
            config,
            name,
            SymbolKind::Enum,
        );
        return;
    }
    if contains(config.type_aliases, kind)
        && let Some(name) = name_text(node, source)
    {
        let qualified = qualify(&current_scope(scopes).qualified_name, name, config);
        builder.add_symbol(name, &qualified, SymbolKind::TypeAlias, node_span(node));
        visit_children(node, source, builder, scopes, config);
        return;
    }
    if contains(config.variables, kind) {
        add_variable(node, source, builder, scopes, config);
        visit_children(node, source, builder, scopes, config);
        return;
    }
    if contains(config.calls, kind) {
        if let Some(name) = call_name(node, source) {
            builder.add_call(current_scope(scopes).node.clone(), &name, node_span(node));
        }
        visit_children(node, source, builder, scopes, config);
        return;
    }
    visit_children(node, source, builder, scopes, config);
}

fn visit_children(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &mut Vec<Scope>,
    config: &BraceLanguage,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        visit_node(child, source, builder, scopes, config);
    }
}

fn add_scope(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &mut Vec<Scope>,
    config: &BraceLanguage,
    name: &str,
    kind: SymbolKind,
) {
    let actual_kind =
        if kind == SymbolKind::Function && current_scope(scopes).kind == SymbolKind::Class {
            SymbolKind::Method
        } else {
            kind
        };
    let qualified = qualify(&current_scope(scopes).qualified_name, name, config);
    let id = builder.add_symbol(name, &qualified, actual_kind, node_span(node));
    scopes.push(Scope {
        node: id,
        qualified_name: qualified,
        kind: actual_kind,
    });
    visit_children(node, source, builder, scopes, config);
    scopes.pop();
}

fn add_import(node: tree_sitter::Node<'_>, source: &str, builder: &mut PartitionBuilder<'_>) {
    let Some(text) = node_text(node, source).map(str::trim) else {
        return;
    };
    let specifier = text
        .strip_prefix("#include")
        .or_else(|| text.strip_prefix("import static"))
        .or_else(|| text.strip_prefix("import"))
        .or_else(|| text.strip_prefix("@import"))
        .unwrap_or(text)
        .trim()
        .trim_end_matches(';')
        .trim_matches('"')
        .trim_matches('`');
    if !specifier.is_empty() {
        builder.add_import(specifier, node_span(node));
    }
}

fn add_variable(
    node: tree_sitter::Node<'_>,
    source: &str,
    builder: &mut PartitionBuilder<'_>,
    scopes: &[Scope],
    config: &BraceLanguage,
) {
    if matches!(
        current_scope(scopes).kind,
        SymbolKind::Function | SymbolKind::Method
    ) {
        return;
    }
    for identifier in identifiers(node) {
        if let Some(name) = node_text(identifier, source)
            && !is_keyword(name)
        {
            let qualified = qualify(&current_scope(scopes).qualified_name, name, config);
            builder.add_symbol(
                name,
                &qualified,
                SymbolKind::Variable,
                node_span(identifier),
            );
            break;
        }
    }
}

fn name_text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> Option<&'a str> {
    child_by_field_name(node, "name")
        .and_then(|name| node_text(name, source))
        .or_else(|| {
            identifiers(node)
                .into_iter()
                .find_map(|identifier| node_text(identifier, source))
        })
}

fn function_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    child_by_field_name(node, "declarator")
        .and_then(|declarator| deepest_named_declarator(declarator, source))
        .or_else(|| name_text(node, source).map(ToOwned::to_owned))
}

fn deepest_named_declarator(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    if matches!(
        node.kind(),
        "identifier" | "field_identifier" | "scoped_identifier" | "qualified_identifier"
    ) {
        return node_text(node, source).map(ToOwned::to_owned);
    }
    for field in ["declarator", "name", "field", "operator_name"] {
        if let Some(child) = child_by_field_name(node, field)
            && let Some(name) = deepest_named_declarator(child, source)
        {
            return Some(name);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        if let Some(name) = deepest_named_declarator(child, source) {
            return Some(name);
        }
    }
    None
}

fn identifiers(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut found = Vec::new();
    if matches!(
        node.kind(),
        "identifier" | "field_identifier" | "type_identifier" | "package_identifier"
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

fn call_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    for field in ["function", "name", "method", "selector"] {
        if let Some(child) = child_by_field_name(node, field)
            && let Some(name) = callable_leaf(child, source)
        {
            return Some(name);
        }
    }
    callable_leaf(node, source)
}

fn callable_leaf(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "method_invocation" | "selector" => {
            node_text(node, source).map(ToOwned::to_owned)
        }
        "field_expression" | "member_expression" => child_by_field_name(node, "field")
            .and_then(|field| node_text(field, source))
            .map(ToOwned::to_owned),
        "scoped_identifier" | "qualified_identifier" => child_by_field_name(node, "name")
            .and_then(|name| node_text(name, source))
            .map(ToOwned::to_owned),
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor).filter(|child| child.is_named()) {
                if let Some(name) = callable_leaf(child, source) {
                    return Some(name);
                }
            }
            None
        }
    }
}

fn current_scope(scopes: &[Scope]) -> &Scope {
    scopes
        .last()
        .expect("brace-language extractor always has a root scope")
}

fn qualify(parent: &str, name: &str, config: &BraceLanguage) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}{}{name}", config.scope_separator)
    }
}

fn contains(haystack: &[&str], needle: &str) -> bool {
    haystack.contains(&needle)
}

fn is_keyword(value: &str) -> bool {
    matches!(
        value,
        "if" | "for"
            | "while"
            | "switch"
            | "return"
            | "class"
            | "struct"
            | "enum"
            | "interface"
            | "public"
            | "private"
            | "protected"
            | "static"
            | "const"
            | "var"
            | "let"
            | "type"
    )
}

fn skip_diagnostic(skip: ParseSkip, file: &SourceFile, language: &str) -> Diagnostic {
    let message = match skip {
        ParseSkip::TooLarge { bytes, limit } => {
            format!("tree-sitter skipped large {language} file: {bytes} bytes > {limit}")
        }
        ParseSkip::Timeout => format!("tree-sitter {language} parse timed out"),
        ParseSkip::Parser(error) => format!("tree-sitter {language} parser failed: {error}"),
    };
    Diagnostic::warning(message, Some(file.file_id.clone()), None)
}
