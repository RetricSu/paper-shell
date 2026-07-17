use egui::{Align, Color32, Frame, Galley, Layout, Pos2, Rect, RichText, Sense, Ui, Vec2};
#[cfg(test)]
use similar::{ChangeTag, TextDiff};
use std::ops::Range;
use std::sync::Arc;

use super::ai_panel::{AiEditPreview, AiPanel, AiPanelAction};
use super::sidebar::Sidebar;
use crate::backend::ai_backend::{
    AiAgentResponse, AiError, AiProgressEvent, AiRequestId, AiSelectionContext,
};
use crate::backend::sidebar_backend::Mark;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
struct SearchReplaceState {
    show_dialog: bool,
    search_text: String,
    replace_text: String,
    case_sensitive: bool,
    whole_word: bool,
    current_match: Option<(usize, usize)>, // (start, end) byte indices
    matches: Vec<(usize, usize)>,          // All matches as (start, end) byte indices
    match_index: usize,                    // Current match index
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
enum AiDiffSegmentKind {
    Context,
    Removed,
    Added,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
struct AiDiffSegment {
    text: String,
    kind: AiDiffSegmentKind,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
struct AiInlineDiff {
    text: String,
    segments: Vec<AiDiffSegment>,
    change_start: usize,
}

#[derive(Clone, Debug)]
struct SelectionAnchor {
    context: AiSelectionContext,
    screen_rect: Rect,
    stale: bool,
}

#[derive(Clone, Debug)]
struct AiUndoEntry {
    before: String,
    after: String,
}

#[derive(Default)]
pub struct Editor {
    content: String,
    cursor_index: Option<usize>,
    last_galley: Option<Arc<Galley>>,
    sidebar: Sidebar,
    ai_panel: AiPanel,
    is_focused: bool,
    current_file: Option<PathBuf>,
    current_file_total_time: u64,
    cached_word_count: Option<usize>,
    ai_preview_scrolled_to: Option<usize>,
    selection_anchor: Option<SelectionAnchor>,
    next_selection_anchor_id: u64,
    inline_ai_open: bool,
    inline_ai_draft: String,
    ai_undo_stack: Vec<AiUndoEntry>,
    // Search and replace state
    search_replace: SearchReplaceState,
}

impl Editor {
    fn handle_ai_undo(&mut self, ui: &mut Ui) {
        let can_undo = self
            .ai_undo_stack
            .last()
            .is_some_and(|entry| entry.after == self.content);
        let shortcut = can_undo
            && ui.input(|input| {
                input.modifiers.command && !input.modifiers.shift && input.key_pressed(egui::Key::Z)
            });
        if shortcut
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
            && let Some(entry) = self.ai_undo_stack.pop()
        {
            self.content = entry.before;
            self.cached_word_count = None;
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<AiPanelAction> {
        self.handle_ai_undo(ui);
        let mut ai_action = None;
        let mut content = std::mem::take(&mut self.content);
        let active_preview = self.ai_panel.active_edit_preview();
        let preview_location = active_preview.as_ref().map(|proposal| {
            locate_ai_edit_range(&content, &proposal.base_content, &proposal.original_text)
        });
        let diff_range_for_layout = preview_location
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned();
        let id = ui.make_persistent_id("main_editor");

        // Sidebar width
        let sidebar_width = 20.0;
        let available_width = ui.available_width() - sidebar_width;

        // Use horizontal layout with top-to-bottom alignment
        ui.horizontal_top(|ui| {
            // 1. Reserve space for sidebar (so editor is pushed right)
            let sidebar_origin = ui.cursor().min;
            ui.allocate_rect(
                Rect::from_min_size(sidebar_origin, Vec2::new(sidebar_width, 0.0)),
                Sense::hover(),
            );

            // 2. Editor Area. A pending AI edit only changes the layouter and adds
            // an anchored review surface; the actual text editor stays interactive.
            let diff_range = diff_range_for_layout.clone();
            let mut layouter = move |ui: &Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                ui.painter().layout_job(ai_live_diff_layout_job(
                    ui,
                    string.as_str(),
                    diff_range.as_ref(),
                    wrap_width,
                ))
            };

            let output = egui::TextEdit::multiline(&mut content)
                .id(id)
                .frame(false)
                .desired_width(available_width)
                .desired_rows(30)
                .layouter(&mut layouter)
                .show(ui);

            Self::enable_scroll_to_cursor(ui, &output);
            Self::fix_macos_ime(&output, ui);
            self.draw_underline_decoration_at_focus_line(&output, ui);
            self.highlight_matches(&output, ui, &content);
            self.highlight_search_matches(&output, ui, &content);
            self.add_context_menu(&output, &mut content);
            self.capture_ai_selection(&output, &content, ui);

            if let (Some(proposal), Some(location)) = (&active_preview, &preview_location) {
                if let Ok(range) = location
                    && self.ai_preview_scrolled_to != Some(proposal.proposal_index)
                    && is_valid_text_byte_range(&content, range)
                {
                    let start_char = content[..range.start].chars().count();
                    if let Some(change_rect) = text_range_screen_rect(
                        &output,
                        start_char,
                        start_char + content[range.clone()].chars().count(),
                    ) {
                        ui.scroll_to_rect(
                            change_rect.expand2(egui::vec2(8.0, 64.0)),
                            Some(Align::Center),
                        );
                    }
                    self.ai_preview_scrolled_to = Some(proposal.proposal_index);
                }
                ai_action = show_ai_edit_overlay(ui.ctx(), &output, proposal, location);
            } else {
                self.ai_preview_scrolled_to = None;
            }

            self.last_galley = Some(output.galley.clone());
            self.content = content;

            let editor_response = &output.response;
            if editor_response.changed() {
                self.cached_word_count = None;
                self.search_replace.matches.clear();
                self.search_replace.current_match = None;
                self.search_replace.match_index = 0;
            }
            if editor_response.clicked() {
                editor_response.request_focus();
            }

            let content_height = editor_response.rect.height();
            self.render_sidebar(
                sidebar_origin,
                sidebar_width,
                content_height,
                output.galley_pos,
                ui,
            );
        });

        if active_preview.is_none() && ai_action.is_none() {
            ai_action = self.show_selection_ai(ui.ctx());
        }

        // Show search and replace dialog
        self.show_search_replace_dialog(ui);

        ai_action
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.cached_word_count = None; // 清除缓存
        self.ai_undo_stack.clear();
    }

    pub fn get_word_count(&mut self) -> usize {
        if let Some(count) = self.cached_word_count {
            return count;
        }

        // 原有的计算逻辑
        let count = self.calculate_word_count_internal();
        self.cached_word_count = Some(count);
        count
    }

    fn calculate_word_count_internal(&self) -> usize {
        let mut count = 0;
        let mut in_word = false;
        for c in self.content.chars() {
            if c.is_whitespace() {
                in_word = false;
            } else if is_cjk(c) {
                count += 1;
                in_word = false;
            } else if !in_word {
                count += 1;
                in_word = true;
            }
        }
        count
    }

    pub fn get_cursor_word_count(&self) -> Option<usize> {
        let cursor_index = self.cursor_index?;

        // Convert character index to byte index safely
        let byte_index = self
            .content
            .char_indices()
            .nth(cursor_index)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.content.len());

        let text_before_cursor = &self.content[..byte_index];

        let mut count = 0;
        let mut in_word = false;
        for c in text_before_cursor.chars() {
            if c.is_whitespace() {
                in_word = false;
            } else if is_cjk(c) {
                count += 1;
                in_word = false;
            } else if !in_word {
                count += 1;
                in_word = true;
            }
        }
        Some(count)
    }

