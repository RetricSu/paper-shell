use super::diff;
use super::types::{DiffLine, DiffRow};
use egui::{Color32, FontId, RichText, TextFormat, Ui, Vec2, text::LayoutJob};
use similar::{ChangeTag, TextDiff};

// Color constants for better maintainability
const REMOVED_LINE_BG: Color32 = Color32::from_rgb(255, 230, 230);
const ADDED_LINE_BG: Color32 = Color32::from_rgb(230, 255, 230);
const REMOVED_WORD_BG: Color32 = Color32::from_rgb(255, 170, 170);
const ADDED_WORD_BG: Color32 = Color32::from_rgb(170, 255, 170);
const REMOVED_TEXT_COLOR: Color32 = Color32::from_rgb(150, 0, 0);
const ADDED_TEXT_COLOR: Color32 = Color32::from_rgb(0, 100, 0);

/// Render the diff view with word-level highlighting
pub fn render_diff_view(ui: &mut Ui, diff_lines: &[DiffLine]) {
    ui.style_mut().spacing.item_spacing.y = 1.0;

    let rows = diff::group_into_rows(diff_lines);

    // Calculate column width based on current available space
    let total_available = ui.available_width();
    // Subtract a little padding to prevent horizontal scrollbar jitter
    // We need space for 2 columns + separator (approx 1.0 width + spacing)
    let col_w = (total_available / 2.0 - 15.0).max(100.0);

    for (row_idx, row) in rows.iter().enumerate() {
        match row {
            DiffRow::Unchanged(text) => {
                // full-width single row for unchanged content
                ui.add(egui::Label::new(RichText::new(text).monospace().size(14.0)).wrap());
            }
            DiffRow::Pair(left_block, right_block) => {
                // CRITICAL FIX: Use push_id to ensure every Grid has a unique ID
                ui.push_id(row_idx, |ui| {
                    egui::Grid::new("diff_pair_grid")
                        .num_columns(3) // Left, Separator, Right
                        .min_col_width(0.0)
                        .spacing(Vec2::new(0.0, 0.0)) // Tight spacing, we handle padding in Frame
                        .show(ui, |ui| {
                            let max = left_block.len().max(right_block.len());

                            for i in 0..max {
                                let left_content = left_block.get(i).map(|l| l.content.as_str());
                                let right_content = right_block.get(i).map(|r| r.content.as_str());

                                // Left Column
                                render_word_highlight(
                                    ui,
                                    left_content,
                                    right_content,
                                    true, // is_left
                                    col_w,
                                );

                                // Right Column
                                render_word_highlight(
                                    ui,
                                    left_content,
                                    right_content,
                                    false, // is_right
                                    col_w,
                                );

                                ui.end_row();
                            }
                        });
                });
            }
        }
    }
}

/// Render a single cell with word-level highlighting
pub fn render_word_highlight(
    ui: &mut Ui,
    left: Option<&str>,
    right: Option<&str>,
    is_left: bool,
    width: f32,
) {
    let font_id = FontId::monospace(14.0);

    let (line_bg, prefix) = if is_left {
        (REMOVED_LINE_BG, "- ")
    } else {
        (ADDED_LINE_BG, "+ ")
    };

    // Determine if we should draw the prefix and background line
    let has_content = if is_left {
        left.is_some()
    } else {
        right.is_some()
    };

    // Use Frame for solid cell background
    egui::Frame::default()
        .fill(if has_content {
            line_bg
        } else {
            Color32::TRANSPARENT
        })
        .inner_margin(8.0) // Increased padding
        .show(ui, |ui| {
            // Ensure the frame takes up the full width
            ui.set_min_width(width - 16.0); // Subtract padding (8.0 * 2)

            if !has_content {
                ui.label(""); // Empty label to maintain height if needed, or just return
                return;
            }

            let mut job = LayoutJob::default();
            let base_text_color = ui.visuals().text_color();

            // Add Prefix
            job.append(
                prefix,
                0.0,
                TextFormat {
                    font_id: font_id.clone(),
                    color: base_text_color.gamma_multiply(0.5),
                    line_height: Some(24.0), // Add line height for better spacing
                    ..Default::default()
                },
            );

            match (left, right) {
                (Some(l), Some(r)) => {
                    // Perform character-level diff (better for CJK)
                    let diff = TextDiff::from_chars(l, r);

                    for change in diff.iter_all_changes() {
                        let text = change.value();
                        match change.tag() {
                            ChangeTag::Equal => {
                                job.append(
                                    text,
                                    0.0,
                                    TextFormat {
                                        font_id: font_id.clone(),
                                        color: base_text_color,
                                        line_height: Some(24.0), // Add line height
                                        ..Default::default()
                                    },
                                );
                            }
                            ChangeTag::Delete => {
                                if is_left {
                                    job.append(
                                        text,
                                        0.0,
                                        TextFormat {
                                            font_id: font_id.clone(),
                                            color: REMOVED_TEXT_COLOR,
                                            background: REMOVED_WORD_BG, // High contrast highlight ON TOP of frame
                                            line_height: Some(24.0),     // Add line height
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                            ChangeTag::Insert => {
                                if !is_left {
                                    job.append(
                                        text,
                                        0.0,
                                        TextFormat {
                                            font_id: font_id.clone(),
                                            color: ADDED_TEXT_COLOR,
                                            background: ADDED_WORD_BG, // High contrast highlight ON TOP of frame
                                            line_height: Some(24.0),   // Add line height
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
                // Fallback for purely added or purely removed lines (no pair match)
                (Some(l), None) if is_left => {
                    job.append(
                        l,
                        0.0,
                        TextFormat {
                            font_id: font_id.clone(),
                            color: base_text_color,
                            line_height: Some(24.0), // Add line height
                            ..Default::default()
                        },
                    );
                }
                (None, Some(r)) if !is_left => {
                    job.append(
                        r,
                        0.0,
                        TextFormat {
                            font_id: font_id.clone(),
                            color: base_text_color,
                            line_height: Some(24.0), // Add line height
                            ..Default::default()
                        },
                    );
                }
                _ => {}
            }

            job.wrap.max_width = width - 16.0; // Adjust wrap width for padding
            ui.add(egui::Label::new(job).wrap());
        });
}
