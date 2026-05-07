mod detect;
mod extract;

pub use detect::detect_json;
pub use extract::extract_json;

use crate::LanguageAdapter;
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct JsonAdapter;

impl LanguageAdapter for JsonAdapter {
    fn language(&self) -> Language {
        Language::Json
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_json(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_json(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_json(file)
    }
}