    pub fn get_stats(&mut self) -> (usize, usize) {
        (
            self.get_word_count(),
            self.get_cursor_word_count().unwrap_or(0),
        )
    }

    pub fn set_uuid(&mut self, uuid: String) {
        self.sidebar.set_uuid(uuid);
    }

    pub fn marks_changed(&self) -> bool {
        self.sidebar.marks_changed()
    }

    pub fn get_marks(&self) -> &HashMap<usize, Mark> {
        self.sidebar.get_marks()
    }

    pub fn get_sidebar_uuid(&self) -> Option<&String> {
        self.sidebar.get_uuid()
    }

    pub fn apply_marks(&mut self, marks: HashMap<usize, Mark>) {
        self.sidebar.apply_marks(marks);
    }

    pub fn reset_marks_changed(&mut self) {
        self.sidebar.reset_marks_changed();
    }

    /// Get the current file path
    pub fn get_current_file(&self) -> Option<&PathBuf> {
        self.current_file.as_ref()
    }

    /// Set the current file path
    pub fn set_current_file(&mut self, path: Option<PathBuf>) {
        self.current_file = path;
    }

    /// Get the current file total time
    pub fn get_current_file_total_time(&self) -> u64 {
        self.current_file_total_time
    }

    /// Set the current file total time
    pub fn set_current_file_total_time(&mut self, time: u64) {
        self.current_file_total_time = time;
    }

    /// Get the current focus state of the editor
    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    /// Format the content by adding two spaces at the beginning of each line.
    /// Blank lines are preserved as is.
    pub fn format(&mut self) {
        let formatted = Self::add_paragraph_indentation(&self.content);
        self.content = formatted;
    }

    /// Helper function to add two spaces at the beginning of each line
    fn add_paragraph_indentation(text: &str) -> String {
        let mut result = String::with_capacity(text.len() + 128);

        for (i, line) in text.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }

            if line.trim().is_empty() {
                // Preserve blank lines as is
                result.push_str(line);
            } else {
                // Always add exactly two spaces after trimming leading whitespace
                result.push_str("  ");
                result.push_str(line.trim_start());
            }
        }

        // Restore trailing newline if present
        if text.ends_with('\n') {
            result.push('\n');
        }

        result
    }

    fn enable_scroll_to_cursor(ui: &mut Ui, output: &egui::text_edit::TextEditOutput) {
        if output.response.has_focus() {
            let should_scroll_to_cursor = ui.input(|i| {
                // Condition A: Left mouse button is held down (dragging to select text)
                let is_dragging_select = i.pointer.is_decidedly_dragging();

                // Condition B: There are keyboard key presses or text input (typing or moving cursor with arrow keys)
                // We need to exclude pure scroll wheel events and only respond to key-related events
                let is_typing_or_navigating = i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Paste(_)
                    )
                });

                is_dragging_select || is_typing_or_navigating
            });

