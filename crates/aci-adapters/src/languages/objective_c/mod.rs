mod detect;
mod extract;
mod resolve;

pub use detect::detect_objective_c;
pub use extract::{extract_objective_c, extract_objective_c_with_options};
pub use resolve::resolve_partition;

use crate::{ExtractionOptions, LanguageAdapter};
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct ObjectiveCAdapter;

impl LanguageAdapter for ObjectiveCAdapter {
    fn language(&self) -> Language {
        Language::ObjectiveC
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_objective_c(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_objective_c(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_objective_c(file)
    }

    fn extract_with_options(
        &self,
        file: &SourceFile,
        options: ExtractionOptions,
    ) -> GraphPartition {
        extract::extract_objective_c_with_options(file, options)
    }
}
