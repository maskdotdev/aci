use std::path::Path;

pub fn detect_rust(path: &Path, bytes: &[u8]) -> bool {
    if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
        return true;
    }
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.lines().next())
        .is_some_and(|line| line.starts_with("#!") && line.contains("rust-script"))
}
