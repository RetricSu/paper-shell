use crate::comments::{
    EditOperation, TextComment, apply_edit, reconcile_comments, sanitize_comments,
};
use egui::{Galley, Ui};
use std::sync::Arc;

use crate::ui::vim::{VimAction, VimMode, VimState};
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

struct CommentComposer {
    comment: TextComment,
    position: egui::Pos2,
    anchor_valid: bool,
    error: Option<String>,
}

struct OpenCommentDraft {
    id: String,
    text: String,
}

struct OpenComment {
    comments: Vec<OpenCommentDraft>,
    selected: usize,
    position: egui::Pos2,
}

pub struct Editor {
    content: String,
    reset_edit_state: bool,
    cursor_index: Option<usize>,
    selection: Option<egui::text::CCursorRange>,
    last_galley: Option<Arc<Galley>>,
    is_focused: bool,
    ime_composing: bool,
    current_file: Option<PathBuf>,
    current_file_total_time: u64,
    cached_word_count: Option<usize>,
    comments: Vec<TextComment>,
    comment_composer: Option<CommentComposer>,
    open_comment: Option<OpenComment>,
    // Search and replace state
    search_replace: SearchReplaceState,
    vim: VimState,
    should_focus: bool,
    pub font_size: f32,
    pub line_spacing: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            content: String::new(),
            reset_edit_state: false,
            cursor_index: None,
            selection: None,
            last_galley: None,
            is_focused: false,
            ime_composing: false,
            current_file: None,
            current_file_total_time: 0,
            cached_word_count: None,
            comments: Vec::new(),
            comment_composer: None,
            open_comment: None,
            search_replace: SearchReplaceState::default(),
            vim: VimState::default(),
            should_focus: false,
            font_size: 14.0,
            line_spacing: 1.5,
        }
    }
}

