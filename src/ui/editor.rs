use egui::{FontId, Galley, Rect, Sense, TextFormat, Ui, Vec2, text::LayoutJob};
use std::sync::Arc;

use super::sidebar::Sidebar;
use crate::sidebar_backend::Mark;
use std::collections::HashMap;

#[derive(Default)]
pub struct Editor {
    content: String,
    cursor_index: Option<usize>,
    last_galley: Option<Arc<Galley>>,
    sidebar: Sidebar,
}

impl Editor {
    pub fn show(&mut self, ui: &mut Ui) {
        let mut content = self.content.clone();
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

            // 2. Editor Area with custom layouter
            let mut layouter = |ui: &Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut layout_job = LayoutJob::default();
                let font_id = FontId::monospace(14.0);
                layout_job.append(
                    string.as_str(),
                    0.0,
                    TextFormat {
                        font_id,
                        color: ui.visuals().text_color(),
                        ..Default::default()
                    },
                );
                layout_job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(layout_job))
            };

            let output = egui::TextEdit::multiline(&mut content)
                .id(id)
                .frame(false)
                .desired_width(available_width)
                .desired_rows(30)
                .layouter(&mut layouter)
                .show(ui);

            let editor_response = output.response;

            // Capture the galley from the editor output
            self.last_galley = Some(output.galley.clone());

            // 3. Handle State & Draw Decoration
            if let Some(cursor_range) = output.cursor_range {
                self.cursor_index = Some(cursor_range.primary.index);

                // Draw Underline
                if editor_response.has_focus() {
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

            // Update content if changed
            if editor_response.changed() {
                self.content = content;
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
                self.sidebar.show(
                    ui,
                    &self.content,
                    galley,
                    editor_response.rect,
                    sidebar_rect,
                );
            }
        });
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
    }

    pub fn get_word_count(&self) -> usize {
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

    pub fn get_stats(&self) -> (usize, usize) {
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

    /// Format the content by adding two spaces at the beginning of each paragraph.
    /// A paragraph is defined as a block of text separated by blank lines.
    pub fn format(&mut self) {
        let formatted = Self::add_paragraph_indentation(&self.content);
        self.content = formatted;
    }

    /// Helper function to add two spaces at the beginning of each paragraph
    fn add_paragraph_indentation(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut result = Vec::new();
        let mut is_new_paragraph = true;

        for line in lines {
            if line.trim().is_empty() {
                // Empty line - preserve it and mark next non-empty line as new paragraph
                result.push(line.to_string());
                is_new_paragraph = true;
            } else {
                // Non-empty line
                if is_new_paragraph && !line.starts_with("  ") {
                    // Add two spaces at the beginning if it's a new paragraph and doesn't already have them
                    result.push(format!("  {}", line));
                } else {
                    // Keep the line as-is
                    result.push(line.to_string());
                }
                is_new_paragraph = false;
            }
        }

        result.join("\n")
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
}
