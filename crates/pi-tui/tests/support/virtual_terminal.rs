// Shared support module: each integration-test binary uses a subset of these helpers.
#![allow(dead_code)]

//! Test double for `packages/tui/test/virtual-terminal.ts`.
//!
//! The TypeScript test double delegates to `@xterm/headless`. This port keeps a
//! small ANSI screen model that is sufficient for the TUI's output: cursor
//! movement, line/screen clears, scrollback, SGR italic tracking, and wide
//! graphemes. Kitty/OSC/APC sequences are ignored, as xterm does for the parts
//! the TUI tests assert on.

use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::terminal::{Terminal, TerminalEvent};
use pi_tui::utils::visible_width;

#[derive(Clone, Default)]
struct Cell {
    text: String,
    width: usize,
    italic: bool,
    underline: bool,
}

impl Cell {
    fn blank() -> Self {
        Self {
            text: " ".to_string(),
            width: 1,
            italic: false,
            underline: false,
        }
    }

    fn continuation() -> Self {
        Self {
            text: String::new(),
            width: 0,
            italic: false,
            underline: false,
        }
    }
}

struct Emulator {
    columns: usize,
    rows: usize,
    scrollback: Vec<Vec<Cell>>,
    screen: Vec<Vec<Cell>>,
    cursor_row: usize,
    cursor_col: usize,
    saved_cursor: Option<(usize, usize)>,
    italic: bool,
    underline: bool,
}

