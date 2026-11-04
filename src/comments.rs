use serde::{Deserialize, Serialize};
use similar::{DiffOp, TextDiff};

/// A comment anchored to a half-open range of Unicode scalar values in the document.
///
/// Character indices (rather than byte indices) match egui's cursor ranges and keep
/// anchors stable for Chinese text and emoji. `quote` preserves the originally
/// selected text for context; `start..end` always refers to the current document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextComment {
    pub id: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
    #[serde(default)]
    pub quote: String,
}

/// One exact editor mutation, expressed in character indices into the old document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditOperation {
    pub start: usize,
    pub end: usize,
    pub inserted_len: usize,
}

/// Apply a known editor mutation to comments and any pending comment anchors.
/// Insertions inside a comment extend it, edits outside it move or leave it alone,
/// and partial deletions shrink it. Replacing the whole range removes the comment.
pub fn apply_edit(comments: &mut Vec<TextComment>, edit: EditOperation) -> bool {
    let before = comments.clone();
    delete_range(comments, edit.start, edit.end.saturating_sub(edit.start));
    insert_range(comments, edit.start, edit.inserted_len);
    *comments != before
}

/// Re-anchor comments for multi-site or whole-document changes where no exact
/// editor operation is available (history restore, undo, and redo).
pub fn reconcile_comments(comments: &mut Vec<TextComment>, old: &str, new: &str) -> bool {
    if old == new {
        return false;
    }

    let before = comments.clone();
    let diff = TextDiff::from_chars(old, new);
    let mut delta = 0isize;

    for op in diff.ops() {
        match *op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                let start = shifted_index(old_index, delta);
                delete_range(comments, start, old_len);
                delta -= old_len as isize;
            }
            DiffOp::Insert {
                old_index, new_len, ..
            } => {
                let start = shifted_index(old_index, delta);
                insert_range(comments, start, new_len);
                delta += new_len as isize;
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_len,
                ..
            } => {
                let start = shifted_index(old_index, delta);
                delete_range(comments, start, old_len);
                insert_range(comments, start, new_len);
                delta += new_len as isize - old_len as isize;
            }
        }
    }

    *comments != before
}

fn shifted_index(index: usize, delta: isize) -> usize {
    if delta.is_negative() {
        index.saturating_sub(delta.unsigned_abs())
    } else {
        index.saturating_add(delta as usize)
    }
}

fn delete_range(comments: &mut Vec<TextComment>, start: usize, len: usize) {
    if len == 0 {
        return;
    }
    let end = start + len;

    for comment in comments.iter_mut() {
        if comment.end <= start {
            continue;
        }
        if comment.start >= end {
            comment.start -= len;
            comment.end -= len;
            continue;
        }

        comment.start = if comment.start < start {
            comment.start
        } else {
            start
        };
        comment.end = if comment.end > end {
            comment.end - len
        } else {
            start
        };
    }
    comments.retain(|comment| comment.start < comment.end);
}

fn insert_range(comments: &mut [TextComment], at: usize, len: usize) {
    if len == 0 {
        return;
    }

    for comment in comments {
        if comment.start >= at {
            comment.start += len;
            comment.end += len;
        } else if comment.end > at {
            // Insertions exactly at either edge stay outside the commented text.
            comment.end += len;
        }
    }
}

/// Drop corrupt or stale persisted ranges instead of risking invalid highlights.
pub fn sanitize_comments(comments: &mut Vec<TextComment>, content: &str) {
    let char_len = content.chars().count();
    comments.retain(|comment| {
        !comment.id.is_empty()
            && !comment.text.trim().is_empty()
            && comment.start < comment.end
            && comment.end <= char_len
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(start: usize, end: usize) -> TextComment {
        TextComment {
            id: "comment-1".into(),
            text: "note".into(),
            start,
            end,
            quote: "original".into(),
        }
    }

    fn range_after(edit: EditOperation, start: usize, end: usize) -> Option<(usize, usize)> {
        let mut comments = vec![comment(start, end)];
        apply_edit(&mut comments, edit);
        comments.first().map(|comment| (comment.start, comment.end))
    }

    fn edit(start: usize, end: usize, inserted_len: usize) -> EditOperation {
        EditOperation {
            start,
            end,
            inserted_len,
        }
    }

    #[test]
    fn insertion_shifts_or_extends_but_does_not_expand_at_boundaries() {
        assert_eq!(range_after(edit(0, 0, 4), 6, 11), Some((10, 15)));
        assert_eq!(range_after(edit(6, 6, 6), 0, 11), Some((0, 17)));
        assert_eq!(range_after(edit(0, 0, 4), 0, 5), Some((4, 9)));
        assert_eq!(range_after(edit(11, 11, 1), 6, 11), Some((6, 11)));
    }

    #[test]
    fn partial_deletion_keeps_a_comment_on_the_remaining_text() {
        assert_eq!(range_after(edit(7, 10, 0), 6, 11), Some((6, 8)));
        assert_eq!(range_after(edit(3, 10, 0), 0, 5), Some((0, 3)));
        assert_eq!(range_after(edit(0, 6, 0), 6, 11), Some((0, 5)));
    }

    #[test]
    fn deleting_or_replacing_the_whole_selection_removes_its_comment() {
        for inserted_len in [0, 8] {
            let mut comments = vec![comment(6, 11)];
            assert!(apply_edit(&mut comments, edit(6, 11, inserted_len)));
            assert!(comments.is_empty());
        }
    }

    #[test]
    fn multi_site_diff_and_unicode_use_character_indices() {
        let old = "  甲😀乙\n第二段评论文字";
        let new = "新的  甲😀乙\n第二段评论文字\n尾";
        let mut comments = vec![comment(10, 13)];
        assert!(reconcile_comments(&mut comments, old, new));
        assert_eq!((comments[0].start, comments[0].end), (12, 15));
        assert_eq!(range_after(edit(2, 3, 0), 1, 4), Some((1, 3)));
    }

    #[test]
    fn undo_uses_diff_instead_of_the_restored_selection() {
        let mut comments = vec![comment(4, 7)];
        assert!(reconcile_comments(&mut comments, "abcXdef", "abcdef"));
        assert_eq!((comments[0].start, comments[0].end), (3, 6));
    }

    #[test]
    fn formatting_many_paragraphs_does_not_delete_untouched_comments() {
        let old = "first comment\n\nsecond comment";
        let new = "  first comment\n\n  second comment";
        let mut comments = vec![comment(0, 5), comment(16, 22)];
        assert!(reconcile_comments(&mut comments, old, new));
        assert_eq!(comments.len(), 2);
        assert_eq!((comments[0].start, comments[0].end), (2, 7));
        assert_eq!((comments[1].start, comments[1].end), (20, 26));
    }

    #[test]
    fn unchanged_content_does_not_mark_comments_dirty() {
        let mut comments = vec![comment(0, 5)];
        assert!(!reconcile_comments(&mut comments, "hello", "hello"));
    }

    #[test]
    fn sanitize_rejects_empty_and_out_of_bounds_ranges() {
        let mut comments = vec![comment(0, 2), comment(2, 2), comment(2, 20)];
        sanitize_comments(&mut comments, "甲乙丙");
        assert_eq!(comments.len(), 1);
    }
}
