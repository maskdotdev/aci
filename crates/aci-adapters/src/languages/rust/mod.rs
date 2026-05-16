mod detect;
mod extract;
mod resolve;

pub use detect::detect_rust;
pub use extract::{extract_rust, extract_rust_with_options};
pub use resolve::resolve_partition;

use crate::{ExtractionOptions, LanguageAdapter};
use aci_core::{GraphPartition, Language, SourceFile};
use std::path::Path;

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn path_candidate(&self, path: &Path) -> bool {
        detect::path_might_be_rust(path)
    }

    fn detect(&self, path: &Path, bytes: &[u8]) -> bool {
        detect::detect_rust(path, bytes)
    }

    fn extract(&self, file: &SourceFile) -> GraphPartition {
        extract::extract_rust(file)
    }

    fn extract_with_options(
        &self,
        file: &SourceFile,
        options: ExtractionOptions,
    ) -> GraphPartition {
        extract::extract_rust_with_options(file, options)
    }
}
