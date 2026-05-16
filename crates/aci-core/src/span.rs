use serde::{Deserialize, Serialize};

/// One-based source location.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LineColumn {
    pub line: u32,
    pub column: u32,
}

impl LineColumn {
    /// Creates a one-based source location.
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// Byte and line/column range for a source fact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub byte_start: u32,
    pub byte_end: u32,
    pub start: LineColumn,
    pub end: LineColumn,
}

impl SourceSpan {
    /// Creates a source span from byte offsets and one-based endpoints.
    pub fn new(byte_start: u32, byte_end: u32, start: LineColumn, end: LineColumn) -> Self {
        Self {
            byte_start,
            byte_end,
            start,
            end,
        }
    }
}
