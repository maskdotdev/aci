mod detect;
mod extract;
mod resolve;

pub use detect::detect_python;
pub use extract::extract_python;
pub use resolve::resolve_partition;

use crate::LanguageAdapter;
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn language(&self) -> Language {
        Language::Python
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_python(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_python(file)
    }
}
