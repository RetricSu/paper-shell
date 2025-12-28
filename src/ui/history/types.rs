use crate::backend::editor_backend::HistoryEntry;

#[derive(Debug, Clone)]
pub enum DiffRow {
    Unchanged(String),
    Pair(Vec<DiffLine>, Vec<DiffLine>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineType {
    Added,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct HistoryVersionData {
    pub entry: HistoryEntry,
    pub content: String,
    pub diff_lines: Vec<DiffLine>,
    pub added_count: usize,
    pub removed_count: usize,
}
