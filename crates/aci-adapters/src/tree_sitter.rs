use std::path::{Path, PathBuf};
use tree_sitter::{Language as TreeSitterLanguage, Parser, Query, Tree};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuerySource {
    pub name: &'static str,
    pub path: PathBuf,
    pub source: &'static str,
}

impl QuerySource {
    pub fn new(name: &'static str, path: impl Into<PathBuf>, source: &'static str) -> Self {
        Self {
            name,
            path: path.into(),
            source,
        }
    }
}

pub fn parse_source(language: &TreeSitterLanguage, source: &str) -> Result<Tree, String> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|error| error.to_string())?;
    parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter parser did not return a tree".to_string())
}

pub fn compile_query(language: &TreeSitterLanguage, source: &str) -> Result<Query, String> {
    Query::new(language, source).map_err(|error| error.to_string())
}

pub fn query_sources_for(language_dir: &Path) -> Vec<QuerySource> {
    ["symbols.scm", "imports.scm", "calls.scm"]
        .into_iter()
        .map(|name| QuerySource::new(name, language_dir.join("queries").join(name), ""))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_standard_query_sources() {
        let sources = query_sources_for(Path::new("languages/python"));
        assert_eq!(sources.len(), 3);
        assert!(sources.iter().any(|source| source.name == "symbols.scm"));
        assert!(
            sources
                .iter()
                .any(|source| source.path.ends_with("queries/calls.scm"))
        );
    }
}
