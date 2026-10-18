//! Ported from `packages/tui/src/alt-screen-search.ts`.
//!
//! Builds a normalized text corpus from rendered transcript lines, maps regex
//! matches back to terminal cell ranges, and provides the search overlay
//! component. Corpus indices are byte offsets; source spans are cell columns, so
//! the resulting segments match the TypeScript UTF-16 scan.

use std::rc::Rc;

use unicode_segmentation::UnicodeSegmentation;

use crate::components::input::{Input, InputOptions};
use crate::tui::{Component, Focusable};
use crate::utils::{strip_terminal_sequences, truncate_to_width_with, visible_width};

/// Terminal cell span of one search match segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AltScreenSearchSegment {
    pub row: i64,
    pub start_col: i64,
    pub end_col: i64,
}

/// One match, split into per-row segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltScreenSearchMatch {
    pub segments: Vec<AltScreenSearchSegment>,
}

#[derive(Debug, Clone, Copy)]
struct SearchSourceSpan {
    row: i64,
    start_col: i64,
    end_col: i64,
}

struct SearchCorpus {
    text: String,
    /// Source span for each byte of `text`; separators are unmapped.
    source: Vec<Option<SearchSourceSpan>>,
}

impl SearchCorpus {
    fn append_mapped(&mut self, text: &str, span: Option<SearchSourceSpan>) {
        self.text.push_str(text);
        self.source.extend(std::iter::repeat_n(span, text.len()));
    }
}

fn is_whitespace_character(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}

fn is_whitespace_segment(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_whitespace_character)
}

fn build_search_corpus(lines: &[String]) -> SearchCorpus {
    let mut corpus = SearchCorpus {
        text: String::new(),
        source: Vec::new(),
    };
    let mut pending_separator = false;

    for (row, line) in lines.iter().enumerate() {
        let line = strip_terminal_sequences(line);
        let mut column = 0_i64;
        for grapheme in line.graphemes(true) {
            let width = visible_width(grapheme) as i64;
            if is_whitespace_segment(grapheme) {
                if !corpus.text.is_empty() {
                    pending_separator = true;
                }
                column += width;
                continue;
            }
            if pending_separator {
                corpus.append_mapped(" ", None);
                pending_separator = false;
            }
            corpus.append_mapped(
                grapheme,
                Some(SearchSourceSpan {
                    row: row as i64,
                    start_col: column,
                    end_col: column + width,
                }),
            );
            column += width;
        }
        if !corpus.text.is_empty() {
            pending_separator = true;
        }
    }

    corpus
}

fn normalize_query(query: &str) -> String {
    let mut result = String::new();
    let mut pending_space = false;
    for character in query.chars() {
        if is_whitespace_character(character) {
            pending_space = !result.is_empty();
            continue;
        }
        if pending_space {
            result.push(' ');
            pending_space = false;
        }
        result.push(character);
    }
    result
}

/// Find all query matches in the normalized rendered transcript.
pub fn find_alt_screen_search_matches(lines: &[String], query: &str) -> Vec<AltScreenSearchMatch> {
    let normalized_query = normalize_query(query);
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let corpus = build_search_corpus(lines);
    let expression = regex::RegexBuilder::new(&regex::escape(&normalized_query))
        .case_insensitive(true)
        .unicode(true)
        .build()
        .expect("escaped literal query must compile");
    let mut matches = Vec::new();

    for found in expression.find_iter(&corpus.text) {
        let mut segments: Vec<AltScreenSearchSegment> = Vec::new();
        for index in found.start()..found.end() {
            let Some(span) = corpus.source[index] else {
                continue;
            };
            match segments.last_mut() {
                Some(previous) if previous.row == span.row && span.start_col <= previous.end_col => {
                    previous.end_col = previous.end_col.max(span.end_col);
                }
                _ => segments.push(AltScreenSearchSegment {
                    row: span.row,
                    start_col: span.start_col,
                    end_col: span.end_col,
                }),
            }
        }
        if !segments.is_empty() {
            matches.push(AltScreenSearchMatch { segments });
        }
    }

    matches
}

/// Stable key used to retain the selected match across transcript updates.
pub fn get_alt_screen_search_match_key(search_match: &AltScreenSearchMatch) -> String {
    let (Some(first), Some(last)) = (search_match.segments.first(), search_match.segments.last()) else {
        return String::new();
    };
    format!("{}:{}:{}:{}", first.row, first.start_col, last.row, last.end_col)
}

/// Search overlay with query input and match counter.
pub struct AltScreenSearchComponent {
    input: Input,
    on_query_change: Rc<dyn Fn(&str)>,
    result_count: usize,
    result_index: i64,
    focused: bool,
}

impl AltScreenSearchComponent {
    pub fn new(on_query_change: Rc<dyn Fn(&str)>) -> Self {
        Self {
            input: Input::new(InputOptions::default()),
            on_query_change,
            result_count: 0,
            result_index: -1,
            focused: false,
        }
    }

    pub fn set_result(&mut self, index: i64, count: usize) {
        self.result_index = index;
        self.result_count = count;
    }

    fn status(&self) -> String {
        if self.input.get_value().is_empty() {
            String::new()
        } else if self.result_count == 0 {
            "No matches ".to_string()
        } else {
            format!("{}/{} ", self.result_index + 1, self.result_count)
        }
    }
}

impl Component for AltScreenSearchComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let safe_width = width.max(1);
        let label = " Find transcript";
        let status = self.status();
        let label_width = visible_width(label);
        let status_width = visible_width(&status);
        let gap_width = (safe_width as i64 - label_width as i64 - status_width as i64).max(1) as usize;
        let title = truncate_to_width_with(
            &format!("{label}{}{status}", " ".repeat(gap_width)),
            safe_width,
            "",
            false,
        );
        let padding = " ".repeat(safe_width.saturating_sub(visible_width(&title)));
        let mut lines = vec![format!("\x1b[7m{title}{padding}\x1b[27m")];
        lines.extend(self.input.render(safe_width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let previous = self.input.get_value().to_string();
        self.input.handle_input(data);
        let query = self.input.get_value();
        if query != previous {
            let query = query.to_string();
            (self.on_query_change)(&query);
        }
    }

    fn invalidate(&mut self) {
        self.input.invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl Focusable for AltScreenSearchComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.input.set_focused(focused);
    }
}
