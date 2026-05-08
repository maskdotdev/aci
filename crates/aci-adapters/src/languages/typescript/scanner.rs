use crate::helpers::{
    PartitionBuilder, call_identifiers, first_identifier_after, line_span, quoted_module,
    read_identifier,
};
use aci_core::{GraphPartition, SourceFile, SymbolKind};

pub fn scanner_extract_typescript(file: &SourceFile) -> GraphPartition {
    let mut builder = PartitionBuilder::new(file);
    let mut current_scope = builder.file_node();

    for (line_index, line) in file.text.lines().enumerate() {
        let trimmed = line.trim();
        let span = line_span(&file.text, line_index);

        if (trimmed.starts_with("import ") || trimmed.contains(" require("))
            && let Some(module) = module_specifier(trimmed)
        {
            builder.add_import(module, span.clone());
        }
        if trimmed.starts_with("export ") {
            let name = export_name(trimmed).unwrap_or("default");
            builder.add_export(name, span.clone());
        }
        if let Some((name, kind)) = symbol_declaration(trimmed) {
            current_scope = builder.add_symbol(name, name, kind, span.clone());
        }
        for call in call_identifiers(trimmed) {
            if !is_declaration_call(trimmed, call) {
                builder.add_call(current_scope.clone(), call, span.clone());
            }
        }
    }

    builder.finish()
}

fn module_specifier(line: &str) -> Option<&str> {
    line.split(" from ")
        .nth(1)
        .and_then(quoted_module)
        .or_else(|| quoted_module(line))
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

fn symbol_declaration(line: &str) -> Option<(&str, SymbolKind)> {
    first_identifier_after(line, "export function ")
        .or_else(|| first_identifier_after(line, "function "))
        .map(|name| (name, SymbolKind::Function))
        .or_else(|| {
            first_identifier_after(line, "export class ")
                .or_else(|| first_identifier_after(line, "class "))
                .map(|name| (name, SymbolKind::Class))
        })
        .or_else(|| {
            first_identifier_after(line, "export interface ")
                .or_else(|| first_identifier_after(line, "interface "))
                .map(|name| (name, SymbolKind::Interface))
        })
        .or_else(|| {
            first_identifier_after(line, "export type ")
                .or_else(|| first_identifier_after(line, "type "))
                .map(|name| (name, SymbolKind::TypeAlias))
        })
        .or_else(|| {
            first_identifier_after(line, "export enum ")
                .or_else(|| first_identifier_after(line, "enum "))
                .map(|name| (name, SymbolKind::Enum))
        })
        .or_else(|| {
            [
                "export const ",
                "const ",
                "export let ",
                "let ",
                "export var ",
                "var ",
            ]
            .iter()
            .find_map(|prefix| variable_symbol(line, prefix))
        })
        .or_else(|| method_symbol(line))
}

fn variable_symbol<'a>(line: &'a str, prefix: &str) -> Option<(&'a str, SymbolKind)> {
    let name = first_identifier_after(line, prefix)?;
    if line.contains("=>") || line.contains("function") {
        Some((name, SymbolKind::Function))
    } else {
        Some((name, SymbolKind::Variable))
    }
}

fn method_symbol(line: &str) -> Option<(&str, SymbolKind)> {
    if line.starts_with("//") || !line.contains('(') || !line.ends_with('{') {
        return None;
    }
    let name = read_identifier(line)?;
    if matches!(name, "if" | "for" | "while" | "switch" | "catch") {
        None
    } else {
        Some((name, SymbolKind::Method))
    }
}

fn is_declaration_call(line: &str, call: &str) -> bool {
    line.starts_with("function ")
        || line.starts_with("class ")
        || line.starts_with("interface ")
        || line.starts_with("export function ")
        || line.starts_with("export class ")
        || line.starts_with("export interface ")
        || call == "require"
}
