//! Ported from `packages/tui/src/components/input.ts`.
//!
//! Single-line text input with horizontal scrolling, bracketed paste, a kill
//! ring, and undo. The TypeScript cursor is a UTF-16 code-unit index; the Rust
//! cursor is a byte index. Both advance by whole grapheme clusters, so rendering
//! and edit boundaries are identical.

use std::rc::Rc;

use unicode_segmentation::UnicodeSegmentation;

use crate::keybindings::get_keybindings;
use crate::keys::decode_kitty_printable;
use crate::kill_ring::KillRing;
use crate::tui::{CURSOR_MARKER, Component, Focusable};
use crate::undo_stack::UndoStack;
use crate::utils::{is_whitespace_char, slice_by_column, visible_width};
use crate::word_navigation::{find_word_backward, find_word_forward};

const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastAction {
    Kill,
    Yank,
    TypeWord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InputState {
    value: String,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InputOptions {
    pub mask: bool,
}

pub type InputSubmitFn = Rc<dyn Fn(&str)>;
pub type InputEscapeFn = Rc<dyn Fn()>;

pub struct Input {
    value: String,
    cursor: usize,
    mask: bool,
    pub on_submit: Option<InputSubmitFn>,
    pub on_escape: Option<InputEscapeFn>,
    focused: bool,
    paste_buffer: String,
    is_in_paste: bool,
    kill_ring: KillRing,
    last_action: Option<LastAction>,
    undo_stack: UndoStack<InputState>,
}

impl Input {
    pub fn new(options: InputOptions) -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            mask: options.mask,
            on_submit: None,
            on_escape: None,
            focused: false,
            paste_buffer: String::new(),
            is_in_paste: false,
            kill_ring: KillRing::new(),
            last_action: None,
            undo_stack: UndoStack::new(),
        }
    }

    pub fn get_value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.cursor.min(self.value.len());
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(&InputState {
            value: self.value.clone(),
            cursor: self.cursor,
        });
    }

    fn insert_character(&mut self, character: &str) {
        if is_whitespace_char(character) || self.last_action != Some(LastAction::TypeWord) {
            self.push_undo();
        }
        self.last_action = Some(LastAction::TypeWord);
        self.value.insert_str(self.cursor, character);
        self.cursor += character.len();
    }

    fn previous_grapheme_start(&self) -> Option<usize> {
        if self.cursor == 0 {
            return None;
        }
        let mut start = 0;
        for (index, _) in self.value[..self.cursor].grapheme_indices(true) {
            start = index;
        }
        Some(start)
    }

    fn next_grapheme_end(&self) -> Option<usize> {
        if self.cursor >= self.value.len() {
            return None;
        }
        let grapheme = self.value[self.cursor..].graphemes(true).next()?;
        Some(self.cursor + grapheme.len())
    }

    fn handle_backspace(&mut self) {
        self.last_action = None;
        if let Some(start) = self.previous_grapheme_start() {
            self.push_undo();
            self.value.replace_range(start..self.cursor, "");
            self.cursor = start;
        }
    }

    fn handle_forward_delete(&mut self) {
        self.last_action = None;
        if let Some(end) = self.next_grapheme_end() {
            self.push_undo();
            self.value.replace_range(self.cursor..end, "");
        }
    }

    fn delete_to_line_start(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.push_undo();
        let deleted_text = self.value[..self.cursor].to_string();
        self.kill_ring
            .push(&deleted_text, true, self.last_action == Some(LastAction::Kill));
        self.last_action = Some(LastAction::Kill);
        self.value.replace_range(..self.cursor, "");
        self.cursor = 0;
    }

    fn delete_to_line_end(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        self.push_undo();
        let deleted_text = self.value[self.cursor..].to_string();
        self.kill_ring
            .push(&deleted_text, false, self.last_action == Some(LastAction::Kill));
        self.last_action = Some(LastAction::Kill);
        self.value.truncate(self.cursor);
    }

    fn delete_word_backwards(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let was_kill = self.last_action == Some(LastAction::Kill);
        self.push_undo();
        let old_cursor = self.cursor;
        let delete_from = find_word_backward(&self.value, self.cursor, None);
        let deleted_text = self.value[delete_from..old_cursor].to_string();
        self.kill_ring.push(&deleted_text, true, was_kill);
        self.last_action = Some(LastAction::Kill);
        self.value.replace_range(delete_from..old_cursor, "");
        self.cursor = delete_from;
    }

    fn delete_word_forward(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let was_kill = self.last_action == Some(LastAction::Kill);
        self.push_undo();
        let delete_to = find_word_forward(&self.value, self.cursor, None);
        let deleted_text = self.value[self.cursor..delete_to].to_string();
        self.kill_ring.push(&deleted_text, false, was_kill);
        self.last_action = Some(LastAction::Kill);
        self.value.replace_range(self.cursor..delete_to, "");
    }

    fn yank(&mut self) {
        let Some(text) = self.kill_ring.peek().map(str::to_owned) else {
            return;
        };
        self.push_undo();
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.last_action = Some(LastAction::Yank);
    }

    fn yank_pop(&mut self) {
        if self.last_action != Some(LastAction::Yank) || self.kill_ring.len() <= 1 {
            return;
        }
        self.push_undo();
        let previous_text = self.kill_ring.peek().unwrap_or_default().to_string();
        let previous_start = self.cursor.saturating_sub(previous_text.len());
        self.value.replace_range(previous_start..self.cursor, "");
        self.cursor = previous_start;

        self.kill_ring.rotate();
        let text = self.kill_ring.peek().unwrap_or_default().to_string();
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.last_action = Some(LastAction::Yank);
    }

    fn undo(&mut self) {
        let Some(snapshot) = self.undo_stack.pop() else {
            return;
        };
        self.value = snapshot.value;
        self.cursor = snapshot.cursor;
        self.last_action = None;
    }

    fn move_word_backwards(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.last_action = None;
        self.cursor = find_word_backward(&self.value, self.cursor, None);
    }

    fn move_word_forwards(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        self.last_action = None;
        self.cursor = find_word_forward(&self.value, self.cursor, None);
    }

    fn handle_paste(&mut self, pasted_text: &str) {
        self.last_action = None;
        self.push_undo();
        let clean_text = clean_pasted_text(pasted_text);
        self.value.insert_str(self.cursor, &clean_text);
        self.cursor += clean_text.len();
    }

    fn get_masked_value(&self) -> String {
        self.value.graphemes(true).map(|_| "•").collect()
    }

    fn get_masked_cursor(&self) -> usize {
        self.value[..self.cursor].graphemes(true).count()
    }

    fn handle_paste_chunk(&mut self, data: &str) {
        if data.contains(PASTE_START) {
            self.is_in_paste = true;
            self.paste_buffer.clear();
        }
        let data = data.replacen(PASTE_START, "", 1);
        if !self.is_in_paste {
            return;
        }
        self.paste_buffer.push_str(&data);

        let Some(end_index) = self.paste_buffer.find(PASTE_END) else {
            return;
        };
        let paste_content = self.paste_buffer[..end_index].to_string();
        self.handle_paste(&paste_content);
        self.is_in_paste = false;
        let remaining = self.paste_buffer[end_index + PASTE_END.len()..].to_string();
        self.paste_buffer.clear();
        if !remaining.is_empty() {
            self.handle_input(&remaining);
        }
    }
}