            // Execute scrolling logic
            if should_scroll_to_cursor && let Some(cursor_range) = output.cursor_range {
                let cursor_relative_rect = output.galley.pos_from_cursor(cursor_range.primary);

                // Convert to absolute screen position
                let global_cursor_rect =
                    cursor_relative_rect.translate(output.galley_pos.to_vec2());

                // Slightly expand the rectangle to provide visual padding
                let padded_rect = global_cursor_rect.expand(2.0);

                // Force ScrollArea to scroll to the cursor position
                ui.scroll_to_rect(padded_rect, None);
            }
        }
    }

    fn fix_macos_ime(output: &egui::text_edit::TextEditOutput, ui: &mut Ui) {
        if cfg!(target_os = "macos")
            && output.response.has_focus()
            && let Some(cursor_range) = output.cursor_range
        {
            // 1. Calculate the absolute position of the cursor on the screen
            // output.galley_pos includes scroll offset and padding, making it the most accurate reference point
            let cursor_rect_in_galley = output.galley.pos_from_cursor(cursor_range.primary);
            let screen_cursor_rect = cursor_rect_in_galley.translate(output.galley_pos.to_vec2());

            // 2. Force override IME position
            ui.ctx().output_mut(|o| {
                // Construct a tiny rectangle representing the cursor position.
                const IME_CURSOR_RECT_WIDTH: f32 = 2.0;
                let ime_rect = egui::Rect::from_min_size(
                    screen_cursor_rect.min,
                    egui::vec2(IME_CURSOR_RECT_WIDTH, screen_cursor_rect.height()),
                );

                // Key: Set both rect (input area) and cursor_rect (cursor area) to this tiny rectangle
                // This "tricks" macOS into thinking the input area is only as big as the cursor,
                // causing the candidate window to appear right next to the cursor
                o.ime = Some(egui::output::IMEOutput {
                    rect: ime_rect,
                    cursor_rect: ime_rect,
                });
            });
        }
        // =========================================================
    }

    fn draw_underline_decoration_at_focus_line(
        &mut self,
        output: &egui::text_edit::TextEditOutput,
        ui: &mut Ui,
    ) {
        let editor_response = output.response.clone();

        // Capture the galley from the editor output
        self.last_galley = Some(output.galley.clone());

        // 3. Handle State & Draw Decoration
        self.is_focused = editor_response.has_focus();
        if let Some(cursor_range) = output.cursor_range {
            self.cursor_index = Some(cursor_range.primary.index);

            // Draw Underline
            if self.is_focused {
                let cursor_rect_in_galley = output.galley.pos_from_cursor(cursor_range.primary);

                // Translate relative galley coordinates to screen coordinates
                let screen_cursor_rect =
                    cursor_rect_in_galley.translate(output.galley_pos.to_vec2());

                // Define underline position
                let underline_y = screen_cursor_rect.max.y;
                let min_x = editor_response.rect.min.x;
                let max_x = editor_response.rect.max.x;

                ui.painter().add(egui::Shape::dashed_line(
                    &[
                        egui::pos2(min_x, underline_y),
                        egui::pos2(max_x, underline_y),
                    ],
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                    4.0, // dash_length
                    2.0, // gap_length
                ));
            }
        } else {
            self.cursor_index = None;
        }
    }

    fn highlight_matches(
        &self,
        output: &egui::text_edit::TextEditOutput,
        ui: &mut Ui,
        content: &str,
    ) {
        let cursor_range = match output.cursor_range {
            Some(range) => range,
            None => return,
        };

        // Get the selected text
        let start = cursor_range.primary.index.min(cursor_range.secondary.index);
        let end = cursor_range.primary.index.max(cursor_range.secondary.index);

        if start == end {
            return; // No selection
        }

        // Convert char indices to byte indices to get the substring
        let start_byte = content
            .char_indices()
            .nth(start)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let end_byte = content
            .char_indices()
            .nth(end)
            .map(|(i, _)| i)
            .unwrap_or(content.len());

        let selected_text = &content[start_byte..end_byte];

        // Skip if selection is just whitespace to avoid excessive highlighting
        if selected_text.trim().is_empty() {
            return;
        }

        // Find all matches
        // Optimization: For very large files this could be slow.
        // We find byte indices first, then convert to char indices for the galley.
        let mut matches = Vec::new();
        for (match_byte_start, part) in content.match_indices(selected_text) {
            let match_char_start = content[..match_byte_start].chars().count();
            let match_char_end = match_char_start + part.chars().count();
            matches.push(match_char_start..match_char_end);
        }

        if matches.is_empty() {
            return;
        }

        // Iterate rows to draw highlights
        let mut row_start_char_idx = 0;
        for row in &output.galley.rows {
            let row_char_count = row.char_count_excluding_newline();
            let row_end_char_idx = row_start_char_idx + row_char_count; // exclusive of newline

            // Check overlaps with matches
            for range in &matches {
                let match_start = range.start;
                let match_end = range.end;

                // Logic: intersection of [match_start, match_end) and [row_start_char_idx, row_end_char_idx)
                let intersect_start = match_start.max(row_start_char_idx);
                let intersect_end = match_end.min(row_end_char_idx);

                if intersect_start < intersect_end {
                    // Found overlap in this row
                    let rel_start = intersect_start - row_start_char_idx;
                    let rel_end = intersect_end - row_start_char_idx;

                    let x_start = row.x_offset(rel_start);
                    let x_end = row.x_offset(rel_end);

                    // Construct rect
                    // row.rect() is relative to galley origin.
                    // output.galley.rect.min is usually (0,0) relative to galley?
                    // Wait, row.rect() is relative to galley start.
                    // output.galley_pos is the screen position of galley start.

                    let screen_min =
                        output.galley_pos + row.rect().min.to_vec2() + egui::vec2(x_start, 0.0);
                    let screen_max = output.galley_pos
                        + row.rect().min.to_vec2()
                        + egui::vec2(x_end, row.rect().height());

                    let highlight_rect = egui::Rect::from_min_max(screen_min, screen_max);

                    // Draw
                    let fill_color = ui.visuals().selection.bg_fill.linear_multiply(0.3);
                    let stroke =
                        egui::Stroke::new(1.0, ui.visuals().selection.bg_fill.linear_multiply(0.6));
                    ui.painter().rect(
                        highlight_rect,
                        2.0,
                        fill_color,
                        stroke,
                        egui::StrokeKind::Middle,
                    );
                }
            }

            // Advance counters
            row_start_char_idx += row_char_count;
            if row.ends_with_newline {
                row_start_char_idx += 1;
            }
        }
    }

    fn highlight_search_matches(
        &self,
        output: &egui::text_edit::TextEditOutput,
        ui: &mut Ui,
        content: &str,
    ) {
        if self.search_replace.matches.is_empty() {
            return;
        }

        // Convert byte indices to char indices for all matches
        let mut char_matches = Vec::new();
        for (start_byte, end_byte) in &self.search_replace.matches {
            let start_char = content[..*start_byte].chars().count();
            let end_char = content[..*end_byte].chars().count();
            char_matches.push(start_char..end_char);
        }

        // Highlight current match differently
        let current_match_range =
            if let Some((start_byte, end_byte)) = self.search_replace.current_match {
                let start_char = content[..start_byte].chars().count();
                let end_char = content[..end_byte].chars().count();
                Some(start_char..end_char)
            } else {
                None
            };

        // Iterate rows to draw highlights
        let mut row_start_char_idx = 0;
        for row in &output.galley.rows {
            let row_char_count = row.char_count_excluding_newline();
            let row_end_char_idx = row_start_char_idx + row_char_count;

            // Check overlaps with matches
            for range in &char_matches {
                let match_start = range.start;
                let match_end = range.end;

                let intersect_start = match_start.max(row_start_char_idx);
                let intersect_end = match_end.min(row_end_char_idx);

                if intersect_start < intersect_end {
                    let rel_start = intersect_start - row_start_char_idx;
                    let rel_end = intersect_end - row_start_char_idx;

                    let x_start = row.x_offset(rel_start);
                    let x_end = row.x_offset(rel_end);

                    let screen_min =
                        output.galley_pos + row.rect().min.to_vec2() + egui::vec2(x_start, 0.0);
                    let screen_max = output.galley_pos
                        + row.rect().min.to_vec2()
                        + egui::vec2(x_end, row.rect().height());

                    let highlight_rect = egui::Rect::from_min_max(screen_min, screen_max);

                    // Different color for current match vs other matches
                    let is_current = current_match_range.as_ref() == Some(range);
                    let (fill_color, stroke_color) = if is_current {
                        (
                            egui::Color32::from_rgb(255, 255, 0).linear_multiply(0.5), // Yellow for current
                            egui::Color32::from_rgb(200, 200, 0),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(200, 200, 255).linear_multiply(0.5), // Light blue for others
                            egui::Color32::from_rgb(150, 150, 200),
                        )
                    };

                    ui.painter().rect(
                        highlight_rect,
                        1.0,
                        fill_color,
                        egui::Stroke::new(1.0, stroke_color),
                        egui::StrokeKind::Middle,
                    );
                }
            }

            row_start_char_idx += row_char_count;
            if row.ends_with_newline {
                row_start_char_idx += 1;
            }
        }
    }

    fn add_context_menu(&mut self, output: &egui::text_edit::TextEditOutput, content: &mut String) {
        // Add context menu for copy-paste operations
        output.response.context_menu(|ui| {
            // Get selected text if any
            let selected_text = if let Some(cursor_range) = output.cursor_range {
                if cursor_range.is_empty() {
                    None
                } else {
                    let start = cursor_range.primary.index.min(cursor_range.secondary.index);
                    let end = cursor_range.primary.index.max(cursor_range.secondary.index);
                    let start_byte = content
                        .char_indices()
                        .nth(start)
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    let end_byte = content
                        .char_indices()
                        .nth(end)
                        .map(|(i, _)| i)
                        .unwrap_or(content.len());
                    Some(content[start_byte..end_byte].to_string())
                }
            } else {
                None
            };

            if ui.button("剪切").clicked() {
                if let Some(selected) = &selected_text {
                    ui.ctx().copy_text(selected.clone());
                    // Remove selected text
                    if let Some(cursor_range) = output.cursor_range {
                        let start = cursor_range.primary.index.min(cursor_range.secondary.index);
                        let end = cursor_range.primary.index.max(cursor_range.secondary.index);
                        let start_byte = content
                            .char_indices()
                            .nth(start)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        let end_byte = content
                            .char_indices()
                            .nth(end)
                            .map(|(i, _)| i)
                            .unwrap_or(content.len());
                        content.replace_range(start_byte..end_byte, "");
                    }
                } else {
                    ui.ctx().copy_text(content.clone());
                    content.clear();
                }
                ui.close();
            }
            if ui.button("复制").clicked() {
                if let Some(selected) = &selected_text {
                    ui.ctx().copy_text(selected.clone());
                } else {
                    ui.ctx().copy_text(content.clone());
                }
                ui.close();
            }
            if ui.button("粘贴").clicked() {
                // Request focus to ensure paste works
                output.response.request_focus();
                ui.close();
            }
        });
    }

    fn capture_ai_selection(
        &mut self,
        output: &egui::text_edit::TextEditOutput,
        content: &str,
        ui: &Ui,
    ) {
        let Some(cursor_range) = output.cursor_range else {
            return;
        };
        let start = cursor_range.primary.index.min(cursor_range.secondary.index);
        let end = cursor_range.primary.index.max(cursor_range.secondary.index);
        if start == end {
            if output.response.has_focus() && !self.inline_ai_open {
                self.selection_anchor = None;
            }
            return;
        }
        let Some(selected_text) = char_range_text(content, start, end) else {
            return;
        };
        if selected_text.trim().is_empty() {
            return;
        }
        let Some(screen_rect) = text_range_screen_rect(output, start, end) else {
            return;
        };

        if let Some(anchor) = self.selection_anchor.as_mut()
            && anchor.context.start_char == start
            && anchor.context.end_char == end
            && anchor.context.text == selected_text
        {
            anchor.screen_rect = screen_rect;
            anchor.stale = false;
            return;
        }

        let pointer_down = ui.input(|input| input.pointer.primary_down());
        if pointer_down {
            return;
        }
        self.next_selection_anchor_id = self.next_selection_anchor_id.wrapping_add(1).max(1);
        self.selection_anchor = Some(SelectionAnchor {
            context: AiSelectionContext {
                anchor_id: self.next_selection_anchor_id,
                start_char: start,
                end_char: end,
                text: selected_text,
            },
            screen_rect,
            stale: false,
        });
        self.inline_ai_open = false;
        self.inline_ai_draft.clear();
    }

    fn refresh_selection_anchor(&mut self) {
        let Some(anchor) = self.selection_anchor.as_mut() else {
            return;
        };
        if char_range_text(
            &self.content,
            anchor.context.start_char,
            anchor.context.end_char,
        )
        .as_deref()
            == Some(anchor.context.text.as_str())
        {
            anchor.stale = false;
            return;
        }

        let mut matches = self.content.match_indices(&anchor.context.text);
        let Some((byte_start, _)) = matches.next() else {
            anchor.stale = true;
            return;
        };
        if matches.next().is_some() {
            anchor.stale = true;
            return;
        }
        let start_char = self.content[..byte_start].chars().count();
        anchor.context.start_char = start_char;
        anchor.context.end_char = start_char + anchor.context.text.chars().count();
        anchor.stale = false;
    }

    fn show_selection_ai(&mut self, ctx: &egui::Context) -> Option<AiPanelAction> {
        self.refresh_selection_anchor();
        let anchor = self.selection_anchor.clone()?;
        let screen = ctx.content_rect();
        let button_pos = egui::pos2(
            (anchor.screen_rect.max.x + 6.0).min(screen.max.x - 72.0),
            (anchor.screen_rect.max.y + 4.0).min(screen.max.y - 32.0),
        );

        if !self.inline_ai_open {
            let clicked = egui::Area::new(egui::Id::new((
                "selection_ai_button",
                anchor.context.anchor_id,
            )))
            .order(egui::Order::Foreground)
            .fixed_pos(button_pos)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_rgb(239, 241, 237))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(195, 202, 192)))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(5, 3))
                    .show(ui, |ui| ui.small_button("问 AI").clicked())
                    .inner
            })
            .inner;
            if clicked {
                self.inline_ai_open = true;
                self.ai_panel.attach_selection(anchor.context.clone());
            }
            return None;
        }

        let width = (screen.width() - 24.0).clamp(220.0, 336.0);
        let estimated_height = 350.0;
        let x = (anchor.screen_rect.max.x + 10.0)
            .min(screen.max.x - width - 8.0)
            .max(screen.min.x + 8.0);
        let below = anchor.screen_rect.max.y + 8.0;
        let y = if below + estimated_height <= screen.max.y {
            below
        } else {
            (anchor.screen_rect.min.y - estimated_height - 8.0).max(screen.min.y + 8.0)
        };

        let messages = self.ai_panel.selection_messages(anchor.context.anchor_id);
        let status = self.ai_panel.request_status_for(anchor.context.anchor_id);
        let partial = self
            .ai_panel
            .partial_response_for(anchor.context.anchor_id)
            .map(str::to_string);
        let is_processing = self.ai_panel.is_processing_for(anchor.context.anchor_id);
        let any_processing = self.ai_panel.is_processing;
        let request_id = self.ai_panel.active_request_id();
        let mut close = false;
        let mut open_side = false;
        let mut send = false;
        let mut stop = false;

        egui::Area::new(egui::Id::new((
            "selection_ai_popover",
            anchor.context.anchor_id,
        )))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(249, 249, 246))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(191, 196, 188)))
                .corner_radius(7.0)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("讨论选区").size(11.0).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("关闭讨论框").clicked() {
                                close = true;
                            }
                            if ui.small_button("在侧栏打开").clicked() {
                                open_side = true;
                            }
                        });
                    });
                    ui.label(
                        RichText::new(format!(
                            "“{}”",
                            preview_selection_text(&anchor.context.text, 100)
                        ))
                        .size(10.0)
                        .italics()
                        .color(Color32::from_gray(88)),
                    );

                    if !messages.is_empty() || partial.is_some() {
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt(("selection_thread", anchor.context.anchor_id))
                            .max_height(150.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for message in messages.iter().rev().take(6).rev() {
                                    let role = if message.role == "user" { "你" } else { "AI" };
                                    ui.label(
                                        RichText::new(role)
                                            .size(9.0)
                                            .strong()
                                            .color(Color32::from_gray(104)),
                                    );
                                    ui.label(
                                        RichText::new(&message.content)
                                            .size(11.0)
                                            .color(Color32::from_gray(48)),
                                    );
                                    ui.add_space(5.0);
                                }
                                if let Some(partial) = &partial {
                                    ui.label(
                                        RichText::new("AI · 正在回复")
                                            .size(9.0)
                                            .strong()
                                            .color(Color32::from_gray(104)),
                                    );
                                    ui.label(RichText::new(partial).size(11.0));
                                }
                            });
                    }

                    if let Some(status) = &status {
                        ui.label(
                            RichText::new(status)
                                .size(9.0)
                                .color(Color32::from_rgb(78, 91, 74)),
                        );
                    }
                    if anchor.stale {
                        ui.label(
                            RichText::new("选区已变化，请重新选择后继续")
                                .size(10.0)
                                .color(Color32::from_rgb(126, 56, 50)),
                        );
                    }

                    let input = egui::TextEdit::multiline(&mut self.inline_ai_draft)
                        .id(egui::Id::new((
                            "selection_ai_input",
                            anchor.context.anchor_id,
                        )))
                        .hint_text("就这段文字提问…")
                        .desired_rows(2);
                    let response = ui.add_sized([width, 48.0], input);
                    let shortcut = response.has_focus()
                        && ctx.input(|input| {
                            input.modifiers.command && input.key_pressed(egui::Key::Enter)
                        });
                    ui.horizontal(|ui| {
                        if is_processing {
                            if ui.button("停止").clicked() {
                                stop = true;
                            }
                        } else if any_processing {
                            ui.add_enabled(false, egui::Button::new("AI 正在处理另一条消息"));
                        } else if ui
                            .add_enabled(
                                !anchor.stale && !self.inline_ai_draft.trim().is_empty(),
                                egui::Button::new("发送"),
                            )
                            .clicked()
                        {
                            send = true;
                        }
                        ui.label(
                            RichText::new("⌘/Ctrl + Enter")
                                .size(9.0)
                                .color(Color32::from_gray(128)),
                        );
                    });
                    if shortcut && !any_processing && !anchor.stale {
                        send = true;
                    }
                });
        });

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if open_side {
            self.ai_panel.is_visible = true;
            self.ai_panel.attach_selection(anchor.context.clone());
        }
        if close {
            self.inline_ai_open = false;
        }
        if stop {
            return request_id.map(|request_id| AiPanelAction::CancelRequest { request_id });
        }
        if send {
            let message = std::mem::take(&mut self.inline_ai_draft);
            return self
                .ai_panel
                .send_selection_message(message, anchor.context.clone());
        }
        None
    }

    fn render_sidebar(
        &mut self,
        sidebar_origin: Pos2,
        sidebar_width: f32,
        content_height: f32,
        galley_pos: Pos2,
        ui: &mut Ui,
    ) {
        // Delegate sidebar rendering to Sidebar component
        // Calculate height based on content height and visible area
        let min_height = ui.clip_rect().height().max(600.0);
        let sidebar_height = content_height.max(min_height);

        let sidebar_rect =
            Rect::from_min_size(sidebar_origin, Vec2::new(sidebar_width, sidebar_height));

        if let Some(galley) = &self.last_galley {
            let clip_rect = ui.clip_rect();
            let text_offset = galley_pos;
            self.sidebar.show(
                ui,
                &self.content,
                galley,
                sidebar_rect,
                clip_rect,
                text_offset,
            );
        }
    }

    // AI Panel control methods
    pub fn get_ai_panel_mut(&mut self) -> &mut AiPanel {
        &mut self.ai_panel
    }

    pub fn begin_ai_request(
        &mut self,
        request_id: AiRequestId,
        editor_snapshot: String,
        selection: Option<AiSelectionContext>,
    ) {
        self.ai_panel
            .begin_request(request_id, editor_snapshot, selection);
    }

    pub fn apply_ai_progress(&mut self, request_id: AiRequestId, event: AiProgressEvent) {
        self.ai_panel.apply_progress(request_id, event);
    }

    pub fn set_ai_response(&mut self, request_id: AiRequestId, response: AiAgentResponse) {
        self.ai_panel.set_response(request_id, response);
    }

    pub fn set_ai_error(&mut self, request_id: AiRequestId, error: AiError) {
        self.ai_panel.set_error(request_id, error);
    }

    pub fn cancel_ai_request(&mut self, request_id: AiRequestId) {
        self.ai_panel.cancel_request(request_id);
    }

    pub fn preview_ai_edit(&mut self, proposal_index: usize) {
        self.ai_panel.preview_edit(proposal_index);
        self.ai_preview_scrolled_to = None;
    }

    pub fn reject_ai_edit(&mut self, proposal_index: usize) {
        self.ai_panel.reject_edit(proposal_index);
        self.ai_preview_scrolled_to = None;
    }

    pub fn navigate_ai_edit(&mut self, direction: i32) {
        self.ai_panel.navigate_edit(direction);
        self.ai_preview_scrolled_to = None;
    }

    pub fn apply_all_ai_edits(&mut self) -> (usize, usize) {
        let proposals = self.ai_panel.ready_edit_previews();
        let mut applied = 0;
        let mut failed = 0;
        for proposal in proposals {
            let result = self.apply_ai_edit(
                &proposal.base_content,
                &proposal.original_text,
                &proposal.replacement_text,
            );
            if result.is_ok() {
                applied += 1;
            } else {
                failed += 1;
            }
            self.ai_panel
                .set_edit_result(proposal.proposal_index, result);
        }
        self.ai_preview_scrolled_to = None;
        (applied, failed)
    }

    pub fn reject_all_ai_edits(&mut self) {
        self.ai_panel.reject_all_edits();
        self.ai_preview_scrolled_to = None;
    }

    pub fn apply_ai_edit(
        &mut self,
        base_content: &str,
        original_text: &str,
        replacement_text: &str,
    ) -> Result<(), String> {
        let range = locate_ai_edit_range(&self.content, base_content, original_text)?;
        let before = self.content.clone();
        self.content.replace_range(range, replacement_text);
        let after = self.content.clone();
        self.ai_undo_stack.push(AiUndoEntry { before, after });
        if self.ai_undo_stack.len() > 20 {
            self.ai_undo_stack.remove(0);
        }
        self.cached_word_count = None;
        Ok(())
    }

    pub fn set_ai_edit_result(&mut self, proposal_index: usize, result: Result<(), String>) {
        self.ai_panel.set_edit_result(proposal_index, result);
    }

    // Search and replace functionality
    pub fn open_search_replace(&mut self) {
        self.search_replace.show_dialog = true;
    }

    fn show_search_replace_dialog(&mut self, ui: &mut Ui) {
        if !self.search_replace.show_dialog {
            // Clear search matches when content changes
            self.search_replace.matches.clear();
            self.search_replace.current_match = None;
            self.search_replace.match_index = 0;
            return;
        }

        let screen_rect = ui.ctx().content_rect();
        let pos = egui::pos2(screen_rect.max.x - 290.0, screen_rect.min.y + 26.0);

        egui::Window::new("查找替换")
            .title_bar(false)
            .fixed_pos(pos)
            .resizable(false)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("查找:");
                    ui.text_edit_singleline(&mut self.search_replace.search_text);
                });

                ui.horizontal(|ui| {
                    ui.label("替换:");
                    ui.text_edit_singleline(&mut self.search_replace.replace_text);
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.search_replace.case_sensitive, "区分大小写");
                    ui.checkbox(&mut self.search_replace.whole_word, "全词匹配");
                });

                ui.horizontal(|ui| {
                    if ui.button("查找").clicked() {
                        self.find_matches();
                    }
                    if ui.button("替换").clicked() {
                        self.replace_current();
                    }
                    if ui.button("全部替换").clicked() {
                        self.replace_all();
                    }
                    if ui.button("下一个").clicked() {
                        self.next_match();
                    }
                    if ui.button("上一个").clicked() {
                        self.previous_match();
                    }
                    if ui.button("退出").clicked() {
                        self.search_replace.show_dialog = false;
                    }
                });

                if !self.search_replace.matches.is_empty() {
                    ui.label(format!(
                        "找到 {} 个匹配 (当前: {})",
                        self.search_replace.matches.len(),
                        self.search_replace.match_index + 1
                    ));
                }
            });
    }

    fn find_matches(&mut self) {
        self.search_replace.matches.clear();
        self.search_replace.match_index = 0;
        self.search_replace.current_match = None;

        if self.search_replace.search_text.is_empty() {
            return;
        }

        let search = if self.search_replace.case_sensitive {
            self.search_replace.search_text.clone()
        } else {
            self.search_replace.search_text.to_lowercase()
        };

        let content = if self.search_replace.case_sensitive {
            self.content.clone()
        } else {
            self.content.to_lowercase()
        };

        let mut start = 0;
        while let Some(pos) = if self.search_replace.whole_word {
            self.find_whole_word(&content, &search, start)
        } else {
            content[start..].find(&search).map(|p| p + start)
        } {
            let end = pos + search.len();
            self.search_replace.matches.push((pos, end));
            start = end;
        }

        if !self.search_replace.matches.is_empty() {
            self.search_replace.current_match = Some(self.search_replace.matches[0]);
        }
    }

    fn find_whole_word(&self, content: &str, search: &str, start: usize) -> Option<usize> {
        let chars: Vec<char> = content.chars().collect();
        let search_chars: Vec<char> = search.chars().collect();

        for i in start..chars.len().saturating_sub(search_chars.len()) {
            // Check if this is a word boundary at start
            let is_word_start = i == 0 || !chars[i - 1].is_alphanumeric();
            // Check if this is a word boundary at end
            let is_word_end = i + search_chars.len() == chars.len()
                || !chars[i + search_chars.len()].is_alphanumeric();

            if is_word_start && is_word_end {
                let mut matches = true;
                for (j, &ch) in search_chars.iter().enumerate() {
                    if chars[i + j] != ch {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    return Some(i);
                }
            }
        }
        None
    }

    fn next_match(&mut self) {
        if self.search_replace.matches.is_empty() {
            return;
        }
        self.search_replace.match_index =
            (self.search_replace.match_index + 1) % self.search_replace.matches.len();
        self.search_replace.current_match =
            Some(self.search_replace.matches[self.search_replace.match_index]);
    }

    fn previous_match(&mut self) {
        if self.search_replace.matches.is_empty() {
            return;
        }
        self.search_replace.match_index = if self.search_replace.match_index == 0 {
            self.search_replace.matches.len() - 1
        } else {
            self.search_replace.match_index - 1
        };
        self.search_replace.current_match =
            Some(self.search_replace.matches[self.search_replace.match_index]);
    }

    fn replace_current(&mut self) {
        if let Some((start, end)) = self.search_replace.current_match {
            self.content
                .replace_range(start..end, &self.search_replace.replace_text);
            // Update matches after replacement
            self.find_matches();
            // Try to find the next match at the same position or after
            if self.search_replace.match_index < self.search_replace.matches.len() {
                self.search_replace.current_match =
                    Some(self.search_replace.matches[self.search_replace.match_index]);
            } else if !self.search_replace.matches.is_empty() {
                self.search_replace.match_index = 0;
                self.search_replace.current_match = Some(self.search_replace.matches[0]);
            }
        }
    }

    fn replace_all(&mut self) {
        if self.search_replace.search_text.is_empty() {
            return;
        }

        let mut new_content = String::new();
        let mut last_end = 0;

        for (start, end) in &self.search_replace.matches {
            new_content.push_str(&self.content[last_end..*start]);
            new_content.push_str(&self.search_replace.replace_text);
            last_end = *end;
        }
        new_content.push_str(&self.content[last_end..]);

        self.content = new_content;
        self.search_replace.matches.clear();
        self.search_replace.current_match = None;
        self.search_replace.match_index = 0;
        self.cached_word_count = None; // Invalidate cache
    }
}

