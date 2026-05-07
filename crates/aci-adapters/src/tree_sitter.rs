use aci_core::{Diagnostic, FileId, LineColumn, SourceSpan};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tree_sitter::{
    Language as TreeSitterLanguage, Node, Parser, Query, QueryCursor, StreamingIterator, Tree,
};

pub const DEFAULT_MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_MAX_QUERY_CAPTURES: usize = 100_000;
pub const DEFAULT_PARSE_TIMEOUT: Duration = Duration::from_millis(250);
pub const DEFAULT_QUERY_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtractionMode {
    ScannerOnly,
    TreeSitterOnly,
    TreeSitterWithFallback,
    TreeSitterWithEnrichment,
}

impl ExtractionMode {
    pub fn from_env() -> Self {
        match std::env::var("ACI_EXTRACTION_MODE").as_deref() {
            Ok("scanner-only") => Self::ScannerOnly,
            Ok("tree-sitter-only") => Self::TreeSitterOnly,
            Ok("tree-sitter-enrichment") => Self::TreeSitterWithEnrichment,
            _ => Self::TreeSitterWithFallback,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParseLimits {
    pub max_file_bytes: usize,
    pub max_query_captures: usize,
    pub parse_timeout: Duration,
    pub query_timeout: Duration,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_query_captures: DEFAULT_MAX_QUERY_CAPTURES,
            parse_timeout: DEFAULT_PARSE_TIMEOUT,
            query_timeout: DEFAULT_QUERY_TIMEOUT,
        }
    }
}

#[derive(Debug)]
pub struct ParseReport {
    pub tree: Tree,
    pub diagnostics: Vec<Diagnostic>,
    pub parse_time: Duration,
}

#[derive(Debug)]
pub enum ParseSkip {
    TooLarge { bytes: usize, limit: usize },
    Timeout,
    Parser(String),
}

pub struct ParserPool {
    language: TreeSitterLanguage,
    parsers: Mutex<Vec<Parser>>,
}

impl ParserPool {
    pub fn new(language: TreeSitterLanguage) -> Self {
        Self {
            language,
            parsers: Mutex::new(Vec::new()),
        }
    }

    pub fn parse(
        &self,
        source: &str,
        file_id: &FileId,
        limits: ParseLimits,
    ) -> Result<ParseReport, ParseSkip> {
        if source.len() > limits.max_file_bytes {
            return Err(ParseSkip::TooLarge {
                bytes: source.len(),
                limit: limits.max_file_bytes,
            });
        }

        let mut parser = self.take_parser()?;
        #[allow(deprecated)]
        parser.set_timeout_micros(limits.parse_timeout.as_micros() as u64);

        let started = Instant::now();
        let parsed = parser.parse(source, None);
        let parse_time = started.elapsed();
        #[allow(deprecated)]
        parser.set_timeout_micros(0);

        match parsed {
            Some(tree) => {
                let diagnostics = parse_diagnostics(tree.root_node(), source, file_id);
                self.return_parser(parser);
                Ok(ParseReport {
                    tree,
                    diagnostics,
                    parse_time,
                })
            }
            None => {
                parser.reset();
                self.return_parser(parser);
                Err(ParseSkip::Timeout)
            }
        }
    }

    fn take_parser(&self) -> Result<Parser, ParseSkip> {
        let mut parser = self
            .parsers
            .lock()
            .map_err(|error| ParseSkip::Parser(error.to_string()))?
            .pop()
            .unwrap_or_else(Parser::new);
        parser
            .set_language(&self.language)
            .map_err(|error| ParseSkip::Parser(error.to_string()))?;
        Ok(parser)
    }