impl Editor {
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::default()
    }

    pub fn focus(&mut self) {
        self.should_focus = true;
    }

    #[cfg(test)]
    pub fn is_vim_enabled(&self) -> bool {
        self.vim.enabled
    }

    pub fn set_vim_mode(&mut self, enabled: bool) {
        self.vim.enabled = enabled;
        if !enabled {
            self.vim.mode = VimMode::Insert;
            self.vim.pending = None;
        }
    }

    pub fn get_vim_mode_label(&self) -> Option<&'static str> {
        if !self.vim.enabled {
            return None;
        }
        match self.vim.mode {
            VimMode::Normal => Some("普通"),
            VimMode::Visual => Some("可视"),
            VimMode::Insert => Some("插入"),
        }
    }

    /// Whether the search/replace dialog currently owns keyboard focus.
    pub fn is_search_dialog_open(&self) -> bool {
        self.search_replace.show_dialog
    }

    /// Feed a synthetic Escape event captured by `raw_input_hook` before egui's
    /// `Focus::begin_pass` clears the focused widget. This guarantees Vim mode
    /// switching and focus retention even when the OS/IME consumes Escape.
    pub fn process_pending_vim_escape(&mut self, ui: &mut Ui) {
        let id = ui.make_persistent_id("main_editor");
        // Re-anchor focus on the editor so Vim mode commands always have a target.
        ui.memory_mut(|m| m.request_focus(id));
        if self.vim.on_escape() == VimAction::ClearSelection
            && let Some(mut edit_state) = egui::text_edit::TextEditState::load(ui.ctx(), id)
            && let Some(range) = edit_state.cursor.char_range()
        {
            edit_state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(range.primary)));
            edit_state.store(ui.ctx(), id);
        }
        ui.ctx().request_repaint();
    }

    pub fn show(&mut self, ui: &mut Ui) {
        self.show_with_paragraph_dots(ui, true);
    }

    pub fn show_with_paragraph_dots(&mut self, ui: &mut Ui, show_dots: bool) {
        let mut content = std::mem::take(&mut self.content);
        let previous_content = content.clone();
        let previous_cursor = self.cursor_index;
        let previous_selection = self.selection.map(|range| {
            (
                range.primary.index.min(range.secondary.index),
                range.primary.index.max(range.secondary.index),
            )
        });
        let id = ui.make_persistent_id("main_editor");
        if self.reset_edit_state {
            egui::text_edit::TextEditState::default().store(ui.ctx(), id);
            self.reset_edit_state = false;
        }

        if self.should_focus {
            ui.memory_mut(|m| m.request_focus(id));
            self.should_focus = false;
        }

        let focused = ui.memory(|m| m.has_focus(id));
        if focused {
            ui.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            ui.input(|input| {
                for event in &input.events {
                    match event {
                        egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                            self.ime_composing = !text.is_empty();
                        }
                        egui::Event::Ime(
                            egui::ImeEvent::Enabled
                            | egui::ImeEvent::Commit(_)
                            | egui::ImeEvent::Disabled,
                        ) => {
                            self.ime_composing = false;
                        }
                        _ => {}
                    }
                }
            });
        } else {
            self.ime_composing = false;
        }
        if self.vim.enabled && focused {
            let cursor = egui::text_edit::TextEditState::load(ui.ctx(), id)
                .and_then(|state| state.cursor.char_range().map(|r| r.primary.index))
                .or(self.cursor_index)
                .unwrap_or(0);

            let action = ui.input_mut(|i| {
                self.vim
                    .process_events(&mut i.events, focused, &content, cursor)
            });

            match action {
                VimAction::ContentChanged {
                    new_content,
                    new_cursor,
                } => {
                    content = new_content;
                    self.cached_word_count = None;
                    self.search_replace.matches.clear();
                    self.search_replace.current_match = None;
                    self.search_replace.match_index = 0;

                    let mut edit_state =
                        egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
                    edit_state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(new_cursor),
                        )));
                    edit_state.store(ui.ctx(), id);
                }
                VimAction::ClearSelection => {
                    if let Some(mut edit_state) = egui::text_edit::TextEditState::load(ui.ctx(), id)
                        && let Some(range) = edit_state.cursor.char_range()
                    {
                        edit_state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(range.primary)));
                        edit_state.store(ui.ctx(), id);
                    }
                    ui.ctx().request_repaint();
                }
                VimAction::SetCursor(pos) => {
                    let mut edit_state =
                        egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
                    edit_state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(pos),
                        )));
                    edit_state.store(ui.ctx(), id);
                }
                VimAction::Yank(text) => {
                    ui.ctx().copy_text(text);
                }
                VimAction::Paste { count } => {
                    let clipboard = arboard::Clipboard::new()
                        .ok()
                        .and_then(|mut cb| cb.get_text().ok())
                        .unwrap_or_default();
                    let text = if !clipboard.is_empty() {
                        clipboard
                    } else {
                        self.vim.register().to_string()
                    };
                    if !text.is_empty() {
                        let pasted = text.repeat(count.max(1));
                        ui.input_mut(|i| i.events.push(egui::Event::Paste(pasted)));
                    }
                }
                VimAction::OpenSearch => {
                    self.open_search_replace();
                }
                VimAction::ModeChanged => {
                    ui.ctx().request_repaint();
                }
                VimAction::None => {}
            }
        }

        ui.horizontal_top(|ui| {
            let gutter_origin = ui.cursor().min;
            if show_dots {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.allocate_space(egui::vec2(14.0, 0.0));
            }
            let font_size = if self.font_size > 0.0 {
                self.font_size
            } else {
                14.0
            };
            let line_spacing = if self.line_spacing > 0.0 {
                self.line_spacing
            } else {
                1.5
            };
            let font_id = egui::FontId::new(font_size, egui::FontFamily::Proportional);
            let text_color = ui.visuals().text_color();

            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                let text_str = text.as_str();
                let line_height = font_size * line_spacing;
                let mut layout_job = egui::text::LayoutJob::simple(
                    text_str.to_string(),
                    font_id.clone(),
                    text_color,
                    wrap_width,
                );
                for section in &mut layout_job.sections {
                    section.format.line_height = Some(line_height);
                }
                ui.fonts_mut(|f| f.layout_job(layout_job))
            };

            let output = egui::TextEdit::multiline(&mut content)
                .id(id)
                .frame(false)
                .margin(egui::Margin::symmetric(2, 2))
                .desired_width(ui.available_width())
                .desired_rows(30)
                .font(font_id.clone())
                .layouter(&mut layouter)
                .lock_focus(true)
                .show(ui);

            if self.vim.enabled {
                let response_focused = output.response.has_focus();
                ui.memory_mut(|m| {
                    if (focused || response_focused) && !m.has_focus(id) {
                        m.request_focus(id);
                    }
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            tab: true,
                            escape: true,
                        },
                    );
                });
            }

            let selection = output.cursor_range.filter(|range| !range.is_empty());
            if self.selection != selection {
                // The title bar is drawn before the editor, so refresh its statistics.
                ui.ctx().request_repaint();
            }
            self.selection = selection;

            Self::enable_scroll_to_cursor(ui, &output);
            if self.vim.enabled && self.vim.mode == VimMode::Normal {
                ui.ctx().output_mut(|o| o.ime = None);
                Self::draw_vim_block_cursor(&output, ui);
            } else {
                Self::fix_macos_ime(&output, ui);
            }
            self.draw_underline_decoration_at_focus_line(&output, ui);
            let context_edit = self.add_context_menu(&output, &mut content);
            let selection_edit = self.show_selection_comment_button(&output, ui, &mut content);

            if content != previous_content {
                let edit = context_edit.or(selection_edit).or_else(|| {
                    Self::edit_operation_from_events(
                        ui,
                        &previous_content,
                        &content,
                        previous_selection,
                        previous_cursor,
                        output.cursor_range.map(|range| range.primary.index),
                    )
                });
                self.reconcile_document_edit(&previous_content, &content, edit);
                self.cached_word_count = None;
                self.search_replace.matches.clear();
                self.search_replace.current_match = None;
                self.search_replace.match_index = 0;
                self.selection = None;
            }

            self.highlight_matches(&output, ui, &content);
            self.highlight_search_matches(&output, ui, &content);
            self.highlight_comments(&output, ui);

            if show_dots {
                Self::draw_paragraph_dots(ui, &output, &content, gutter_origin);
            }
            self.last_galley = Some(output.galley.clone());
            self.content = content;

            let editor_response = &output.response;
            if editor_response.clicked() && !editor_response.has_focus() {
                editor_response.request_focus();
            }
        });

        self.show_comment_windows(ui);
        self.show_search_replace_dialog(ui);
    }

    /// Paint-only gutter: no interaction regions, marks, or note windows.
    fn draw_paragraph_dots(
        ui: &Ui,
        output: &egui::text_edit::TextEditOutput,
        content: &str,
        gutter_origin: egui::Pos2,
    ) {
        // Match the original 14-point gutter and its full-height hairline.
        let gutter_rect = egui::Rect::from_min_size(
            gutter_origin,
            egui::vec2(
                14.0,
                output
                    .response
                    .rect
                    .height()
                    .max(ui.clip_rect().height())
                    .max(600.0),
            ),
        );
        let painter = ui.painter_at(gutter_rect);
        painter.line_segment(
            [gutter_rect.right_top(), gutter_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, ui.visuals().text_color().gamma_multiply(0.18)),
        );
        let active_line = output
            .cursor_range
            .filter(|_| output.response.has_focus() && ui.input(|input| input.focused))
            .map(|range| {
                content
                    .chars()
                    .take(range.primary.index)
                    .filter(|c| *c == '\n')
                    .count()
            });
        let animate = ui.style().animation_time > 0.0;
        let pulse = if animate {
            ui.input(|input| ((input.time * std::f64::consts::TAU / 3.2).sin() as f32 + 1.0) * 0.5)
        } else {
            0.5
        };
        let color = ui.visuals().text_color();
        let active_color = crate::style::STATUS_GREEN;
        let mut logical_line = 0;
        let mut paragraph_start = true;
        for row in &output.galley.rows {
            let center = egui::pos2(
                gutter_rect.center().x,
                output.galley_pos.y + row.rect().center().y,
            );
            if paragraph_start && ui.clip_rect().contains(center) {
                if active_line == Some(logical_line) {
                    painter.circle_filled(
                        center,
                        3.5 + pulse * 2.8,
                        active_color.gamma_multiply(0.015 + pulse * 0.14),
                    );
                    painter.circle_stroke(
                        center,
                        2.5 + pulse * 0.45,
                        egui::Stroke::new(
                            1.0 + pulse * 0.3,
                            active_color.gamma_multiply(0.25 + pulse * 0.75),
                        ),
                    );
                    if animate {
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(40));
                    }
                } else {
                    painter.circle_stroke(
                        center,
                        2.5,
                        egui::Stroke::new(1.0_f32, color.gamma_multiply(0.3)),
                    );
                }
            }
            paragraph_start = row.ends_with_newline;
            if row.ends_with_newline {
                logical_line += 1;
            }
        }
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.selection = None;
        self.cursor_index = None;
        self.last_galley = None;
        self.ime_composing = false;
        self.search_replace.matches.clear();
        self.search_replace.current_match = None;
        self.search_replace.match_index = 0;
        self.comments.clear();
        self.comment_composer = None;
        self.open_comment = None;
        self.reset_edit_state = true;
        self.cached_word_count = None; // 清除缓存
    }

    pub fn comments(&self) -> &[TextComment] {
        &self.comments
    }

    pub fn set_comments(&mut self, mut comments: Vec<TextComment>) {
        sanitize_comments(&mut comments, &self.content);
        self.comments = comments;
        self.comment_composer = None;
        self.open_comment = None;
    }

    /// Restore a historical version using a diff rather than guessing one edit range.
    pub fn restore_content(&mut self, content: String) {
        self.replace_content_preserving_comments(content);
        self.selection = None;
        self.cursor_index = None;
        self.last_galley = None;
        self.search_replace.matches.clear();
        self.search_replace.current_match = None;
        self.search_replace.match_index = 0;
        self.reset_edit_state = true;
    }

    fn replace_content_preserving_comments(&mut self, content: String) {
        let previous = self.content.clone();
        self.reconcile_document_edit(&previous, &content, None);
        self.content = content;
        self.cached_word_count = None;
    }

    fn reconcile_document_edit(
        &mut self,
        previous: &str,
        content: &str,
        edit: Option<EditOperation>,
    ) {
        self.transform_anchors(|comments| {
            if let Some(edit) = edit {
                apply_edit(comments, edit);
            } else {
                reconcile_comments(comments, previous, content);
            }
        });
    }

    fn apply_operation_to_anchors(&mut self, edit: EditOperation) {
        self.transform_anchors(|comments| {
            apply_edit(comments, edit);
        });
    }

    fn transform_anchors(&mut self, transform: impl Fn(&mut Vec<TextComment>)) {
        transform(&mut self.comments);
        if let Some(composer) = &mut self.comment_composer
            && composer.anchor_valid
        {
            let mut pending = vec![composer.comment.clone()];
            transform(&mut pending);
            if let Some(updated) = pending.pop() {
                composer.comment = updated;
            } else {
                composer.anchor_valid = false;
                composer.error = Some("原选区已被完整删除，草稿仍保留，但无法绑定评论。".into());
            }
        }
        self.prune_open_comments();
    }

    fn prune_open_comments(&mut self) {
        if let Some(open) = &mut self.open_comment {
            open.comments
                .retain(|draft| self.comments.iter().any(|comment| comment.id == draft.id));
            open.selected = open.selected.min(open.comments.len().saturating_sub(1));
        }
        if self
            .open_comment
            .as_ref()
            .is_some_and(|open| open.comments.is_empty())
        {
            self.open_comment = None;
        }
    }

    fn edit_operation_from_events(
        ui: &Ui,
        old: &str,
        new: &str,
        previous_selection: Option<(usize, usize)>,
        previous_cursor: Option<usize>,
        new_cursor: Option<usize>,
    ) -> Option<EditOperation> {
        let events = ui.input(|input| input.events.clone());
        let is_undo_or_redo = events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Z | egui::Key::Y,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command || modifiers.ctrl
            )
        });
        if is_undo_or_redo {
            return None;
        }

        let mut insertion = false;
        let mut deletion = false;
        let mut cut = false;
        for event in &events {
            match event {
                egui::Event::Text(_) | egui::Event::Paste(_) => insertion = true,
                egui::Event::Cut => cut = true,
                egui::Event::Key {
                    key: egui::Key::Backspace | egui::Key::Delete,
                    pressed: true,
                    ..
                } => deletion = true,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    ..
                } => insertion = true,
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.shift => insertion = true,
                _ => {}
            }
        }
        if u8::from(insertion) + u8::from(deletion) + u8::from(cut) != 1 {
            return None;
        }

        let old_len = old.chars().count();
        let new_len = new.chars().count();
        let selection = previous_selection.filter(|(start, end)| start < end && *end <= old_len);
        if insertion {
            let (start, end) = match selection {
                Some(selection) => selection,
                None => {
                    let cursor = previous_cursor?;
                    (cursor, cursor)
                }
            };
            let inserted_len = new_len.checked_sub(old_len.checked_sub(end - start)?)?;
            Some(EditOperation {
                start,
                end,
                inserted_len,
            })
        } else if cut {
            let (start, end) = selection?;
            Some(EditOperation {
                start,
                end,
                inserted_len: 0,
            })
        } else if let Some((start, end)) = selection {
            Some(EditOperation {
                start,
                end,
                inserted_len: 0,
            })
        } else {
            let start = new_cursor?;
            let end = start.checked_add(old_len.checked_sub(new_len)?)?;
            (end <= old_len).then_some(EditOperation {
                start,
                end,
                inserted_len: 0,
            })
        }
    }

    pub fn add_comment(&mut self, start: usize, end: usize, text: String) -> bool {
        if text.trim().is_empty() || start >= end || end > self.content.chars().count() {
            return false;
        }
        let quote = Self::char_slice(&self.content, start, end).to_string();
        self.comments.push(TextComment {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            start,
            end,
            quote,
        });
        true
    }

    fn char_byte_range(content: &str, start: usize, end: usize) -> std::ops::Range<usize> {
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
        start_byte..end_byte
    }

    fn char_slice(content: &str, start: usize, end: usize) -> &str {
        let range = Self::char_byte_range(content, start, end);
        &content[range]
    }

    fn delete_selection(
        ctx: &egui::Context,
        editor_id: egui::Id,
        content: &mut String,
        start: usize,
        end: usize,
    ) -> EditOperation {
        let byte_range = Self::char_byte_range(content, start, end);
        content.replace_range(byte_range, "");
        if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, editor_id) {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(start),
                )));
            state.store(ctx, editor_id);
        }
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        EditOperation {
            start,
            end,
            inserted_len: 0,
        }
    }

    pub fn get_word_count(&mut self) -> usize {
        if let Some(count) = self.cached_word_count {
            return count;
        }

        let count = Self::count_words(&self.content);
        self.cached_word_count = Some(count);
        count
    }

    fn count_words(text: &str) -> usize {
        crate::statistics::count_words(text)
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

        Some(Self::count_words(&self.content[..byte_index]))
    }

    pub fn get_selection_word_count(&self) -> Option<usize> {
        self.selection
            .map(|range| Self::count_words(range.slice_str(&self.content)))
    }

    pub fn get_stats(&mut self) -> (usize, usize) {
        (
            self.get_word_count(),
            self.get_cursor_word_count().unwrap_or(0),
        )
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
        self.replace_content_preserving_comments(formatted);
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

    fn draw_vim_block_cursor(output: &egui::text_edit::TextEditOutput, ui: &mut Ui) {
        if !output.response.has_focus() {
            return;
        }
        if let Some(cursor_range) = output.cursor_range {
            let i = cursor_range.primary.index;
            let r1 = output.galley.pos_from_cursor(cursor_range.primary);
            let r2 = output
                .galley
                .pos_from_cursor(egui::text::CCursor::new(i.saturating_add(1)));

            let screen_min = output.galley_pos + r1.min.to_vec2();
            let height = if r1.height() > 0.0 { r1.height() } else { 18.0 };
            let width = if (r2.min.y - r1.min.y).abs() < 1.0 && r2.min.x > r1.min.x {
                r2.min.x - r1.min.x
            } else {
                (height * 0.5).max(8.0)
            };
            let rect = egui::Rect::from_min_size(screen_min, egui::vec2(width, height));
            let color = ui.visuals().text_color().gamma_multiply(0.4);
            ui.painter().rect_filled(rect, 0.0, color);
        }
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
                    egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color()),
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
                    let stroke = egui::Stroke::new(
                        1.0_f32,
                        ui.visuals().selection.bg_fill.linear_multiply(0.6),
                    );
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
                        egui::Stroke::new(1.0_f32, stroke_color),
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

    fn selected_range(output: &egui::text_edit::TextEditOutput) -> Option<(usize, usize)> {
        // Clicking the comment tool transfers focus; the selection is still in state.
        let range = output
            .cursor_range
            .or_else(|| output.state.cursor.char_range())
            .filter(|range| !range.is_empty())?;
        Some((
            range.primary.index.min(range.secondary.index),
            range.primary.index.max(range.secondary.index),
        ))
    }

    fn comment_popup_position(
        output: &egui::text_edit::TextEditOutput,
        char_index: usize,
    ) -> egui::Pos2 {
        let cursor = egui::text::CCursor::new(char_index);
        let rect = output
            .galley
            .pos_from_cursor(cursor)
            .translate(output.galley_pos.to_vec2());
        egui::pos2(rect.min.x, rect.max.y + 6.0)
    }

    fn begin_comment(
        &mut self,
        output: &egui::text_edit::TextEditOutput,
        start: usize,
        end: usize,
    ) {
        self.comment_composer = Some(CommentComposer {
            comment: TextComment {
                id: String::new(),
                text: String::new(),
                start,
                end,
                quote: String::new(),
            },
            position: Self::comment_popup_position(output, end),
            anchor_valid: true,
            error: None,
        });
        self.open_comment = None;
    }

    fn show_selection_comment_button(
        &mut self,
        output: &egui::text_edit::TextEditOutput,
        ui: &mut Ui,
        content: &mut String,
    ) -> Option<EditOperation> {
        let (start, end) = Self::selected_range(output)?;
        // IME preedit text uses a non-empty cursor range too. Keep the tool hidden
        // throughout composition, including repaint frames with no input events.
        if (!output.response.has_focus() && !output.response.lost_focus())
            || self.comment_composer.is_some()
            || self.ime_composing
            || output.response.changed()
            || output.response.is_pointer_button_down_on()
        {
            return None;
        }

        let screen_rect = ui.ctx().content_rect();
        let mut position = Self::comment_popup_position(output, end);
        position.x = position.x.clamp(
            screen_rect.min.x,
            (screen_rect.max.x - 240.0).max(screen_rect.min.x),
        );
        position.y = position.y.clamp(
            screen_rect.min.y,
            (screen_rect.max.y - 38.0).max(screen_rect.min.y),
        );
        let mut edit = None;
        egui::Area::new(ui.make_persistent_id("selection_comment_tool"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(4, 3))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                        ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                        ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::NONE;
                        ui.visuals_mut().widgets.active.bg_stroke = egui::Stroke::NONE;
                        ui.horizontal(|ui| {
                            if ui
                                .button(format!("{} 评论", egui_phosphor::regular::CHAT_TEXT))
                                .clicked()
                            {
                                self.begin_comment(output, start, end);
                            }
                            if ui
                                .button(format!("{} 复制", egui_phosphor::regular::COPY))
                                .clicked()
                            {
                                ui.ctx()
                                    .copy_text(Self::char_slice(content, start, end).to_string());
                                output.response.request_focus();
                            }
                            if ui
                                .button(format!("{} 剪切", egui_phosphor::regular::SCISSORS))
                                .clicked()
                            {
                                ui.ctx()
                                    .copy_text(Self::char_slice(content, start, end).to_string());
                                edit = Some(Self::delete_selection(
                                    ui.ctx(),
                                    output.response.id,
                                    content,
                                    start,
                                    end,
                                ));
                            }
                            if ui
                                .button(format!("{} 删除", egui_phosphor::regular::TRASH))
                                .clicked()
                            {
                                edit = Some(Self::delete_selection(
                                    ui.ctx(),
                                    output.response.id,
                                    content,
                                    start,
                                    end,
                                ));
                            }
                        });
                    });
            });
        edit
    }

    fn highlight_comments(&mut self, output: &egui::text_edit::TextEditOutput, ui: &mut Ui) {
        if self.comments.is_empty() {
            return;
        }

        let fill = egui::Color32::from_rgba_unmultiplied(230, 174, 55, 30);
        let underline = egui::Color32::from_rgb(173, 120, 24);
        let mut row_start = 0;
        for row in &output.galley.rows {
            let row_end = row_start + row.char_count_excluding_newline();
            for comment in &self.comments {
                let start = comment.start.max(row_start);
                let end = comment.end.min(row_end);
                if start < end {
                    let x_start = row.x_offset(start - row_start);
                    let x_end = row.x_offset(end - row_start);
                    let min =
                        output.galley_pos + row.rect().min.to_vec2() + egui::vec2(x_start, 0.0);
                    let max = output.galley_pos
                        + row.rect().min.to_vec2()
                        + egui::vec2(x_end, row.rect().height());
                    let rect = egui::Rect::from_min_max(min, max);
                    ui.painter().rect_filled(rect, 1.0, fill);
                    ui.painter().line_segment(
                        [rect.left_bottom(), rect.right_bottom()],
                        egui::Stroke::new(1.0_f32, underline),
                    );
                }
            }
            row_start = row_end + usize::from(row.ends_with_newline);
        }

        if output.response.clicked()
            && !output.response.dragged()
            && let Some(range) = output.cursor_range.filter(|range| range.is_empty())
        {
            let index = range.primary.index;
            self.open_comments_at(index, Self::comment_popup_position(output, index));
            if self.open_comment.is_some() {
                self.comment_composer = None;
            }
        }
    }

    fn open_comments_at(&mut self, index: usize, position: egui::Pos2) {
        let comments: Vec<OpenCommentDraft> = self
            .comments
            .iter()
            .filter(|comment| comment.start <= index && index < comment.end)
            .map(|comment| OpenCommentDraft {
                id: comment.id.clone(),
                text: comment.text.clone(),
            })
            .collect();
        if !comments.is_empty() {
            self.open_comment = Some(OpenComment {
                comments,
                selected: 0,
                position,
            });
        }
    }

    fn submit_comment(&mut self) {
        let Some(composer) = &mut self.comment_composer else {
            return;
        };
        if !composer.anchor_valid
            || composer.comment.text.trim().is_empty()
            || composer.comment.start >= composer.comment.end
            || composer.comment.end > self.content.chars().count()
        {
            composer.error = Some("无法绑定原选区；草稿已保留，请取消或返回继续编辑。".into());
            return;
        }

        let mut composer = self
            .comment_composer
            .take()
            .expect("composer checked above");
        composer.comment.id = uuid::Uuid::new_v4().to_string();
        composer.comment.quote =
            Self::char_slice(&self.content, composer.comment.start, composer.comment.end)
                .to_string();
        self.comments.push(composer.comment);
    }

    fn show_comment_windows(&mut self, ui: &mut Ui) {
        if let Some(mut composer) = self.comment_composer.take() {
            let mut cancel = false;
            let mut save = false;
            egui::Window::new("添加评论 / 备注")
                .fixed_pos(composer.position)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut composer.comment.text)
                            .hint_text("写下评论或备注…")
                            .desired_width(280.0)
                            .desired_rows(5),
                    );
                    if let Some(error) = &composer.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                composer.anchor_valid && !composer.comment.text.trim().is_empty(),
                                egui::Button::new("添加评论"),
                            )
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                    });
                });
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                cancel = true;
            }
            if save {
                self.comment_composer = Some(composer);
                self.submit_comment();
            } else if !cancel {
                self.comment_composer = Some(composer);
            }
        }

        let Some(mut open) = self.open_comment.take() else {
            return;
        };
        let mut visible = true;
        let mut done = false;
        let mut delete = false;
        egui::Window::new("评论 / 备注")
            .open(&mut visible)
            .fixed_pos(open.position)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                if open.comments.len() > 1 {
                    ui.horizontal_wrapped(|ui| {
                        for index in 0..open.comments.len() {
                            if ui
                                .selectable_label(
                                    open.selected == index,
                                    format!("评论 {}", index + 1),
                                )
                                .clicked()
                            {
                                open.selected = index;
                            }
                        }
                    });
                    ui.separator();
                }
                let selected_id = &open.comments[open.selected].id;
                let Some(comment) = self
                    .comments
                    .iter()
                    .find(|comment| comment.id == *selected_id)
                else {
                    return;
                };
                let quote = if comment.quote.chars().count() > 80 {
                    format!("{}…", comment.quote.chars().take(80).collect::<String>())
                } else {
                    comment.quote.clone()
                };
                ui.label(egui::RichText::new(format!("“{}”", quote)).small().weak());
                ui.add(
                    egui::TextEdit::multiline(&mut open.comments[open.selected].text)
                        .desired_width(280.0)
                        .desired_rows(5),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !open.comments[open.selected].text.trim().is_empty(),
                            egui::Button::new("完成"),
                        )
                        .clicked()
                    {
                        done = true;
                    }
                    if ui.button("删除评论").clicked() {
                        delete = true;
                    }
                });
            });
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            visible = false;
        }

        if delete {
            let selected_id = open.comments[open.selected].id.clone();
            self.comments.retain(|comment| comment.id != selected_id);
            open.comments.remove(open.selected);
            open.selected = open.selected.min(open.comments.len().saturating_sub(1));
            if visible && !open.comments.is_empty() {
                self.open_comment = Some(open);
            }
        } else if done {
            for draft in open.comments {
                if !draft.text.trim().is_empty()
                    && let Some(comment) = self
                        .comments
                        .iter_mut()
                        .find(|comment| comment.id == draft.id)
                {
                    comment.text = draft.text;
                }
            }
        } else if visible {
            self.open_comment = Some(open);
        }
    }

    fn add_context_menu(
        &mut self,
        output: &egui::text_edit::TextEditOutput,
        content: &mut String,
    ) -> Option<EditOperation> {
        let mut context_edit = None;
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

            if selected_text.is_some()
                && let Some((start, end)) = Self::selected_range(output)
                && ui
                    .button(format!(
                        "{} 添加评论 / 备注",
                        egui_phosphor::regular::CHAT_TEXT
                    ))
                    .clicked()
            {
                self.begin_comment(output, start, end);
                ui.close();
            }
            if ui
                .button(format!("{} 剪切", egui_phosphor::regular::SCISSORS))
                .clicked()
            {
                if let Some(selected) = &selected_text {
                    ui.ctx().copy_text(selected.clone());
                    // Remove selected text
                    if let Some(cursor_range) = output.cursor_range {
                        let start = cursor_range.primary.index.min(cursor_range.secondary.index);
                        let end = cursor_range.primary.index.max(cursor_range.secondary.index);
                        context_edit = Some(EditOperation {
                            start,
                            end,
                            inserted_len: 0,
                        });
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
                    context_edit = Some(EditOperation {
                        start: 0,
                        end: content.chars().count(),
                        inserted_len: 0,
                    });
                    content.clear();
                }
                ui.close();
            }
            if ui
                .button(format!("{} 复制", egui_phosphor::regular::COPY))
                .clicked()
            {
                if let Some(selected) = &selected_text {
                    ui.ctx().copy_text(selected.clone());
                } else {
                    ui.ctx().copy_text(content.clone());
                }
                ui.close();
            }
            if ui
                .button(format!("{} 粘贴", egui_phosphor::regular::CLIPBOARD))
                .clicked()
            {
                // Request focus to ensure paste works
                output.response.request_focus();
                ui.close();
            }
        });
        context_edit
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
                    ui.label(
                        egui::RichText::new(format!(
                            "{} 查找与替换",
                            egui_phosphor::regular::MAGNIFYING_GLASS
                        ))
                        .small()
                        .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui_phosphor::regular::X)
                            .on_hover_text("关闭 (Esc)")
                            .clicked()
                        {
                            self.search_replace.show_dialog = false;
                        }
                    });
                });
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("查找:").small());
                    let search_response = ui.add(
                        egui::TextEdit::singleline(&mut self.search_replace.search_text)
                            .desired_width(120.0),
                    );
                    if search_response.changed() {
                        self.find_matches();
                    }

                    if ui
                        .button(egui_phosphor::regular::CARET_UP)
                        .on_hover_text("上一个匹配项")
                        .clicked()
                    {
                        self.previous_match();
                    }
                    if ui
                        .button(egui_phosphor::regular::CARET_DOWN)
                        .on_hover_text("下一个匹配项")
                        .clicked()
                    {
                        self.next_match();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("替换:").small());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search_replace.replace_text)
                            .desired_width(120.0),
                    );
                    if ui.button("替换").on_hover_text("替换当前项").clicked() {
                        self.replace_current();
                    }
                    if ui.button("全部").on_hover_text("替换所有匹配项").clicked() {
                        self.replace_all();
                    }
                });

                ui.horizontal(|ui| {
                    let case_changed = ui
                        .checkbox(&mut self.search_replace.case_sensitive, "区分大小写")
                        .changed();
                    let word_changed = ui
                        .checkbox(&mut self.search_replace.whole_word, "全词匹配")
                        .changed();
                    if case_changed || word_changed {
                        self.find_matches();
                    }
                });

                if !self.search_replace.matches.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} 匹配: {} / {}",
                            egui_phosphor::regular::INFO,
                            self.search_replace.match_index + 1,
                            self.search_replace.matches.len()
                        ))
                        .small(),
                    );
                } else if !self.search_replace.search_text.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} 未找到匹配项",
                            egui_phosphor::regular::INFO
                        ))
                        .small()
                        .weak(),
                    );
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.search_replace.show_dialog = false;
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
            let char_start = self.content[..start].chars().count();
            let char_end = self.content[..end].chars().count();
            self.apply_operation_to_anchors(EditOperation {
                start: char_start,
                end: char_end,
                inserted_len: self.search_replace.replace_text.chars().count(),
            });
            self.content
                .replace_range(start..end, &self.search_replace.replace_text);
            self.cached_word_count = None;
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

        let replacements = self.search_replace.matches.clone();
        for (start, end) in replacements.iter().rev() {
            let char_start = self.content[..*start].chars().count();
            let char_end = self.content[..*end].chars().count();
            self.apply_operation_to_anchors(EditOperation {
                start: char_start,
                end: char_end,
                inserted_len: self.search_replace.replace_text.chars().count(),
            });
        }
        self.content = new_content;
        self.search_replace.matches.clear();
        self.search_replace.current_match = None;
        self.search_replace.match_index = 0;
        self.cached_word_count = None; // Invalidate cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_comment_tool(
        ctx: &egui::Context,
        editor: &mut Editor,
        events: Vec<egui::Event>,
        selection: Option<(usize, usize)>,
    ) -> bool {
        let output = render_editor(ctx, editor, events, selection);
        output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().ends_with(" 评论"))
        })
    }

    fn render_editor(
        ctx: &egui::Context,
        editor: &mut Editor,
        events: Vec<egui::Event>,
        selection: Option<(usize, usize)>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some((start, end)) = selection {
                        let id = ui.make_persistent_id("main_editor");
                        let mut state =
                            egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(start),
                                egui::text::CCursor::new(end),
                            )));
                        state.store(ctx, id);
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                    editor.show(ui);
                });
            },
        )
    }

    #[test]
    fn comment_tool_stays_hidden_during_ime_composition_and_typing() {
        use egui::{Event, ImeEvent};

        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.set_content("hello world".into());
        editor.focus();
        assert!(!render_comment_tool(&ctx, &mut editor, vec![], None));
        render_comment_tool(&ctx, &mut editor, vec![], Some((0, 5)));
        assert!(render_comment_tool(&ctx, &mut editor, vec![], None));
        // Enabling an IME alone does not mean text is being composed.
        assert!(render_comment_tool(
            &ctx,
            &mut editor,
            vec![Event::Ime(ImeEvent::Enabled)],
            None,
        ));

        assert!(!render_comment_tool(
            &ctx,
            &mut editor,
            vec![Event::Ime(ImeEvent::Preedit("ni".into()))],
            None,
        ));
        // Preedit remains selected on idle frames even though no new event arrives.
        for _ in 0..3 {
            assert!(!render_comment_tool(&ctx, &mut editor, vec![], None));
        }
        assert!(editor.selection.is_some());
        assert!(!render_comment_tool(
            &ctx,
            &mut editor,
            vec![Event::Ime(ImeEvent::Preedit("ni".into()))],
            None,
        ));
        assert!(!render_comment_tool(
            &ctx,
            &mut editor,
            vec![
                Event::Ime(ImeEvent::Commit("你".into())),
                Event::Ime(ImeEvent::Disabled)
            ],
            None,
        ));
        assert!(!render_comment_tool(&ctx, &mut editor, vec![], None));
        assert_eq!(editor.get_content(), "你 world");

        // A deliberate selection still offers comments after composition ends.
        render_comment_tool(&ctx, &mut editor, vec![], Some((0, 1)));
        assert!(render_comment_tool(&ctx, &mut editor, vec![], None));
        assert!(!render_comment_tool(
            &ctx,
            &mut editor,
            vec![Event::Text("hello".into())],
            None,
        ));
        assert!(!render_comment_tool(&ctx, &mut editor, vec![], None));
        assert_eq!(editor.get_content(), "hello world");
    }

    #[test]
    fn comment_tool_waits_for_selection_release_and_remains_clickable() {
        use egui::{Event, Modifiers, PointerButton};

        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.set_content("hello world".into());
        editor.focus();
        let output = render_editor(&ctx, &mut editor, vec![], None);
        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "hello world" => Some(text),
                _ => None,
            })
            .unwrap();
        let start = text.pos
            + text
                .galley
                .pos_from_cursor(egui::text::CCursor::new(0))
                .center()
                .to_vec2();
        let end = text.pos
            + text
                .galley
                .pos_from_cursor(egui::text::CCursor::new(5))
                .center()
                .to_vec2();
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        render_editor(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(start), pointer(start, true)],
            None,
        );
        assert!(!render_comment_tool(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(end)],
            None
        ));
        assert!(editor.selection.is_some());
        assert!(!render_comment_tool(&ctx, &mut editor, vec![], None));
        render_editor(&ctx, &mut editor, vec![pointer(end, false)], None);
        let output = render_editor(&ctx, &mut editor, vec![], None);
        let button = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().ends_with(" 评论") => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("comment button appears after releasing the selection");
        assert!(render_comment_tool(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(button), pointer(button, true)],
            None
        ));
        render_editor(&ctx, &mut editor, vec![pointer(button, false)], None);
        assert!(editor.comment_composer.is_some());
        let comment = &editor.comment_composer.as_ref().unwrap().comment;
        assert_eq!((comment.start, comment.end), (0, 5));
    }

    #[test]
    fn selection_toolbar_offers_actions_without_tooltip_and_deletes_selection() {
        use egui::{Event, Modifiers, PointerButton};

        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let mut editor = Editor::default();
        editor.set_content("hello world".into());
        assert!(editor.add_comment(6, 11, "keep world".into()));
        editor.focus();

        render_editor(&ctx, &mut editor, vec![], None);
        render_editor(&ctx, &mut editor, vec![], Some((0, 6)));
        let output = render_editor(&ctx, &mut editor, vec![], None);
        for suffix in [" 评论", " 复制", " 剪切", " 删除"] {
            assert!(
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().ends_with(suffix))
                }),
                "missing toolbar action {suffix}"
            );
        }

        let comment_button = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().ends_with(" 评论") => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("comment button rendered");
        let hovered = render_editor(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(comment_button)],
            None,
        );
        assert!(
            !hovered.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("为选中文字添加评论"))
            }),
            "selection toolbar should not show hover tooltip"
        );

        let delete_button = hovered
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().ends_with(" 删除") => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("delete button rendered");
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        render_editor(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(delete_button),
                pointer(delete_button, true),
            ],
            None,
        );
        render_editor(&ctx, &mut editor, vec![pointer(delete_button, false)], None);
        assert_eq!(editor.get_content(), "world");
        assert_eq!(
            (editor.comments()[0].start, editor.comments()[0].end),
            (0, 5)
        );
        assert!(!render_comment_tool(&ctx, &mut editor, vec![], None));
    }

    fn gutter_circles(output: &egui::FullOutput) -> Vec<egui::epaint::CircleShape> {
        output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Circle(circle) = &shape.shape {
                    Some(*circle)
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn paragraph_dots_follow_logical_lines_and_hide_with_article_list() {
        for (content, count) in [
            ("", 1),
            ("first\n", 2),
            (
                "你好，this paragraph wraps across several visual lines in a narrow editor.\nsecond\n",
                3,
            ),
        ] {
            let ctx = egui::Context::default();
            let mut editor = Editor::default();
            editor.set_content(content.to_string());
            for show_dots in [true, false] {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(200.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .show(ctx, |ui| editor.show_with_paragraph_dots(ui, show_dots));
                    },
                );
                assert_eq!(
                    gutter_circles(&output).len(),
                    if show_dots { count } else { 0 },
                    "{content:?}"
                );
                let divider_count = output.shapes.iter().filter(|shape| {
                    matches!(&shape.shape, egui::Shape::LineSegment { points, stroke }
                        if points[0].x == points[1].x && (points[1].y - points[0].y) >= 600.0 && stroke.width == 1.0)
                }).count();
                assert_eq!(divider_count, usize::from(show_dots));
            }
        }
    }

    #[test]
    fn active_paragraph_dot_breathes_only_while_editor_has_focus() {
        use egui::text::{CCursor, CCursorRange};
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.set_content("你好\nsecond paragraph".to_string());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| editor.show(ui));
        });
        let mut render = |time, focused| {
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    focused,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let id = ui.make_persistent_id("main_editor");
                        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap();
                        state
                            .cursor
                            .set_char_range(Some(CCursorRange::one(CCursor::new(5))));
                        state.store(ctx, id);
                        ui.memory_mut(|memory| memory.request_focus(id));
                        editor.show(ui);
                    });
                },
            )
        };
        let bright = gutter_circles(&render(0.8, true));
        let dim = gutter_circles(&render(2.4, true));
        // Two logical lines: the first has a static outline, the second a halo and core.
        assert_eq!(bright.len(), 3);
        assert_eq!(bright[0].fill, egui::Color32::TRANSPARENT);
        assert!(bright[1].center.y > bright[0].center.y);
        assert_eq!(bright[2].fill, egui::Color32::TRANSPARENT);
        assert!(bright[2].stroke.color.a() > dim[2].stroke.color.a());
        assert!(bright[2].stroke.color.g() > bright[2].stroke.color.r());
        assert!(bright[1].radius - dim[1].radius > 2.0);
        let unfocused = gutter_circles(&render(3.0, false));
        assert_eq!(unfocused.len(), 2);
        assert!(
            unfocused
                .iter()
                .all(|circle| circle.fill == egui::Color32::TRANSPARENT)
        );
    }

    #[test]
    fn undo_restores_comment_anchors_by_diff_not_the_previous_selection() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.set_content("abcXdef".into());
        assert!(editor.add_comment(4, 7, "keep def".into()));
        let events = vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }];
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let edit = Editor::edit_operation_from_events(
                        ui,
                        "abcXdef",
                        "abcdef",
                        Some((4, 7)),
                        Some(7),
                        Some(4),
                    );
                    assert_eq!(edit, None);
                    editor.reconcile_document_edit("abcXdef", "abcdef", edit);
                });
            },
        );
        assert_eq!(
            (editor.comments()[0].start, editor.comments()[0].end),
            (3, 6)
        );
    }

    #[test]
    fn pending_comment_anchor_tracks_edits_and_failed_submit_keeps_draft() {
        let mut editor = Editor::default();
        editor.set_content("abc def".into());
        editor.comment_composer = Some(CommentComposer {
            comment: TextComment {
                id: String::new(),
                text: "draft".into(),
                start: 4,
                end: 7,
                quote: String::new(),
            },
            position: egui::Pos2::ZERO,
            anchor_valid: true,
            error: None,
        });
        editor.apply_operation_to_anchors(EditOperation {
            start: 0,
            end: 0,
            inserted_len: 1,
        });
        editor.content = "Xabc def".into();
        editor.submit_comment();
        assert_eq!(
            (editor.comments()[0].start, editor.comments()[0].end),
            (5, 8)
        );
        assert_eq!(editor.comments()[0].quote, "def");
        assert!(editor.add_comment(0, 3, "keep abc".into()));

        editor.comment_composer = Some(CommentComposer {
            comment: TextComment {
                id: String::new(),
                text: "do not lose me".into(),
                start: 5,
                end: 8,
                quote: String::new(),
            },
            position: egui::Pos2::ZERO,
            anchor_valid: true,
            error: None,
        });
        editor.apply_operation_to_anchors(EditOperation {
            start: 5,
            end: 8,
            inserted_len: 0,
        });
        editor.content = "Xabc ".into();
        editor.submit_comment();
        assert_eq!(editor.comments().len(), 1);
        let composer = editor.comment_composer.as_ref().unwrap();
        assert!(!composer.anchor_valid);
        assert_eq!(composer.comment.text, "do not lose me");
        assert!(composer.error.is_some());
    }

    #[test]
    fn clicking_overlapping_comments_opens_every_match() {
        let mut editor = Editor::default();
        editor.set_content("abcdef".into());
        assert!(editor.add_comment(0, 4, "first".into()));
        assert!(editor.add_comment(2, 6, "second".into()));
        editor.open_comments_at(3, egui::Pos2::ZERO);
        let open = editor.open_comment.as_ref().unwrap();
        assert_eq!(open.comments.len(), 2);
        assert_eq!(open.comments[0].text, "first");
        assert_eq!(open.comments[1].text, "second");
    }

    #[test]
    fn search_replace_all_uses_the_exact_replaced_ranges() {
        let mut editor = Editor::default();
        editor.set_content("world and world".into());
        assert!(editor.add_comment(0, 5, "first".into()));
        assert!(editor.add_comment(10, 15, "second".into()));
        editor.search_replace.search_text = "world".into();
        editor.search_replace.replace_text = "universe".into();
        editor.find_matches();
        editor.replace_all();
        assert_eq!(editor.get_content(), "universe and universe");
        assert!(editor.comments().is_empty());
    }

    #[test]
    fn comments_keep_unicode_character_ranges_and_original_quotes() {
        let mut editor = Editor::default();
        editor.set_content("甲😀乙丙".into());
        assert!(editor.add_comment(1, 3, "备注".into()));
        assert_eq!(editor.comments()[0].quote, "😀乙");
        assert!(!editor.add_comment(2, 2, "空选区".into()));
        assert!(!editor.add_comment(0, 1, "   ".into()));
        editor.restore_content("甲😀丙".into());
        assert_eq!(
            (editor.comments()[0].start, editor.comments()[0].end),
            (1, 2)
        );
    }

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
    fn selection_statistics_follow_editor_cursor_range() {
        use egui::text::{CCursor, CCursorRange};

        for (text, start, end, expected) in [
            ("你好世界", 1, 3, Some(2)),
            ("你好世界", 3, 1, Some(2)),
            ("Hello 世界\nRust editor", 0, 20, Some(5)),
            ("Hello world", 1, 4, Some(1)),
            ("甲😀乙", 1, 3, Some(2)),
            ("A \n\t B", 1, 5, Some(0)),
            ("Hello world", 3, 3, None),
            ("", 0, 0, None),
        ] {
            let ctx = egui::Context::default();
            let mut editor = Editor::default();
            editor.set_content(text.to_string());
            // Initialize the newly loaded document before simulating selection.
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| editor.show(ui));
            });
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let id = ui.make_persistent_id("main_editor");
                    let mut state = egui::text_edit::TextEditState::default();
                    state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(start),
                        CCursor::new(end),
                    )));
                    state.store(ctx, id);
                    ui.memory_mut(|memory| memory.request_focus(id));
                    editor.show(ui);
                });
            });
            assert_eq!(editor.get_selection_word_count(), expected, "{text:?}");

            // Keyboard navigation collapses the selection and clears its statistic.
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::ArrowRight,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| editor.show(ui));
                },
            );
            assert_eq!(editor.get_selection_word_count(), None);
        }
    }

    #[test]
    fn loading_content_clears_selection_statistics() {
        use egui::text::{CCursor, CCursorRange};

        let mut editor = Editor::default();
        editor.set_content("你好世界".to_string());
        editor.selection = Some(CCursorRange::two(CCursor::new(0), CCursor::new(4)));
        assert_eq!(editor.get_selection_word_count(), Some(4));
        editor.set_content("New document".to_string());
        assert_eq!(editor.get_selection_word_count(), None);
    }

    #[test]
    fn switching_to_empty_document_clears_search_and_previous_undo() {
        use egui::text::{CCursor, CCursorRange};
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.set_content("previous article".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| editor.show(ui));
        });
        editor.search_replace.matches = vec![(0, 8)];
        editor.search_replace.current_match = Some((0, 8));
        editor.set_content(String::new());
        assert!(editor.search_replace.matches.is_empty());
        assert!(editor.search_replace.current_match.is_none());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let id = ui.make_persistent_id("main_editor");
                editor.show(ui);
                let state = egui::text_edit::TextEditState::load(ctx, id).unwrap();
                let cursor = CCursorRange::one(CCursor::new(0));
                assert!(!state.undoer().has_undo(&(cursor, String::new())));
            });
        });
        assert!(editor.get_content().is_empty());
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
    fn typography_layout_changes_row_height() {
        let ctx = egui::Context::default();
        let mut editor1 = Editor {
            font_size: 14.0,
            line_spacing: 1.0,
            ..Default::default()
        };
        editor1.set_content("First line\nSecond line\nThird line".to_string());

        let mut editor2 = Editor {
            font_size: 14.0,
            line_spacing: 2.0,
            ..Default::default()
        };
        editor2.set_content("First line\nSecond line\nThird line".to_string());

        let mut editor3 = Editor {
            font_size: 20.0,
            line_spacing: 1.5,
            ..Default::default()
        };
        editor3.set_content("First line\nSecond line\nThird line".to_string());

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                editor1.show(ui);
                editor2.show(ui);
                editor3.show(ui);
            });
        });

        let galley1 = editor1.last_galley.as_ref().unwrap();
        let galley2 = editor2.last_galley.as_ref().unwrap();
        let galley3 = editor3.last_galley.as_ref().unwrap();

        // 14pt * 2.0 line spacing should be taller than 14pt * 1.0 line spacing
        assert!(galley2.size().y > galley1.size().y);
        // 20pt * 1.5 should be taller than 14pt * 1.0
        assert!(galley3.size().y > galley1.size().y);
    }

    #[test]
    fn tab_key_keeps_editor_focused_and_inserts_tab() {
        use egui::{Event, Key, Modifiers};

        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.set_content("hello".into());
        assert!(editor.add_comment(0, 5, "greeting".into()));
        editor.focus();

        let run_frame = |ctx: &egui::Context, editor: &mut Editor, events: Vec<Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let _ = ui.button("before");
                        editor.show(ui);
                        let _ = ui.button("after");
                    });
                },
            );
        };

        // Frame 1: initialize editor and place cursor at start (index 0).
        run_frame(&ctx, &mut editor, vec![]);
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = ui.button("before");
                    let id = ui.make_persistent_id("main_editor");
                    let mut state =
                        egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(0),
                        )));
                    state.store(ctx, id);
                    editor.show(ui);
                    let _ = ui.button("after");
                });
            },
        );
        assert!(editor.is_focused());

        // Frame 2: press Tab immediately on the next frame; editor must keep focus and insert '\t'.
        let tab_event = |shift: bool| Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: if shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            },
        };
        run_frame(&ctx, &mut editor, vec![tab_event(false)]);
        assert!(editor.is_focused(), "editor lost focus after first Tab");
        assert_eq!(editor.get_content(), "\thello");
        assert_eq!(
            (editor.comments()[0].start, editor.comments()[0].end),
            (1, 6)
        );

        // Frame 3: continue typing after Tab without re-focusing.
        run_frame(&ctx, &mut editor, vec![Event::Text("world ".into())]);
        assert!(editor.is_focused());
        assert_eq!(editor.get_content(), "\tworld hello");

        // Frame 4: Shift+Tab decreases indentation and still keeps focus.
        run_frame(&ctx, &mut editor, vec![tab_event(true)]);
        assert!(editor.is_focused(), "editor lost focus after Shift+Tab");
        assert_eq!(editor.get_content(), "world hello");

        // Frame 5: Vim mode Insert and Normal also retain focus on Tab.
        editor.set_vim_mode(true);
        run_frame(&ctx, &mut editor, vec![tab_event(false)]);
        assert!(
            editor.is_focused(),
            "editor lost focus on Tab in Vim Insert mode"
        );
        assert_eq!(editor.get_content(), "world \thello");

        run_frame(
            &ctx,
            &mut editor,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(editor.get_vim_mode_label(), Some("普通"));
        run_frame(&ctx, &mut editor, vec![tab_event(false)]);
        assert!(
            editor.is_focused(),
            "editor lost focus on Tab in Vim Normal mode"
        );
        assert_eq!(editor.get_content(), "world \thello");
    }
}
