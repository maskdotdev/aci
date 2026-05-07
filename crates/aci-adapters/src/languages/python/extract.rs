use crate::helpers::{
    PartitionBuilder, call_identifiers, first_identifier_after, line_span, read_identifier,
};
use aci_core::{GraphPartition, NodeId, SourceFile, SymbolKind};

pub fn extract_python(file: &SourceFile) -> GraphPartition {
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
        if let Some(module) = import_module(trimmed) {
            let import_id = builder.add_import(module, span.clone());
            let caller = scopes
                .last()
                .map(|scope| scope.2.clone())
                .unwrap_or_else(|| builder.file_node());
            builder.add_reference(caller, module, span.clone());
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

fn import_module(line: &str) -> Option<&str> {
    first_identifier_after(line, "import ").or_else(|| first_identifier_after(line, "from "))
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
