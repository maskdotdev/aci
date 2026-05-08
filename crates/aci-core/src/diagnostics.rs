use crate::FileId;
use crate::SourceSpan;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub file_id: Option<FileId>,
    pub span: Option<SourceSpan>,
}

impl Diagnostic {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, thiserror::Error)]
pub enum AciError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, AciError>;
