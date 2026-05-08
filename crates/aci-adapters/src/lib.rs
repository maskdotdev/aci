use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub mod helpers;
pub mod languages;
pub mod tree_sitter;

pub trait LanguageAdapter: Send + Sync {
    fn language(&self) -> Language;
    fn path_candidate(&self, _path: &Path) -> bool {
        true
    }
    fn detect(&self, path: &Path, bytes: &[u8]) -> bool;
    fn extract(&self, file: &SourceFile) -> GraphPartition;
}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        Self::new()
            .register(languages::objective_c::ObjectiveCAdapter)
            .register(languages::cpp::CppAdapter)
            .register(languages::c::CAdapter)
            .register(languages::go::GoAdapter)
            .register(languages::java::JavaAdapter)
            .register(languages::json::JsonAdapter)
            .register(languages::rust::RustAdapter)
            .register(languages::typescript::JavaScriptAdapter)
            .register(languages::typescript::TypeScriptAdapter)
            .register(languages::python::PythonAdapter)
    }

    pub fn register(mut self, adapter: impl LanguageAdapter + 'static) -> Self {
        self.adapters.push(Box::new(adapter));
        self
    }

    pub fn detect_language(&self, path: &Path, bytes: &[u8]) -> Language {
        self.adapters
            .iter()
            .find(|adapter| adapter.detect(path, bytes))
            .map(|adapter| adapter.language())
            .unwrap_or(Language::Unknown)
    }

    pub fn path_candidate(&self, path: &Path) -> bool {
        self.adapters
            .iter()
            .any(|adapter| adapter.path_candidate(path))
    }

    pub fn extract(&self, file: &SourceFile) -> GraphPartition {
        self.adapters
            .iter()
            .find(|adapter| adapter.language() == file.language)
            .map(|adapter| adapter.extract(file))
            .unwrap_or_else(|| GraphPartition::empty(file))
    }
}

