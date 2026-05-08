use aci_core::{Diagnostic, FileId, LineColumn, SourceSpan};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tree_sitter::{
    Language as TreeSitterLanguage, Node, ParseOptions, ParseState, Parser, Query, Tree,
};

pub const DEFAULT_MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_MAX_PARSE_DIAGNOSTICS: usize = 4;
pub const DEFAULT_PARSE_TIMEOUT: Duration = Duration::from_millis(250);
static EXTRACTION_MODE_OVERRIDE: AtomicU8 = AtomicU8::new(0);
static ENV_EXTRACTION_MODE: OnceLock<ExtractionMode> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtractionMode {
    ScannerOnly,
    TreeSitterOnly,
    TreeSitterWithFallback,
    TreeSitterWithEnrichment,
}

impl ExtractionMode {
    pub fn current() -> Self {
        match EXTRACTION_MODE_OVERRIDE.load(Ordering::Relaxed) {
            1 => return Self::ScannerOnly,
            2 => return Self::TreeSitterOnly,
            3 => return Self::TreeSitterWithFallback,
            4 => return Self::TreeSitterWithEnrichment,
            _ => {}
        }
        *ENV_EXTRACTION_MODE.get_or_init(Self::from_env)
    }

    fn from_env() -> Self {
        match std::env::var("ACI_EXTRACTION_MODE").as_deref() {
            Ok("scanner-only") => Self::ScannerOnly,
            Ok("tree-sitter-only") => Self::TreeSitterOnly,
            Ok("tree-sitter-enrichment") => Self::TreeSitterWithEnrichment,
            _ => Self::TreeSitterWithFallback,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScannerOnly => "scanner-only",
            Self::TreeSitterOnly => "tree-sitter-only",
            Self::TreeSitterWithFallback => "tree-sitter-fallback",
            Self::TreeSitterWithEnrichment => "tree-sitter-enrichment",
        }
    }
}

pub fn set_extraction_mode(mode: ExtractionMode) {
    let value = match mode {
        ExtractionMode::ScannerOnly => 1,
        ExtractionMode::TreeSitterOnly => 2,
        ExtractionMode::TreeSitterWithFallback => 3,
        ExtractionMode::TreeSitterWithEnrichment => 4,
    };
    EXTRACTION_MODE_OVERRIDE.store(value, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug)]
pub struct ParseLimits {
    pub max_file_bytes: usize,
    pub max_parse_diagnostics: usize,
    pub parse_timeout: Duration,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_parse_diagnostics: DEFAULT_MAX_PARSE_DIAGNOSTICS,
            parse_timeout: DEFAULT_PARSE_TIMEOUT,
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
        let started = Instant::now();
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut read = |offset, _| {
            if offset < len {
                &bytes[offset..]
            } else {
                Default::default()
            }
        };
        let mut should_cancel = |_: &ParseState| timeout_exceeded(started, limits.parse_timeout);
        let parsed = parser.parse_with_options(
            &mut read,
            None,
            Some(ParseOptions::new().progress_callback(&mut should_cancel)),
        );
        let parse_time = started.elapsed();

        match parsed {
            Some(tree) => {
                let diagnostics = parse_diagnostics(
                    tree.root_node(),
                    source,
                    file_id,
                    limits.max_parse_diagnostics,
                );
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

fn timeout_exceeded(started: Instant, timeout: Duration) -> bool {
    !timeout.is_zero() && started.elapsed() >= timeout
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

pub fn c_language() -> TreeSitterLanguage {
    tree_sitter_c::LANGUAGE.into()
}

pub fn cpp_language() -> TreeSitterLanguage {
    tree_sitter_cpp::LANGUAGE.into()
}

pub fn go_language() -> TreeSitterLanguage {
    tree_sitter_go::LANGUAGE.into()
}

pub fn java_language() -> TreeSitterLanguage {
    tree_sitter_java::LANGUAGE.into()
}

pub fn objective_c_language() -> TreeSitterLanguage {
    tree_sitter_objc::LANGUAGE.into()
}

pub fn rust_language() -> TreeSitterLanguage {
    tree_sitter_rust::LANGUAGE.into()
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

fn parse_diagnostics(
    root: Node<'_>,
    source: &str,
    file_id: &FileId,
    max_diagnostics: usize,
) -> Vec<Diagnostic> {
    if !root.has_error() {
        return Vec::new();
    }
    if max_diagnostics == 0 {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();
    collect_error_nodes(root, source, file_id, max_diagnostics, &mut diagnostics);
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
    max_diagnostics: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if diagnostics.len() >= max_diagnostics {
        return;
    }
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
        if diagnostics.len() >= max_diagnostics {
            break;
        }
        if child.has_error() || child.is_error() || child.is_missing() {
            collect_error_nodes(child, source, file_id, max_diagnostics, diagnostics);
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

    #[test]
    fn parser_reports_size_guardrail_without_panicking() {
        let pool = ParserPool::new(python_language());
        let repo = aci_core::RepositoryId::new("repo", &["tree-sitter-guardrail"]);
        let file_id = aci_core::FileId::new("file", &[repo.as_str(), "large.py", "python"]);
        let limits = ParseLimits {
            max_file_bytes: 4,
            ..ParseLimits::default()
        };
        let skip = pool
            .parse("def main():\n    pass\n", &file_id, limits)
            .expect_err("large input should be skipped");
        assert!(matches!(skip, ParseSkip::TooLarge { .. }));
    }

    #[test]
    fn parser_caps_parse_diagnostics() {
        let pool = ParserPool::new(python_language());
        let repo = aci_core::RepositoryId::new("repo", &["tree-sitter-diagnostics"]);
        let file_id = aci_core::FileId::new("file", &[repo.as_str(), "bad.py", "python"]);
        let limits = ParseLimits {
            max_parse_diagnostics: 2,
            ..ParseLimits::default()
        };

        let report = pool
            .parse(
                "def one(:\n    pass\ndef two(:\n    pass\n",
                &file_id,
                limits,
            )
            .expect("parse with recoverable errors");

        assert!(report.tree.root_node().has_error());
        assert!(report.diagnostics.len() <= 2);
    }
}
