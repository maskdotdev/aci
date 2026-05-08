mod detect;
mod extract;
mod resolve;

pub use detect::detect_cpp;
pub use extract::extract_cpp;
pub use resolve::resolve_partition;

use crate::LanguageAdapter;
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct CppAdapter;

impl LanguageAdapter for CppAdapter {
    fn language(&self) -> Language {
        Language::Cpp
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_cpp(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_cpp(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_cpp(file)
    }
}
