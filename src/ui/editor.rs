use egui::{Galley, Rect, Sense, Ui, Vec2};
use std::sync::Arc;

use super::ai_panel::{AiPanel, AiPanelAction};
use super::sidebar::Sidebar;
use crate::backend::sidebar_backend::Mark;
use std::collections::HashMap;
use std::path::PathBuf;

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
            let output = egui::TextEdit::multiline(&mut content)
                .id(id)
                .frame(false)
                .desired_width(available_width)
                .desired_rows(30)
                .font(egui::FontId::monospace(14.0))
                .show(ui);

            // =========================================================
            //   enable auto-scroll to cursor when typing or selecting
            // =========================================================
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

            // =========================================================
            //  Critical Fixes on my macOS M1 Machine for IME candidate window Positioning
            // =========================================================
            if cfg!(target_os = "macos")
                && output.response.has_focus()
                && let Some(cursor_range) = output.cursor_range
            {
                // 1. Calculate the absolute position of the cursor on the screen
                // output.galley_pos includes scroll offset and padding, making it the most accurate reference point
                let cursor_rect_in_galley = output.galley.pos_from_cursor(cursor_range.primary);
                let screen_cursor_rect =
                    cursor_rect_in_galley.translate(output.galley_pos.to_vec2());

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

            let editor_response = output.response;

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

            // Content is always taken back
            self.content = content;

            if editor_response.changed() {
                self.cached_word_count = None; // 标记为脏
            }

            if editor_response.clicked() {
                editor_response.request_focus();
            }

            // 3. Delegate sidebar rendering to Sidebar component
            // Calculate height based on content height and visible area
            let content_height = editor_response.rect.height();
            let min_height = ui.clip_rect().height().max(600.0);
            let sidebar_height = content_height.max(min_height);

            let sidebar_rect =
                Rect::from_min_size(sidebar_origin, Vec2::new(sidebar_width, sidebar_height));

            if let Some(galley) = &self.last_galley {
                let clip_rect = ui.clip_rect();
                let text_offset = output.galley_pos;
                self.sidebar.show(
                    ui,
                    &self.content,
                    galley,
                    sidebar_rect,
                    clip_rect,
                    text_offset,
                );
            }
        });

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
