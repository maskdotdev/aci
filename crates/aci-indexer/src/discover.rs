use aci_core::Result;
use ignore::WalkBuilder;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn discover_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkBuilder::new(root).standard_filters(true).build() {
        let entry = entry.map_err(|error| aci_core::AciError::Message(error.to_string()))?;
        let path = entry.path();
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0) || std::str::from_utf8(bytes).is_err()
}

pub fn is_vendor_or_generated(path: &Path) -> bool {
    let vendor_dir = path.components().any(|component| {
        let value = component.as_os_str();
        matches!(
            value.to_str(),
            Some(
                "node_modules"
                    | ".git"
                    | "target"
                    | "dist"
                    | "build"
                    | "third_party"
                    | "vendor"
                    | ".venv"
                    | "__pycache__"
            )
        )
    });
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
    vendor_dir
        || name.ends_with(".min.js")
        || name.ends_with(".generated.ts")
        || name.ends_with(".generated.py")
        || name.ends_with(".pb.go")
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
