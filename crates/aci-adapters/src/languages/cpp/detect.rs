use std::path::Path;

pub fn path_might_be_cpp(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    matches!(
        extension,
        "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "h++" | "ipp" | "h"
    ) || path.extension().is_none()
}

pub fn detect_cpp(path: &Path, bytes: &[u8]) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if matches!(
        extension,
        "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "h++" | "ipp"
    ) {
        return true;
    }
    extension == "h" && looks_like_cpp(bytes)
}

fn looks_like_cpp(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| {
        text.contains("namespace ") || text.contains("template <") || text.contains("class ")
    })
}
