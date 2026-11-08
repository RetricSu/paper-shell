//! Vim modal editing state machine.
//!
//! Pure logic only: it rewrites `egui::Event` streams and returns `VimAction`
//! for side effects the editor must perform (cursor jumps, clipboard, search).
//! Chinese text motions (`w`/`b`) use word segmentation (via `jieba-rs`)
//! on contiguous CJK segments, and contiguous ASCII alphanumerics/underscore as one word.

use egui::{Event, Key, Modifiers};
use jieba_rs::Jieba;
use std::sync::LazyLock;

static JIEBA: LazyLock<Jieba> = LazyLock::new(Jieba::new);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Insert,
    Normal,
    Visual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VimAction {
    None,
    ModeChanged,
    ContentChanged {
        new_content: String,
        new_cursor: usize,
    },
    ClearSelection,
    /// Absolute char-index jump (used by Chinese-aware w/b motions).
    SetCursor(usize),
    /// Copy text to the system clipboard (yank).
    Yank(String),
    /// Paste the yank register (or system clipboard when empty) N times.
    Paste {
        count: usize,
    },
    OpenSearch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Delete,
    Change,
    Yank,
}

#[derive(Clone, Debug, Default)]
pub struct VimState {
    pub enabled: bool,
    pub mode: VimMode,
    pub pending: Option<char>,
    count: Option<usize>,
    operator: Option<Operator>,
    register: String,
}

fn key_event(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn is_cjk_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF
        | 0x3400..=0x4DBF
        | 0x20000..=0x2A6DF
        | 0x2A700..=0x2B73F
        | 0x2B740..=0x2B81F
        | 0x2B820..=0x2CEAF
        | 0x2CEB0..=0x2EBE0
        | 0x30000..=0x3134F
        | 0xF900..=0xFAFF
        | 0x2F800..=0x2FA1F
    )
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn char_class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if is_word_char(c) {
        1
    } else if is_cjk_ideograph(c) {
        2
    } else {
        3
    }
}

/// Start of the next word. ASCII words move as a block; CJK segments are
/// segmented into words using Jieba; contiguous punctuation moves as a block.
/// Whitespace is skipped.
pub fn next_word_start(content: &str, cursor: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();
    if cursor >= chars.len() {
        return chars.len();
    }
    let mut i = cursor;
    let initial_class = char_class(chars[i]);
    match initial_class {
        0 => {}
        1 => {
            while i < chars.len() && char_class(chars[i]) == 1 {
                i += 1;
            }
        }
        2 => {
            // Find contiguous CJK ideograph slice
            let mut seg_start = i;
            while seg_start > 0 && char_class(chars[seg_start - 1]) == 2 {
                seg_start -= 1;
            }
            let mut seg_end = i;
            while seg_end < chars.len() && char_class(chars[seg_end]) == 2 {
                seg_end += 1;
            }

            let cjk_text: String = chars[seg_start..seg_end].iter().collect();
            let words = JIEBA.cut(&cjk_text, false);

            let offset_in_seg = i - seg_start;
            let mut rel_pos = 0;
            let mut next_target = None;

            for w in words {
                let w_len = w.chars().count();
                let w_end = rel_pos + w_len;
                if rel_pos > offset_in_seg {
                    next_target = Some(seg_start + rel_pos);
                    break;
                }
                rel_pos = w_end;
            }

            if let Some(target) = next_target {
                i = target;
            } else {
                i = seg_end;
            }
        }
        _ => {
            while i < chars.len() && char_class(chars[i]) == 3 {
                i += 1;
            }
        }
    }
    while i < chars.len() && char_class(chars[i]) == 0 {
        i += 1;
    }
    i
}