fn char_range_text(content: &str, start: usize, end: usize) -> Option<String> {
    if start >= end {
        return None;
    }
    let start_byte = content
        .char_indices()
        .nth(start)
        .map(|(index, _)| index)
        .unwrap_or(content.len());
    let end_byte = content
        .char_indices()
        .nth(end)
        .map(|(index, _)| index)
        .unwrap_or(content.len());
    (start_byte < end_byte).then(|| content[start_byte..end_byte].to_string())
}

fn text_range_screen_rect(
    output: &egui::text_edit::TextEditOutput,
    start: usize,
    end: usize,
) -> Option<Rect> {
    let mut row_start = 0;
    let mut combined: Option<Rect> = None;
    for row in &output.galley.rows {
        let row_chars = row.char_count_excluding_newline();
        let row_end = row_start + row_chars;
        let intersect_start = start.max(row_start);
        let intersect_end = end.min(row_end);
        if intersect_start < intersect_end {
            let x_start = row.x_offset(intersect_start - row_start);
            let x_end = row.x_offset(intersect_end - row_start);
            let min = output.galley_pos + row.rect().min.to_vec2() + egui::vec2(x_start, 0.0);
            let max = output.galley_pos
                + row.rect().min.to_vec2()
                + egui::vec2(x_end, row.rect().height());
            let rect = Rect::from_min_max(min, max);
            combined = Some(combined.map_or(rect, |combined| combined.union(rect)));
        }
        row_start = row_end + usize::from(row.ends_with_newline);
    }
    combined
}

