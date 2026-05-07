mod detect;
mod extract;
mod resolve;

pub use detect::{detect_javascript, detect_typescript};
pub use extract::extract_typescript;
pub use resolve::resolve_partition;

use crate::LanguageAdapter;
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct TypeScriptAdapter;
pub struct JavaScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_typescript(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_typescript(file)
    }
}

impl LanguageAdapter for JavaScriptAdapter {
    fn language(&self) -> Language {
        Language::JavaScript
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_javascript(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_typescript(file)
    }
}
