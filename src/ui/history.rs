use crate::backend::{EditorBackend, HistoryEntry};
use egui::{Color32, Context, FontId, RichText, ScrollArea, TextFormat, Ui, Vec2, text::LayoutJob};
use similar::{ChangeTag, TextDiff};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_unchanged_lines() {
        let old = "a\nb\n";
        let new = "a\nb\n";
        let diff = HistoryWindow::compute_diff(old, new);
        let rows = HistoryWindow::group_into_rows(&diff);
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
        let diff = HistoryWindow::compute_diff(old, new);
        let rows = HistoryWindow::group_into_rows(&diff);
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

#[derive(Debug, Clone)]
enum DiffRow {
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
}

pub struct HistoryWindow {
    open: bool,
    history_data: Option<Vec<HistoryVersionData>>,
    selected_index: Option<usize>,
    viewport_id: egui::ViewportId,
}

impl Default for HistoryWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            history_data: None,
            selected_index: None,
            viewport_id: egui::ViewportId::from_hash_of("history_window"),
        }
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn set_history(
        &mut self,
        entries: Vec<HistoryEntry>,
        backend: &EditorBackend,
    ) -> Result<(), String> {
        let mut history_data: Vec<HistoryVersionData> = Vec::new();

        for entry in entries.iter() {
            // Load content for this version
            let content = backend
                .restore_version(&entry.hash)
                .map_err(|e| e.to_string())?;

            // Calculate diff with previous meaningful version
            let diff_lines = if !history_data.is_empty() {
                let prev_content = &history_data.last().unwrap().content;
                Self::compute_diff(prev_content, &content)
            } else {
                // First version - show full content as unchanged
                content
                    .lines()
                    .map(|line| DiffLine {
                        line_type: DiffLineType::Unchanged,
                        content: line.to_string(),
                    })
                    .collect()
            };

            // Check if this version has meaningful changes
            let has_changes = history_data.is_empty() || Self::has_meaningful_changes(&diff_lines);

            if has_changes {
                history_data.push(HistoryVersionData {
                    entry: entry.clone(),
                    content,
                    diff_lines,
                });
            }
        }

        let data_len = history_data.len();
        self.history_data = Some(history_data);
        self.selected_index = Some(data_len.saturating_sub(1)); // Select latest
        Ok(())
    }

    fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
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

    // Group raw diff lines into rows where unchanged identical lines are single rows,
    // and contiguous removed/added blocks become paired rows.
    fn group_into_rows(diff_lines: &[DiffLine]) -> Vec<DiffRow> {
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

    fn has_meaningful_changes(diff_lines: &[DiffLine]) -> bool {
        diff_lines.iter().any(|line| {
            matches!(line.line_type, DiffLineType::Added | DiffLineType::Removed)
                && !line.content.trim().is_empty()
        })
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }

        let viewport_id = self.viewport_id;

        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_resizable(true)
                .with_transparent(true),
            |ctx, _class| {
                // Title bar
                egui::TopBottomPanel::top("history_title_bar").show(ctx, |ui| {
                    self.show_title_bar(ui);
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_content(ui);
                });

                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
            },
        );
    }

    fn show_title_bar(&mut self, ui: &mut Ui) {
        let title_bar_rect = ui.available_rect_before_wrap();

        // Dragging logic - registered BEFORE widgets so they can steal input
        let interact = ui.interact(
            title_bar_rect,
            ui.id().with("history_title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if interact.dragged() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if interact.double_clicked() {
            let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        ui.horizontal(|ui| {
            // Title
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label("📜 History");
            });

            // Window Controls
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Close button
                if ui.button("❌").on_hover_text("Close").clicked() {
                    self.open = false;
                }

                // Maximize/Restore button
                let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                if ui.button("⛶").on_hover_text("Maximize/Restore").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                // Minimize button
                if ui.button("➖").on_hover_text("Minimize").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            });
        });
    }

    fn show_content(&mut self, ui: &mut Ui) {
        if let Some(history_data) = &self.history_data {
            if history_data.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.heading("No history available for this file");
                    ui.add_space(10.0);
                    ui.label("Make some edits and save to build up version history.");
                });
                return;
            }

            // Use SidePanel for better layout (left panel for versions)
            egui::SidePanel::left("version_list_panel")
                .resizable(true)
                .show_inside(ui, |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        // Show in reverse order (newest first)
                        for (i, version_data) in history_data.iter().enumerate().rev() {
                            let is_selected = self.selected_index == Some(i);
                            let timestamp = version_data
                                .entry
                                .timestamp
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string();

                            let version_label = timestamp.to_string();

                            if ui.selectable_label(is_selected, version_label).clicked() {
                                self.selected_index = Some(i);
                            }
                        }
                    });
                });

            // Central panel for diff view
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if let Some(selected_idx) = self.selected_index {
                    if let Some(version_data) = history_data.get(selected_idx) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔑 Hash:").strong());
                            ui.label(RichText::new(&version_data.entry.hash).monospace());
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                Self::show_diff_static(ui, &version_data.diff_lines);
                            });
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.heading("Select a version to view details");
                        ui.add_space(10.0);
                        ui.label("Choose a version from the list on the left");
                    });
                }
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("Loading history...");
                ui.add_space(10.0);
                ui.spinner();
            });
        }
    }

    fn show_diff_static(ui: &mut Ui, diff_lines: &[DiffLine]) {
        ui.style_mut().spacing.item_spacing.y = 1.0;

        let rows = Self::group_into_rows(diff_lines);

        // Calculate column width based on current available space
        let total_available = ui.available_width();
        // Subtract a little padding to prevent horizontal scrollbar jitter
        // We need space for 2 columns + separator (approx 1.0 width + spacing)
        let col_w = (total_available / 2.0 - 15.0).max(100.0);

        for (row_idx, row) in rows.iter().enumerate() {
            match row {
                DiffRow::Unchanged(text) => {
                    // full-width single row for unchanged content
                    ui.add(egui::Label::new(RichText::new(text).monospace()).wrap());
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
                                    let left_content =
                                        left_block.get(i).map(|l| l.content.as_str());
                                    let right_content =
                                        right_block.get(i).map(|r| r.content.as_str());

                                    // Left Column
                                    Self::render_word_highlight(
                                        ui,
                                        left_content,
                                        right_content,
                                        true, // is_left
                                        col_w,
                                    );

                                    // Right Column
                                    Self::render_word_highlight(
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

    fn render_word_highlight(
        ui: &mut Ui,
        left: Option<&str>,
        right: Option<&str>,
        is_left: bool,
        width: f32,
    ) {
        let font_id = FontId::monospace(14.0);

        // IMPROVED COLORS: Higher contrast
        // Backgrounds for the full line (Pale)
        let removed_line_bg = Color32::from_rgb(255, 230, 230);
        let added_line_bg = Color32::from_rgb(230, 255, 230);

        // Backgrounds for the specific changed words (Saturated)
        let removed_word_bg = Color32::from_rgb(255, 170, 170);
        let added_word_bg = Color32::from_rgb(170, 255, 170);

        // Text colors
        let base_text_color = ui.visuals().text_color();
        let removed_text_color = Color32::from_rgb(150, 0, 0);
        let added_text_color = Color32::from_rgb(0, 100, 0);

        let (line_bg, prefix) = if is_left {
            (removed_line_bg, "- ")
        } else {
            (added_line_bg, "+ ")
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
                                                color: removed_text_color,
                                                background: removed_word_bg, // High contrast highlight ON TOP of frame
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
                                                color: added_text_color,
                                                background: added_word_bg, // High contrast highlight ON TOP of frame
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
}
