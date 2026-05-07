use std::path::Path;

pub fn detect_typescript(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "ts" | "tsx" | "mts" | "cts") {
        return true;
    }
    if matches!(extension, "js" | "jsx" | "mjs" | "cjs") {
        return true;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if matches!(
        filename,
        "package.json" | "vite.config.js" | "vite.config.ts"
    ) {
        return true;
    }
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.lines().next())
        .is_some_and(|line| line.starts_with("#!") && line.contains("node"))
}
