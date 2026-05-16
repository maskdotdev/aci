use aci_core::{GraphNode, Language, SymbolKind};
use std::cmp::Ordering;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SymbolKey {
    pub(crate) file: String,
    pub(crate) language: Language,
    pub(crate) kind: Option<SymbolKind>,
    pub(crate) scoped_name: String,
}

impl Ord for SymbolKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.file
            .cmp(&other.file)
            .then(self.language.cmp(&other.language))
            .then(kind_key(self.kind).cmp(kind_key(other.kind)))
            .then(self.scoped_name.cmp(&other.scoped_name))
    }
}

impl PartialOrd for SymbolKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) fn symbol_key(node: &GraphNode, comparison_file: String) -> SymbolKey {
    SymbolKey {
        file: comparison_file,
        language: node.language,
        kind: node.symbol_kind,
        scoped_name: scoped_name(node),
    }
}

fn scoped_name(node: &GraphNode) -> String {
    if node.symbol_kind == Some(SymbolKind::Module) {
        return "$module".to_string();
    }
    let display = node
        .qualified_name
        .as_deref()
        .or(node.name.as_deref())
        .unwrap_or_default();
    let simple = node.name.as_deref().unwrap_or(display);
    match strip_file_module(display) {
        Some("") => "$module".to_string(),
        Some(stripped) => stripped.to_string(),
        None => simple.to_string(),
    }
}

fn strip_file_module(qualified: &str) -> Option<&str> {
    let (module, rest) = qualified.split_once('.')?;
    if module.is_empty() || rest.is_empty() {
        return None;
    }
    Some(rest)
}

fn kind_key(kind: Option<SymbolKind>) -> &'static str {
    match kind {
        Some(SymbolKind::Function) => "function",
        Some(SymbolKind::Method) => "method",
        Some(SymbolKind::Class) => "class",
        Some(SymbolKind::Interface) => "interface",
        Some(SymbolKind::TypeAlias) => "type-alias",
        Some(SymbolKind::Enum) => "enum",
        Some(SymbolKind::Variable) => "variable",
        Some(SymbolKind::Module) => "module",
        Some(SymbolKind::Field) => "field",
        Some(SymbolKind::Unknown) => "unknown",
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aci_core::{
        Confidence, FactProvenance, GraphNode, Language, LineColumn, NodeKind, RepositoryId,
        SourceSpan, SymbolKind,
    };

    #[test]
    fn scoped_identity_keeps_owner_after_file_module_prefix() {
        let left = node("lib.A.run", "run", SymbolKind::Method);
        let right = node("lib.B.run", "run", SymbolKind::Method);

        assert_ne!(
            symbol_key(&left, "src/lib.ts".to_string()),
            symbol_key(&right, "src/lib.ts".to_string())
        );
    }

    #[test]
    fn module_identity_survives_file_rename() {
        let left = node("old", "old", SymbolKind::Module);
        let right = node("renamed", "renamed", SymbolKind::Module);

        assert_eq!(
            symbol_key(&left, "src/renamed.ts".to_string()).scoped_name,
            symbol_key(&right, "src/renamed.ts".to_string()).scoped_name
        );
    }

    fn node(qualified: &str, name: &str, kind: SymbolKind) -> GraphNode {
        GraphNode::deterministic(
            &RepositoryId::new("repo", &["fixture"]),
            None,
            NodeKind::Symbol,
            Language::TypeScript,
            Some(name.to_string()),
            Some(qualified.to_string()),
            Some(SourceSpan::new(
                0,
                1,
                LineColumn::new(1, 1),
                LineColumn::new(1, 2),
            )),
        )
        .with_symbol_kind(kind)
        .with_fact_quality(FactProvenance::TreeSitter, Confidence::High)
    }
}