impl Emulator {
    fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns,
            rows,
            scrollback: Vec::new(),
            screen: (0..rows).map(|_| Self::blank_row(columns)).collect(),
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor: None,
            italic: false,
            underline: false,
        }
    }

    fn blank_row(columns: usize) -> Vec<Cell> {
        (0..columns).map(|_| Cell::blank()).collect()
    }

    fn viewport(&self) -> Vec<String> {
        self.screen.iter().map(|row| translate_row(row)).collect()
    }

    fn scroll_buffer(&self) -> Vec<String> {
        self.scrollback
            .iter()
            .chain(self.screen.iter())
            .map(|row| translate_row(row))
            .collect()
    }

    fn scroll(&mut self) {
        if self.screen.is_empty() {
            return;
        }
        let first = self.screen.remove(0);
        self.scrollback.push(first);
        self.screen.push(Self::blank_row(self.columns));
    }

    fn line_feed(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            self.scroll();
        } else {
            self.cursor_row += 1;
        }
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    fn put_char(&mut self, character: char) {
        let text = character.to_string();
        let width = visible_width(&text).max(1);
        if width > 1 && self.cursor_col + width > self.columns {
            self.carriage_return();
            self.line_feed();
        }
        if self.cursor_col >= self.columns {
            self.carriage_return();
            self.line_feed();
        }
        if self.cursor_row >= self.rows {
            self.scroll();
        }
        if self.cursor_col < self.columns {
            self.screen[self.cursor_row][self.cursor_col] = Cell {
                text,
                width,
                italic: self.italic,
                underline: self.underline,
            };
            for offset in 1..width {
                if self.cursor_col + offset < self.columns {
                    self.screen[self.cursor_row][self.cursor_col + offset] = Cell::continuation();
                }
            }
        }
        self.cursor_col += width;
        if self.cursor_col > self.columns {
            self.cursor_col = self.columns;
        }
    }

    fn feed(&mut self, data: &str) {
        let mut index = 0;
        while index < data.len() {
            if data[index..].starts_with('\x1b') {
                index = self.feed_escape(data, index);
                continue;
            }
            let character = data[index..].chars().next().expect("valid char boundary");
            match character {
                '\r' => self.carriage_return(),
                '\n' => self.line_feed(),
                '\t' => {
                    let next = ((self.cursor_col / 8) + 1) * 8;
                    self.cursor_col = next.min(self.columns.saturating_sub(1));
                }
                '\u{7}' => {}
                other => self.put_char(other),
            }
            index += character.len_utf8();
        }
    }

    fn feed_escape(&mut self, data: &str, start: usize) -> usize {
        let rest = &data[start..];
        if rest.starts_with("\x1b[") {
            return self.feed_csi(data, start);
        }
        if rest.starts_with("\x1b]") {
            return consume_string_terminator(data, start + 2);
        }
        if rest.starts_with("\x1b_") {
            return consume_string_terminator(data, start + 2);
        }
        if rest.starts_with("\x1b7") {
            self.saved_cursor = Some((self.cursor_row, self.cursor_col));
            return start + 2;
        }
        if rest.starts_with("\x1b8") {
            if let Some((row, col)) = self.saved_cursor {
                self.cursor_row = row.min(self.rows.saturating_sub(1));
                self.cursor_col = col.min(self.columns.saturating_sub(1));
            }
            return start + 2;
        }
        // Unknown single-character escape: skip ESC and the next char.
        let next = rest[1..]
            .chars()
            .next()
            .map(|character| character.len_utf8())
            .unwrap_or(0);
        start + 1 + next
    }

    fn feed_csi(&mut self, data: &str, start: usize) -> usize {
        let body_start = start + 2;
        let bytes = data.as_bytes();
        let mut index = body_start;
        while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
            index += 1;
        }
        if index >= bytes.len() {
            return bytes.len();
        }
        let final_byte = bytes[index] as char;
        let mut body = &data[body_start..index];
        if let Some(stripped) = body.strip_prefix('?') {
            body = stripped;
        }
        let params: Vec<i64> = body.split(';').map(|value| value.parse::<i64>().unwrap_or(0)).collect();
        let first = params.first().copied().unwrap_or(0);
        match final_byte {
            'A' => {
                let amount = first.max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(amount);
            }
            'B' => {
                let amount = first.max(1) as usize;
                self.cursor_row = (self.cursor_row + amount).min(self.rows.saturating_sub(1));
            }
            'C' => {
                let amount = first.max(1) as usize;
                self.cursor_col = (self.cursor_col + amount).min(self.columns.saturating_sub(1));
            }
            'D' => {
                let amount = first.max(1) as usize;
                self.cursor_col = self.cursor_col.saturating_sub(amount);
            }
            'G' => {
                self.cursor_col = (first.max(1) as usize - 1).min(self.columns.saturating_sub(1));
            }
            'H' | 'f' => {
                let row = params.first().copied().unwrap_or(1).max(1) as usize - 1;
                let col = params.get(1).copied().unwrap_or(1).max(1) as usize - 1;
                self.cursor_row = row.min(self.rows.saturating_sub(1));
                self.cursor_col = col.min(self.columns.saturating_sub(1));
            }
            'J' => match first {
                2 => {
                    self.screen = (0..self.rows).map(|_| Self::blank_row(self.columns)).collect();
                }
                3 => {
                    self.scrollback.clear();
                    self.screen = (0..self.rows).map(|_| Self::blank_row(self.columns)).collect();
                }
                _ => {
                    self.erase_to_end_of_line();
                    for row in self.cursor_row + 1..self.rows {
                        self.screen[row] = Self::blank_row(self.columns);
                    }
                }
            },
            'K' => match first {
                2 => {
                    self.screen[self.cursor_row] = Self::blank_row(self.columns);
                }
                _ => self.erase_to_end_of_line(),
            },
            'm' => self.apply_sgr(&params),
            _ => {}
        }
        index + 1
    }

    fn erase_to_end_of_line(&mut self) {
        if self.cursor_row < self.rows {
            for col in self.cursor_col..self.columns {
                self.screen[self.cursor_row][col] = Cell::blank();
            }
        }
    }

    fn apply_sgr(&mut self, params: &[i64]) {
        if params.is_empty() {
            self.italic = false;
            self.underline = false;
            return;
        }
        for parameter in params {
            match parameter {
                0 => {
                    self.italic = false;
                    self.underline = false;
                }
                3 => self.italic = true,
                4 => self.underline = true,
                23 => self.italic = false,
                24 => self.underline = false,
                _ => {}
            }
        }
    }

    fn resize(&mut self, columns: usize, rows: usize) {
        if columns != self.columns {
            for row in self.scrollback.iter_mut().chain(self.screen.iter_mut()) {
                row.resize(columns, Cell::blank());
                for cell in row.iter_mut() {
                    if cell.width > 1 && cell.text.is_empty() {
                        *cell = Cell::blank();
                    }
                }
            }
            self.columns = columns;
            self.cursor_col = self.cursor_col.min(columns.saturating_sub(1));
        }
        if rows != self.rows {
            let cursor_absolute = self.scrollback.len() + self.cursor_row;
            let mut buffer: Vec<Vec<Cell>> = self.scrollback.drain(..).chain(self.screen.drain(..)).collect();
            while buffer.len() < rows {
                buffer.push(Self::blank_row(self.columns));
            }
            let keep_from = (cursor_absolute + 1).saturating_sub(rows);
            let split = buffer.len().saturating_sub(rows).max(keep_from);
            let split = split.min(buffer.len());
            self.scrollback = buffer[..split].to_vec();
            self.screen = buffer[split..].to_vec();
            while self.screen.len() < rows {
                self.screen.push(Self::blank_row(self.columns));
            }
            self.rows = rows;
            self.cursor_row = cursor_absolute.saturating_sub(self.scrollback.len());
            self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        }
    }
}

