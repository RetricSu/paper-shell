use crate::sidebar_backend::{Mark, SidebarBackend};
use egui::{Color32, Galley, Pos2, Rect, Sense, Ui};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct Sidebar {
    marks: HashMap<usize, Mark>,
    popup_mark: Option<usize>,
    backend: Option<Arc<SidebarBackend>>,
    current_uuid: Option<String>,
}

impl Sidebar {
    pub fn set_backend(&mut self, backend: Arc<SidebarBackend>) {
        self.backend = Some(backend);
    }

    pub fn set_uuid(&mut self, uuid: String) {
        if self.current_uuid.as_ref() != Some(&uuid) {
            self.current_uuid = Some(uuid.clone());
            self.load_marks();
        }
    }

    fn load_marks(&mut self) {
        if let Some(backend) = &self.backend
            && let Some(uuid) = &self.current_uuid
        {
            match backend.load_marks(uuid) {
                Ok(marks) => self.marks = marks,
                Err(e) => eprintln!("Failed to load marks: {}", e),
            }
        }
    }

    fn save_marks(&self) {
        if let Some(backend) = &self.backend
            && let Some(uuid) = &self.current_uuid
            && let Err(e) = backend.save_marks(uuid, &self.marks)
        {
            eprintln!("Failed to save marks: {}", e);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        content: &str,
        galley: &Arc<Galley>,
        editor_rect: Rect,
        sidebar_rect: Rect,
    ) {
        let painter = ui.painter_at(sidebar_rect);

        // Draw right border line (separator)
        painter.line_segment(
            [
                Pos2::new(sidebar_rect.right(), sidebar_rect.top()),
                Pos2::new(sidebar_rect.right(), sidebar_rect.bottom()),
            ],
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        // Safe line height retrieval
        let line_height = if !galley.rows.is_empty() {
            galley.rows[0].rect().height()
        } else {
            14.0 // Fallback
        };

        // Handle sidebar click
        let response = ui.interact(sidebar_rect, ui.id().with("sidebar"), Sense::click());
        let mut clicked_line: Option<usize> = None;

        if response.clicked()
            && let Some(pointer_pos) = response.interact_pointer_pos()
        {
            // Find which line was clicked based on Y position
            for (current_line, _line) in content.split_inclusive('\n').enumerate() {
                let line_start_byte: usize = content
                    .split_inclusive('\n')
                    .take(current_line)
                    .map(|l| l.len())
                    .sum();

                let char_idx = content[..line_start_byte].chars().count();
                let cursor = egui::text::CCursor::new(char_idx);
                let rect = galley.pos_from_cursor(cursor);
                let line_y = editor_rect.min.y + rect.center().y;

                let dist = (pointer_pos.y - line_y).abs();
                if dist < line_height / 2.0 {
                    clicked_line = Some(current_line);
                    break;
                }
            }

            // Handle the trailing empty line if text ends with newline
            if clicked_line.is_none() && content.ends_with('\n') {
                let logical_line_idx = content.split_inclusive('\n').count();
                let line_start_byte = content.len();
                let char_idx = content[..line_start_byte].chars().count();
                let cursor = egui::text::CCursor::new(char_idx);
                let rect = galley.pos_from_cursor(cursor);
                let line_y = editor_rect.min.y + rect.center().y;

                let dist = (pointer_pos.y - line_y).abs();
                if dist < line_height / 2.0 {
                    clicked_line = Some(logical_line_idx);
                }
            }
        }

        // Process the click if we found a line
        if let Some(line_idx) = clicked_line {
            if let std::collections::hash_map::Entry::Vacant(e) = self.marks.entry(line_idx) {
                // No mark - create one and open popup
                e.insert(Mark::default());
                self.popup_mark = Some(line_idx);
                self.save_marks();
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
        for (current_line, _line) in content.split_inclusive('\n').enumerate() {
            let line_start_byte: usize = content
                .split_inclusive('\n')
                .take(current_line)
                .map(|l| l.len())
                .sum();

            let char_idx = content[..line_start_byte].chars().count();
            let cursor = egui::text::CCursor::new(char_idx);
            let rect = galley.pos_from_cursor(cursor);
            let center = Pos2::new(sidebar_rect.center().x, editor_rect.min.y + rect.center().y);

            // Draw subtle hint circle for clickable position
            painter.circle_stroke(
                center,
                2.5,
                egui::Stroke::new(1.0, ui.visuals().text_color().gamma_multiply(0.3)),
            );

            // Draw filled dot if this line has a mark
            if self.marks.contains_key(&current_line) {
                painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
            }
        }

        // Handle the trailing empty line if text ends with newline
        if content.ends_with('\n') {
            let logical_line_idx = content.split_inclusive('\n').count();
            let line_start_byte = content.len();
            let char_idx = content[..line_start_byte].chars().count();
            let cursor = egui::text::CCursor::new(char_idx);
            let rect = galley.pos_from_cursor(cursor);
            let center = Pos2::new(sidebar_rect.center().x, editor_rect.min.y + rect.center().y);

            // Draw subtle hint circle for clickable position
            painter.circle_stroke(
                center,
                2.5,
                egui::Stroke::new(1.0, ui.visuals().text_color().gamma_multiply(0.3)),
            );

            if self.marks.contains_key(&logical_line_idx) {
                painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
            }
        }

        // Render popup if active
        self.show_popup(ui, content);
    }

    fn show_popup(&mut self, ui: &Ui, content: &str) {
        if let Some(line_idx) = self.popup_mark {
            let mut open = true;

            // Calculate word count before this mark
            let words_before = self.calculate_words_before(content, line_idx);

            let mut changed = false;
            {
                let mark_note = self.marks.get_mut(&line_idx).map(|m| &mut m.note);

                if let Some(note) = mark_note {
                    egui::Window::new(
                        egui::RichText::new(format!("{} words", words_before)).size(11.0),
                    )
                    .open(&mut open)
                    .resizable(true)
                    .collapsible(false)
                    .default_width(300.0)
                    .title_bar(true)
                    .show(ui.ctx(), |ui| {
                        // Reduce spacing in the window
                        ui.spacing_mut().item_spacing.y = 4.0;

                        if ui
                            .add(
                                egui::TextEdit::multiline(note)
                                    .desired_rows(8)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }
            }

            if changed {
                self.save_marks();
            }

            if !open {
                self.popup_mark = None;
            }
        }
    }

    fn calculate_words_before(&self, content: &str, line_idx: usize) -> usize {
        let mut byte_count = 0;

        for (current_line, line) in content.split_inclusive('\n').enumerate() {
            if current_line >= line_idx {
                break;
            }
            byte_count += line.len();
        }

        // Use the same word counting logic
        let text_before = &content[..byte_count.min(content.len())];
        let mut count = 0;
        let mut in_word = false;
        for c in text_before.chars() {
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
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{2F800}'..='\u{2FA1F}').contains(&c)
}
