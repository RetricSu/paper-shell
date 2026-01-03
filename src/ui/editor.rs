use egui::{Galley, Pos2, Rect, Sense, Ui, Vec2};
use std::sync::Arc;

use super::ai_panel::{AiPanel, AiPanelAction};
use super::sidebar::Sidebar;
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
    // Search and replace state
    search_replace: SearchReplaceState,
}

impl Editor {
    pub fn show(&mut self, ui: &mut Ui) -> Option<AiPanelAction> {
        let ai_action = None;
        let mut content = std::mem::take(&mut self.content);
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

            // 2. Editor Area
            let mut layouter = |ui: &Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut job = egui::text::LayoutJob::simple(
                    string.as_str().to_owned(),
                    egui::FontId::monospace(14.0),
                    ui.visuals().text_color(),
                    wrap_width,
                );
                // "Punch Tailing" fix: changing break_anywhere to false prevents
                // breaks in the middle of words (and ideally keeps punctuation with text).
                job.wrap.break_anywhere = false;
                ui.painter().layout_job(job)
            };

            let output = egui::TextEdit::multiline(&mut content)
                .id(id)
                .frame(false)
                .desired_width(available_width)
                .desired_rows(30)
                .layouter(&mut layouter)
                .show(ui);

            // =========================================================
            //   enable auto-scroll to cursor when typing or selecting
            // =========================================================
            Self::enable_scroll_to_cursor(ui, &output);
            Self::fix_macos_ime(&output, ui);
            self.draw_underline_decoration_at_focus_line(&output, ui);
            self.highlight_matches(&output, ui, &content);
            self.highlight_search_matches(&output, ui, &content);
            self.add_context_menu(&output, &mut content);

            // Capture the galley from the editor output
            self.last_galley = Some(output.galley.clone());
            // Content is always taken back
            self.content = content;

            let editor_response = &output.response;

            if editor_response.changed() {
                self.cached_word_count = None;
                // Clear search matches when content changes
                self.search_replace.matches.clear();
                self.search_replace.current_match = None;
                self.search_replace.match_index = 0;
            }
            if editor_response.clicked() {
                editor_response.request_focus();
            }

            // 3. Sidebar Rendering
            let content_height = editor_response.rect.height();
            self.render_sidebar(
                sidebar_origin,
                sidebar_width,
                content_height,
                output.galley_pos,
                ui,
            );
        });

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
                            egui::Color32::from_rgb(255, 255, 0), // Yellow for current
                            egui::Color32::from_rgb(200, 200, 0),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(200, 200, 255), // Light blue for others
                            egui::Color32::from_rgb(150, 150, 200),
                        )
                    };

                    ui.painter().rect(
                        highlight_rect,
                        2.0,
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

    pub fn set_ai_processing(&mut self, processing: bool) {
        self.ai_panel.set_processing(processing);
    }

    pub fn set_ai_response(&mut self, response: Vec<String>) {
        self.ai_panel.set_response(response);
    }

    pub fn apply_narrative_map(&mut self, map: Vec<String>) {
        self.ai_panel.apply_narrative_map(map);
    }

    pub fn narrative_map_changed(&self) -> bool {
        self.ai_panel.narrative_map_changed()
    }

    pub fn get_narrative_map(&self) -> Option<&Vec<String>> {
        self.ai_panel.get_narrative_map()
    }

    pub fn reset_narrative_map_changed(&mut self) {
        self.ai_panel.reset_narrative_map_changed();
    }

    // Search and replace functionality
    pub fn open_search_replace(&mut self) {
        self.search_replace.show_dialog = true;
    }

    fn show_search_replace_dialog(&mut self, ui: &mut Ui) {
        if !self.search_replace.show_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("查找替换")
            .open(&mut open)
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
                });

                if !self.search_replace.matches.is_empty() {
                    ui.label(format!(
                        "找到 {} 个匹配 (当前: {})",
                        self.search_replace.matches.len(),
                        self.search_replace.match_index + 1
                    ));
                }
            });

        if !open {
            self.search_replace.show_dialog = false;
            self.search_replace.matches.clear();
            self.search_replace.current_match = None;
            self.search_replace.match_index = 0;
        }
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
}
