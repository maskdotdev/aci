use std::path::Path;

pub fn path_might_be_go(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("go") || path.extension().is_none()
}

pub fn detect_go(path: &Path, _bytes: &[u8]) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("go")
}
