//! ANSI-aware text measurement, wrapping, truncation, and slicing.
//!
//! Ported from `packages/tui/src/utils.ts`. Grapheme segmentation uses the
//! Unicode 17.0 tables of `unicode-segmentation`; character-property and emoji
//! tables are generated in `unicode_tables.rs` from the same Unicode version.

use unicode_segmentation::UnicodeSegmentation;

use crate::unicode_tables::{east_asian_width, is_cjk_break, is_leading_non_printing, is_rgi_emoji, is_zero_width};
use pi_js_compat::string::{is_js_whitespace_char, js_trim, js_trim_end};

/// An extracted terminal escape sequence and its byte length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiCode<'a> {
    pub code: &'a str,
    pub length: usize,
}

/// Terminal-cell range occupied by a grapheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphemeCellRange {
    pub start: usize,
    pub end: usize,
}

/// Visible text extracted from a column range, plus its width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceWithWidth {
    pub text: String,
    pub width: usize,
}

/// "Before" and "after" segments extracted around an overlay region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segments {
    pub before: String,
    pub before_width: usize,
    pub after: String,
    pub after_width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Osc8Terminator {
    Bel,
    St,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveHyperlink {
    params: String,
    url: String,
    terminator: Osc8Terminator,
}

/// Extract an ANSI, OSC, or APC escape sequence starting at byte offset `pos`.
///
/// The TypeScript source indexes UTF-16 code units; Rust callers pass a byte
/// offset. Sequences are ASCII-framed, so both offsets agree whenever `pos`
/// points at the ESC byte.
pub fn extract_ansi_code(value: &str, pos: usize) -> Option<AnsiCode<'_>> {
    if value.as_bytes().get(pos) != Some(&0x1b) {
        return None;
    }

    match value.as_bytes().get(pos + 1) {
        Some(b'[') => {
            let mut end = pos + 2;
            while let Some(byte) = value.as_bytes().get(end) {
                if matches!(byte, b'm' | b'G' | b'K' | b'H' | b'J') {
                    return Some(AnsiCode {
                        code: &value[pos..=end],
                        length: end + 1 - pos,
                    });
                }
                end += 1;
            }
            None
        }
        Some(b']') | Some(b'_') => {
            let mut end = pos + 2;
            while let Some(byte) = value.as_bytes().get(end) {
                if *byte == 0x07 {
                    return Some(AnsiCode {
                        code: &value[pos..=end],
                        length: end + 1 - pos,
                    });
                }
                if *byte == 0x1b && value.as_bytes().get(end + 1) == Some(&b'\\') {
                    return Some(AnsiCode {
                        code: &value[pos..end + 2],
                        length: end + 2 - pos,
                    });
                }
                end += 1;
            }
            None
        }
        _ => None,
    }
}

fn next_char_len(value: &str, index: usize) -> usize {
    value[index..].chars().next().map_or(1, char::len_utf8)
}

