use super::diff;
use super::stats;
use super::types::{
    BranchFilter, DiffLine, DiffLineType, GraphEdge, HistoryBranchInfo, HistoryLayout, HistoryTree,
    HistoryVersionData,
};
use crate::backend::editor_backend::{
    EditorBackend, HistoryEntry, normalize_history_entries, visual_representative_ids,
};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Build a visual tree from every raw save. Parent links, rather than JSON
/// adjacency, determine each diff so changes cannot bleed between branches.
pub fn build_version_tree(
    entries: Vec<HistoryEntry>,
    backend: &EditorBackend,
    current_version_id: Option<&str>,
) -> Result<HistoryTree, String> {
    let entries = normalize_history_entries(entries);
    let visual_ids = visual_representative_ids(&entries);
    let mut nodes: Vec<HistoryVersionData> = Vec::new();
    let mut index_by_raw_id: HashMap<String, usize> = HashMap::new();

    for entry in entries {
        let raw_id = entry.id.clone().unwrap();
        let visible_id = visual_ids.get(&raw_id).unwrap().clone();
        let raw_parent_id = entry.parent_id.as_deref().map(|parent| {
            visual_ids
                .get(parent)
                .cloned()
                .unwrap_or_else(|| parent.to_string())
        });
        if visible_id != raw_id {
            let parent = index_by_raw_id
                .get(&visible_id)
                .copied()
                .ok_or_else(|| format!("Visible parent not found for folded version: {raw_id}"))?;
            nodes[parent].merged_ids.push(raw_id.clone());
            nodes[parent].entry.time_spent =
                Some(nodes[parent].entry.time_spent.unwrap_or(0) + entry.time_spent.unwrap_or(0));
            index_by_raw_id.insert(raw_id, parent);
            continue;
        }
        let content = backend
            .restore_version(&entry.hash)
            .map_err(|error| error.to_string())?;
        let parent_index = raw_parent_id
            .as_deref()
            .and_then(|parent| index_by_raw_id.get(parent).copied());

        let diff_lines = if let Some(parent) = parent_index {
            diff::compute_diff(&nodes[parent].content, &content)
        } else {
            content
                .lines()
                .map(|line| DiffLine {
                    line_type: DiffLineType::Unchanged,
                    content: line.to_string(),
                })
                .collect()
        };
        let stats = stats::calculate_stats(&diff::group_into_rows(&diff_lines));
        let node = HistoryVersionData {
            id: raw_id.clone(),
            parent_id: raw_parent_id.filter(|id| index_by_raw_id.contains_key(id)),
            children_ids: Vec::new(),
            merged_ids: Vec::new(),
            entry,
            content,
            diff_lines,
            added_count: stats.added_count,
            removed_count: stats.removed_count,
            branch_id: usize::MAX,
            branch_name: String::new(),
            lane: 0,
            is_current: false,
            is_branch_head: false,
            is_fork_point: false,
            subtree_size: 1,
            subtree_contains_current: false,
            can_delete_branch: false,
        };
        let node_index = nodes.len();
        if let Some(parent) = parent_index {
            nodes[parent].children_ids.push(raw_id.clone());
        }
        index_by_raw_id.insert(raw_id.clone(), node_index);
        nodes.push(node);
    }
    if nodes.is_empty() {
        return Ok(HistoryTree::default());
    }

    let current_index = current_version_id
        .and_then(|id| index_by_raw_id.get(id).copied())
        .unwrap_or(nodes.len() - 1);
    nodes[current_index].is_current = true;
    for node in &mut nodes {
        node.is_fork_point = node.children_ids.len() > 1;
    }

    // Continue an interior current version through its latest descendant. A
    // rollback alone does not invent a branch; the next save creates the fork.
    let mut trunk_head = current_index;
    let mut trunk_visited = HashSet::new();
    while trunk_visited.insert(trunk_head) && !nodes[trunk_head].children_ids.is_empty() {
        trunk_head = nodes[trunk_head]
            .children_ids
            .iter()
            .filter_map(|id| index_by_raw_id.get(id).copied())
            .max_by_key(|index| nodes[*index].entry.timestamp)
            .unwrap_or(trunk_head);
    }

    let mut branches = Vec::new();
    let trunk_path = unassigned_path_to_root(trunk_head, &nodes, &index_by_raw_id);
    assign_branch(&mut nodes, &trunk_path, 0, "主线");
    branches.push(make_branch(0, "主线", &trunk_path, &nodes));

    // Each unassigned leaf starts a side branch. Walking upwards stops at the
    // already assigned fork, so primary-line nodes are never stolen.
    for tip in (0..nodes.len()).rev() {
        if !nodes[tip].children_ids.is_empty() || nodes[tip].branch_id != usize::MAX {
            continue;
        }
        let path = unassigned_path_to_root(tip, &nodes, &index_by_raw_id);
        let branch_id = branches.len();
        let name = format!("分叉 {branch_id}");
        assign_branch(&mut nodes, &path, branch_id, &name);
        branches.push(make_branch(branch_id, &name, &path, &nodes));
    }
    // Malformed histories can contain a detached chain without a visible leaf.
    // Keep it accessible instead of silently dropping it from the window.
    for start in 0..nodes.len() {
        if nodes[start].branch_id != usize::MAX {
            continue;
        }
        let path = unassigned_path_to_root(start, &nodes, &index_by_raw_id);
        let branch_id = branches.len();
        let name = format!("分叉 {branch_id}");
        assign_branch(&mut nodes, &path, branch_id, &name);
        branches.push(make_branch(branch_id, &name, &path, &nodes));
    }
    for branch in &branches {
        if let Some(head) = index_by_raw_id.get(&branch.head_id) {
            nodes[*head].is_branch_head = true;
        }
    }

    // Raw saves are chronological, so this reverse pass visits children first.
    for index in (0..nodes.len()).rev() {
        let children: Vec<usize> = nodes[index]
            .children_ids
            .iter()
            .filter_map(|id| index_by_raw_id.get(id).copied())
            .collect();
        nodes[index].subtree_size = 1 + children
            .iter()
            .map(|child| nodes[*child].subtree_size)
            .sum::<usize>();
        nodes[index].subtree_contains_current = nodes[index].is_current
            || children
                .iter()
                .any(|child| nodes[*child].subtree_contains_current);
    }
    for index in 0..nodes.len() {
        nodes[index].lane = nodes[index].branch_id;
        nodes[index].can_delete_branch =
            !nodes[index].subtree_contains_current && nodes[index].subtree_size < nodes.len();
    }

    Ok(HistoryTree { nodes, branches })
}

