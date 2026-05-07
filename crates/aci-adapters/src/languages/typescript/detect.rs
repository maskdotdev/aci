use std::path::Path;

pub fn path_might_be_typescript(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    matches!(extension, "ts" | "tsx" | "mts" | "cts") || path.extension().is_none()
}

pub fn detect_typescript(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "ts" | "tsx" | "mts" | "cts") {
        return true;
    }
    detect_node_shebang(bytes)
}

pub fn path_might_be_javascript(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "js" | "jsx" | "mjs" | "cjs") {
        return true;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    matches!(filename, "vite.config.js") || path.extension().is_none()
}

pub fn detect_javascript(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "js" | "jsx" | "mjs" | "cjs") {
        return true;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if matches!(filename, "vite.config.js") {
        return true;
    }
    detect_node_shebang(bytes)
}

fn detect_node_shebang(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.lines().next())
        .is_some_and(|line| line.starts_with("#!") && line.contains("node"))
}