fn is_printable_ascii(value: &str) -> bool {
    value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_zero_width_segment(value: &str) -> bool {
    !value.is_empty() && value.chars().all(is_zero_width)
}

fn could_be_emoji(segment: &str) -> bool {
    let Some(first) = segment.chars().next() else {
        return false;
    };
    let code_point = first as u32;
    (0x1f000..=0x1fbff).contains(&code_point)
        || (0x2300..=0x23ff).contains(&code_point)
        || (0x2600..=0x27bf).contains(&code_point)
        || (0x2b50..=0x2b55).contains(&code_point)
        || segment.contains('\u{fe0f}')
        || segment.encode_utf16().count() > 2
}

/// Terminal width of one grapheme cluster.
fn grapheme_width(segment: &str) -> usize {
    if segment == "\t" {
        return 3;
    }

    if is_zero_width_segment(segment) {
        return 0;
    }

    if could_be_emoji(segment) && is_rgi_emoji(segment) {
        return 2;
    }

    let base = segment.trim_start_matches(is_leading_non_printing);
    let Some(first) = base.chars().next() else {
        return 0;
    };

    let code_point = first as u32;
    if (0x1f1e6..=0x1f1ff).contains(&code_point) {
        return 2;
    }

    let mut width = east_asian_width(first);
    // TS slices one UTF-16 unit from the original segment, not its stripped base.
    for value in segment.chars().skip(1) {
        let next = value as u32;
        if (0xff00..=0xffef).contains(&next) {
            width += east_asian_width(value);
        } else if next == 0x0e33 || next == 0x0eb3 {
            width += 1;
        }
    }

    width
}

/// Calculate the visible width of a string in terminal columns.
pub fn visible_width(value: &str) -> usize {
    if value.is_empty() {
        return 0;
    }
    if is_printable_ascii(value) {
        return value.len();
    }

    let mut clean = value.replace('\t', "   ");
    if clean.contains('\x1b') {
        let mut stripped = String::with_capacity(clean.len());
        let mut index = 0;
        while index < clean.len() {
            if let Some(ansi) = extract_ansi_code(&clean, index) {
                index += ansi.length;
            } else {
                let length = next_char_len(&clean, index);
                stripped.push_str(&clean[index..index + length]);
                index += length;
            }
        }
        clean = stripped;
    }

    clean.graphemes(true).map(grapheme_width).sum()
}

/// Remove ANSI, OSC, and APC control sequences while preserving visible text.
pub fn strip_terminal_sequences(value: &str) -> String {
    if !value.contains('\x1b') {
        return value.to_string();
    }

    let mut result = String::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        if let Some(ansi) = extract_ansi_code(value, index) {
            index += ansi.length;
        } else {
            let length = next_char_len(value, index);
            result.push_str(&value[index..index + length]);
            index += length;
        }
    }
    result
}

/// Return the terminal-cell range occupied by the grapheme at a visible column.
pub fn get_grapheme_cell_range(line: &str, column: usize) -> Option<GraphemeCellRange> {
    let mut current_col = 0;
    let mut index = 0;
    while index < line.len() {
        if let Some(ansi) = extract_ansi_code(line, index) {
            index += ansi.length;
            continue;
        }

        let mut text_end = index;
        while text_end < line.len() && extract_ansi_code(line, text_end).is_none() {
            text_end += next_char_len(line, text_end);
        }

        for segment in line[index..text_end].graphemes(true) {
            let width = grapheme_width(segment);
            if width > 0 && column >= current_col && column < current_col + width {
                return Some(GraphemeCellRange {
                    start: current_col,
                    end: current_col + width,
                });
            }
            current_col += width;
        }
        index = text_end;
    }
    None
}

/// Return the OSC 8 hyperlink covering a visible terminal column.
pub fn get_osc8_link_at_column(line: &str, column: usize) -> Option<&str> {
    let mut active_url = None;
    let mut current_col = 0;
    let mut index = 0;
    while index < line.len() {
        if let Some(ansi) = extract_ansi_code(line, index) {
            match parse_osc8_url(ansi.code) {
                Some(Some(url)) => active_url = Some(url),
                Some(None) => active_url = None,
                None => {}
            }
            index += ansi.length;
            continue;
        }

        let mut text_end = index;
        while text_end < line.len() && extract_ansi_code(line, text_end).is_none() {
            text_end += next_char_len(line, text_end);
        }

        for segment in line[index..text_end].graphemes(true) {
            let width = grapheme_width(segment);
            if column >= current_col && column < current_col + width {
                return active_url;
            }
            current_col += width;
        }
        index = text_end;
    }
    None
}

/// Normalize text for terminal output without changing logical editor content.
pub fn normalize_terminal_output(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\u{0e33}' => normalized.push_str("\u{0e4d}\u{0e32}"),
            '\u{0eb3}' => normalized.push_str("\u{0ecd}\u{0eb2}"),
            _ => normalized.push(character),
        }
    }

    normalized
}

