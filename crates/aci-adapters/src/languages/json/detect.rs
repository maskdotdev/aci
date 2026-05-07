use std::path::Path;

pub fn detect_json(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "json" | "webmanifest") {
        return true;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    matches!(filename, "package-lock.json" | "tsconfig")
        && std::str::from_utf8(bytes)
            .ok()
            .map(str::trim_start)
            .is_some_and(|text| text.starts_with('{'))
}
