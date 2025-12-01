use super::types::{DiffLine, DiffLineType, DiffRow};
use similar::{ChangeTag, TextDiff};

/// Compute line-based diff between old and new text
pub fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut diff_lines = Vec::new();

    for change in diff.iter_all_changes() {
        let line_type = match change.tag() {
            ChangeTag::Delete => DiffLineType::Removed,
            ChangeTag::Insert => DiffLineType::Added,
            ChangeTag::Equal => DiffLineType::Unchanged,
        };

        diff_lines.push(DiffLine {
            line_type,
            content: change.to_string().trim_end().to_string(),
        });
    }

    diff_lines
}

/// Group raw diff lines into rows where unchanged identical lines are single rows,
/// and contiguous removed/added blocks become paired rows.
pub fn group_into_rows(diff_lines: &[DiffLine]) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut i = 0usize;

    while i < diff_lines.len() {
        match &diff_lines[i].line_type {
            DiffLineType::Unchanged => {
                // Collect contiguous unchanged lines and emit each as Unchanged row
                rows.push(DiffRow::Unchanged(diff_lines[i].content.clone()));
                i += 1;
            }
            DiffLineType::Removed => {
                // collect removed block
                let mut removed_block = Vec::new();
                removed_block.push(diff_lines[i].clone());
                i += 1;
                while i < diff_lines.len() && diff_lines[i].line_type == DiffLineType::Removed {
                    removed_block.push(diff_lines[i].clone());
                    i += 1;
                }

                // collect following added block (if any)
                let mut added_block = Vec::new();
                let mut j = i;
                while j < diff_lines.len() && diff_lines[j].line_type == DiffLineType::Added {
                    added_block.push(diff_lines[j].clone());
                    j += 1;
                }

                if !added_block.is_empty() {
                    // pair removed and added blocks
                    rows.push(DiffRow::Pair(removed_block, added_block));
                    i = j;
                } else {
                    // no added block - show removed lines as left-only pairs
                    rows.push(DiffRow::Pair(removed_block, Vec::new()));
                }
            }
            DiffLineType::Added => {
                // added without preceding removal -> right-only
                rows.push(DiffRow::Pair(Vec::new(), vec![diff_lines[i].clone()]));
                i += 1;
            }
        }
    }

    rows
}

/// Check if diff lines contain meaningful changes (non-empty added or removed content)
pub fn has_meaningful_changes(diff_lines: &[DiffLine]) -> bool {
    diff_lines.iter().any(|line| {
        matches!(line.line_type, DiffLineType::Added | DiffLineType::Removed)
            && !line.content.trim().is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_unchanged_lines() {
        let old = "a\nb\n";
        let new = "a\nb\n";
        let diff = compute_diff(old, new);
        let rows = group_into_rows(&diff);
        assert_eq!(rows.len(), 2);
        match &rows[0] {
            DiffRow::Unchanged(s) => assert_eq!(s, "a"),
            _ => panic!(),
        }
        match &rows[1] {
            DiffRow::Unchanged(s) => assert_eq!(s, "b"),
            _ => panic!(),
        }
    }

    #[test]
    fn grouping_removed_added_pair() {
        let old = "a\nold\nc\n";
        let new = "a\nnew\nc\n";
        let diff = compute_diff(old, new);
        let rows = group_into_rows(&diff);
        // rows: a (unchanged), pair(old,new), c (unchanged)
        assert_eq!(rows.len(), 3);
        match &rows[1] {
            DiffRow::Pair(l, r) => {
                assert_eq!(l.len(), 1);
                assert_eq!(r.len(), 1);
            }
            _ => panic!(),
        }
    }
}