fn preview_selection_text(text: &str, limit: usize) -> String {
    if limit == 0 {
        return if text.chars().any(|c| !c.is_whitespace()) {
            "…".to_string()
        } else {
            String::new()
        };
    }

    let mut preview = String::with_capacity(limit.saturating_add(4));
    let mut chars = text.chars();
    let mut char_count = 0;
    let mut last_was_whitespace = true;

    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            if last_was_whitespace || char_count == 0 {
                last_was_whitespace = true;
                continue;
            }
            preview.push(' ');
            last_was_whitespace = true;
        } else {
            preview.push(c);
            last_was_whitespace = false;
        }
        char_count += 1;
        if char_count >= limit {
            break;
        }
    }

    let preview = preview.trim_end();
    if chars.next().is_some() {
        format!("{}…", preview)
    } else {
        preview.to_string()
    }
}

fn preview_diff_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let preview = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{}\n…", preview)
    } else {
        preview
    }
}

fn locate_ai_edit_range(
    content: &str,
    base_content: &str,
    original_text: &str,
) -> Result<Range<usize>, String> {
    if original_text.is_empty() {
        return Err("模型没有提供可定位的原文".to_string());
    }

    let mut base_matches = base_content.match_indices(original_text);
    if base_matches.next().is_none() {
        return Err("提案引用的原文不在请求时的文档中".to_string());
    }
    if base_matches.next().is_some() {
        return Err("提案引用的原文在请求时并不唯一，请让 AI 缩小修改范围".to_string());
    }

    let mut matches = content.match_indices(original_text);
    let Some((start, _)) = matches.next() else {
        return Err("目标文字已经发生变化，无法安全应用这项修改".to_string());
    };
    if matches.next().is_some() {
        return Err("当前正文中出现了多处相同原文，无法安全定位这项修改".to_string());
    }

    Ok(start..start + original_text.len())
}

