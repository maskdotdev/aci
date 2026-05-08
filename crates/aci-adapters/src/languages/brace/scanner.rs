use crate::helpers::PartitionBuilder;
use aci_core::{GraphPartition, SourceFile, SymbolKind};

use super::BraceLanguage;

pub(super) fn extract(file: &SourceFile, _config: &BraceLanguage) -> GraphPartition {
    let mut builder = PartitionBuilder::new(file);
    for (line_index, line) in file.text.lines().enumerate() {
        let trimmed = line.trim();
        let span = crate::helpers::line_span(&file.text, line_index);
        if let Some(import) = scanner_import(trimmed) {
            builder.add_import(import, span.clone());
        }
        if let Some(name) = scanner_type_name(trimmed) {
            builder.add_symbol(name, name, SymbolKind::Class, span.clone());
        }
        if let Some(name) = scanner_function_name(trimmed) {
            builder.add_symbol(name, name, SymbolKind::Function, span);
        }
    }
    super::resolve::partition(builder.finish())
}

fn scanner_import(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("#include") {
        return Some(rest.trim());
    }
    if let Some(rest) = line.strip_prefix("import ") {
        return Some(rest.trim().trim_end_matches(';').trim_matches('"'));
    }
    if let Some(rest) = line.strip_prefix("@import ") {
        return Some(rest.trim().trim_end_matches(';'));
    }
    None
}

fn scanner_type_name(line: &str) -> Option<&str> {
    for prefix in [
        "class ",
        "public class ",
        "interface ",
        "public interface ",
        "struct ",
        "enum ",
        "@interface ",
        "@implementation ",
        "@protocol ",
        "type ",
    ] {
        if let Some(name) = crate::helpers::first_identifier_after(line, prefix) {
            return Some(name);
        }
    }
    None
}

fn scanner_function_name(line: &str) -> Option<&str> {
    if line.ends_with(';') || !line.contains('(') {
        return None;
    }
    let before_paren = line.split_once('(')?.0.trim_end();
    before_paren
        .rsplit(|ch: char| !is_identifier_char(ch))
        .find(|part| !part.is_empty())
        .filter(|name| !matches!(*name, "if" | "for" | "while" | "switch" | "return"))
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}
