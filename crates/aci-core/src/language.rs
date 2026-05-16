use serde::{Deserialize, Serialize};

/// Source language detected for a file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    C,
    Cpp,
    Go,
    JavaScript,
    Json,
    Java,
    ObjectiveC,
    TypeScript,
    Python,
    Rust,
    Unknown,
}

impl Language {
    /// Returns the stable lowercase key for this language.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Go => "go",
            Self::JavaScript => "javascript",
            Self::Json => "json",
            Self::Java => "java",
            Self::ObjectiveC => "objective-c",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Unknown => "unknown",
        }
    }
}