    fn return_parser(&self, parser: Parser) {
        if let Ok(mut parsers) = self.parsers.lock() {
            parsers.push(parser);
        }
    }
}

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

pub fn compile_query(language: &TreeSitterLanguage, source: &str) -> Result<Query, String> {
    Query::new(language, source).map_err(|error| error.to_string())
}

pub fn validate_queries(
    language: &TreeSitterLanguage,
    sources: &[QuerySource],
) -> Result<Vec<Query>, String> {
    sources
        .iter()
        .map(|source| {
            compile_query(language, source.source)
                .map_err(|error| format!("{}: {error}", source.path.display()))
        })
        .collect()
}

pub fn count_query_captures(
    query: &Query,
    root: Node<'_>,
    source: &str,
    limits: ParseLimits,
) -> Result<usize, String> {
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(limits.max_query_captures as u32);
    #[allow(deprecated)]
    cursor.set_timeout_micros(limits.query_timeout.as_micros() as u64);
    let mut captures = cursor.captures(query, root, source.as_bytes());
    let mut count = 0_usize;
    while let Some((_capture, _index)) = captures.next() {
        count += 1;
        if count > limits.max_query_captures {
            return Err(format!(
                "tree-sitter query capture limit exceeded: {count} > {}",
                limits.max_query_captures
            ));
        }
    }
    drop(captures);
    if cursor.did_exceed_match_limit() {
        return Err("tree-sitter query match limit exceeded".to_string());
    }
    Ok(count)
}

pub fn node_span(node: Node<'_>) -> SourceSpan {
    let range = node.range();
    SourceSpan::new(
        range.start_byte as u32,
        range.end_byte as u32,
        LineColumn::new(
            range.start_point.row as u32 + 1,
            range.start_point.column as u32 + 1,
        ),
        LineColumn::new(
            range.end_point.row as u32 + 1,
            range.end_point.column as u32 + 1,
        ),
    )
}

pub fn node_text<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    node.utf8_text(source.as_bytes()).ok()
}

pub fn child_by_field_name<'tree>(node: Node<'tree>, field: &str) -> Option<Node<'tree>> {
    node.child_by_field_name(field)
}

pub fn query_sources_for(language_dir: &Path) -> Vec<QuerySource> {
    ["symbols.scm", "imports.scm", "calls.scm"]
        .into_iter()
        .map(|name| QuerySource::new(name, language_dir.join("queries").join(name), ""))
        .collect()
}

pub fn python_language() -> TreeSitterLanguage {
    tree_sitter_python::LANGUAGE.into()
}

pub fn javascript_language() -> TreeSitterLanguage {
    tree_sitter_javascript::LANGUAGE.into()
}

pub fn typescript_language() -> TreeSitterLanguage {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

pub fn tsx_language() -> TreeSitterLanguage {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

fn parse_diagnostics(root: Node<'_>, source: &str, file_id: &FileId) -> Vec<Diagnostic> {
    if !root.has_error() {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();
    collect_error_nodes(root, source, file_id, &mut diagnostics);
    if diagnostics.is_empty() {
        diagnostics.push(Diagnostic::warning(
            "tree-sitter parsed file with syntax errors",
            Some(file_id.clone()),
            Some(node_span(root)),
        ));
    }
    diagnostics
}

fn collect_error_nodes(
    node: Node<'_>,
    source: &str,
    file_id: &FileId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if node.is_error() || node.is_missing() {
        let label = if node.is_missing() {
            format!("missing {}", node.kind())
        } else {
            format!("syntax error near {}", snippet(node, source))
        };
        diagnostics.push(Diagnostic::warning(
            label,
            Some(file_id.clone()),
            Some(node_span(node)),
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.has_error() || child.is_error() || child.is_missing() {
            collect_error_nodes(child, source, file_id, diagnostics);
        }
    }
}

fn snippet(node: Node<'_>, source: &str) -> String {
    node_text(node, source)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(40).collect())
        .unwrap_or_else(|| node.kind().to_string())
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

    #[test]
    fn converts_node_range_to_one_based_span() {
        let pool = ParserPool::new(python_language());
        let repo = aci_core::RepositoryId::new("repo", &["tree-sitter-test"]);
        let file_id = aci_core::FileId::new("file", &[repo.as_str(), "a.py", "python"]);
        let report = pool
            .parse("def main():\n    pass\n", &file_id, ParseLimits::default())
            .expect("parse python");
        let root = report.tree.root_node();
        let span = node_span(root);
        assert_eq!(span.start, LineColumn::new(1, 1));
        assert_eq!(span.end.line, 3);
    }
}
