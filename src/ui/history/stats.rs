use super::types::DiffRow;
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Default, Clone, Copy)]
pub struct DiffStats {
    pub added_count: usize,
    pub removed_count: usize,
}

/// Calculate character-level statistics from diff rows
pub fn calculate_stats(rows: &[DiffRow]) -> DiffStats {
    let mut stats = DiffStats::default();

    for row in rows {
        match row {
            DiffRow::Pair(left, right) => {
                let left_str: String = left.iter().map(|l| l.content.as_str()).collect();
                let right_str: String = right.iter().map(|r| r.content.as_str()).collect();

                let diff = TextDiff::from_chars(&left_str, &right_str);
                for change in diff.iter_all_changes() {
                    match change.tag() {
                        ChangeTag::Insert => stats.added_count += change.value().chars().count(),
                        ChangeTag::Delete => stats.removed_count += change.value().chars().count(),
                        _ => {}
                    }
                }
            }
            DiffRow::Unchanged(_) => {}
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use similar::{ChangeTag, TextDiff};

    #[test]
    fn stats_counting_english() {
        let old = "hello cat";
        let new = "hello dog";
        // diff: "hello " (equal), "cat" (delete), "dog" (insert)
        // removed: 3 chars ("cat"), added: 3 chars ("dog")

        let diff = TextDiff::from_chars(old, new);
        let mut added = 0;
        let mut removed = 0;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => added += change.value().chars().count(),
                ChangeTag::Delete => removed += change.value().chars().count(),
                _ => {}
            }
        }
        assert_eq!(added, 3);
        assert_eq!(removed, 3);
    }

    #[test]
    fn stats_counting_chinese() {
        let old = "我爱你";
        let new = "我不爱你";
        // diff: "我" (equal), "不" (insert), "爱你" (equal)
        // added: 1 char ("不")

        let diff = TextDiff::from_chars(old, new);
        let mut added = 0;
        let mut removed = 0;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => added += change.value().chars().count(),
                ChangeTag::Delete => removed += change.value().chars().count(),
                _ => {}
            }
        }
        assert_eq!(added, 1);
        assert_eq!(removed, 0);
    }
}
