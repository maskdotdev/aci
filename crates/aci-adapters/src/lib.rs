use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub mod helpers;
pub mod languages;
pub mod tree_sitter;

pub trait LanguageAdapter: Send + Sync {
    fn language(&self) -> Language;
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
    use aci_core::{RepositoryId, SourceFile};
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
}