pub fn default_registry() -> AdapterRegistry {
    AdapterRegistry::with_defaults()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aci_core::{EdgeKind, FactProvenance, NodeKind, RepositoryId, SourceFile};
    use std::path::{Path, PathBuf};

    #[test]
    fn registry_detects_supported_languages() {
        let registry = default_registry();
        assert_eq!(
            registry.detect_language(Path::new("src/app.ts"), b"export function app() {}"),
            Language::TypeScript
        );
        assert_eq!(
            registry.detect_language(Path::new("tools/build.py"), b"#!/usr/bin/env python\n"),
            Language::Python
        );
        assert_eq!(
            registry.detect_language(Path::new("crates/app/src/lib.rs"), b"pub fn app() {}"),
            Language::Rust
        );
        assert_eq!(
            registry.detect_language(Path::new("package.json"), br#"{ "name": "app" }"#),
            Language::Json
        );
        assert_eq!(
            registry.detect_language(Path::new("src/app.c"), b"int main(void) { return 0; }"),
            Language::C
        );
        assert_eq!(
            registry.detect_language(Path::new("src/app.cpp"), b"namespace app {}"),
            Language::Cpp
        );
        assert_eq!(
            registry.detect_language(Path::new("src/App.java"), b"public class App {}"),
            Language::Java
        );
        assert_eq!(
            registry.detect_language(Path::new("src/app.m"), b"@interface App\n@end\n"),
            Language::ObjectiveC
        );
        assert_eq!(
            registry.detect_language(Path::new("src/app.go"), b"package app\n"),
            Language::Go
        );
    }

    #[test]
    fn registry_prefilters_obvious_unsupported_paths() {
        let registry = default_registry();
        assert!(registry.path_candidate(Path::new("src/app.ts")));
        assert!(registry.path_candidate(Path::new("tools/build.py")));
        assert!(registry.path_candidate(Path::new("crates/app/src/lib.rs")));
        assert!(registry.path_candidate(Path::new("package.json")));
        assert!(registry.path_candidate(Path::new("scripts/run")));
        assert!(registry.path_candidate(Path::new("src/app.cc")));
        assert!(registry.path_candidate(Path::new("src/app.java")));
        assert!(registry.path_candidate(Path::new("src/app.m")));
        assert!(registry.path_candidate(Path::new("src/app.go")));
        assert!(!registry.path_candidate(Path::new("assets/logo.png")));
    }

    #[test]
    fn adapters_extract_fixture_shapes() {
        let registry = default_registry();
        let repo = RepositoryId::new("repo", &["adapter-fixtures"]);
        let fixtures = [
            (
                PathBuf::from("fixtures/typescript/coverage.ts"),
                include_str!("../fixtures/typescript/coverage.ts"),
                Language::TypeScript,
                ["Service", "run", "main", "local"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/python/coverage.py"),
                include_str!("../fixtures/python/coverage.py"),
                Language::Python,
                ["Service", "run", "main", "value"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/rust/coverage.rs"),
                include_str!("../fixtures/rust/coverage.rs"),
                Language::Rust,
                ["Service", "run", "Mode", "Runner", "main"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/json/package.json"),
                include_str!("../fixtures/json/package.json"),
                Language::Json,
                ["package.json", "fixture-package"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/c/coverage.c"),
                include_str!("../fixtures/c/coverage.c"),
                Language::C,
                ["Point", "add", "main"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/cpp/coverage.cpp"),
                include_str!("../fixtures/cpp/coverage.cpp"),
                Language::Cpp,
                ["demo", "Widget", "size", "make_widget"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/go/coverage.go"),
                include_str!("../fixtures/go/coverage.go"),
                Language::Go,
                ["demo", "Counter", "Add", "Inc"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/java/coverage.java"),
                include_str!("../fixtures/java/coverage.java"),
                Language::Java,
                ["demo", "Widget", "size"].as_slice(),
            ),
            (
                PathBuf::from("fixtures/objective_c/coverage.m"),
                include_str!("../fixtures/objective_c/coverage.m"),
                Language::ObjectiveC,
                ["Widget", "run"].as_slice(),
            ),
        ];

        for (path, text, language, expected_symbols) in fixtures {
            let file = SourceFile::new(
                repo.clone(),
                Path::new("."),
                path,
                language,
                text.to_string(),
            );
            let partition = registry.extract(&file);
            for expected in expected_symbols {
                assert!(
                    partition
                        .nodes
                        .iter()
                        .any(|node| node.name.as_deref() == Some(*expected)),
                    "missing {expected} in {language:?}"
                );
            }
        }
    }

    #[test]
    fn adapters_recover_from_syntax_error_fixtures() {
        let registry = default_registry();
        let repo = RepositoryId::new("repo", &["syntax-fixtures"]);
        for (path, text, language) in [
            (
                PathBuf::from("fixtures/typescript/syntax-error.ts"),
                include_str!("../fixtures/typescript/syntax-error.ts"),
                Language::TypeScript,
            ),
            (
                PathBuf::from("fixtures/python/syntax-error.py"),
                include_str!("../fixtures/python/syntax-error.py"),
                Language::Python,
            ),
            (
                PathBuf::from("fixtures/rust/syntax-error.rs"),
                include_str!("../fixtures/rust/syntax-error.rs"),
                Language::Rust,
            ),
            (
                PathBuf::from("fixtures/c/syntax-error.c"),
                include_str!("../fixtures/c/syntax-error.c"),
                Language::C,
            ),
            (
                PathBuf::from("fixtures/cpp/syntax-error.cpp"),
                include_str!("../fixtures/cpp/syntax-error.cpp"),
                Language::Cpp,
            ),
            (
                PathBuf::from("fixtures/go/syntax-error.go"),
                include_str!("../fixtures/go/syntax-error.go"),
                Language::Go,
            ),
            (
                PathBuf::from("fixtures/java/syntax-error.java"),
                include_str!("../fixtures/java/syntax-error.java"),
                Language::Java,
            ),
            (
                PathBuf::from("fixtures/objective_c/syntax-error.m"),
                include_str!("../fixtures/objective_c/syntax-error.m"),
                Language::ObjectiveC,
            ),
        ] {
            let file = SourceFile::new(
                repo.clone(),
                Path::new("."),
                path,
                language,
                text.to_string(),
            );
            let partition = registry.extract(&file);
            assert!(!partition.nodes.is_empty());
        }
    }

    #[test]
    fn adapters_handle_large_files() {
        let registry = default_registry();
        let repo = RepositoryId::new("repo", &["large-fixture"]);
        let text = include_str!("../fixtures/python/large.py").repeat(400);
        let file = SourceFile::new(
            repo,
            Path::new("."),
            PathBuf::from("fixtures/python/large.py"),
            Language::Python,
            text,
        );
        let partition = registry.extract(&file);
        assert!(partition.nodes.len() >= 400);
    }

    #[test]
    fn tree_sitter_queries_compile_for_supported_grammars() {
        let python = tree_sitter::python_language();
        tree_sitter::validate_queries(
            &python,
            &[
                tree_sitter::QuerySource::new(
                    "symbols.scm",
                    "languages/python/queries/symbols.scm",
                    include_str!("languages/python/queries/symbols.scm"),
                ),
                tree_sitter::QuerySource::new(
                    "imports.scm",
                    "languages/python/queries/imports.scm",
                    include_str!("languages/python/queries/imports.scm"),
                ),
                tree_sitter::QuerySource::new(
                    "calls.scm",
                    "languages/python/queries/calls.scm",
                    include_str!("languages/python/queries/calls.scm"),
                ),
            ],
        )
        .expect("python queries compile");

        for language in [
            tree_sitter::typescript_language(),
            tree_sitter::tsx_language(),
        ] {
            tree_sitter::validate_queries(
                &language,
                &[
                    tree_sitter::QuerySource::new(
                        "symbols.scm",
                        "languages/typescript/queries/symbols.scm",
                        include_str!("languages/typescript/queries/symbols.scm"),
                    ),
                    tree_sitter::QuerySource::new(
                        "imports.scm",
                        "languages/typescript/queries/imports.scm",
                        include_str!("languages/typescript/queries/imports.scm"),
                    ),
                    tree_sitter::QuerySource::new(
                        "calls.scm",
                        "languages/typescript/queries/calls.scm",
                        include_str!("languages/typescript/queries/calls.scm"),
                    ),
                ],
            )
            .expect("typescript queries compile");
        }

        let rust = tree_sitter::rust_language();
        tree_sitter::validate_queries(
            &rust,
            &[
                tree_sitter::QuerySource::new(
                    "symbols.scm",
                    "languages/rust/queries/symbols.scm",
                    include_str!("languages/rust/queries/symbols.scm"),
                ),
                tree_sitter::QuerySource::new(
                    "imports.scm",
                    "languages/rust/queries/imports.scm",
                    include_str!("languages/rust/queries/imports.scm"),
                ),
                tree_sitter::QuerySource::new(
                    "calls.scm",
                    "languages/rust/queries/calls.scm",
                    include_str!("languages/rust/queries/calls.scm"),
                ),
            ],
        )
        .expect("rust queries compile");
    }

    #[test]
    fn python_tree_sitter_golden_graph_is_stable() {
        let repo = RepositoryId::new("repo", &["python-golden"]);
        let file = SourceFile::new(
            repo,
            Path::new("."),
            PathBuf::from("fixtures/python/coverage.py"),
            Language::Python,
            include_str!("../fixtures/python/coverage.py").to_string(),
        );
        let left = languages::python::extract_python(&file);
        let right = languages::python::extract_python(&file);

        assert_eq!(
            symbol_qualified_names(&left),
            symbol_qualified_names(&right)
        );
        assert_eq!(
            symbol_qualified_names(&left),
            vec![
                "coverage",
                "coverage.Service",
                "coverage.Service.run",
                "coverage.main",
                "coverage.value",
            ]
        );
        assert!(
            left.nodes
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::Symbol | NodeKind::Import))
                .all(|node| node.provenance == FactProvenance::TreeSitter)
        );
        assert!(left.edges.iter().any(|edge| edge.kind == EdgeKind::Calls));
    }

    #[test]
    fn typescript_tree_sitter_golden_graph_is_stable() {
        let repo = RepositoryId::new("repo", &["typescript-golden"]);
        let file = SourceFile::new(
            repo,
            Path::new("."),
            PathBuf::from("fixtures/typescript/coverage.ts"),
            Language::TypeScript,
            include_str!("../fixtures/typescript/coverage.ts").to_string(),
        );
        let left = languages::typescript::extract_typescript(&file);
        let right = languages::typescript::extract_typescript(&file);

        assert_eq!(
            symbol_qualified_names(&left),
            symbol_qualified_names(&right)
        );
        assert_eq!(
            symbol_qualified_names(&left),
            vec![
                "coverage",
                "coverage.Service",
                "coverage.Service.run",
                "coverage.local",
                "coverage.main",
            ]
        );
        assert!(
            left.nodes
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::Symbol | NodeKind::Import))
                .all(|node| node.provenance == FactProvenance::TreeSitter)
        );
        assert!(left.edges.iter().any(|edge| edge.kind == EdgeKind::Exports));
    }

    #[test]
    fn rust_tree_sitter_golden_graph_is_stable() {
        let repo = RepositoryId::new("repo", &["rust-golden"]);
        let file = SourceFile::new(
            repo,
            Path::new("."),
            PathBuf::from("fixtures/rust/coverage.rs"),
            Language::Rust,
            include_str!("../fixtures/rust/coverage.rs").to_string(),
        );
        let left = languages::rust::extract_rust(&file);
        let right = languages::rust::extract_rust(&file);

        assert_eq!(
            symbol_qualified_names(&left),
            symbol_qualified_names(&right)
        );
        assert!(
            symbol_qualified_names(&left)
                .iter()
                .any(|name| *name == "coverage::Service")
        );
        assert!(left.edges.iter().any(|edge| edge.kind == EdgeKind::Calls));
    }

    #[test]
    fn json_package_dependencies_are_indexed() {
        let repo = RepositoryId::new("repo", &["json-golden"]);
        let file = SourceFile::new(
            repo,
            Path::new("."),
            PathBuf::from("fixtures/json/package.json"),
            Language::Json,
            include_str!("../fixtures/json/package.json").to_string(),
        );
        let partition = languages::json::extract_json(&file);
        assert!(
            partition
                .nodes
                .iter()
                .any(|node| node.name.as_deref() == Some("react"))
        );
        assert!(
            partition
                .edges
                .iter()
                .any(|edge| edge.kind == EdgeKind::DependsOn)
        );
    }

    fn symbol_qualified_names(partition: &GraphPartition) -> Vec<&str> {
        let mut names = partition
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Symbol)
            .filter_map(|node| node.qualified_name.as_deref())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }
}