fn unassigned_path_to_root(
    start: usize,
    nodes: &[HistoryVersionData],
    index_by_id: &HashMap<String, usize>,
) -> Vec<usize> {
    let mut path = Vec::new();
    let mut current = Some(start);
    let mut visited = HashSet::new();
    while let Some(index) = current {
        if nodes[index].branch_id != usize::MAX || !visited.insert(index) {
            break;
        }
        path.push(index);
        current = nodes[index]
            .parent_id
            .as_deref()
            .and_then(|id| index_by_id.get(id).copied());
    }
    path
}

fn assign_branch(nodes: &mut [HistoryVersionData], path: &[usize], id: usize, name: &str) {
    for index in path {
        nodes[*index].branch_id = id;
        nodes[*index].branch_name = name.to_string();
    }
}

fn make_branch(
    id: usize,
    name: &str,
    path: &[usize],
    nodes: &[HistoryVersionData],
) -> HistoryBranchInfo {
    let head_id = path
        .first()
        .map(|index| nodes[*index].id.clone())
        .unwrap_or_default();
    let fork_parent_id = if id == 0 {
        None
    } else {
        path.last()
            .and_then(|index| nodes[*index].parent_id.clone())
    };
    HistoryBranchInfo {
        id,
        name: name.to_string(),
        head_id,
        fork_parent_id,
        node_count: path.len(),
    }
}