fn parse_osc8_url(ansi_code: &str) -> Option<Option<&str>> {
    if !ansi_code.starts_with("\x1b]8;") {
        return None;
    }
    let body_end = if ansi_code.ends_with('\x07') {
        ansi_code.len() - 1
    } else {
        ansi_code.len().saturating_sub(2)
    };
    let body = if body_end >= 4 { &ansi_code[4..body_end] } else { "" };
    let separator = body.find(';')?;
    let url = &body[separator + 1..];
    if url.is_empty() { Some(None) } else { Some(Some(url)) }
}

fn parse_osc8_hyperlink(ansi_code: &str) -> Option<Option<ActiveHyperlink>> {
    if !ansi_code.starts_with("\x1b]8;") {
        return None;
    }

    let terminator = if ansi_code.ends_with('\x07') {
        Osc8Terminator::Bel
    } else {
        Osc8Terminator::St
    };
    let body_end = match terminator {
        Osc8Terminator::Bel => ansi_code.len() - 1,
        Osc8Terminator::St => ansi_code.len().saturating_sub(2),
    };
    let body = if body_end >= 4 { &ansi_code[4..body_end] } else { "" };
    let separator = body.find(';')?;
    let params = &body[..separator];
    let url = &body[separator + 1..];
    if url.is_empty() {
        return Some(None);
    }
    Some(Some(ActiveHyperlink {
        params: params.to_string(),
        url: url.to_string(),
        terminator,
    }))
}

fn format_osc8_hyperlink(hyperlink: &ActiveHyperlink) -> String {
    let terminator = match hyperlink.terminator {
        Osc8Terminator::Bel => "\x07",
        Osc8Terminator::St => "\x1b\\",
    };
    format!("\x1b]8;{};{}{terminator}", hyperlink.params, hyperlink.url)
}

fn format_osc8_close(terminator: Osc8Terminator) -> String {
    match terminator {
        Osc8Terminator::Bel => "\x1b]8;;\x07".to_string(),
        Osc8Terminator::St => "\x1b]8;;\x1b\\".to_string(),
    }
}

#[derive(Debug, Default)]
struct AnsiCodeTracker {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    blink: bool,
    inverse: bool,
    hidden: bool,
    strikethrough: bool,
    fg_color: Option<String>,
    bg_color: Option<String>,
    active_hyperlink: Option<ActiveHyperlink>,
}

impl AnsiCodeTracker {
    fn process(&mut self, ansi_code: &str) {
        if let Some(hyperlink) = parse_osc8_hyperlink(ansi_code) {
            self.active_hyperlink = hyperlink;
            return;
        }

        if !ansi_code.ends_with('m') {
            return;
        }
        let Some(params) = ansi_code.split("\x1b[").skip(1).find_map(|value| {
            let end = value.find('m')?;
            let params = &value[..end];
            params
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b';')
                .then_some(params)
        }) else {
            return;
        };
        if params.is_empty() || params == "0" {
            self.reset();
            return;
        }