#[cfg(test)]
fn build_ai_inline_diff(
    content: &str,
    base_content: &str,
    original_text: &str,
    replacement_text: &str,
) -> Result<AiInlineDiff, String> {
    let range = locate_ai_edit_range(content, base_content, original_text)?;
    let mut segments = Vec::new();
    let prefix = &content[..range.start];
    let suffix = &content[range.end..];
    let original_start = prefix.chars().count();
    let mut display_cursor = original_start;
    let mut change_start = None;

    if !prefix.is_empty() {
        segments.push(AiDiffSegment {
            text: prefix.to_string(),
            kind: AiDiffSegmentKind::Context,
        });
    }

    for change in TextDiff::from_chars(original_text, replacement_text).iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Equal => AiDiffSegmentKind::Context,
            ChangeTag::Delete => AiDiffSegmentKind::Removed,
            ChangeTag::Insert => AiDiffSegmentKind::Added,
        };
        if !matches!(change.tag(), ChangeTag::Equal) && change_start.is_none() {
            change_start = Some(display_cursor);
        }
        display_cursor += change.value().chars().count();
        segments.push(AiDiffSegment {
            text: change.value().to_string(),
            kind,
        });
    }

    if !suffix.is_empty() {
        segments.push(AiDiffSegment {
            text: suffix.to_string(),
            kind: AiDiffSegmentKind::Context,
        });
    }

    let text = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    Ok(AiInlineDiff {
        text,
        segments,
        change_start: change_start.unwrap_or(original_start),
    })
}

