mod detect;
mod extract;
mod resolve;

pub use detect::detect_c;
pub use extract::{extract_c, extract_c_with_options};
pub use resolve::resolve_partition;

use crate::{ExtractionOptions, LanguageAdapter};
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct CAdapter;

impl LanguageAdapter for CAdapter {
    fn language(&self) -> Language {
        Language::C
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_c(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_c(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_c(file)
    }

    fn extract_with_options(
        &self,
        file: &SourceFile,
        options: ExtractionOptions,
    ) -> GraphPartition {
        extract::extract_c_with_options(file, options)
    }
}