/// Start of the previous word.
pub fn prev_word_start(content: &str, cursor: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() || cursor == 0 {
        return 0;
    }
    let mut i = cursor.min(chars.len());
    while i > 0 && (i == chars.len() || char_class(chars[i - 1]) == 0) {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    match char_class(chars[i - 1]) {
        1 => {
            while i > 0 && char_class(chars[i - 1]) == 1 {
                i -= 1;
            }
            i
        }
        2 => {
            let mut seg_end = i;
            while seg_end < chars.len() && char_class(chars[seg_end]) == 2 {
                seg_end += 1;
            }
            let mut seg_start = i - 1;
            while seg_start > 0 && char_class(chars[seg_start - 1]) == 2 {
                seg_start -= 1;
            }

            let cjk_text: String = chars[seg_start..seg_end].iter().collect();
            let words = JIEBA.cut(&cjk_text, false);

            let target_offset = (i - 1) - seg_start;
            let mut rel_pos = 0;
            let mut last_start = 0;

            for w in words {
                if rel_pos <= target_offset {
                    last_start = rel_pos;
                } else {
                    break;
                }
                rel_pos += w.chars().count();
            }

            seg_start + last_start
        }
        _ => {
            while i > 0 && char_class(chars[i - 1]) == 3 {
                i -= 1;
            }
            i
        }
    }
}

fn line_start(chars: &[char], cursor: usize) -> usize {
    let mut i = cursor.min(chars.len());
    while i > 0 && chars[i - 1] != '\n' {
        i -= 1;
    }
    i
}

fn line_end(chars: &[char], cursor: usize) -> usize {
    let mut i = cursor.min(chars.len());
    while i < chars.len() && chars[i] != '\n' {
        i += 1;
    }
    i
}

impl VimState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            mode: VimMode::Insert,
            pending: None,
            count: None,
            operator: None,
            register: String::new(),
        }
    }

    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    fn reset_transient(&mut self) {
        self.pending = None;
        self.operator = None;
        self.count = None;
    }

    /// Process an explicit Escape event.
    ///
    /// Repeated Escape presses toggle between `Insert` and `Normal` modes
    /// (and exit `Visual` mode back to `Normal`).
    pub fn on_escape(&mut self) -> VimAction {
        if !self.enabled {
            return VimAction::None;
        }
        self.reset_transient();
        match self.mode {
            VimMode::Insert => {
                self.mode = VimMode::Normal;
                VimAction::ModeChanged
            }
            VimMode::Normal => {
                self.mode = VimMode::Insert;
                VimAction::ModeChanged
            }
            VimMode::Visual => {
                self.mode = VimMode::Normal;
                VimAction::ClearSelection
            }
        }
    }

    /// Yank register contents (empty when nothing was yanked this session).
    pub fn register(&self) -> &str {
        &self.register
    }

    fn push_repeated(out: &mut Vec<Event>, key: Key, modifiers: Modifiers, count: usize) {
        for _ in 0..count {
            out.push(key_event(key, modifiers));
        }
    }

    fn motion_event(out: &mut Vec<Event>, c: char, visual: bool, count: usize) {
        let shift = if visual {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        let (key, modifiers) = match c {
            'h' => (Key::ArrowLeft, shift),
            'j' => (Key::ArrowDown, shift),
            'k' => (Key::ArrowUp, shift),
            'l' => (Key::ArrowRight, shift),
            '0' => (Key::Home, shift),
            '$' => (Key::End, shift),
            'G' => (Key::ArrowDown, Modifiers::COMMAND | shift),
            'g' => (Key::ArrowUp, Modifiers::COMMAND | shift),
            _ => return,
        };
        Self::push_repeated(out, key, modifiers, count);
    }

    fn motion_target(content: &str, cursor: usize, c: char, count: usize) -> Option<usize> {
        let chars: Vec<char> = content.chars().collect();
        let mut target = cursor;
        for _ in 0..count {
            target = match c {
                'w' => next_word_start(content, target),
                'b' => prev_word_start(content, target),
                '0' => line_start(&chars, target),
                '$' => line_end(&chars, target),
                'h' => target.saturating_sub(1),
                'l' => (target + 1).min(chars.len()),
                'G' => chars.len(),
                'g' => 0,
                _ => return None,
            };
        }
        Some(target)
    }

    /// Apply a pending operator with the given motion char.
    fn apply_operator(&mut self, content: &str, cursor: usize, c: char, count: usize) -> VimAction {
        let op = self.operator.take().unwrap();
        self.count = None;

        // Line-wise dgg / dG: delete whole lines from/to the document edge.
        if c == 'g' || c == 'G' {
            let (text, cur) = if c == 'g' {
                let line_no = content[..content
                    .char_indices()
                    .nth(cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(content.len())]
                    .matches('\n')
                    .count();
                vim_dd_count(content, 0, line_no + 1)
            } else {
                vim_dd_count(content, cursor, usize::MAX / 2)
            };
            return self.finish_edit(op, text, cur);
        }

        // Line-wise dj / dk: delete current line plus adjacent lines.
        if c == 'j' || c == 'k' {
            let mut text = content.to_string();
            let mut cur = cursor;
            for _ in 0..count {
                if c == 'j' {
                    let (t, u) = vim_dd_count(&text, cur, 2);
                    text = t;
                    cur = u;
                } else {
                    let chars: Vec<char> = text.chars().collect();
                    let start = line_start(&chars, cur);
                    if start == 0 {
                        let (t, u) = vim_dd(&text, cur);
                        text = t;
                        cur = u;
                    } else {
                        let (t, u) = vim_dd_count(&text, start.saturating_sub(1), 2);
                        text = t;
                        cur = u;
                    }
                }
            }
            return self.finish_edit(op, text, cur);
        }

        let Some(target) = Self::motion_target(content, cursor, c, count) else {
            return VimAction::None;
        };
        let chars: Vec<char> = content.chars().collect();
        let (start, mut end) = if target >= cursor {
            (cursor, target)
        } else {
            (target, cursor)
        };
        if c == '$' || c == 'G' || c == 'l' {
            end = (end + 1).min(chars.len());
        }
        if start >= end || start >= chars.len() {
            return VimAction::None;
        }
        let removed: String = chars[start..end].iter().collect();
        match op {
            Operator::Yank => {
                self.register = removed.clone();
                VimAction::Yank(removed)
            }
            Operator::Delete | Operator::Change => {
                let mut new_chars = Vec::with_capacity(chars.len());
                new_chars.extend_from_slice(&chars[..start]);
                new_chars.extend_from_slice(&chars[end..]);
                let text: String = new_chars.into_iter().collect();
                self.finish_edit(op, text, start)
            }
        }
    }

    fn finish_edit(&mut self, op: Operator, new_content: String, new_cursor: usize) -> VimAction {
        if op == Operator::Change {
            self.mode = VimMode::Insert;
        }
        VimAction::ContentChanged {
            new_content,
            new_cursor,
        }
    }

    pub fn process_events(
        &mut self,
        events: &mut Vec<Event>,
        focused: bool,
        content: &str,
        cursor: usize,
    ) -> VimAction {
        if !self.enabled || !focused {
            return VimAction::None;
        }

        let old_events = std::mem::take(events);
        let mut out_events = Vec::with_capacity(old_events.len());
        let mut overall_action = VimAction::None;
        let mut current_content = content.to_string();
        let mut current_cursor = cursor;

        for event in old_events {
            if self.mode == VimMode::Insert {
                match event {
                    Event::Key {
                        key: Key::Escape,
                        pressed: true,
                        repeat,
                        ..
                    } => {
                        if !repeat {
                            overall_action = self.on_escape();
                        }
                    }
                    _ => out_events.push(event),
                }
                continue;
            }

            match event {
                Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    repeat,
                    ..
                } => {
                    if !repeat {
                        overall_action = self.on_escape();
                    }
                }
                Event::Key {
                    key: Key::R,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.mac_cmd => {
                    self.reset_transient();
                    out_events.push(key_event(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT));
                }
                Event::Key {
                    key: Key::Backspace | Key::Delete | Key::Enter | Key::Tab,
                    ..
                } => {}
                Event::Key {
                    key: Key::H | Key::K | Key::U | Key::W,
                    modifiers,
                    ..
                } if modifiers.ctrl || modifiers.command => {}
                Event::Key { modifiers, .. } if modifiers.command => {
                    out_events.push(event);
                }
                Event::Key {
                    key:
                        Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::Home
                        | Key::End
                        | Key::PageUp
                        | Key::PageDown,
                    ..
                } => {
                    out_events.push(event);
                }
                Event::Text(text) => {
                    for c in text.chars() {
                        if c.is_control() {
                            continue;
                        }
                        let action =
                            self.handle_char(c, &current_content, current_cursor, &mut out_events);
                        match &action {
                            VimAction::None => {}
                            VimAction::ContentChanged {
                                new_content,
                                new_cursor,
                            } => {
                                current_content = new_content.clone();
                                current_cursor = *new_cursor;
                                overall_action = action;
                            }
                            VimAction::SetCursor(pos) => {
                                current_cursor = *pos;
                                overall_action = action;
                            }
                            _ => overall_action = action,
                        }
                    }
                }
                Event::Ime(egui::ImeEvent::Disabled) => out_events.push(event),
                Event::Ime(_) => {}
                Event::Cut | Event::Paste(_) => {}
                Event::Key { .. } => {}
                _ => out_events.push(event),
            }
        }

        *events = out_events;
        overall_action
    }

    fn handle_char(
        &mut self,
        c: char,
        content: &str,
        cursor: usize,
        out_events: &mut Vec<Event>,
    ) -> VimAction {
        let visual = self.mode == VimMode::Visual;

        // Count prefix: 1-9 starts a count, 0 extends an existing one.
        if c.is_ascii_digit() && (c != '0' || self.count.is_some()) {
            let digit = c.to_digit(10).unwrap() as usize;
            self.count = Some(self.count.unwrap_or(0) * 10 + digit);
            return VimAction::None;
        }

        // Pending gg prefix.
        if self.pending == Some('g') {
            self.pending = None;
            if c == 'g' {
                if self.operator.is_some() {
                    let count = self.take_count();
                    return self.apply_operator(content, cursor, 'g', count);
                }
                Self::motion_event(out_events, 'g', visual, 1);
                return VimAction::None;
            }
        }

        // Pending operator: the next char is the motion.
        if self.operator.is_some() {
            let same = matches!(
                (self.operator, c),
                (Some(Operator::Delete), 'd')
                    | (Some(Operator::Change), 'c')
                    | (Some(Operator::Yank), 'y')
            );
            if same {
                let count = self.take_count();
                let (text, cur) = vim_dd_count(content, cursor, count);
                let op = self.operator.take().unwrap();
                return self.finish_edit(op, text, cur);
            }
            let count = self.take_count();
            return self.apply_operator(content, cursor, c, count);
        }

        // Visual-mode direct actions.
        if visual {
            match c {
                'v' => {
                    self.mode = VimMode::Normal;
                    return VimAction::ClearSelection;
                }
                'd' | 'x' => {
                    self.mode = VimMode::Normal;
                    out_events.push(key_event(Key::Backspace, Modifiers::NONE));
                    return VimAction::ModeChanged;
                }
                'c' => {
                    self.mode = VimMode::Insert;
                    out_events.push(key_event(Key::Backspace, Modifiers::NONE));
                    return VimAction::ModeChanged;
                }
                'y' => {
                    self.mode = VimMode::Normal;
                    out_events.push(key_event(Key::C, Modifiers::COMMAND));
                    return VimAction::ClearSelection;
                }
                'p' => {
                    self.mode = VimMode::Normal;
                    let count = self.take_count();
                    return VimAction::Paste { count };
                }
                _ => {}
            }
        }

        match c {
            'v' if !visual => {
                self.mode = VimMode::Visual;
                VimAction::ModeChanged
            }
            'g' => {
                self.pending = Some('g');
                VimAction::None
            }
            'd' => {
                self.operator = Some(Operator::Delete);
                VimAction::None
            }
            'c' => {
                self.operator = Some(Operator::Change);
                VimAction::None
            }
            'y' => {
                self.operator = Some(Operator::Yank);
                VimAction::None
            }
            'h' | 'j' | 'k' | 'l' | '0' | '$' | 'G' => {
                let count = self.take_count();
                Self::motion_event(out_events, c, visual, count);
                VimAction::None
            }
            'w' | 'b' => {
                let count = self.take_count();
                if visual {
                    // In Visual mode extend the selection with the native word motion.
                    let key = if c == 'w' {
                        Key::ArrowRight
                    } else {
                        Key::ArrowLeft
                    };
                    Self::push_repeated(out_events, key, Modifiers::ALT | Modifiers::SHIFT, count);
                    return VimAction::None;
                }
                let mut target = cursor;
                for _ in 0..count {
                    target = if c == 'w' {
                        next_word_start(content, target)
                    } else {
                        prev_word_start(content, target)
                    };
                }
                VimAction::SetCursor(target)
            }
            'x' => {
                let count = self.take_count();
                let chars: Vec<char> = content.chars().collect();
                let mut idx = cursor;
                let mut remaining = count;
                while remaining > 0 && idx < chars.len() && chars[idx] != '\n' {
                    out_events.push(key_event(Key::Delete, Modifiers::NONE));
                    idx += 1;
                    remaining -= 1;
                }
                VimAction::None
            }
            'u' => {
                out_events.push(key_event(Key::Z, Modifiers::COMMAND));
                VimAction::None
            }
            'p' => VimAction::Paste {
                count: self.take_count(),
            },
            '/' => VimAction::OpenSearch,
            'i' => {
                self.mode = VimMode::Insert;
                VimAction::ModeChanged
            }
            'a' => {
                self.mode = VimMode::Insert;
                out_events.push(key_event(Key::ArrowRight, Modifiers::NONE));
                VimAction::ModeChanged
            }
            'I' => {
                self.mode = VimMode::Insert;
                out_events.push(key_event(Key::Home, Modifiers::NONE));
                VimAction::ModeChanged
            }
            'A' => {
                self.mode = VimMode::Insert;
                out_events.push(key_event(Key::End, Modifiers::NONE));
                VimAction::ModeChanged
            }
            'o' => {
                self.mode = VimMode::Insert;
                let (new_content, new_cursor) = vim_o(content, cursor);
                VimAction::ContentChanged {
                    new_content,
                    new_cursor,
                }
            }
            'O' => {
                self.mode = VimMode::Insert;
                let (new_content, new_cursor) = vim_upper_o(content, cursor);
                VimAction::ContentChanged {
                    new_content,
                    new_cursor,
                }
            }
            _ => VimAction::None,
        }
    }
}

/// Delete `count` lines starting at the cursor's line.
pub fn vim_dd_count(content: &str, cursor_char: usize, count: usize) -> (String, usize) {
    let mut text = content.to_string();
    let mut cursor = cursor_char;
    for _ in 0..count.max(1) {
        let (new_text, new_cursor) = vim_dd(&text, cursor);
        text = new_text;
        cursor = new_cursor;
        if text.is_empty() {
            break;
        }
    }
    (text, cursor)
}

/// Delete current line (paragraph). Uses char indices for full Unicode/Chinese safety.
pub fn vim_dd(content: &str, cursor_char: usize) -> (String, usize) {
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() {
        return (String::new(), 0);
    }

    let cursor = cursor_char.min(chars.len().saturating_sub(1));
    let mut line_start = cursor;
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }
    let mut line_end = cursor;
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }

    let (del_start, del_end, new_cursor) = if line_end < chars.len() {
        (line_start, line_end + 1, line_start)
    } else if line_start > 0 {
        let prev_nl = line_start - 1;
        let mut prev_start = prev_nl;
        while prev_start > 0 && chars[prev_start - 1] != '\n' {
            prev_start -= 1;
        }
        (prev_nl, chars.len(), prev_start)
    } else {
        (0, chars.len(), 0)
    };

    let mut new_chars = Vec::with_capacity(chars.len());
    new_chars.extend_from_slice(&chars[..del_start]);
    new_chars.extend_from_slice(&chars[del_end..]);
    let new_cursor = new_cursor.min(new_chars.len());
    (new_chars.into_iter().collect(), new_cursor)
}