fn clean_pasted_text(pasted_text: &str) -> String {
    let mut cleaned = String::with_capacity(pasted_text.len());
    let mut characters = pasted_text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
            }
            '\n' => {}
            '\t' => cleaned.push_str("    "),
            other => cleaned.push(other),
        }
    }
    cleaned
}

impl Component for Input {
    fn render(&mut self, width: usize) -> Vec<String> {
        const PROMPT: &str = "> ";
        let Some(available_width) = width.checked_sub(PROMPT.len()) else {
            return vec![PROMPT.to_string()];
        };

        let display_value = if self.mask {
            self.get_masked_value()
        } else {
            self.value.clone()
        };
        let cursor_col = if self.mask {
            self.get_masked_cursor()
        } else {
            visible_width(&self.value[..self.cursor])
        };
        let total_width = visible_width(&display_value);

        let (visible_text, cursor_display) = if total_width < available_width {
            let cursor_display = slice_by_column(&display_value, 0, cursor_col, true).len();
            (display_value.clone(), cursor_display)
        } else {
            let scroll_width = if self.cursor == self.value.len() {
                available_width.saturating_sub(1)
            } else {
                available_width
            };
            if scroll_width > 0 {
                let half_width = scroll_width / 2;
                let start_col = if cursor_col < half_width {
                    0
                } else if cursor_col > total_width.saturating_sub(half_width) {
                    total_width.saturating_sub(scroll_width)
                } else {
                    cursor_col.saturating_sub(half_width)
                };
                let before_cursor =
                    slice_by_column(&display_value, start_col, cursor_col.saturating_sub(start_col), true);
                let visible_text = slice_by_column(&display_value, start_col, scroll_width, true);
                (visible_text, before_cursor.len())
            } else {
                (String::new(), 0)
            }
        };

        let after_cursor = visible_text.get(cursor_display..).unwrap_or_default();
        let at_cursor = after_cursor.graphemes(true).next().unwrap_or(" ").to_string();
        let before_cursor = visible_text.get(..cursor_display).unwrap_or_default();
        let after_cursor = visible_text.get(cursor_display + at_cursor.len()..).unwrap_or_default();
        let marker = if self.focused { CURSOR_MARKER } else { "" };
        let cursor_character = format!("\x1b[7m{at_cursor}\x1b[27m");
        let text_with_cursor = format!("{before_cursor}{marker}{cursor_character}{after_cursor}");
        let visual_length = visible_width(&text_with_cursor);
        let padding = " ".repeat(available_width.saturating_sub(visual_length));
        vec![format!("{PROMPT}{text_with_cursor}{padding}")]
    }

