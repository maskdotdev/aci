use std::path::Path;

pub fn path_might_be_c(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    matches!(extension, "c" | "h") || path.extension().is_none()
}

pub fn detect_c(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    match extension {
        "c" => true,
        "h" => !looks_like_cpp_or_objective_c(bytes),
        _ => false,
    }
}

fn looks_like_cpp_or_objective_c(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    text.contains("@interface")
        || text.contains("@protocol")
        || text.contains("namespace ")
        || text.contains("template <")
        || text.contains("class ")
}
