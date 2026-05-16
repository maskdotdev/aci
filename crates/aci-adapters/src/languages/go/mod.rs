mod detect;
mod extract;
mod resolve;

pub use detect::detect_go;
pub use extract::{extract_go, extract_go_with_options};
pub use resolve::resolve_partition;

use crate::{ExtractionOptions, LanguageAdapter};
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct GoAdapter;

impl LanguageAdapter for GoAdapter {
    fn language(&self) -> Language {
        Language::Go
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_go(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_go(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_go(file)
    }

    fn extract_with_options(
        &self,
        file: &SourceFile,
        options: ExtractionOptions,
    ) -> GraphPartition {
        extract::extract_go_with_options(file, options)
    }
}
