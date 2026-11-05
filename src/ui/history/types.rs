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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffCompareMode {
    #[default]
    Parent,
    Current,
    Pinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BranchFilter {
    #[default]
    All,
    CurrentLineage,
    Branch(usize),
}

#[derive(Debug, Clone)]
pub struct HistoryBranchInfo {
    pub id: usize,
    pub name: String,
    pub head_id: String,
    pub fork_parent_id: Option<String>,
    pub node_count: usize,
}

/// One visible version. Consecutive unchanged saves are folded into `merged_ids`
/// rather than deleted, preserving their focus time and save records.
#[derive(Debug, Clone)]
pub struct HistoryVersionData {
    pub id: String,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub merged_ids: Vec<String>,
    pub entry: HistoryEntry,
    pub content: String,
    pub diff_lines: Vec<DiffLine>,
    pub added_count: usize,
    pub removed_count: usize,
    pub branch_id: usize,
    pub branch_name: String,
    pub lane: usize,
    pub is_current: bool,
    pub is_branch_head: bool,
    pub is_fork_point: bool,
    pub subtree_size: usize,
    pub subtree_contains_current: bool,
    pub can_delete_branch: bool,
}

impl HistoryVersionData {
    pub fn label(&self) -> Option<&str> {
        self.entry.label.as_deref()
    }
}

/// Connection from a visible child row to its visible parent row.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub child_row: usize,
    pub child_lane: usize,
    pub parent_row: usize,
    pub parent_lane: usize,
    pub branch_id: usize,
}

#[derive(Debug, Default, Clone)]
pub struct HistoryLayout {
    pub display_order: Vec<usize>,
    pub lane_count: usize,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Default)]
pub struct HistoryTree {
    pub nodes: Vec<HistoryVersionData>,
    pub branches: Vec<HistoryBranchInfo>,
}
