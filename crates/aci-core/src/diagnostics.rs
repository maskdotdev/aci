use crate::FileId;
use crate::SourceSpan;
use serde::{Deserialize, Serialize};

/// Warning or error produced while indexing a file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub file_id: Option<FileId>,
    pub span: Option<SourceSpan>,
}

impl Diagnostic {
    /// Creates a warning diagnostic.
    pub fn warning(
        message: impl Into<String>,
        file_id: Option<FileId>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            file_id,
            span,
        }
    }
}

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Error type shared by ACI library crates.
#[derive(Debug, thiserror::Error)]
pub enum AciError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

/// Result alias used by ACI library crates.
pub type Result<T> = std::result::Result<T, AciError>;