fn ai_live_diff_layout_job(
    ui: &Ui,
    text: &str,
    removed_range: Option<&Range<usize>>,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    let font_id = egui::FontId::monospace(14.0);
    let normal = egui::TextFormat {
        font_id: font_id.clone(),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();

    if let Some(range) = removed_range
        && range.start <= range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
    {
        job.append(&text[..range.start], 0.0, normal.clone());
        job.append(
            &text[range.clone()],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: Color32::from_rgb(126, 52, 52),
                background: Color32::from_rgb(250, 226, 224),
                strikethrough: egui::Stroke::new(1.0, Color32::from_rgb(126, 52, 52)),
                ..Default::default()
            },
        );
        job.append(&text[range.end..], 0.0, normal);
    } else {
        job.append(text, 0.0, normal);
    }
    job.wrap.max_width = wrap_width;
    job.wrap.break_anywhere = false;
    job
}

fn show_ai_edit_overlay(
    ctx: &egui::Context,
    output: &egui::text_edit::TextEditOutput,
    proposal: &AiEditPreview,
    location: &Result<Range<usize>, String>,
) -> Option<AiPanelAction> {
    let anchor_rect = location
        .as_ref()
        .ok()
        .and_then(|range| {
            let text = output.galley.text();
            if is_valid_text_byte_range(text, range) {
                let start = text[..range.start].chars().count();
                let end = start + text[range.clone()].chars().count();
                text_range_screen_rect(output, start, end)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            Rect::from_min_size(
                output.response.rect.right_top(),
                egui::vec2(1.0, output.response.rect.height().min(24.0)),
            )
        });
    let screen = ctx.content_rect();
    let width = (screen.width() - 24.0).clamp(220.0, 330.0);
    let right_x = anchor_rect.max.x + 10.0;
    let x = if right_x + width <= screen.max.x - 8.0 {
        right_x
    } else {
        (anchor_rect.min.x - width - 10.0)
            .max(screen.min.x + 8.0)
            .min(screen.max.x - width - 8.0)
    };
    let y = anchor_rect
        .min
        .y
        .max(screen.min.y + 8.0)
        .min(screen.max.y - 250.0);
    let mut action = None;

    egui::Area::new(egui::Id::new(("ai_edit_overlay", proposal.proposal_index)))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(249, 249, 246))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(190, 196, 187)))
                .corner_radius(7.0)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "修改 {} / {}",
                                proposal.review_position, proposal.review_total
                            ))
                            .size(11.0)
                            .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("›").on_hover_text("下一处修改").clicked() {
                                action = Some(AiPanelAction::NavigateEdit { direction: 1 });
                            }
                            if ui.small_button("‹").on_hover_text("上一处修改").clicked() {
                                action = Some(AiPanelAction::NavigateEdit { direction: -1 });
                            }
                        });
                    });

                    match location {
                        Ok(_) => {
                            ui.label(
                                RichText::new("− 原文已在正文中标记")
                                    .size(9.0)
                                    .color(Color32::from_rgb(126, 52, 52)),
                            );
                            Frame::new()
                                .fill(Color32::from_rgb(225, 242, 230))
                                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(181, 216, 190)))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::same(7))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "+ {}",
                                            preview_diff_text(&proposal.replacement_text, 1_600)
                                        ))
                                        .size(11.0)
                                        .color(Color32::from_rgb(39, 91, 59)),
                                    );
                                });
                            if !proposal.explanation.trim().is_empty() {
                                ui.label(
                                    RichText::new(&proposal.explanation)
                                        .size(10.0)
                                        .color(Color32::from_gray(88)),
                                );
                            }
                            ui.horizontal(|ui| {
                                if ui.button("接受修改").clicked() {
                                    action = Some(AiPanelAction::ApplyEdit {
                                        proposal_index: proposal.proposal_index,
                                        base_content: proposal.base_content.clone(),
                                        original_text: proposal.original_text.clone(),
                                        replacement_text: proposal.replacement_text.clone(),
                                    });
                                }
                                if ui.button("拒绝修改").clicked() {
                                    action = Some(AiPanelAction::RejectEdit {
                                        proposal_index: proposal.proposal_index,
                                    });
                                }
                            });
                            if proposal.review_total > 1 {
                                ui.horizontal(|ui| {
                                    if ui.small_button("全部接受").clicked() {
                                        action = Some(AiPanelAction::ApplyAllEdits);
                                    }
                                    if ui.small_button("全部拒绝").clicked() {
                                        action = Some(AiPanelAction::RejectAllEdits);
                                    }
                                });
                            }
                        }
                        Err(error) => {
                            ui.label(
                                RichText::new(error)
                                    .size(10.0)
                                    .color(Color32::from_rgb(126, 52, 52)),
                            );
                            if ui.button("关闭这项提案").clicked() {
                                action = Some(AiPanelAction::RejectEdit {
                                    proposal_index: proposal.proposal_index,
                                });
                            }
                        }
                    }
                });
        });
    action
}