        let parts: Vec<&str> = params.split(';').collect();
        let mut index = 0;
        while index < parts.len() {
            let code = parts[index].parse::<i64>().ok();

            if matches!(code, Some(38) | Some(48)) {
                if parts.get(index + 1) == Some(&"5") && parts.get(index + 2).is_some() {
                    let color_code = format!("{};{};{}", parts[index], parts[index + 1], parts[index + 2]);
                    if code == Some(38) {
                        self.fg_color = Some(color_code);
                    } else {
                        self.bg_color = Some(color_code);
                    }
                    index += 3;
                    continue;
                }
                if parts.get(index + 1) == Some(&"2") && parts.get(index + 4).is_some() {
                    let color_code = format!(
                        "{};{};{};{};{}",
                        parts[index],
                        parts[index + 1],
                        parts[index + 2],
                        parts[index + 3],
                        parts[index + 4]
                    );
                    if code == Some(38) {
                        self.fg_color = Some(color_code);
                    } else {
                        self.bg_color = Some(color_code);
                    }
                    index += 5;
                    continue;
                }
            }

            match code {
                Some(0) => self.reset(),
                Some(1) => self.bold = true,
                Some(2) => self.dim = true,
                Some(3) => self.italic = true,
                Some(4) => self.underline = true,
                Some(5) => self.blink = true,
                Some(7) => self.inverse = true,
                Some(8) => self.hidden = true,
                Some(9) => self.strikethrough = true,
                Some(21) => self.bold = false,
                Some(22) => {
                    self.bold = false;
                    self.dim = false;
                }
                Some(23) => self.italic = false,
                Some(24) => self.underline = false,
                Some(25) => self.blink = false,
                Some(27) => self.inverse = false,
                Some(28) => self.hidden = false,
                Some(29) => self.strikethrough = false,
                Some(39) => self.fg_color = None,
                Some(49) => self.bg_color = None,
                Some(value) if (30..=37).contains(&value) || (90..=97).contains(&value) => {
                    self.fg_color = Some(value.to_string());
                }
                Some(value) if (40..=47).contains(&value) || (100..=107).contains(&value) => {
                    self.bg_color = Some(value.to_string());
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn reset(&mut self) {
        self.bold = false;
        self.dim = false;
        self.italic = false;
        self.underline = false;
        self.blink = false;
        self.inverse = false;
        self.hidden = false;
        self.strikethrough = false;
        self.fg_color = None;
        self.bg_color = None;
    }

    fn clear(&mut self) {
        self.reset();
        self.active_hyperlink = None;
    }

    fn get_active_codes(&self) -> String {
        let mut codes: Vec<&str> = Vec::new();
        if self.bold {
            codes.push("1");
        }
        if self.dim {
            codes.push("2");
        }
        if self.italic {
            codes.push("3");
        }
        if self.underline {
            codes.push("4");
        }
        if self.blink {
            codes.push("5");
        }
        if self.inverse {
            codes.push("7");
        }
        if self.hidden {
            codes.push("8");
        }
        if self.strikethrough {
            codes.push("9");
        }
        if let Some(fg_color) = &self.fg_color {
            codes.push(fg_color);
        }
        if let Some(bg_color) = &self.bg_color {
            codes.push(bg_color);
        }

        let mut result = if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        };
        if let Some(hyperlink) = &self.active_hyperlink {
            result.push_str(&format_osc8_hyperlink(hyperlink));
        }
        result
    }

    fn get_line_end_reset(&self) -> String {
        let mut result = String::new();
        if self.underline {
            result.push_str("\x1b[24m");
        }
        if let Some(hyperlink) = &self.active_hyperlink {
            result.push_str(&format_osc8_close(hyperlink.terminator));
        }
        result
    }
}

fn update_tracker_from_text(text: &str, tracker: &mut AnsiCodeTracker) {
    let mut index = 0;
    while index < text.len() {
        if let Some(ansi) = extract_ansi_code(text, index) {
            tracker.process(ansi.code);
            index += ansi.length;
        } else {
            index += next_char_len(text, index);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Space,
    Word,
}

fn split_into_tokens_with_ansi(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut pending_ansi = String::new();
    let mut current_kind: Option<TokenKind> = None;
    let mut index = 0;

    while index < text.len() {
        if let Some(ansi) = extract_ansi_code(text, index) {
            pending_ansi.push_str(ansi.code);
            index += ansi.length;
            continue;
        }

        let mut end = index;
        while end < text.len() && extract_ansi_code(text, end).is_none() {
            end += next_char_len(text, end);
        }

        for segment in text[index..end].graphemes(true) {
            let segment_is_space = segment == " ";
            if !segment_is_space && segment.chars().any(is_cjk_break) {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                let mut token = std::mem::take(&mut pending_ansi);
                token.push_str(segment);
                tokens.push(token);
                continue;
            }

            let segment_kind = if segment_is_space {
                TokenKind::Space
            } else {
                TokenKind::Word
            };
            if !current.is_empty() && current_kind != Some(segment_kind) {
                tokens.push(std::mem::take(&mut current));
            }
            if !pending_ansi.is_empty() {
                current.push_str(&pending_ansi);
                pending_ansi.clear();
            }
            current_kind = Some(segment_kind);
            current.push_str(segment);
        }
        index = end;
    }

    if !pending_ansi.is_empty() {
        if !current.is_empty() {
            current.push_str(&pending_ansi);
        } else if let Some(last) = tokens.last_mut() {
            last.push_str(&pending_ansi);
        } else {
            current = pending_ansi;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Wrap text with ANSI codes preserved. Performs word wrapping only; lines are
/// not padded and carry no background colors.
pub fn wrap_text_with_ansi(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let input_lines = split_lines(text);
    let mut result = Vec::new();
    let mut tracker = AnsiCodeTracker::default();

    for input_line in input_lines {
        let prefix = if result.is_empty() {
            String::new()
        } else {
            tracker.get_active_codes()
        };
        let wrapped_lines = wrap_single_line(&(prefix + input_line), width);
        result.extend(wrapped_lines);
        update_tracker_from_text(input_line, &mut tracker);
    }

    if result.is_empty() { vec![String::new()] } else { result }
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

fn wrap_single_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    if visible_width(line) <= width {
        return vec![line.to_string()];
    }

    let mut wrapped: Vec<String> = Vec::new();
    let mut tracker = AnsiCodeTracker::default();
    let tokens = split_into_tokens_with_ansi(line);

    let mut current_line = String::new();
    let mut current_visible_length = 0;

    for token in tokens {
        let token_visible_length = visible_width(&token);
        let is_whitespace = js_trim(&token).is_empty();

        if token_visible_length > width && !is_whitespace {
            if !current_line.is_empty() {
                current_line.push_str(&tracker.get_line_end_reset());
                wrapped.push(std::mem::take(&mut current_line));
            }

            let broken = break_long_word(&token, width, &mut tracker);
            for line in &broken[..broken.len() - 1] {
                wrapped.push(line.clone());
            }
            current_line = broken.last().cloned().unwrap_or_default();
            current_visible_length = visible_width(&current_line);
            continue;
        }

        let total_needed = current_visible_length + token_visible_length;
        if total_needed > width && current_visible_length > 0 {
            let mut line_to_wrap = js_trim_end(&current_line).to_string();
            line_to_wrap.push_str(&tracker.get_line_end_reset());
            wrapped.push(line_to_wrap);
            if is_whitespace {
                current_line = tracker.get_active_codes();
                current_visible_length = 0;
            } else {
                current_line = tracker.get_active_codes() + &token;
                current_visible_length = token_visible_length;
            }
        } else {
            current_line.push_str(&token);
            current_visible_length += token_visible_length;
        }

        update_tracker_from_text(&token, &mut tracker);
    }

    if !current_line.is_empty() {
        wrapped.push(current_line);
    }

    if wrapped.is_empty() {
        vec![String::new()]
    } else {
        wrapped.iter().map(|line| js_trim_end(line).to_string()).collect()
    }
}

enum WordSegment<'a> {
    Ansi(&'a str),
    Grapheme(&'a str),
}

fn break_long_word(word: &str, width: usize, tracker: &mut AnsiCodeTracker) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = tracker.get_active_codes();
    let mut current_width = 0;

    let mut segments: Vec<WordSegment<'_>> = Vec::new();
    let mut index = 0;
    while index < word.len() {
        if let Some(ansi) = extract_ansi_code(word, index) {
            segments.push(WordSegment::Ansi(ansi.code));
            index += ansi.length;
        } else {
            let mut end = index;
            while end < word.len() && extract_ansi_code(word, end).is_none() {
                end += next_char_len(word, end);
            }
            for segment in word[index..end].graphemes(true) {
                segments.push(WordSegment::Grapheme(segment));
            }
            index = end;
        }
    }

    for segment in segments {
        match segment {
            WordSegment::Ansi(value) => {
                current_line.push_str(value);
                tracker.process(value);
            }
            WordSegment::Grapheme(grapheme) => {
                if grapheme.is_empty() {
                    continue;
                }
                let grapheme_width = visible_width(grapheme);
                if current_width + grapheme_width > width {
                    current_line.push_str(&tracker.get_line_end_reset());
                    lines.push(std::mem::take(&mut current_line));
                    current_line = tracker.get_active_codes();
                    current_width = 0;
                }
                current_line.push_str(grapheme);
                current_width += grapheme_width;
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() { vec![String::new()] } else { lines }
}

// `width` is the wrap limit; grapheme widths are tracked separately.

/// Check if a character is whitespace (ECMAScript `\s`).
pub fn is_whitespace_char(value: &str) -> bool {
    value.chars().any(is_js_whitespace_char)
}

const PUNCTUATION: &str = "(){}[]<>.,;:'\"!?+-=*/\\|&%^$#@~`";

/// Check if a character is punctuation.
pub fn is_punctuation_char(value: &str) -> bool {
    value.chars().any(is_punctuation_char_value)
}

pub(crate) fn is_punctuation_char_value(character: char) -> bool {
    PUNCTUATION.contains(character)
}

fn truncate_fragment_to_width(text: &str, max_width: usize) -> (String, usize) {
    if max_width == 0 || text.is_empty() {
        return (String::new(), 0);
    }

    if is_printable_ascii(text) {
        let clipped = &text[..max_width.min(text.len())];
        return (clipped.to_string(), clipped.len());
    }

    let has_ansi = text.contains('\x1b');
    let has_tabs = text.contains('\t');
    if !has_ansi && !has_tabs {
        let mut result = String::new();
        let mut width = 0;
        for segment in text.graphemes(true) {
            let segment_width = grapheme_width(segment);
            if width + segment_width > max_width {
                break;
            }
            result.push_str(segment);
            width += segment_width;
        }
        return (result, width);
    }

    let mut result = String::new();
    let mut width = 0;
    let mut index = 0;
    let mut pending_ansi = String::new();

    while index < text.len() {
        if let Some(ansi) = extract_ansi_code(text, index) {
            pending_ansi.push_str(ansi.code);
            index += ansi.length;
            continue;
        }

        if text.as_bytes()[index] == b'\t' {
            if width + 3 > max_width {
                break;
            }
            result.push_str(&pending_ansi);
            pending_ansi.clear();
            result.push('\t');
            width += 3;
            index += 1;
            continue;
        }

        let mut end = index;
        while end < text.len() && text.as_bytes()[end] != b'\t' {
            if extract_ansi_code(text, end).is_some() {
                break;
            }
            end += next_char_len(text, end);
        }

        for segment in text[index..end].graphemes(true) {
            let segment_width = grapheme_width(segment);
            if width + segment_width > max_width {
                return (result, width);
            }
            result.push_str(&pending_ansi);
            pending_ansi.clear();
            result.push_str(segment);
            width += segment_width;
        }
        index = end;
    }

    (result, width)
}

fn finalize_truncated_result(
    prefix: &str,
    prefix_width: usize,
    ellipsis: &str,
    ellipsis_width: usize,
    max_width: usize,
    pad: bool,
) -> String {
    let visible = prefix_width + ellipsis_width;
    let mut result = if ellipsis.is_empty() {
        format!("{prefix}\x1b[0m")
    } else {
        format!("{prefix}\x1b[0m{ellipsis}\x1b[0m")
    };
    if pad {
        result.push_str(&" ".repeat(max_width.saturating_sub(visible)));
    }
    result
}

/// Truncate text to `max_width` with the default `"..."` ellipsis and no padding.
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    truncate_to_width_with(text, max_width, "...", false)
}

/// Truncate text to fit `max_width`, optionally padding to exactly `max_width`.
pub fn truncate_to_width_with(text: &str, max_width: usize, ellipsis: &str, pad: bool) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.is_empty() {
        return if pad { " ".repeat(max_width) } else { String::new() };
    }

    let ellipsis_width = visible_width(ellipsis);
    if ellipsis_width >= max_width {
        let text_width = visible_width(text);
        if text_width <= max_width {
            return if pad {
                format!("{text}{}", " ".repeat(max_width - text_width))
            } else {
                text.to_string()
            };
        }

        let (clipped_ellipsis, clipped_width) = truncate_fragment_to_width(ellipsis, max_width);
        if clipped_width == 0 {
            return if pad { " ".repeat(max_width) } else { String::new() };
        }
        return finalize_truncated_result("", 0, &clipped_ellipsis, clipped_width, max_width, pad);
    }

    if is_printable_ascii(text) {
        if text.len() <= max_width {
            return if pad {
                format!("{text}{}", " ".repeat(max_width - text.len()))
            } else {
                text.to_string()
            };
        }
        let target_width = max_width - ellipsis_width;
        return finalize_truncated_result(
            &text[..target_width],
            target_width,
            ellipsis,
            ellipsis_width,
            max_width,
            pad,
        );
    }

    let target_width = max_width - ellipsis_width;
    let mut result = String::new();
    let mut pending_ansi = String::new();
    let mut visible_so_far = 0;
    let mut kept_width = 0;
    let mut keep_contiguous_prefix = true;
    let mut overflowed = false;
    let has_ansi = text.contains('\x1b');
    let has_tabs = text.contains('\t');

    let exhausted_input = if !has_ansi && !has_tabs {
        for segment in text.graphemes(true) {
            let width = grapheme_width(segment);
            if keep_contiguous_prefix && kept_width + width <= target_width {
                result.push_str(segment);
                kept_width += width;
            } else {
                keep_contiguous_prefix = false;
            }
            visible_so_far += width;
            if visible_so_far > max_width {
                overflowed = true;
                break;
            }
        }
        !overflowed
    } else {
        let mut index = 0;
        while index < text.len() {
            if let Some(ansi) = extract_ansi_code(text, index) {
                pending_ansi.push_str(ansi.code);
                index += ansi.length;
                continue;
            }

            if text.as_bytes()[index] == b'\t' {
                if keep_contiguous_prefix && kept_width + 3 <= target_width {
                    result.push_str(&pending_ansi);
                    pending_ansi.clear();
                    result.push('\t');
                    kept_width += 3;
                } else {
                    keep_contiguous_prefix = false;
                    pending_ansi.clear();
                }
                visible_so_far += 3;
                if visible_so_far > max_width {
                    overflowed = true;
                    break;
                }
                index += 1;
                continue;
            }

            let mut end = index;
            while end < text.len() && text.as_bytes()[end] != b'\t' {
                if extract_ansi_code(text, end).is_some() {
                    break;
                }
                end += next_char_len(text, end);
            }

            for segment in text[index..end].graphemes(true) {
                let width = grapheme_width(segment);
                if keep_contiguous_prefix && kept_width + width <= target_width {
                    result.push_str(&pending_ansi);
                    pending_ansi.clear();
                    result.push_str(segment);
                    kept_width += width;
                } else {
                    keep_contiguous_prefix = false;
                    pending_ansi.clear();
                }

                visible_so_far += width;
                if visible_so_far > max_width {
                    overflowed = true;
                    break;
                }
            }
            if overflowed {
                break;
            }
            index = end;
        }
        index >= text.len()
    };

    if !overflowed && exhausted_input {
        return if pad {
            format!("{text}{}", " ".repeat(max_width.saturating_sub(visible_so_far)))
        } else {
            text.to_string()
        };
    }

    finalize_truncated_result(&result, kept_width, ellipsis, ellipsis_width, max_width, pad)
}

/// Extract a range of visible columns from a line. Handles ANSI codes and wide chars.
pub fn slice_by_column(line: &str, start_col: usize, length: usize, strict: bool) -> String {
    slice_with_width(line, start_col, length, strict).text
}

/// Like `slice_by_column` but also returns the actual visible width of the result.
pub fn slice_with_width(line: &str, start_col: usize, length: usize, strict: bool) -> SliceWithWidth {
    slice_text_with_width(line, start_col, length, strict)
}

fn slice_text_with_width(line: &str, start_col: usize, length: usize, strict: bool) -> SliceWithWidth {
    if length == 0 {
        return SliceWithWidth {
            text: String::new(),
            width: 0,
        };
    }
    let end_col = start_col + length;
    let mut result = String::new();
    let mut result_width = 0;
    let mut current_col = 0;
    let mut index = 0;
    let mut pending_ansi = String::new();

    while index < line.len() {
        if let Some(ansi) = extract_ansi_code(line, index) {
            if current_col >= start_col && current_col < end_col {
                result.push_str(ansi.code);
            } else if current_col < start_col {
                pending_ansi.push_str(ansi.code);
            }
            index += ansi.length;
            continue;
        }

        let mut text_end = index;
        while text_end < line.len() && extract_ansi_code(line, text_end).is_none() {
            text_end += next_char_len(line, text_end);
        }

        for segment in line[index..text_end].graphemes(true) {
            let width = grapheme_width(segment);
            let in_range = current_col >= start_col && current_col < end_col;
            let fits = !strict || current_col + width <= end_col;
            if in_range && fits {
                result.push_str(&pending_ansi);
                pending_ansi.clear();
                result.push_str(segment);
                result_width += width;
            }
            current_col += width;
            if current_col >= end_col {
                break;
            }
        }
        index = text_end;
        if current_col >= end_col {
            break;
        }
    }

    SliceWithWidth {
        text: result,
        width: result_width,
    }
}

/// Extract "before" and "after" segments from a line in a single pass.
pub fn extract_segments(
    line: &str,
    before_end: usize,
    after_start: usize,
    after_len: usize,
    strict_after: bool,
) -> Segments {
    let mut before = String::new();
    let mut before_width = 0;
    let mut after = String::new();
    let mut after_width = 0;
    let mut current_col = 0;
    let mut index = 0;
    let mut pending_ansi_before = String::new();
    let mut after_started = false;
    let after_end = after_start + after_len;
    let mut tracker = AnsiCodeTracker::default();
    tracker.clear();

    while index < line.len() {
        if let Some(ansi) = extract_ansi_code(line, index) {
            tracker.process(ansi.code);
            if current_col < before_end {
                pending_ansi_before.push_str(ansi.code);
            } else if current_col >= after_start && current_col < after_end && after_started {
                after.push_str(ansi.code);
            }
            index += ansi.length;
            continue;
        }

        let mut text_end = index;
        while text_end < line.len() && extract_ansi_code(line, text_end).is_none() {
            text_end += next_char_len(line, text_end);
        }

        for segment in line[index..text_end].graphemes(true) {
            let width = grapheme_width(segment);
            if current_col < before_end && current_col + width <= before_end {
                before.push_str(&pending_ansi_before);
                pending_ansi_before.clear();
                before.push_str(segment);
                before_width += width;
            } else if current_col >= after_start && current_col < after_end {
                let fits = !strict_after || current_col + width <= after_end;
                if fits {
                    if !after_started {
                        after.push_str(&tracker.get_active_codes());
                        after_started = true;
                    }
                    after.push_str(segment);
                    after_width += width;
                }
            }

            current_col += width;
            let done = if after_len == 0 {
                current_col >= before_end
            } else {
                current_col >= after_end
            };
            if done {
                break;
            }
        }
        index = text_end;
        let done = if after_len == 0 {
            current_col >= before_end
        } else {
            current_col >= after_end
        };
        if done {
            break;
        }
    }

    Segments {
        before,
        before_width,
        after,
        after_width,
    }
}

/// Apply a background color to a line, padding to `width`.
pub fn apply_background_to_line(line: &str, width: usize, bg_fn: impl Fn(&str) -> String) -> String {
    let visible_len = visible_width(line);
    let padding_needed = width.saturating_sub(visible_len);
    let with_padding = format!("{line}{}", " ".repeat(padding_needed));
    bg_fn(&with_padding)
}
