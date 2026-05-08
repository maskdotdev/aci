use std::path::Path;

pub fn path_might_be_java(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("java") || path.extension().is_none()
}

pub fn detect_java(path: &Path, _bytes: &[u8]) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("java")
}
