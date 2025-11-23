use egui::{Color32, FontId, Galley, Pos2, Sense, TextFormat, Ui, Vec2, text::LayoutJob};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
struct Mark {
    note: String,
}

#[derive(Default)]
pub struct Editor {
    content: String,
    cursor_index: Option<usize>,
    marks: HashMap<usize, Mark>, // line_idx -> Mark
    last_galley: Option<Arc<Galley>>,
    popup_mark: Option<usize>, // line_idx of the open popup
}

impl Editor {
    pub fn show(&mut self, ui: &mut Ui) {
        let mut content = self.content.clone();
        let id = ui.make_persistent_id("main_editor");

        // Sidebar width
        let sidebar_width = 20.0;
        let available_width = ui.available_width() - sidebar_width;

        ui.horizontal(|ui| {
            // 1. Sidebar Area
            // Calculate height based on content or available space (but handle infinity)
            let sidebar_height = if let Some(galley) = &self.last_galley {
                galley.rect.height().max(ui.available_height().min(2000.0)) // Use galley height but at least fill screen if possible (clamped)
            } else {
                ui.available_height().min(2000.0) // Fallback for first frame
            };

            // Ensure we don't allocate infinite height
            let sidebar_height = if sidebar_height.is_infinite() {
                800.0 // Reasonable default if everything else fails
            } else {
                sidebar_height
            };

            let (response, painter) =
                ui.allocate_painter(Vec2::new(sidebar_width, sidebar_height), Sense::click());

            let sidebar_rect = response.rect;

            // 2. Editor Area with custom layouter
            let mut layouter = |ui: &Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut layout_job = LayoutJob::default();
                // Use default font settings from style or context
                let font_id = FontId::monospace(14.0); // Fallback or configurable
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
            self.last_galley = Some(output.galley);

            if let Some(state) = egui::TextEdit::load_state(ui.ctx(), id) {
                if let Some(range) = state.cursor.char_range() {
                    self.cursor_index = Some(range.primary.index);
                } else {
                    self.cursor_index = None;
                }
            }

            // Update content if changed
            if editor_response.changed() {
                self.content = content;
            }

            if editor_response.clicked() {
                editor_response.request_focus();
            }

            // 3. Render Sidebar Content (Marks)
            if let Some(galley) = &self.last_galley {
                // Draw right border line (separator)
                painter.line_segment(
                    [
                        Pos2::new(sidebar_rect.right(), sidebar_rect.top()),
                        Pos2::new(sidebar_rect.right(), sidebar_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                );

                let text = &self.content;

                // Safe line height retrieval
                let line_height = if !galley.rows.is_empty() {
                    galley.rows[0].rect().height()
                } else {
                    14.0 // Fallback
                };

                // Handle sidebar click ONCE
                let mut clicked_line: Option<usize> = None;
                if response.clicked()
                    && let Some(pointer_pos) = response.interact_pointer_pos()
                {
                    // Find which line was clicked based on Y position
                    let mut line_start_byte = 0;
                    let mut logical_line_idx = 0;

                    for line in text.split_inclusive('\n') {
                        let char_idx = text[..line_start_byte].chars().count();
                        let cursor = egui::text::CCursor::new(char_idx);
                        let rect = galley.pos_from_cursor(cursor);
                        let line_y = editor_response.rect.min.y + rect.center().y;

                        let dist = (pointer_pos.y - line_y).abs();
                        if dist < line_height / 2.0 {
                            clicked_line = Some(logical_line_idx);
                            break;
                        }

                        line_start_byte += line.len();
                        logical_line_idx += 1;
                    }

                    // Handle the trailing empty line if text ends with newline
                    if clicked_line.is_none() && text.ends_with('\n') {
                        let char_idx = text[..line_start_byte].chars().count();
                        let cursor = egui::text::CCursor::new(char_idx);
                        let rect = galley.pos_from_cursor(cursor);
                        let line_y = editor_response.rect.min.y + rect.center().y;

                        let dist = (pointer_pos.y - line_y).abs();
                        if dist < line_height / 2.0 {
                            clicked_line = Some(logical_line_idx);
                        }
                    }
                }

                // Process the click if we found a line
                if let Some(line_idx) = clicked_line {
                    if let std::collections::hash_map::Entry::Vacant(e) = self.marks.entry(line_idx)
                    {
                        // No mark - create one and open popup
                        e.insert(Mark::default());
                        self.popup_mark = Some(line_idx);
                    } else {
                        // Mark exists - toggle popup
                        if self.popup_mark == Some(line_idx) {
                            self.popup_mark = None;
                        } else {
                            self.popup_mark = Some(line_idx);
                        }
                    }
                }

                // Draw all marks and clickable hints
                let mut line_start_byte = 0;
                let mut logical_line_idx = 0;

                for line in text.split_inclusive('\n') {
                    let char_idx = text[..line_start_byte].chars().count();
                    let cursor = egui::text::CCursor::new(char_idx);
                    let rect = galley.pos_from_cursor(cursor);
                    let center = Pos2::new(
                        sidebar_rect.center().x,
                        editor_response.rect.min.y + rect.center().y,
                    );

                    // Draw subtle hint circle for clickable position
                    painter.circle_stroke(
                        center,
                        2.5,
                        egui::Stroke::new(0.5, ui.visuals().text_color().gamma_multiply(0.15)),
                    );

                    // Draw filled dot if this line has a mark
                    if self.marks.contains_key(&logical_line_idx) {
                        painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
                    }

                    line_start_byte += line.len();
                    logical_line_idx += 1;
                }

                // Handle the trailing empty line if text ends with newline
                if text.ends_with('\n') {
                    let char_idx = text[..line_start_byte].chars().count();
                    let cursor = egui::text::CCursor::new(char_idx);
                    let rect = galley.pos_from_cursor(cursor);
                    let center = Pos2::new(
                        sidebar_rect.center().x,
                        editor_response.rect.min.y + rect.center().y,
                    );

                    // Draw subtle hint circle for clickable position
                    painter.circle_stroke(
                        center,
                        2.5,
                        egui::Stroke::new(0.5, ui.visuals().text_color().gamma_multiply(0.15)),
                    );

                    if self.marks.contains_key(&logical_line_idx) {
                        painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
                    }
                }
            }
        });

        // 4. Render Popup if active
        if let Some(line_idx) = self.popup_mark {
            // We need to find the position again.
            // Ideally we store the rect or position during the loop, but recalculating is cheap enough.
            // Or we just render it centered on screen or near the mouse?
            // Anchored near the sidebar line is best.

            let mut open = true;
            let mark_note = self.marks.get_mut(&line_idx).map(|m| &mut m.note);

            if let Some(note) = mark_note {
                egui::Window::new(format!("Note for Line {}", line_idx + 1))
                    .open(&mut open)
                    .resizable(true)
                    .default_width(200.0)
                    .show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::multiline(note).desired_rows(5));
                    });
            }

            if !open {
                self.popup_mark = None;
            }
        }
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
        // Clear marks that are out of bounds? Or keep them?
        // For now, keep them, but they might point to non-existent lines.
        // A robust implementation would adjust marks on edit.
    }

    pub fn get_stats(&self) -> (usize, usize) {
        let count_words = |text: &str| -> usize {
            let mut count = 0;
            let mut in_word = false;
            for c in text.chars() {
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
        };

        let total_words = count_words(&self.content);
        let cursor_words = if let Some(idx) = self.cursor_index {
            let byte_index = self
                .content
                .char_indices()
                .map(|(i, _)| i)
                .nth(idx)
                .unwrap_or(self.content.len());

            count_words(&self.content[..byte_index])
        } else {
            0
        };
        (total_words, cursor_words)
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
    fn test_get_stats() {
        let mut editor = Editor::default();
        assert_eq!(editor.get_stats(), (0, 0));
        editor.set_content("Hello world".to_string());
        editor.cursor_index = Some(0);
        assert_eq!(editor.get_stats(), (2, 0));
    }
}