/// Place children above parents using a topological order. Branch lanes are
/// stable (main = 0, side branches = 1..n), which keeps the graph predictable.
pub fn compute_display_layout(tree: &HistoryTree, filter: &BranchFilter) -> HistoryLayout {
    let index_by_id: HashMap<&str, usize> = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect();
    let visible: HashSet<usize> = match filter {
        BranchFilter::All => (0..tree.nodes.len()).collect(),
        BranchFilter::CurrentLineage => tree
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| (node.branch_id == 0).then_some(index))
            .collect(),
        BranchFilter::Branch(branch_id) => {
            let mut visible = HashSet::new();
            let mut stack: Vec<usize> = tree
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| (node.branch_id == *branch_id).then_some(index))
                .collect();
            while let Some(index) = stack.pop() {
                if !visible.insert(index) {
                    continue;
                }
                if let Some(parent) = tree.nodes[index]
                    .parent_id
                    .as_deref()
                    .and_then(|id| index_by_id.get(id))
                {
                    stack.push(*parent);
                }
            }
            visible
        }
    };
    let mut remaining_children: HashMap<usize, usize> = visible
        .iter()
        .map(|index| {
            (
                *index,
                tree.nodes[*index]
                    .children_ids
                    .iter()
                    .filter_map(|id| index_by_id.get(id.as_str()))
                    .filter(|child| visible.contains(child))
                    .count(),
            )
        })
        .collect();
    let mut ready: BTreeSet<(i64, usize)> = visible
        .iter()
        .filter(|index| remaining_children[index] == 0)
        .map(|index| {
            (
                -tree.nodes[*index].entry.timestamp.timestamp_millis(),
                *index,
            )
        })
        .collect();
    let mut display_order = Vec::with_capacity(visible.len());
    while let Some((_, index)) = ready.pop_first() {
        display_order.push(index);
        if let Some(parent) = tree.nodes[index]
            .parent_id
            .as_deref()
            .and_then(|id| index_by_id.get(id))
            .filter(|parent| visible.contains(parent))
        {
            let remaining = remaining_children.get_mut(parent).unwrap();
            *remaining -= 1;
            if *remaining == 0 {
                ready.insert((
                    -tree.nodes[*parent].entry.timestamp.timestamp_millis(),
                    *parent,
                ));
            }
        }
    }
    // Never hide valid records behind a malformed cycle.
    for index in &visible {
        if !display_order.contains(index) {
            display_order.push(*index);
        }
    }
    let row_by_index: HashMap<usize, usize> = display_order
        .iter()
        .enumerate()
        .map(|(row, index)| (*index, row))
        .collect();
    let lane_count = display_order
        .iter()
        .map(|index| tree.nodes[*index].lane + 1)
        .max()
        .unwrap_or(1);
    let mut edges = Vec::new();
    for (row, index) in display_order.iter().enumerate() {
        if let Some(parent) = tree.nodes[*index]
            .parent_id
            .as_deref()
            .and_then(|id| index_by_id.get(id))
            .filter(|parent| visible.contains(parent))
        {
            edges.push(GraphEdge {
                child_row: row,
                child_lane: tree.nodes[*index].lane,
                parent_row: row_by_index[parent],
                parent_lane: tree.nodes[*parent].lane,
                branch_id: tree.nodes[*index].branch_id,
            });
        }
    }
    HistoryLayout {
        display_order,
        lane_count,
        edges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, branch: usize, timestamp: &str) -> HistoryVersionData {
        HistoryVersionData {
            id: id.into(),
            parent_id: parent.map(Into::into),
            children_ids: Vec::new(),
            merged_ids: Vec::new(),
            entry: HistoryEntry {
                hash: format!("{id:0>16}"),
                timestamp: timestamp.parse().unwrap(),
                file_path: None,
                time_spent: None,
                id: Some(id.into()),
                parent_id: parent.map(Into::into),
                label: None,
            },
            content: id.into(),
            diff_lines: Vec::new(),
            added_count: 0,
            removed_count: 0,
            branch_id: branch,
            branch_name: if branch == 0 {
                "主线".into()
            } else {
                format!("分叉 {branch}")
            },
            lane: branch,
            is_current: false,
            is_branch_head: false,
            is_fork_point: false,
            subtree_size: 1,
            subtree_contains_current: false,
            can_delete_branch: false,
        }
    }

    #[test]
    fn layout_places_two_branch_tips_above_their_shared_fork() {
        let mut nodes = vec![
            node("root", None, 0, "2026-01-01T00:00:00Z"),
            node("main", Some("root"), 0, "2026-01-03T00:00:00Z"),
            node("side", Some("root"), 1, "2026-01-02T00:00:00Z"),
        ];
        nodes[0].children_ids = vec!["main".into(), "side".into()];
        let tree = HistoryTree {
            nodes,
            branches: Vec::new(),
        };
        let layout = compute_display_layout(&tree, &BranchFilter::All);
        assert_eq!(layout.display_order.len(), 3);
        assert_eq!(layout.lane_count, 2);
        assert_eq!(layout.edges.len(), 2);
        assert!(
            layout
                .edges
                .iter()
                .all(|edge| edge.child_row < edge.parent_row)
        );
        assert!(
            layout.edges.iter().any(|edge| {
                edge.child_lane == 1 && edge.parent_lane == 0 && edge.parent_row == 2
            })
        );
    }

    #[test]
    fn branch_filter_includes_shared_ancestors() {
        let mut nodes = vec![
            node("root", None, 0, "2026-01-01T00:00:00Z"),
            node("main", Some("root"), 0, "2026-01-03T00:00:00Z"),
            node("side", Some("root"), 1, "2026-01-02T00:00:00Z"),
        ];
        nodes[0].children_ids = vec!["main".into(), "side".into()];
        let tree = HistoryTree {
            nodes,
            branches: Vec::new(),
        };
        let layout = compute_display_layout(&tree, &BranchFilter::Branch(1));
        let ids: Vec<&str> = layout
            .display_order
            .iter()
            .map(|index| tree.nodes[*index].id.as_str())
            .collect();
        assert_eq!(ids, vec!["side", "root"]);
        assert_eq!(layout.edges.len(), 1);
    }
}
