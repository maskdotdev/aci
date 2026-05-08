use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LineColumn {
    pub line: u32,
    pub column: u32,
}

impl LineColumn {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub byte_start: u32,
    pub byte_end: u32,
    pub start: LineColumn,
    pub end: LineColumn,
}

impl SourceSpan {
    pub fn new(byte_start: u32, byte_end: u32, start: LineColumn, end: LineColumn) -> Self {
        Self {
            byte_start,
            byte_end,
            start,
            end,
        }
    }
}