    fn handle_input(&mut self, data: &str) {
        if data.contains(PASTE_START) {
            self.handle_paste_chunk(data);
            return;
        }
        if self.is_in_paste {
            self.handle_paste_chunk(data);
            return;
        }

        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.cancel") {
            if let Some(on_escape) = &self.on_escape {
                on_escape();
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.undo") {
            self.undo();
            return;
        }
        if keybindings.matches(data, "tui.input.submit") || data == "\n" {
            if let Some(on_submit) = &self.on_submit {
                on_submit(&self.value);
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteCharBackward") {
            self.handle_backspace();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteCharForward") {
            self.handle_forward_delete();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteWordBackward") {
            self.delete_word_backwards();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteWordForward") {
            self.delete_word_forward();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteToLineStart") {
            self.delete_to_line_start();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteToLineEnd") {
            self.delete_to_line_end();
            return;
        }
        if keybindings.matches(data, "tui.editor.yank") {
            self.yank();
            return;
        }
        if keybindings.matches(data, "tui.editor.yankPop") {
            self.yank_pop();
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLeft") {
            self.last_action = None;
            if let Some(start) = self.previous_grapheme_start() {
                self.cursor = start;
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorRight") {
            self.last_action = None;
            if let Some(end) = self.next_grapheme_end() {
                self.cursor = end;
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLineStart") {
            self.last_action = None;
            self.cursor = 0;
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLineEnd") {
            self.last_action = None;
            self.cursor = self.value.len();
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorWordLeft") {
            self.move_word_backwards();
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorWordRight") {
            self.move_word_forwards();
            return;
        }

        if let Some(printable) = decode_kitty_printable(data) {
            self.insert_character(&printable);
            return;
        }

        let has_control_characters = data.chars().any(|character| {
            let code = character as u32;
            code < 32 || code == 0x7f || (0x80..=0x9f).contains(&code)
        });
        if !has_control_characters {
            self.insert_character(data);
        }
    }

    fn invalidate(&mut self) {}

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for Input {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