fn consume_string_terminator(data: &str, start: usize) -> usize {
    let rest = &data[start..];
    if let Some(position) = rest.find('\u{7}') {
        return start + position + 1;
    }
    if let Some(position) = rest.find("\x1b\\") {
        return start + position + 2;
    }
    data.len()
}

fn translate_row(row: &[Cell]) -> String {
    let mut text = String::new();
    for cell in row {
        if cell.width == 0 {
            continue;
        }
        text.push_str(&cell.text);
    }
    text.trim_end().to_string()
}

/// Shared terminal used by the ported TUI tests.
pub struct VirtualTerminal {
    columns: usize,
    rows: usize,
    emulator: Emulator,
    writes: Vec<String>,
    events: Vec<TerminalEvent>,
    kitty_protocol_active: bool,
    capture_cursor_writes: bool,
}

impl VirtualTerminal {
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns,
            rows,
            emulator: Emulator::new(columns, rows),
            writes: Vec::new(),
            events: Vec::new(),
            kitty_protocol_active: true,
            capture_cursor_writes: false,
        }
    }

    pub fn send_input(&mut self, data: &str) {
        self.events.push(TerminalEvent::Input(data.to_string()));
    }

    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.columns = columns;
        self.rows = rows;
        self.emulator.resize(columns, rows);
        self.events.push(TerminalEvent::Resize);
    }

    /// Capture helper-method writes (cursor moves, start/stop) as well as `write`.
    /// Ported tests mirror `LoggingVirtualTerminal`, which only captures `write`.
    pub fn set_capture_cursor_writes(&mut self, capture: bool) {
        self.capture_cursor_writes = capture;
    }

    fn emit(&mut self, data: &str) {
        if self.capture_cursor_writes {
            self.writes.push(data.to_string());
        }
        self.emulator.feed(data);
    }

    pub fn writes(&self) -> String {
        self.writes.concat()
    }

    pub fn clear_writes(&mut self) {
        self.writes.clear();
    }

    pub fn viewport(&self) -> Vec<String> {
        self.emulator.viewport()
    }

    pub fn scroll_buffer(&self) -> Vec<String> {
        self.emulator.scroll_buffer()
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.emulator.cursor_col, self.emulator.cursor_row)
    }

    pub fn cell_italic(&self, row: usize, col: usize) -> bool {
        self.emulator.screen[row][col].italic
    }

    pub fn cell_underline(&self, row: usize, col: usize) -> bool {
        self.emulator.screen[row][col].underline
    }

    pub fn flush(&mut self) {}

    pub fn flush_and_get_viewport(&mut self) -> Vec<String> {
        self.viewport()
    }

    pub fn clear(&mut self) {
        self.emulator.screen = (0..self.rows).map(|_| Emulator::blank_row(self.columns)).collect();
        self.emulator.scrollback.clear();
    }

    pub fn reset(&mut self) {
        self.emulator = Emulator::new(self.columns, self.rows);
    }
}

impl Terminal for VirtualTerminal {
    fn start(&mut self) -> Result<(), pi_tui::terminal::TerminalRawModeError> {
        self.emit("\x1b[?2004h");
        Ok(())
    }

    fn stop(&mut self) {
        self.emit("\x1b[?2004l");
    }

    fn drain_input(&mut self, _max_ms: u64, _idle_ms: u64) {}

    fn write(&mut self, data: &str) {
        self.writes.push(data.to_string());
        self.emulator.feed(data);
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn rows(&self) -> usize {
        self.rows
    }

    fn kitty_protocol_active(&self) -> bool {
        self.kitty_protocol_active
    }

    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            self.emit(&format!("\x1b[{lines}B"));
        } else if lines < 0 {
            self.emit(&format!("\x1b[{}A", -lines));
        }
    }

    fn hide_cursor(&mut self) {
        self.emit("\x1b[?25l");
    }

    fn show_cursor(&mut self) {
        self.emit("\x1b[?25h");
    }

    fn clear_line(&mut self) {
        self.emit("\x1b[K");
    }

    fn clear_from_cursor(&mut self) {
        self.emit("\x1b[J");
    }

    fn clear_screen(&mut self) {
        self.emit("\x1b[2J\x1b[H");
    }

    fn set_title(&mut self, title: &str) {
        self.emit(&format!("\x1b]0;{title}\x07"));
    }

    fn set_progress(&mut self, _active: bool) {}

    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.events)
    }
}

/// Downcast helper: the tests keep the concrete handle alongside the trait handle.
pub fn concrete(columns: usize, rows: usize) -> (Rc<RefCell<VirtualTerminal>>, Rc<RefCell<dyn Terminal>>) {
    let terminal = Rc::new(RefCell::new(VirtualTerminal::new(columns, rows)));
    let trait_terminal: Rc<RefCell<dyn Terminal>> = terminal.clone();
    (terminal, trait_terminal)
}