/// Open newline below current line and return (new_content, new_cursor_char).
pub fn vim_o(content: &str, cursor_char: usize) -> (String, usize) {
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() {
        return ("\n".to_string(), 1);
    }

    let cursor = cursor_char.min(chars.len().saturating_sub(1));
    let mut line_end = cursor;
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }
    let insert_pos = if line_end < chars.len() {
        line_end + 1
    } else {
        chars.len()
    };

    let mut new_chars = Vec::with_capacity(chars.len() + 1);
    new_chars.extend_from_slice(&chars[..insert_pos]);
    new_chars.push('\n');
    new_chars.extend_from_slice(&chars[insert_pos..]);
    (new_chars.into_iter().collect(), line_end + 1)
}

/// Open newline above current line and return (new_content, new_cursor_char).
pub fn vim_upper_o(content: &str, cursor_char: usize) -> (String, usize) {
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() {
        return ("\n".to_string(), 0);
    }

    let cursor = cursor_char.min(chars.len().saturating_sub(1));
    let mut line_start = cursor;
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }

    let mut new_chars = Vec::with_capacity(chars.len() + 1);
    new_chars.extend_from_slice(&chars[..line_start]);
    new_chars.push('\n');
    new_chars.extend_from_slice(&chars[line_start..]);
    (new_chars.into_iter().collect(), line_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal_state() -> VimState {
        let mut vim = VimState::new(true);
        vim.mode = VimMode::Normal;
        vim
    }

    fn feed(
        vim: &mut VimState,
        text: &str,
        content: &str,
        cursor: usize,
    ) -> (Vec<Event>, VimAction) {
        let mut events = vec![Event::Text(text.to_string())];
        let action = vim.process_events(&mut events, true, content, cursor);
        (events, action)
    }

    fn esc_event() -> Event {
        key_event(Key::Escape, Modifiers::NONE)
    }

    #[test]
    fn insert_esc_switches_to_normal_and_consumes_event() {
        let mut vim = VimState::new(true);
        let mut events = vec![Event::Text("a".into()), esc_event()];
        let action = vim.process_events(&mut events, true, "abc", 0);
        assert_eq!(vim.mode, VimMode::Normal);
        assert_eq!(action, VimAction::ModeChanged);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn disabled_or_unfocused_is_noop() {
        let mut vim = VimState::new(false);
        let mut events = vec![Event::Text("j".into()), esc_event()];
        assert_eq!(
            vim.process_events(&mut events, true, "", 0),
            VimAction::None
        );
        assert_eq!(events.len(), 2);

        let mut vim = normal_state();
        let mut events = vec![Event::Text("j".into())];
        assert_eq!(
            vim.process_events(&mut events, false, "", 0),
            VimAction::None
        );
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn normal_motions_synthesize_keys() {
        let mut vim = normal_state();
        for (ch, key) in [
            ('h', Key::ArrowLeft),
            ('j', Key::ArrowDown),
            ('k', Key::ArrowUp),
            ('l', Key::ArrowRight),
            ('0', Key::Home),
            ('$', Key::End),
        ] {
            let (events, _) = feed(&mut vim, ch.to_string().as_str(), "abc", 0);
            assert!(
                matches!(&events[0], Event::Key { key: k, .. } if *k == key),
                "motion {ch}"
            );
        }
    }

    #[test]
    fn count_prefix_repeats_motions_and_x() {
        let mut vim = normal_state();
        let (events, _) = feed(&mut vim, "3j", "a\nb\nc", 0);
        assert_eq!(events.len(), 3);
        let (events, _) = feed(&mut vim, "2x", "abcd", 0);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn chinese_word_motions() {
        let text = "你好 world，再见";
        // 你(0) 好(1) ' '(2) w(3) o r l d ，(8) 再(9) 见(10)
        // With jieba segmentation: "你好" is a word, "再见" is a word.
        assert_eq!(next_word_start(text, 0), 3);
        assert_eq!(next_word_start(text, 1), 3);
        assert_eq!(next_word_start(text, 3), 8);
        assert_eq!(next_word_start(text, 8), 9);
        assert_eq!(next_word_start(text, 9), 11);
        assert_eq!(prev_word_start(text, 11), 9);
        assert_eq!(prev_word_start(text, 9), 8);
        assert_eq!(prev_word_start(text, 8), 3);
        assert_eq!(prev_word_start(text, 3), 0);

        // Multiple Chinese words in sequence: "你好世界，再见！"
        // 你(0) 好(1) 世(2) 界(3) ，(4) 再(5) 见(6) ！(7)
        let multi = "你好世界，再见！";
        assert_eq!(next_word_start(multi, 0), 2); // "你好" -> "世界"
        assert_eq!(next_word_start(multi, 2), 4); // "世界" -> "，"
        assert_eq!(next_word_start(multi, 4), 5); // "，" -> "再见"
        assert_eq!(next_word_start(multi, 5), 7); // "再见" -> "！"
        assert_eq!(prev_word_start(multi, 7), 5); // "！" -> "再见"
        assert_eq!(prev_word_start(multi, 5), 4); // "再见" -> "，"
        assert_eq!(prev_word_start(multi, 4), 2); // "，" -> "世界"
        assert_eq!(prev_word_start(multi, 2), 0); // "世界" -> "你好"
    }

    #[test]
    fn w_b_return_set_cursor() {
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "w", "你好世界 world", 0);
        assert_eq!(action, VimAction::SetCursor(2));
        let (_, action) = feed(&mut vim, "2w", "你好世界 world", 0);
        assert_eq!(action, VimAction::SetCursor(5));
        let (_, action) = feed(&mut vim, "b", "你好世界 world", 5);
        assert_eq!(action, VimAction::SetCursor(2));
        let (_, action) = feed(&mut vim, "2b", "你好世界 world", 5);
        assert_eq!(action, VimAction::SetCursor(0));
    }

    #[test]
    fn gg_pending_across_frames() {
        let mut vim = normal_state();
        let (events, _) = feed(&mut vim, "g", "a\nb", 1);
        assert!(events.is_empty());
        assert_eq!(vim.pending, Some('g'));
        let (events, _) = feed(&mut vim, "g", "a\nb", 1);
        assert!(
            matches!(&events[0], Event::Key { key: Key::ArrowUp, modifiers, .. } if modifiers.command)
        );
    }

    #[test]
    fn dd_and_count_dd() {
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "dd", "第一行\n第二行\n第三行", 1);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "第二行\n第三行".into(),
                new_cursor: 0
            }
        );
        let (_, action) = feed(&mut vim, "2dd", "a\nb\nc", 0);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "c".into(),
                new_cursor: 0
            }
        );
    }

    #[test]
    fn operators_dw_cw_yw() {
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "dw", "hello world", 0);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "world".into(),
                new_cursor: 0
            }
        );

        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "dw", "你好世界", 0);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "世界".into(),
                new_cursor: 0
            }
        );

        let (_, action) = feed(&mut vim, "cw", "hello world", 0);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "world".into(),
                new_cursor: 0
            }
        );
        assert_eq!(vim.mode, VimMode::Insert);

        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "yw", "hello world", 0);
        assert_eq!(action, VimAction::Yank("hello ".into()));
        assert_eq!(vim.register(), "hello ");

        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "yw", "你好世界", 0);
        assert_eq!(action, VimAction::Yank("你好".into()));
        assert_eq!(vim.register(), "你好");
    }

    #[test]
    fn operator_dollar_and_gg() {
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "d$", "hello world", 5);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "hello".into(),
                new_cursor: 5
            }
        );
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "dgg", "a\nb\nc", 3);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "c".into(),
                new_cursor: 0
            }
        );
    }

    #[test]
    fn visual_motions_use_shift_and_actions() {
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "v", "hello", 0);
        assert_eq!(action, VimAction::ModeChanged);
        assert_eq!(vim.mode, VimMode::Visual);
        let (events, _) = feed(&mut vim, "l", "hello", 0);
        assert!(
            matches!(&events[0], Event::Key { key: Key::ArrowRight, modifiers, .. } if modifiers.shift)
        );
        let (events, action) = feed(&mut vim, "d", "hello", 0);
        assert!(matches!(
            &events[0],
            Event::Key {
                key: Key::Backspace,
                ..
            }
        ));
        assert_eq!(vim.mode, VimMode::Normal);
        assert_eq!(action, VimAction::ModeChanged);
    }

    #[test]
    fn visual_yank_and_escape() {
        let mut vim = normal_state();
        feed(&mut vim, "v", "hello", 0);
        let (events, action) = feed(&mut vim, "y", "hello", 0);
        assert!(
            matches!(&events[0], Event::Key { key: Key::C, modifiers, .. } if modifiers.command)
        );
        assert_eq!(action, VimAction::ClearSelection);
        assert_eq!(vim.mode, VimMode::Normal);

        feed(&mut vim, "v", "hello", 0);
        assert_eq!(vim.mode, VimMode::Visual);
        let mut events = vec![esc_event()];
        let action = vim.process_events(&mut events, true, "hello", 0);
        assert_eq!(action, VimAction::ClearSelection);
        assert_eq!(vim.mode, VimMode::Normal);
    }

    #[test]
    fn paste_and_search_actions() {
        let mut vim = normal_state();
        let (_, action) = feed(&mut vim, "p", "abc", 0);
        assert_eq!(action, VimAction::Paste { count: 1 });
        let (_, action) = feed(&mut vim, "3p", "abc", 0);
        assert_eq!(action, VimAction::Paste { count: 3 });
        let (_, action) = feed(&mut vim, "/", "abc", 0);
        assert_eq!(action, VimAction::OpenSearch);
    }

    #[test]
    fn normal_drops_text_ime_paste_and_dangerous_keys() {
        let mut vim = normal_state();
        let mut events = vec![
            Event::Text("你".into()),
            Event::Ime(egui::ImeEvent::Commit("你好".into())),
            Event::Paste("剪贴板".into()),
            Event::Cut,
            key_event(Key::Backspace, Modifiers::NONE),
            key_event(Key::Delete, Modifiers::COMMAND),
            key_event(Key::W, Modifiers::CTRL),
        ];
        let action = vim.process_events(&mut events, true, "content", 0);
        assert_eq!(action, VimAction::None);
        assert!(events.is_empty());
    }

    #[test]
    fn normal_esc_switches_to_insert_and_clears_pending() {
        let mut vim = normal_state();
        vim.pending = Some('d');
        let mut events = vec![esc_event()];
        let action = vim.process_events(&mut events, true, "abc", 0);
        assert_eq!(vim.mode, VimMode::Insert);
        assert_eq!(action, VimAction::ModeChanged);
        assert_eq!(vim.pending, None);
    }

    #[test]
    fn repeated_esc_toggles_between_modes() {
        let mut vim = VimState::new(true);
        assert_eq!(vim.mode, VimMode::Insert);

        // 1st Esc: Insert -> Normal
        let mut events = vec![esc_event()];
        assert_eq!(
            vim.process_events(&mut events, true, "abc", 0),
            VimAction::ModeChanged
        );
        assert_eq!(vim.mode, VimMode::Normal);

        // 2nd Esc: Normal -> Insert
        let mut events = vec![esc_event()];
        assert_eq!(
            vim.process_events(&mut events, true, "abc", 0),
            VimAction::ModeChanged
        );
        assert_eq!(vim.mode, VimMode::Insert);

        // 3rd Esc: Insert -> Normal
        let mut events = vec![esc_event()];
        assert_eq!(
            vim.process_events(&mut events, true, "abc", 0),
            VimAction::ModeChanged
        );
        assert_eq!(vim.mode, VimMode::Normal);

        // Enter Visual mode ('v'), then Esc -> Normal, then Esc -> Insert
        let (_, action) = feed(&mut vim, "v", "abc", 0);
        assert_eq!(action, VimAction::ModeChanged);
        assert_eq!(vim.mode, VimMode::Visual);

        let mut events = vec![esc_event()];
        assert_eq!(
            vim.process_events(&mut events, true, "abc", 0),
            VimAction::ClearSelection
        );
        assert_eq!(vim.mode, VimMode::Normal);

        let mut events = vec![esc_event()];
        assert_eq!(
            vim.process_events(&mut events, true, "abc", 0),
            VimAction::ModeChanged
        );
        assert_eq!(vim.mode, VimMode::Insert);
    }

    #[test]
    fn insert_commands_switch_mode() {
        for (ch, expected_key) in [('a', Key::ArrowRight), ('I', Key::Home), ('A', Key::End)] {
            let mut vim = normal_state();
            let (events, action) = feed(&mut vim, ch.to_string().as_str(), "abc", 0);
            assert_eq!(vim.mode, VimMode::Insert);
            assert_eq!(action, VimAction::ModeChanged);
            assert!(matches!(&events[0], Event::Key { key, .. } if *key == expected_key));
        }
        let mut vim = normal_state();
        let (events, action) = feed(&mut vim, "i", "abc", 0);
        assert!(events.is_empty());
        assert_eq!(action, VimAction::ModeChanged);
        let (_, action) = feed(&mut normal_state(), "o", "abc", 1);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "abc\n".into(),
                new_cursor: 4
            }
        );
        let (_, action) = feed(&mut normal_state(), "O", "abc", 1);
        assert_eq!(
            action,
            VimAction::ContentChanged {
                new_content: "\nabc".into(),
                new_cursor: 0
            }
        );
    }

    #[test]
    fn ctrl_r_redo() {
        let mut vim = normal_state();
        let mut events = vec![key_event(Key::R, Modifiers::CTRL)];
        vim.process_events(&mut events, true, "abc", 0);
        assert!(
            matches!(&events[0], Event::Key { key: Key::Z, modifiers, .. } if modifiers.command && modifiers.shift)
        );
    }

    #[test]
    fn vim_dd_edges() {
        assert_eq!(vim_dd("", 0), (String::new(), 0));
        assert_eq!(vim_dd("独苗中文段落", 2), (String::new(), 0));
        assert_eq!(
            vim_dd("第一行\n第二行\n第三行", 9),
            ("第一行\n第二行".into(), 4)
        );
        assert_eq!(vim_o("", 0), ("\n".into(), 1));
        assert_eq!(vim_upper_o("", 0), ("\n".into(), 0));
    }
}
