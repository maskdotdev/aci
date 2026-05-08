use std::path::Path;

pub fn path_might_be_objective_c(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    matches!(extension, "m" | "mm" | "h") || path.extension().is_none()
}

pub fn detect_objective_c(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(extension, "m" | "mm") {
        return true;
    }
    extension == "h" && looks_like_objective_c(bytes)
}

fn looks_like_objective_c(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| {
        text.contains("@interface")
            || text.contains("@implementation")
            || text.contains("@protocol")
    })
}
