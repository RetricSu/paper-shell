use egui::Ui;

#[derive(Default)]
pub struct Editor {
    content: String,
    cursor_index: Option<usize>,
}

impl Editor {
    pub fn show(&mut self, ui: &mut Ui) {
        // We want the editor to take up the available space and look like a paper
        // For now, just a simple text edit
        let id = ui.make_persistent_id("main_editor");
        let response = ui.add(
            egui::TextEdit::multiline(&mut self.content)
                .id(id)
                .frame(false) // No border for that clean look
                .desired_width(f32::INFINITY)
                .desired_rows(30),
        );

        if response.clicked() {
            response.request_focus();
        }

        if let Some(state) = egui::TextEdit::load_state(ui.ctx(), id) {
            if let Some(range) = state.cursor.char_range() {
                self.cursor_index = Some(range.primary.index);
            } else {
                self.cursor_index = None;
            }
        }
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
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
            // idx is a character index, we need to find the corresponding byte index
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
    // Basic CJK ranges
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || // CJK Unified Ideographs
    ('\u{3400}'..='\u{4DBF}').contains(&c) || // CJK Unified Ideographs Extension A
    ('\u{20000}'..='\u{2A6DF}').contains(&c) || // CJK Unified Ideographs Extension B
    ('\u{F900}'..='\u{FAFF}').contains(&c) || // CJK Compatibility Ideographs
    ('\u{2F800}'..='\u{2FA1F}').contains(&c) // CJK Compatibility Ideographs Supplement
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stats() {
        let mut editor = Editor::default();

        // Empty
        assert_eq!(editor.get_stats(), (0, 0));

        // Content
        editor.set_content("Hello world".to_string());

        // Cursor at 0
        editor.cursor_index = Some(0);
        assert_eq!(editor.get_stats(), (2, 0));

        // Cursor at 5 ("Hello| world")
        editor.cursor_index = Some(5);
        assert_eq!(editor.get_stats(), (2, 1));

        // Cursor at 6 ("Hello |world")
        editor.cursor_index = Some(6);
        assert_eq!(editor.get_stats(), (2, 1));

        // Cursor at 11 ("Hello world|")
        editor.cursor_index = Some(11);
        assert_eq!(editor.get_stats(), (2, 2));

        // Cursor None
        editor.cursor_index = None;
        assert_eq!(editor.get_stats(), (2, 0));
    }

    #[test]
    fn test_multiline_stats() {
        let mut editor = Editor::default();
        editor.set_content("Hello\nworld".to_string());

        // Cursor at 6 (after newline)
        editor.cursor_index = Some(6);
        assert_eq!(editor.get_stats(), (2, 1));
    }

    #[test]
    fn test_unicode_stats() {
        let mut editor = Editor::default();
        editor.set_content("你好 世界".to_string());

        // Total words: 4 (Each CJK character counts as 1 word)
        // Cursor at 2 chars ("你好| 世界") -> byte index 6.
        // "你好" -> 2 words.
        editor.cursor_index = Some(2);
        assert_eq!(editor.get_stats(), (4, 2));
    }

    #[test]
    fn test_cjk_word_count() {
        let mut editor = Editor::default();
        // "你好世界" (Hello World in Chinese without space) should be 4 words (or 2 depending on definition, but definitely not 1 if we count chars)
        // Usually in editors, CJK characters are counted individually or by semantic words.
        // Simple editors often count every CJK character as a word.
        // "你好世界" -> 4 words.
        editor.set_content("你好世界".to_string());

        let (total, _) = editor.get_stats();
        // Current implementation will return 1 because there are no spaces.
        // We expect it to be 4 (if counting chars) or at least > 1.
        assert_ne!(
            total, 1,
            "CJK text without spaces should count as multiple words"
        );
    }
}