fn is_valid_text_byte_range(text: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{2F800}'..='\u{2FA1F}').contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_count() {
        let mut editor = Editor::default();
        editor.set_content("Hello world".to_string());
        assert_eq!(editor.get_word_count(), 2);

        editor.set_content("你好世界".to_string());
        assert_eq!(editor.get_word_count(), 4);

        editor.set_content("Hello 世界".to_string());
        assert_eq!(editor.get_word_count(), 3);
    }

    #[test]
    fn test_format_basic() {
        let mut editor = Editor::default();
        editor.set_content("This is a paragraph.".to_string());
        editor.format();
        assert_eq!(editor.get_content(), "  This is a paragraph.");
    }

    #[test]
    fn test_format_multiple_paragraphs() {
        let mut editor = Editor::default();
        editor.set_content("First paragraph.\n\nSecond paragraph.".to_string());
        editor.format();
        assert_eq!(
            editor.get_content(),
            "  First paragraph.\n\n  Second paragraph."
        );
    }

    #[test]
    fn test_format_already_formatted() {
        let mut editor = Editor::default();
        editor.set_content("  Already formatted.".to_string());
        editor.format();
        // Should not add more spaces if already formatted
        assert_eq!(editor.get_content(), "  Already formatted.");
    }

    #[test]
    fn test_format_with_empty_lines() {
        let mut editor = Editor::default();
        editor.set_content("First paragraph.\n\n\n\nSecond paragraph.".to_string());
        editor.format();
        assert_eq!(
            editor.get_content(),
            "  First paragraph.\n\n\n\n  Second paragraph."
        );
    }

    #[test]
    fn test_format_mixed_content() {
        let mut editor = Editor::default();
        editor
            .set_content("  Already indented.\n\nNot indented.\n\nAnother paragraph.".to_string());
        editor.format();
        assert_eq!(
            editor.get_content(),
            "  Already indented.\n\n  Not indented.\n\n  Another paragraph."
        );
    }

    #[test]
    fn test_add_paragraph_indentation() {
        assert_eq!(
            Editor::add_paragraph_indentation("First paragraph.\n\nSecond paragraph."),
            "  First paragraph.\n\n  Second paragraph."
        );
        assert_eq!(
            Editor::add_paragraph_indentation("Already indented.\n\nNot indented."),
            "  Already indented.\n\n  Not indented."
        );
        assert_eq!(
            Editor::add_paragraph_indentation("Single line."),
            "  Single line."
        );
        assert_eq!(Editor::add_paragraph_indentation(""), "");
        assert_eq!(
            Editor::add_paragraph_indentation("    Extra spaces."),
            "  Extra spaces."
        );
    }

    #[test]
    fn preview_selection_text_compacts_text_lazily() {
        assert_eq!(
            preview_selection_text("  第一段\n\n第二段\t第三段  ", 7),
            "第一段 第二段…"
        );
        assert_eq!(preview_selection_text("短句", 100), "短句");
        assert_eq!(preview_selection_text("   ", 100), "");
    }

    #[test]
    fn text_byte_range_validation_rejects_stale_or_split_ranges() {
        let text = "a你b";

        assert!(is_valid_text_byte_range(text, &(1..4)));
        assert!(!is_valid_text_byte_range(text, &(1..99)));
        assert!(!is_valid_text_byte_range(text, &(3..2)));
        assert!(!is_valid_text_byte_range(text, &(2..4)));
    }

    #[test]
    fn ai_edit_applies_an_exact_unique_match() {
        let mut editor = Editor::default();
        editor.set_content("开头。旧句。结尾。".to_string());
        let base = editor.get_content();

        editor.apply_ai_edit(&base, "旧句", "新句").unwrap();

        assert_eq!(editor.get_content(), "开头。新句。结尾。");
    }

    #[test]
    fn ai_edit_rejects_a_stale_document_snapshot() {
        let mut editor = Editor::default();
        editor.set_content("旧内容".to_string());
        let base = editor.get_content();
        editor.set_content("用户刚刚改过".to_string());

        let error = editor.apply_ai_edit(&base, "旧内容", "新内容").unwrap_err();

        assert!(error.contains("目标文字已经发生变化"));
        assert_eq!(editor.get_content(), "用户刚刚改过");
    }

    #[test]
    fn ai_edit_rejects_an_ambiguous_match() {
        let mut editor = Editor::default();
        editor.set_content("重复，重复".to_string());
        let base = editor.get_content();

        let error = editor.apply_ai_edit(&base, "重复", "替换").unwrap_err();

        assert!(error.contains("不唯一"));
        assert_eq!(editor.get_content(), base);
    }

    #[test]
    fn ai_inline_diff_keeps_document_context_and_marks_character_changes() {
        let content = "开头。旧句。结尾。";

        let preview = build_ai_inline_diff(content, content, "旧句", "新句").unwrap();

        assert_eq!(preview.text, "开头。旧新句。结尾。");
        assert_eq!(preview.change_start, 3);
        assert!(preview.segments.iter().any(|segment| {
            segment.kind == AiDiffSegmentKind::Removed && segment.text == "旧"
        }));
        assert!(
            preview
                .segments
                .iter()
                .any(|segment| segment.kind == AiDiffSegmentKind::Added && segment.text == "新")
        );
    }

    #[test]
    fn ai_inline_diff_rejects_a_stale_preview() {
        let error = build_ai_inline_diff("用户改过", "旧内容", "旧", "新").unwrap_err();

        assert!(error.contains("目标文字已经发生变化"));
    }

    #[test]
    fn ai_edit_rebases_when_only_other_document_regions_changed() {
        let base = "目标句。后文。".to_string();
        let mut editor = Editor::default();
        editor.set_content("新增开头。目标句。后文。".to_string());

        editor.apply_ai_edit(&base, "目标句", "改写句").unwrap();

        assert_eq!(editor.get_content(), "新增开头。改写句。后文。");
        let undo = editor.ai_undo_stack.last().unwrap();
        assert_eq!(undo.before, "新增开头。目标句。后文。");
        assert_eq!(undo.after, "新增开头。改写句。后文。");
    }

    #[test]
    fn batch_review_applies_each_safe_proposal_and_keeps_undo_entries() {
        let mut editor = Editor::default();
        let base = "第一句。第二句。".to_string();
        editor.set_content(base.clone());
        editor.begin_ai_request(9, base.clone(), None);
        editor.set_ai_response(
            9,
            AiAgentResponse {
                content: String::new(),
                tool_calls: vec![
                    crate::backend::ai_backend::AiToolCall::ProposeDocumentEdit {
                        original_text: "第一句".to_string(),
                        replacement_text: "第一处".to_string(),
                        explanation: String::new(),
                    },
                    crate::backend::ai_backend::AiToolCall::ProposeDocumentEdit {
                        original_text: "第二句".to_string(),
                        replacement_text: "第二处".to_string(),
                        explanation: String::new(),
                    },
                ],
            },
        );

        assert_eq!(editor.apply_all_ai_edits(), (2, 0));
        assert_eq!(editor.get_content(), "第一处。第二处。");
        assert_eq!(editor.ai_undo_stack.len(), 2);
    }
}
