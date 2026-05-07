use std::path::Path;

pub fn path_might_be_python(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "py" | "pyw") {
        return true;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    matches!(filename, "SConstruct" | "SConscript") || path.extension().is_none()
}

pub fn detect_python(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "py" | "pyw") {
        return true;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if matches!(filename, "SConstruct" | "SConscript") {
        return true;
    }
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.lines().next())
        .is_some_and(|line| line.starts_with("#!") && line.contains("python"))
}
