mod detect;
mod extract;
mod resolve;

pub use detect::detect_java;
pub use extract::{extract_java, extract_java_with_options};
pub use resolve::resolve_partition;

use crate::{ExtractionOptions, LanguageAdapter};
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct JavaAdapter;

impl LanguageAdapter for JavaAdapter {
    fn language(&self) -> Language {
        Language::Java
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_java(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_java(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_java(file)
    }

    fn extract_with_options(
        &self,
        file: &SourceFile,
        options: ExtractionOptions,
    ) -> GraphPartition {
        extract::extract_java_with_options(file, options)
    }
}
