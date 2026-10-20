//! Ported from `packages/tui/src/components/editor.ts`.
//!
//! The editor owns its buffer, cursor, history, kill ring, undo stack, paste
//! registry, and autocomplete UI. Timer-driven behavior (`setInterval` in the
//! source) is driver-owned: call [`Editor::poll_working`] at the animation
//! deadline. Autocomplete requests are likewise resolved by
//! [`Editor::poll_autocomplete`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::autocomplete::{AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions};
use crate::components::loader::{
    DEFAULT_FRAMES as DEFAULT_PROMPT_FRAMES, DEFAULT_INTERVAL_MS as DEFAULT_PROMPT_INTERVAL_MS, LoaderIndicatorOptions,
};
use crate::components::select_list::{SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme};
use crate::components::text::StyleFn;
use crate::keybindings::{KeybindingsManager, get_keybindings};
use crate::keys::{decode_printable_key, matches_key};
use crate::kill_ring::KillRing;
use crate::tui::{CURSOR_MARKER, Component, FixedCell, Focusable, TuiMainScreen, TuiRenderer};
use crate::undo_stack::UndoStack;
use crate::unicode_tables::is_cjk_break;
use crate::utils::{is_whitespace_char, slice_by_column, truncate_to_width, truncate_to_width_with, visible_width};
use crate::word_navigation::{WordNavigationOptions, WordSegment, find_word_backward, find_word_forward};

static PASTE_MARKER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[paste #(\d+)( (\+\d+ lines|\d+ chars))?\]").expect("paste marker regex"));
static PASTE_MARKER_SINGLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[paste #(\d+)( (\+\d+ lines|\d+ chars))?\]$").expect("single paste marker regex"));

const ATTACHMENT_AUTOCOMPLETE_DEBOUNCE_MS: u64 = 20;
const DEFAULT_AUTOCOMPLETE_TRIGGER_CHARACTERS: [&str; 2] = ["@", "#"];
const HISTORY_SEARCH_MAX_RENDERED_ROWS: usize = 20;

/// One grapheme or word segment with its byte offset in the source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSegment {
    pub text: String,
    pub index: usize,
}

/// One word-wrap chunk with byte offsets into the original line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChunk {
    pub text: String,
    pub start_index: usize,
    pub end_index: usize,
}

/// Editor colors and prompt prefix.
#[derive(Clone)]
pub struct EditorTheme {
    pub border_color: StyleFn,
    pub prompt_prefix: Option<StyleFn>,
    pub select_list: SelectListTheme,
}

impl EditorTheme {
    /// Identity theme used by tests and unstyled callers.
    pub fn plain() -> Self {
        let identity: StyleFn = Rc::new(|text: &str| text.to_string());
        Self {
            border_color: identity,
            prompt_prefix: None,
            select_list: SelectListTheme::plain(),
        }
    }
}

/// Editor construction options.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditorOptions {
    pub padding_x: Option<usize>,
    pub autocomplete_max_visible: Option<usize>,
}

/// Error raised by invalid editor configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorError(String);

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EditorError {}

/// Minimal fixed-cell surface used by the editor prompt.
pub trait EditorFixedCell {
    fn place(&self, row: i64, col: i64);
    fn clear(&self);
    fn update(&self, value: &str) -> bool;
}

impl EditorFixedCell for FixedCell {
    fn place(&self, row: i64, col: i64) {
        FixedCell::place(self, row, col);
    }

    fn clear(&self) {
        FixedCell::clear(self);
    }

    fn update(&self, value: &str) -> bool {
        FixedCell::update(self, value)
    }
}

/// TUI operations the editor needs.
pub trait EditorHost {
    fn terminal_rows(&self) -> usize;
    fn request_render(&self);
    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell>;
}

impl EditorHost for Rc<TuiMainScreen> {
    fn terminal_rows(&self) -> usize {
        crate::tui::TuiExt::terminal(self.as_ref()).borrow().rows()
    }

    fn request_render(&self) {
        crate::tui::TuiExt::request_render(self.as_ref(), false);
    }

    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        Rc::new(TuiMainScreen::create_fixed_cell(self))
    }
}

impl EditorHost for Rc<TuiRenderer> {
    fn terminal_rows(&self) -> usize {
        crate::tui::TuiExt::terminal(self.as_ref()).borrow().rows()
    }

    fn request_render(&self) {
        crate::tui::TuiExt::request_render(self.as_ref(), false);
    }

    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        Rc::new(TuiRenderer::create_fixed_cell(self))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorState {
    lines: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
}

impl EditorState {
    fn empty() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorSnapshot {
    state: EditorState,
    pastes: BTreeMap<i64, String>,
    paste_counter: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LayoutLine {
    text: String,
    has_cursor: bool,
    cursor_pos: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisualLine {
    logical_line: usize,
    start_col: usize,
    length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LastAction {
    Kill,
    Yank,
    TypeWord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JumpMode {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutocompleteState {
    Regular,
    Force,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorPlacement {
    Start,
    End,
}

struct PendingAutocomplete {
    request_id: u64,
    controller: tokio_util::sync::CancellationToken,
    snapshot_text: String,
    snapshot_line: usize,
    snapshot_col: usize,
    options: AutocompleteOptions,
    explicit_tab: bool,
    future: Pin<Box<dyn Future<Output = Option<AutocompleteSuggestions>>>>,
}

struct DeferredAutocomplete {
    start_token: u64,
    deadline: Instant,
    options: AutocompleteRequestOptions,
}

/// Autocomplete request options, mapping the source's `{ signal, force }`.
#[derive(Clone, Copy)]
struct AutocompleteRequestOptions {
    force: bool,
    explicit_tab: bool,
}

/// Editor text callback.
pub type EditorCallback = Rc<dyn Fn(&str)>;

/// The prompt editor.
pub struct Editor {
    state: EditorState,
    focused: bool,
    tui: Rc<dyn EditorHost>,
    theme: EditorTheme,
    padding_x: usize,
    prompt_prefix: String,
    prompt_prefix_fn: StyleFn,
    prompt_cell: Rc<dyn EditorFixedCell>,
    working: bool,
    working_frames: Vec<String>,
    working_interval_ms: u64,
    working_frame: usize,
    working_next_frame_at: Option<Instant>,
    prompt_position: Option<(i64, i64)>,
    screen_origin: Option<(i64, i64)>,
    last_width: usize,
    scroll_offset: usize,
    border_color: StyleFn,
    autocomplete_provider: Option<Rc<dyn AutocompleteProvider>>,
    autocomplete_trigger_characters: Vec<String>,
    autocomplete_trigger_pattern: Regex,
    autocomplete_debounce_pattern: Regex,
    autocomplete_list: Option<SelectList>,
    autocomplete_state: Option<AutocompleteState>,
    autocomplete_prefix: String,
    autocomplete_max_visible: usize,
    autocomplete_abort: Option<tokio_util::sync::CancellationToken>,
    autocomplete_start_token: u64,
    autocomplete_request_id: u64,
    autocomplete_pending: Option<PendingAutocomplete>,
    autocomplete_cancelled: Vec<PendingAutocomplete>,
    autocomplete_deferred: Option<DeferredAutocomplete>,
    pastes: BTreeMap<i64, String>,
    paste_counter: i64,
    paste_buffer: String,
    is_in_paste: bool,
    history: Vec<String>,
    history_index: i64,
    history_draft: Option<EditorState>,
    history_search_query: String,
    history_search_index: i64,
    history_search_draft: Option<EditorState>,
    kill_ring: KillRing,
    last_action: Option<LastAction>,
    jump_mode: Option<JumpMode>,
    preferred_visual_col: Option<usize>,
    snapped_from_cursor_col: Option<usize>,
    undo_stack: UndoStack<EditorSnapshot>,
    pub on_submit: Option<EditorCallback>,
    pub on_change: Option<EditorCallback>,
    pub disable_submit: bool,
}

impl Editor {
    pub fn new(tui: Rc<dyn EditorHost>, theme: EditorTheme, options: EditorOptions) -> Self {
        let prompt_prefix_fn = theme
            .prompt_prefix
            .clone()
            .unwrap_or_else(|| Rc::new(|_| String::new()));
        let prompt_prefix = theme
            .prompt_prefix
            .as_ref()
            .map(|prefix| prefix("›"))
            .unwrap_or_default();
        let prompt_cell = tui.create_fixed_cell();
        let padding_x = options.padding_x.filter(|padding| *padding != usize::MAX).unwrap_or(0);
        let autocomplete_max_visible = options
            .autocomplete_max_visible
            .map(|max| max.clamp(3, 20))
            .unwrap_or(5);
        let mut editor = Self {
            state: EditorState::empty(),
            focused: false,
            tui,
            theme,
            padding_x,
            prompt_prefix,
            prompt_prefix_fn,
            prompt_cell,
            working: false,
            working_frames: Vec::new(),
            working_interval_ms: DEFAULT_PROMPT_INTERVAL_MS,
            working_frame: 0,
            working_next_frame_at: None,
            prompt_position: None,
            screen_origin: None,
            last_width: 80,
            scroll_offset: 0,
            border_color: Rc::new(|text: &str| text.to_string()),
            autocomplete_provider: None,
            autocomplete_trigger_characters: DEFAULT_AUTOCOMPLETE_TRIGGER_CHARACTERS
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            autocomplete_trigger_pattern: build_trigger_pattern(
                &DEFAULT_AUTOCOMPLETE_TRIGGER_CHARACTERS
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect::<Vec<_>>(),
            ),
            autocomplete_debounce_pattern: build_debounce_pattern(
                &DEFAULT_AUTOCOMPLETE_TRIGGER_CHARACTERS
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect::<Vec<_>>(),
            ),
            autocomplete_list: None,
            autocomplete_state: None,
            autocomplete_prefix: String::new(),
            autocomplete_max_visible,
            autocomplete_abort: None,
            autocomplete_start_token: 0,
            autocomplete_request_id: 0,
            autocomplete_pending: None,
            autocomplete_cancelled: Vec::new(),
            autocomplete_deferred: None,
            pastes: BTreeMap::new(),
            paste_counter: 0,
            paste_buffer: String::new(),
            is_in_paste: false,
            history: Vec::new(),
            history_index: -1,
            history_draft: None,
            history_search_query: String::new(),
            history_search_index: -1,
            history_search_draft: None,
            kill_ring: KillRing::new(),
            last_action: None,
            jump_mode: None,
            preferred_visual_col: None,
            snapped_from_cursor_col: None,
            undo_stack: UndoStack::new(),
            on_submit: None,
            on_change: None,
            disable_submit: false,
        };
        editor.border_color = editor.theme.border_color.clone();
        editor.set_working_indicator(None).expect("default frames are one cell");
        editor
    }

    pub fn get_padding_x(&self) -> usize {
        self.padding_x
    }

    pub fn get_border_color(&self) -> StyleFn {
        Rc::clone(&self.border_color)
    }

    pub fn set_padding_x(&mut self, padding: usize) {
        if self.padding_x != padding {
            self.padding_x = padding;
            self.tui.request_render();
        }
    }

    pub fn get_autocomplete_max_visible(&self) -> usize {
        self.autocomplete_max_visible
    }

    pub fn set_autocomplete_max_visible(&mut self, max_visible: usize) {
        let next = max_visible.clamp(3, 20);
        if self.autocomplete_max_visible != next {
            self.autocomplete_max_visible = next;
            self.tui.request_render();
        }
    }

    pub fn set_border_color(&mut self, border_color: StyleFn) {
        self.border_color = border_color;
    }

    pub fn set_autocomplete_provider(&mut self, provider: Rc<dyn AutocompleteProvider>) {
        self.cancel_autocomplete();
        self.autocomplete_provider = Some(provider.clone());
        self.set_autocomplete_trigger_characters(provider.trigger_characters());
    }

    pub fn is_showing_autocomplete(&self) -> bool {
        self.autocomplete_state.is_some()
    }

    pub fn set_working(&mut self, working: bool) {
        if self.working == working {
            return;
        }
        self.working = working;
        self.working_frame = 0;
        self.restart_working_animation();
        self.update_prompt_cell();
    }

    pub fn set_working_indicator(&mut self, options: Option<LoaderIndicatorOptions>) -> Result<(), EditorError> {
        let frames = options.as_ref().and_then(|options| options.frames.clone());
        if frames
            .as_ref()
            .is_some_and(|frames| frames.iter().any(|frame| visible_width(frame) != 1))
        {
            return Err(EditorError(
                "Editor working indicator frames must occupy exactly one terminal cell".to_string(),
            ));
        }
        self.working_frames = match frames {
            Some(frames) => frames,
            None => DEFAULT_PROMPT_FRAMES
                .iter()
                .map(|frame| (self.prompt_prefix_fn)(frame))
                .collect(),
        };
        self.working_interval_ms = options
            .as_ref()
            .and_then(|options| options.interval_ms)
            .filter(|interval| *interval > 0)
            .unwrap_or(DEFAULT_PROMPT_INTERVAL_MS);
        self.working_frame = 0;
        self.restart_working_animation();
        self.update_prompt_cell();
        Ok(())
    }

    pub fn get_prompt_position(&self) -> Option<(i64, i64)> {
        self.prompt_position
    }

    pub fn set_screen_origin(&mut self, row: i64, col: i64) {
        self.screen_origin = Some((row, col));
        self.update_prompt_cell_placement();
    }

    pub fn clear_screen_origin(&mut self) {
        self.screen_origin = None;
        self.prompt_cell.clear();
    }

    /// Advance the working animation when `now` reached the next frame deadline.
    pub fn poll_working(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.working_next_frame_at else {
            return false;
        };
        if !self.working || now < deadline {
            return false;
        }
        if self.working_frames.is_empty() {
            self.working_next_frame_at = Some(now + Duration::from_millis(self.working_interval_ms));
            return false;
        }
        self.working_frame = (self.working_frame + 1) % self.working_frames.len();
        self.working_next_frame_at = Some(now + Duration::from_millis(self.working_interval_ms));
        self.update_prompt_cell();
        true
    }

    pub fn working_next_deadline(&self) -> Option<Instant> {
        self.working_next_frame_at
    }

    fn restart_working_animation(&mut self) {
        self.working_next_frame_at = None;
        if !self.working || self.working_frames.len() <= 1 {
            return;
        }
        self.working_next_frame_at = Some(Instant::now() + Duration::from_millis(self.working_interval_ms));
    }

    fn get_working_prompt_frame(&self) -> &str {
        self.working_frames.get(self.working_frame).map_or(" ", String::as_str)
    }

    fn get_prompt_text(&self) -> String {
        if self.working {
            self.get_working_prompt_frame().to_string()
        } else {
            self.prompt_prefix.clone()
        }
    }

    fn render_prompt_for_line(
        &self,
        prompt_prefix_width: usize,
        padding_x: usize,
        visible_line_index: usize,
        row: usize,
    ) -> (String, Option<(i64, i64)>) {
        let has_prompt = prompt_prefix_width > 0 && self.scroll_offset + visible_line_index == 0;
        if !has_prompt {
            return (" ".repeat(prompt_prefix_width), None);
        }
        let prompt_text = self.get_prompt_text();
        let text = prompt_text.clone() + &" ".repeat(prompt_prefix_width.saturating_sub(visible_width(&prompt_text)));
        (text, Some((row as i64, padding_x as i64)))
    }

    fn update_prompt_cell_placement(&self) {
        let (Some(position), Some(origin)) = (self.prompt_position, self.screen_origin) else {
            self.prompt_cell.clear();
            return;
        };
        self.prompt_cell.place(origin.0 + position.0, origin.1 + position.1);
    }

    fn update_prompt_cell(&self) {
        self.update_prompt_cell_placement();
        if self.prompt_position.is_none() || self.screen_origin.is_none() {
            return;
        }
        self.prompt_cell.update(&self.get_prompt_text());
    }

    pub fn add_to_history(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.history.first().is_some_and(|first| first == trimmed) {
            return;
        }
        self.history.insert(0, trimmed.to_string());
        if self.history.len() > 100 {
            self.history.pop();
        }
    }

    pub fn get_history(&self) -> Vec<String> {
        self.history.clone()
    }

    pub fn set_history(&mut self, history: Vec<String>) {
        self.history = history
            .into_iter()
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .take(100)
            .collect();
        self.history_index = -1;
        self.history_draft = None;
    }

    fn is_editor_empty(&self) -> bool {
        self.state.lines.len() == 1 && self.state.lines[0].is_empty()
    }

    fn is_on_first_visual_line(&self) -> bool {
        let visual_lines = self.build_visual_line_map(self.last_width);
        self.find_current_visual_line(&visual_lines) == 0
    }

    fn is_on_last_visual_line(&self) -> bool {
        let visual_lines = self.build_visual_line_map(self.last_width);
        self.find_current_visual_line(&visual_lines) + 1 == visual_lines.len()
    }

    fn navigate_history(&mut self, direction: i64) {
        self.last_action = None;
        self.exit_history_search();
        if self.history.is_empty() {
            return;
        }
        let new_index = self.history_index - direction;
        if new_index < -1 || new_index >= self.history.len() as i64 {
            return;
        }
        if self.history_index == -1 && new_index >= 0 {
            self.push_undo_snapshot();
            self.history_draft = Some(self.state.clone());
        }
        self.history_index = new_index;
        if self.history_index == -1 {
            if let Some(draft) = self.history_draft.take() {
                self.state = draft;
                self.preferred_visual_col = None;
                self.snapped_from_cursor_col = None;
                self.scroll_offset = 0;
                self.emit_change();
            } else {
                self.set_text_internal("", CursorPlacement::End);
            }
        } else {
            let entry = self
                .history
                .get(self.history_index as usize)
                .cloned()
                .unwrap_or_default();
            let placement = if direction == -1 {
                CursorPlacement::Start
            } else {
                CursorPlacement::End
            };
            self.set_text_internal(&entry, placement);
        }
    }

    fn exit_history_browsing(&mut self) {
        self.history_index = -1;
        self.history_draft = None;
    }

    fn start_or_step_history_search(&mut self) {
        self.last_action = None;
        self.cancel_autocomplete();
        self.exit_history_browsing();
        if self.history_search_draft.is_none() {
            self.history_search_draft = Some(self.state.clone());
            self.history_search_query = self.get_text();
            self.history_search_index = -1;
            let query = self.history_search_query.clone();
            self.set_text_internal(&query, CursorPlacement::End);
            self.update_history_search(0);
            self.tui.request_render();
            return;
        }
        self.step_history_search();
    }

    fn get_history_search_matches(&self) -> Vec<usize> {
        if self.history_search_draft.is_none() {
            return Vec::new();
        }
        let query = self.history_search_query.to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.history
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    fn update_history_search(&mut self, start_match_index: usize) {
        if self.history_search_draft.is_none() {
            return;
        }
        let query = self.history_search_query.clone();
        self.set_text_internal(&query, CursorPlacement::End);
        let matches = self.get_history_search_matches();
        self.history_search_index = matches.get(start_match_index).map_or(-1, |index| *index as i64);
        self.tui.request_render();
    }

    fn step_history_search(&mut self) {
        let matches = self.get_history_search_matches();
        if matches.is_empty() {
            self.history_search_index = -1;
            self.tui.request_render();
            return;
        }
        let current = matches
            .iter()
            .position(|index| *index as i64 == self.history_search_index);
        let next = current.map_or(0, |index| (index + 1) % matches.len());
        self.history_search_index = matches[next] as i64;
        self.tui.request_render();
    }

    fn accept_history_search(&mut self) {
        let selected = if self.history_search_index >= 0 {
            self.history.get(self.history_search_index as usize).cloned()
        } else {
            None
        };
        self.history_search_query.clear();
        self.history_search_index = -1;
        self.history_search_draft = None;
        if let Some(selected) = selected {
            self.set_text_internal(&selected, CursorPlacement::End);
        }
        self.tui.request_render();
    }

    fn cancel_history_search(&mut self) {
        let draft = self.history_search_draft.take();
        self.accept_history_search();
        if let Some(draft) = draft {
            self.state = draft;
            self.preferred_visual_col = None;
            self.snapped_from_cursor_col = None;
            self.scroll_offset = 0;
            self.emit_change();
        }
    }

    fn exit_history_search(&mut self) {
        if self.history_search_draft.is_none() {
            return;
        }
        self.history_search_query.clear();
        self.history_search_index = -1;
        self.history_search_draft = None;
    }

    fn handle_history_search_input(&mut self, data: &str, keybindings: &KeybindingsManager) -> bool {
        if keybindings.matches(data, "tui.editor.historySearch") {
            self.start_or_step_history_search();
            return true;
        }
        if self.history_search_draft.is_none() {
            return false;
        }
        if keybindings.matches(data, "tui.select.cancel") {
            self.cancel_history_search();
            return true;
        }
        if keybindings.matches(data, "tui.input.submit") {
            self.accept_history_search();
            return true;
        }
        if keybindings.matches(data, "tui.editor.deleteCharBackward") {
            self.history_search_query.pop();
            self.update_history_search(0);
            return true;
        }
        let printable = decode_printable_key(data).or_else(|| {
            if data.chars().next().is_some_and(|character| character >= ' ') {
                Some(data.to_string())
            } else {
                None
            }
        });
        if let Some(printable) = printable {
            self.history_search_query.push_str(&printable);
            self.update_history_search(0);
            return true;
        }
        self.accept_history_search();
        false
    }

    fn history_search_preview_line(&self, entry: &str) -> String {
        let lines: Vec<&str> = entry.split('\n').map(|line| line.trim_end_matches('\r')).collect();
        let displayable: Vec<&str> = lines
            .into_iter()
            .filter(|line| self.is_history_search_preview_line(line))
            .collect();
        let query = self.history_search_query.to_lowercase();
        let preview = displayable
            .iter()
            .find(|line| line.to_lowercase().contains(&query))
            .copied()
            .or_else(|| displayable.first().copied())
            .unwrap_or("");
        preview.trim_start().to_string()
    }

    fn is_history_search_preview_line(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty() && trimmed != "```"
    }

    fn highlight_history_search_match(&self, text: &str) -> String {
        let query = &self.history_search_query;
        if query.is_empty() {
            return text.to_string();
        }
        let lower = text.to_lowercase();
        let Some(match_index) = lower.find(&query.to_lowercase()) else {
            return text.to_string();
        };
        if !text.is_char_boundary(match_index) {
            return text.to_string();
        }
        let end = match_index + query.len();
        if end > text.len() || !text.is_char_boundary(end) {
            return text.to_string();
        }
        format!(
            "{}\x1b[7m{}\x1b[27m{}",
            &text[..match_index],
            &text[match_index..end],
            &text[end..]
        )
    }

    fn render_history_search_lines(&self, width: usize) -> Vec<String> {
        let matches = self.get_history_search_matches();
        let match_position = matches
            .iter()
            .position(|index| *index as i64 == self.history_search_index)
            .map(|index| index as i64)
            .unwrap_or(-1);
        let counter = format!(
            "{}/{}",
            if match_position < 0 { 0 } else { match_position + 1 },
            matches.len()
        );
        if matches.is_empty() {
            let available = width.saturating_sub(counter.len() + 1);
            let line = format!("{} {}", truncate_to_width_with("> ", available, "...", true), counter);
            return vec![truncate_to_width(&line, width)];
        }
        let start = 0.max(match_position.min(matches.len() as i64 - HISTORY_SEARCH_MAX_RENDERED_ROWS as i64)) as usize;
        let end = (start + HISTORY_SEARCH_MAX_RENDERED_ROWS).min(matches.len());
        let mut lines = Vec::new();
        for visible_index in start..end {
            let index = visible_index;
            let history_index = matches[index];
            let marker = if history_index as i64 == self.history_search_index {
                ">"
            } else {
                " "
            };
            let suffix = if index as i64 == match_position {
                counter.clone()
            } else {
                String::new()
            };
            let available = width.saturating_sub(suffix.len() + usize::from(!suffix.is_empty()));
            let first_line =
                self.history_search_preview_line(self.history.get(history_index).map_or("", String::as_str));
            let preview = format!("{marker} {}", self.highlight_history_search_match(&first_line));
            let preview_cell = truncate_to_width_with(&preview, available, "...", true);
            let line = if suffix.is_empty() {
                truncate_to_width_with(&preview, width, "...", true)
            } else {
                format!("{preview_cell} {suffix}")
            };
            lines.push(truncate_to_width(&line, width));
        }
        lines
    }

    fn set_text_internal(&mut self, text: &str, placement: CursorPlacement) {
        self.state.lines = text.split('\n').map(str::to_string).collect();
        if self.state.lines.is_empty() {
            self.state.lines.push(String::new());
        }
        self.state.cursor_line = if placement == CursorPlacement::Start {
            0
        } else {
            self.state.lines.len() - 1
        };
        let col = if placement == CursorPlacement::Start {
            0
        } else {
            self.state.lines[self.state.cursor_line].len()
        };
        self.set_cursor_col(col);
        self.scroll_offset = 0;
        self.emit_change();
    }

    fn emit_change(&self) {
        if let Some(on_change) = &self.on_change {
            on_change(&self.get_text());
        }
    }

    pub fn get_text(&self) -> String {
        self.state.lines.join("\n")
    }

    pub fn get_lines(&self) -> Vec<String> {
        self.state.lines.clone()
    }

    pub fn get_cursor(&self) -> (usize, usize) {
        (self.state.cursor_line, self.state.cursor_col)
    }

    pub fn get_expanded_text(&self) -> String {
        self.expand_paste_markers(&self.state.lines.join("\n"))
    }

    fn expand_paste_markers(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (paste_id, content) in &self.pastes {
            let pattern = format!(
                r"\[paste #{}( (\+\d+ lines|\d+ chars))?\]",
                regex::escape(&paste_id.to_string())
            );
            let Ok(regex) = Regex::new(&pattern) else {
                continue;
            };
            let content = content.clone();
            result = regex
                .replace_all(&result, move |_: &regex::Captures<'_>| content.clone())
                .into_owned();
        }
        result
    }

    pub fn set_text(&mut self, text: &str) {
        self.cancel_autocomplete();
        self.last_action = None;
        self.exit_history_browsing();
        self.exit_history_search();
        let normalized = normalize_text(text);
        if self.get_text() != normalized {
            self.push_undo_snapshot();
        }
        self.pastes.clear();
        self.paste_counter = 0;
        self.set_text_internal(&normalized, CursorPlacement::End);
    }

    pub fn insert_text_at_cursor(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.cancel_autocomplete();
        self.push_undo_snapshot();
        self.last_action = None;
        self.exit_history_browsing();
        self.exit_history_search();
        self.insert_text_at_cursor_internal(text);
    }

    fn insert_text_at_cursor_internal(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let normalized = normalize_text(text);
        let inserted_lines: Vec<&str> = normalized.split('\n').collect();
        let current_line = self.state.lines[self.state.cursor_line].clone();
        let before = &current_line[..self.state.cursor_col];
        let after = &current_line[self.state.cursor_col..];
        if inserted_lines.len() == 1 {
            self.state.lines[self.state.cursor_line] = format!("{before}{normalized}{after}");
            let col = self.state.cursor_col + normalized.len();
            self.set_cursor_col(col);
        } else {
            let mut lines: Vec<String> = self.state.lines[..self.state.cursor_line].to_vec();
            lines.push(format!("{before}{}", inserted_lines[0]));
            lines.extend(
                inserted_lines[1..inserted_lines.len() - 1]
                    .iter()
                    .map(|line| (*line).to_string()),
            );
            let last = inserted_lines[inserted_lines.len() - 1];
            lines.push(format!("{last}{after}"));
            lines.extend_from_slice(&self.state.lines[self.state.cursor_line + 1..]);
            self.state.lines = lines;
            self.state.cursor_line += inserted_lines.len() - 1;
            self.set_cursor_col(last.len());
        }
        self.emit_change();
    }

    fn insert_character(&mut self, character: &str, skip_undo_coalescing: bool) {
        self.exit_history_browsing();
        if !skip_undo_coalescing {
            if is_whitespace_char(character) || self.last_action != Some(LastAction::TypeWord) {
                self.push_undo_snapshot();
            }
            self.last_action = Some(LastAction::TypeWord);
        }
        let line = self.state.lines[self.state.cursor_line].clone();
        let before = &line[..self.state.cursor_col];
        let after = &line[self.state.cursor_col..];
        self.state.lines[self.state.cursor_line] = format!("{before}{character}{after}");
        let col = self.state.cursor_col + character.len();
        self.set_cursor_col(col);
        self.emit_change();
        self.update_autocomplete_after_insert(character);
    }

    fn update_autocomplete_after_insert(&mut self, character: &str) {
        if self.autocomplete_state.is_none() {
            if character == "/" && self.is_at_start_of_message() {
                self.try_trigger_autocomplete(false);
            } else if self
                .autocomplete_trigger_characters
                .iter()
                .any(|trigger| trigger == character)
            {
                let current_line = &self.state.lines[self.state.cursor_line];
                let before = &current_line[..self.state.cursor_col];
                let chars: Vec<char> = before.chars().collect();
                let before_symbol = chars.get(chars.len().saturating_sub(2)).copied();
                if chars.len() == 1 || before_symbol == Some(' ') || before_symbol == Some('\t') {
                    self.try_trigger_autocomplete(false);
                }
            } else if is_word_character(character) {
                let current_line = &self.state.lines[self.state.cursor_line];
                let before = &current_line[..self.state.cursor_col];
                if self.is_in_slash_command_context(before) || self.autocomplete_trigger_pattern.is_match(before) {
                    self.try_trigger_autocomplete(false);
                }
            }
        } else {
            self.update_autocomplete();
        }
    }

    fn handle_paste(&mut self, pasted_text: &str) {
        self.cancel_autocomplete();
        self.exit_history_browsing();
        self.last_action = None;
        self.push_undo_snapshot();

        let decoded = decode_csi_u_ctrl_letters(pasted_text);
        let clean_text = normalize_text(&decoded);
        let mut filtered: String = clean_text
            .chars()
            .filter(|character| *character == '\n' || *character as u32 >= 32)
            .collect();

        if filtered.starts_with(['/', '~', '.']) {
            let current_line = &self.state.lines[self.state.cursor_line];
            let before = if self.state.cursor_col > 0 {
                current_line[..self.state.cursor_col].chars().next_back()
            } else {
                None
            };
            if before.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_') {
                filtered.insert(0, ' ');
            }
        }

        let pasted_lines: Vec<&str> = filtered.split('\n').collect();
        let total_chars = filtered.encode_utf16().count();
        if pasted_lines.len() > 10 || total_chars > 1000 {
            self.paste_counter += 1;
            let paste_id = self.paste_counter;
            self.pastes.insert(paste_id, filtered.clone());
            let marker = if pasted_lines.len() > 10 {
                format!("[paste #{paste_id} +{} lines]", pasted_lines.len())
            } else {
                format!("[paste #{paste_id} {total_chars} chars]")
            };
            self.insert_text_at_cursor_internal(&marker);
            return;
        }
        self.insert_text_at_cursor_internal(&filtered);
    }

    fn add_new_line(&mut self) {
        self.cancel_autocomplete();
        self.exit_history_browsing();
        self.last_action = None;
        self.push_undo_snapshot();
        let current_line = self.state.lines[self.state.cursor_line].clone();
        let before = current_line[..self.state.cursor_col].to_string();
        let after = current_line[self.state.cursor_col..].to_string();
        self.state.lines[self.state.cursor_line] = before;
        self.state.lines.insert(self.state.cursor_line + 1, after);
        self.state.cursor_line += 1;
        self.set_cursor_col(0);
        self.emit_change();
    }

    fn should_submit_on_backslash_enter(&self, data: &str, keybindings: &KeybindingsManager) -> bool {
        if self.disable_submit {
            return false;
        }
        if !matches_key(data, "enter") {
            return false;
        }
        let submit_keys = keybindings.get_keys("tui.input.submit");
        let has_shift_enter = submit_keys
            .iter()
            .any(|key| key == "shift+enter" || key == "shift+return");
        if !has_shift_enter {
            return false;
        }
        let current_line = &self.state.lines[self.state.cursor_line];
        self.state.cursor_col > 0 && current_line[..self.state.cursor_col].ends_with('\\')
    }

    fn submit_value(&mut self) {
        self.cancel_autocomplete();
        let result = self
            .expand_paste_markers(&self.state.lines.join("\n"))
            .trim()
            .to_string();
        self.state = EditorState::empty();
        self.pastes.clear();
        self.paste_counter = 0;
        self.exit_history_browsing();
        self.scroll_offset = 0;
        self.undo_stack.clear();
        self.last_action = None;
        if let Some(on_change) = &self.on_change {
            on_change("");
        }
        if let Some(on_submit) = &self.on_submit {
            on_submit(&result);
        }
    }

    fn handle_backspace(&mut self) {
        self.exit_history_browsing();
        self.last_action = None;
        if self.state.cursor_col > 0 {
            self.push_undo_snapshot();
            let mut line = self.state.lines[self.state.cursor_line].clone();
            let before_cursor = &line[..self.state.cursor_col];
            let graphemes = self.segment_graphemes(before_cursor);
            let (grapheme_text, grapheme_length) = graphemes
                .last()
                .map_or((String::new(), 1), |segment| (segment.text.clone(), segment.text.len()));
            if let Some(captures) = PASTE_MARKER_SINGLE.captures(&grapheme_text) {
                let target_id: i64 = captures[1].parse().unwrap_or(0);
                self.pastes.remove(&target_id);
                self.paste_counter -= 1;
                let higher_ids: Vec<i64> = self.pastes.keys().copied().filter(|id| *id > target_id).collect();
                for id in higher_ids {
                    if let Some(value) = self.pastes.remove(&id) {
                        self.pastes.insert(id - 1, value);
                    }
                }
                self.state.lines = self
                    .state
                    .lines
                    .iter()
                    .map(|line| renumber_paste_markers(line, target_id))
                    .collect();
            }
            line = self.state.lines[self.state.cursor_line].clone();
            let before = &line[..self.state.cursor_col - grapheme_length];
            let after = &line[self.state.cursor_col..];
            self.state.lines[self.state.cursor_line] = format!("{before}{after}");
            let col = self.state.cursor_col - grapheme_length;
            self.set_cursor_col(col);
        } else if self.state.cursor_line > 0 {
            self.push_undo_snapshot();
            let current_line = self.state.lines[self.state.cursor_line].clone();
            let previous_line = self.state.lines[self.state.cursor_line - 1].clone();
            self.state.lines[self.state.cursor_line - 1] = format!("{previous_line}{current_line}");
            self.state.lines.remove(self.state.cursor_line);
            self.state.cursor_line -= 1;
            self.set_cursor_col(previous_line.len());
        }
        self.emit_change();
        self.update_autocomplete_after_edit();
    }

    fn set_cursor_col(&mut self, col: usize) {
        self.state.cursor_col = col;
        self.preferred_visual_col = None;
        self.snapped_from_cursor_col = None;
    }

    fn move_to_visual_line(
        &mut self,
        visual_lines: &[VisualLine],
        current_visual_line: usize,
        target_visual_line: usize,
    ) {
        let (Some(current_vl), Some(target_vl)) = (
            visual_lines.get(current_visual_line),
            visual_lines.get(target_visual_line),
        ) else {
            return;
        };
        let current_visual_col = if let Some(snapped) = self.snapped_from_cursor_col {
            let index = self.find_visual_line_at(visual_lines, current_vl.logical_line, snapped);
            snapped - visual_lines[index].start_col
        } else {
            self.state.cursor_col - current_vl.start_col
        };

        let is_last_source_segment = current_visual_line + 1 == visual_lines.len()
            || visual_lines[current_visual_line + 1].logical_line != current_vl.logical_line;
        let source_max_visual_col = if is_last_source_segment {
            current_vl.length
        } else {
            current_vl.length.saturating_sub(1)
        };
        let is_last_target_segment = target_visual_line + 1 == visual_lines.len()
            || visual_lines[target_visual_line + 1].logical_line != target_vl.logical_line;
        let target_max_visual_col = if is_last_target_segment {
            target_vl.length
        } else {
            target_vl.length.saturating_sub(1)
        };

        let move_to_visual_col =
            self.compute_vertical_move_column(current_visual_col, source_max_visual_col, target_max_visual_col);

        self.state.cursor_line = target_vl.logical_line;
        let target_col = target_vl.start_col + move_to_visual_col;
        let logical_line = self.state.lines.get(target_vl.logical_line).map_or("", String::as_str);
        self.state.cursor_col = target_col.min(logical_line.len());

        let logical_line = self.state.lines[target_vl.logical_line].clone();
        let segments = self.segment_graphemes(&logical_line);
        for segment in &segments {
            if segment.index > self.state.cursor_col {
                break;
            }
            if segment.text.len() <= 1 {
                continue;
            }
            if self.state.cursor_col < segment.index + segment.text.len() {
                let is_continuation = segment.index < target_vl.start_col;
                let is_moving_down = target_visual_line > current_visual_line;
                if is_continuation && is_moving_down {
                    let segment_end = segment.index + segment.text.len();
                    let mut next = target_visual_line + 1;
                    while next < visual_lines.len()
                        && visual_lines[next].logical_line == target_vl.logical_line
                        && visual_lines[next].start_col < segment_end
                    {
                        next += 1;
                    }
                    if next < visual_lines.len() {
                        self.move_to_visual_line(visual_lines, current_visual_line, next);
                        return;
                    }
                }
                self.snapped_from_cursor_col = Some(self.state.cursor_col);
                self.state.cursor_col = segment.index;
                return;
            }
        }
        self.snapped_from_cursor_col = None;
    }

    fn compute_vertical_move_column(
        &mut self,
        current_visual_col: usize,
        source_max_visual_col: usize,
        target_max_visual_col: usize,
    ) -> usize {
        let has_preferred = self.preferred_visual_col.is_some();
        let cursor_in_middle = current_visual_col < source_max_visual_col;
        let target_too_short = target_max_visual_col < current_visual_col;

        if !has_preferred || cursor_in_middle {
            if target_too_short {
                self.preferred_visual_col = Some(current_visual_col);
                return target_max_visual_col;
            }
            self.preferred_visual_col = None;
            return current_visual_col;
        }

        let preferred = self.preferred_visual_col.unwrap_or(0);
        let target_cannot_fit_preferred = target_max_visual_col < preferred;
        if target_too_short || target_cannot_fit_preferred {
            return target_max_visual_col;
        }
        self.preferred_visual_col = None;
        preferred
    }

    fn move_to_line_start(&mut self) {
        self.last_action = None;
        self.set_cursor_col(0);
    }

    fn move_to_line_end(&mut self) {
        self.last_action = None;
        let length = self.state.lines[self.state.cursor_line].len();
        self.set_cursor_col(length);
    }

    fn delete_to_start_of_line(&mut self) {
        self.exit_history_browsing();
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col > 0 {
            self.push_undo_snapshot();
            let deleted = current_line[..self.state.cursor_col].to_string();
            self.kill_ring
                .push(&deleted, true, self.last_action == Some(LastAction::Kill));
            self.last_action = Some(LastAction::Kill);
            self.state.lines[self.state.cursor_line] = current_line[self.state.cursor_col..].to_string();
            self.set_cursor_col(0);
        } else if self.state.cursor_line > 0 {
            self.push_undo_snapshot();
            self.kill_ring
                .push("\n", true, self.last_action == Some(LastAction::Kill));
            self.last_action = Some(LastAction::Kill);
            let previous_line = self.state.lines[self.state.cursor_line - 1].clone();
            self.state.lines[self.state.cursor_line - 1] = format!("{previous_line}{current_line}");
            self.state.lines.remove(self.state.cursor_line);
            self.state.cursor_line -= 1;
            self.set_cursor_col(previous_line.len());
        }
        self.emit_change();
    }

    fn delete_to_end_of_line(&mut self) {
        self.exit_history_browsing();
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col < current_line.len() {
            self.push_undo_snapshot();
            let deleted = current_line[self.state.cursor_col..].to_string();
            self.kill_ring
                .push(&deleted, false, self.last_action == Some(LastAction::Kill));
            self.last_action = Some(LastAction::Kill);
            self.state.lines[self.state.cursor_line] = current_line[..self.state.cursor_col].to_string();
        } else if self.state.cursor_line + 1 < self.state.lines.len() {
            self.push_undo_snapshot();
            self.kill_ring
                .push("\n", false, self.last_action == Some(LastAction::Kill));
            self.last_action = Some(LastAction::Kill);
            let next_line = self.state.lines[self.state.cursor_line + 1].clone();
            self.state.lines[self.state.cursor_line] = format!("{current_line}{next_line}");
            self.state.lines.remove(self.state.cursor_line + 1);
        }
        self.emit_change();
    }

    fn delete_word_backwards(&mut self) {
        self.exit_history_browsing();
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col == 0 {
            if self.state.cursor_line > 0 {
                self.push_undo_snapshot();
                self.kill_ring
                    .push("\n", true, self.last_action == Some(LastAction::Kill));
                self.last_action = Some(LastAction::Kill);
                let previous_line = self.state.lines[self.state.cursor_line - 1].clone();
                self.state.lines[self.state.cursor_line - 1] = format!("{previous_line}{current_line}");
                self.state.lines.remove(self.state.cursor_line);
                self.state.cursor_line -= 1;
                self.set_cursor_col(previous_line.len());
            }
        } else {
            self.push_undo_snapshot();
            let was_kill = self.last_action == Some(LastAction::Kill);
            let old_cursor_col = self.state.cursor_col;
            self.move_word_backwards();
            let delete_from = self.state.cursor_col;
            self.set_cursor_col(old_cursor_col);
            let deleted = current_line[delete_from..self.state.cursor_col].to_string();
            self.kill_ring.push(&deleted, true, was_kill);
            self.last_action = Some(LastAction::Kill);
            self.state.lines[self.state.cursor_line] = format!(
                "{}{}",
                &current_line[..delete_from],
                &current_line[self.state.cursor_col..]
            );
            self.set_cursor_col(delete_from);
        }
        self.emit_change();
    }

    fn delete_word_forward(&mut self) {
        self.exit_history_browsing();
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col >= current_line.len() {
            if self.state.cursor_line + 1 < self.state.lines.len() {
                self.push_undo_snapshot();
                self.kill_ring
                    .push("\n", false, self.last_action == Some(LastAction::Kill));
                self.last_action = Some(LastAction::Kill);
                let next_line = self.state.lines[self.state.cursor_line + 1].clone();
                self.state.lines[self.state.cursor_line] = format!("{current_line}{next_line}");
                self.state.lines.remove(self.state.cursor_line + 1);
            }
        } else {
            self.push_undo_snapshot();
            let was_kill = self.last_action == Some(LastAction::Kill);
            let old_cursor_col = self.state.cursor_col;
            self.move_word_forwards();
            let delete_to = self.state.cursor_col;
            self.set_cursor_col(old_cursor_col);
            let deleted = current_line[self.state.cursor_col..delete_to].to_string();
            self.kill_ring.push(&deleted, false, was_kill);
            self.last_action = Some(LastAction::Kill);
            self.state.lines[self.state.cursor_line] = format!(
                "{}{}",
                &current_line[..self.state.cursor_col],
                &current_line[delete_to..]
            );
        }
        self.emit_change();
    }

    fn handle_forward_delete(&mut self) {
        self.exit_history_browsing();
        self.last_action = None;
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col < current_line.len() {
            self.push_undo_snapshot();
            let after_cursor = &current_line[self.state.cursor_col..];
            let graphemes = self.segment_graphemes(after_cursor);
            let grapheme_length = graphemes.first().map_or(1, |segment| segment.text.len());
            let before = &current_line[..self.state.cursor_col];
            let after = &current_line[self.state.cursor_col + grapheme_length..];
            self.state.lines[self.state.cursor_line] = format!("{before}{after}");
        } else if self.state.cursor_line + 1 < self.state.lines.len() {
            self.push_undo_snapshot();
            let next_line = self.state.lines[self.state.cursor_line + 1].clone();
            self.state.lines[self.state.cursor_line] = format!("{current_line}{next_line}");
            self.state.lines.remove(self.state.cursor_line + 1);
        }
        self.emit_change();
        self.update_autocomplete_after_edit();
    }

    fn build_visual_line_map(&self, width: usize) -> Vec<VisualLine> {
        let mut visual_lines = Vec::new();
        for (index, line) in self.state.lines.iter().enumerate() {
            let line_visible_width = visible_width(line);
            if line.is_empty() {
                visual_lines.push(VisualLine {
                    logical_line: index,
                    start_col: 0,
                    length: 0,
                });
            } else if line_visible_width <= width {
                visual_lines.push(VisualLine {
                    logical_line: index,
                    start_col: 0,
                    length: line.len(),
                });
            } else {
                let segments = self.segment_graphemes(line);
                for chunk in word_wrap_line(line, width, Some(&segments)) {
                    visual_lines.push(VisualLine {
                        logical_line: index,
                        start_col: chunk.start_index,
                        length: chunk.end_index - chunk.start_index,
                    });
                }
            }
        }
        visual_lines
    }

    fn find_visual_line_at(&self, visual_lines: &[VisualLine], line: usize, col: usize) -> usize {
        for (index, visual_line) in visual_lines.iter().enumerate() {
            if visual_line.logical_line != line {
                continue;
            }
            let offset = col.saturating_sub(visual_line.start_col);
            let is_last_segment =
                index + 1 == visual_lines.len() || visual_lines[index + 1].logical_line != visual_line.logical_line;
            if col >= visual_line.start_col
                && (offset < visual_line.length || (is_last_segment && offset == visual_line.length))
            {
                return index;
            }
        }
        visual_lines.len().saturating_sub(1)
    }

    fn find_current_visual_line(&self, visual_lines: &[VisualLine]) -> usize {
        self.find_visual_line_at(visual_lines, self.state.cursor_line, self.state.cursor_col)
    }

    fn move_cursor(&mut self, delta_line: isize, delta_col: isize) {
        self.last_action = None;
        let visual_lines = self.build_visual_line_map(self.last_width);
        let current_visual_line = self.find_current_visual_line(&visual_lines);

        if delta_line != 0 {
            let target = current_visual_line as isize + delta_line;
            if target >= 0 && (target as usize) < visual_lines.len() {
                self.move_to_visual_line(&visual_lines, current_visual_line, target as usize);
            }
        }

        if delta_col != 0 {
            let current_line = self.state.lines[self.state.cursor_line].clone();
            if delta_col > 0 {
                if self.state.cursor_col < current_line.len() {
                    let after = &current_line[self.state.cursor_col..];
                    let graphemes = self.segment_graphemes(after);
                    let length = graphemes.first().map_or(1, |segment| segment.text.len());
                    let col = self.state.cursor_col + length;
                    self.set_cursor_col(col);
                } else if self.state.cursor_line + 1 < self.state.lines.len() {
                    self.state.cursor_line += 1;
                    self.set_cursor_col(0);
                } else if let Some(current_vl) = visual_lines.get(current_visual_line) {
                    self.preferred_visual_col = Some(self.state.cursor_col - current_vl.start_col);
                }
            } else if self.state.cursor_col > 0 {
                let before = &current_line[..self.state.cursor_col];
                let graphemes = self.segment_graphemes(before);
                let length = graphemes.last().map_or(1, |segment| segment.text.len());
                let col = self.state.cursor_col - length;
                self.set_cursor_col(col);
            } else if self.state.cursor_line > 0 {
                self.state.cursor_line -= 1;
                let length = self.state.lines[self.state.cursor_line].len();
                self.set_cursor_col(length);
            }
        }

        if self.autocomplete_state.is_some() {
            self.update_autocomplete();
        }
    }

    fn page_scroll(&mut self, direction: isize) {
        self.last_action = None;
        let page_size = 5.max(self.tui.terminal_rows() * 3 / 10);
        let visual_lines = self.build_visual_line_map(self.last_width);
        let current_visual_line = self.find_current_visual_line(&visual_lines);
        let target = (current_visual_line as isize + direction * page_size as isize)
            .clamp(0, visual_lines.len().saturating_sub(1) as isize) as usize;
        self.move_to_visual_line(&visual_lines, current_visual_line, target);
    }

    fn move_word_backwards(&mut self) {
        self.last_action = None;
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col == 0 {
            if self.state.cursor_line > 0 {
                self.state.cursor_line -= 1;
                let length = self.state.lines[self.state.cursor_line].len();
                self.set_cursor_col(length);
            }
            return;
        }
        let cursor = self.state.cursor_col;
        let col = {
            let segment = |text: &str| self.word_segments(text);
            let options = WordNavigationOptions {
                segment: Some(&segment),
                is_atomic_segment: Some(&is_paste_marker),
            };
            find_word_backward(&current_line, cursor, Some(&options))
        };
        self.set_cursor_col(col);
    }

    fn move_word_forwards(&mut self) {
        self.last_action = None;
        let current_line = self.state.lines[self.state.cursor_line].clone();
        if self.state.cursor_col >= current_line.len() {
            if self.state.cursor_line + 1 < self.state.lines.len() {
                self.state.cursor_line += 1;
                self.set_cursor_col(0);
            }
            return;
        }
        let cursor = self.state.cursor_col;
        let col = {
            let segment = |text: &str| self.word_segments(text);
            let options = WordNavigationOptions {
                segment: Some(&segment),
                is_atomic_segment: Some(&is_paste_marker),
            };
            find_word_forward(&current_line, cursor, Some(&options))
        };
        self.set_cursor_col(col);
    }

    fn yank(&mut self) {
        if self.kill_ring.is_empty() {
            return;
        }
        self.push_undo_snapshot();
        let text = self.kill_ring.peek().unwrap_or("").to_string();
        self.insert_yanked_text(&text);
        self.last_action = Some(LastAction::Yank);
    }

    fn yank_pop(&mut self) {
        if self.last_action != Some(LastAction::Yank) || self.kill_ring.len() <= 1 {
            return;
        }
        self.push_undo_snapshot();
        self.delete_yanked_text();
        self.kill_ring.rotate();
        let text = self.kill_ring.peek().unwrap_or("").to_string();
        self.insert_yanked_text(&text);
        self.last_action = Some(LastAction::Yank);
    }

    fn insert_yanked_text(&mut self, text: &str) {
        self.exit_history_browsing();
        let lines: Vec<&str> = text.split('\n').collect();
        if lines.len() == 1 {
            let current_line = self.state.lines[self.state.cursor_line].clone();
            let before = &current_line[..self.state.cursor_col];
            let after = &current_line[self.state.cursor_col..];
            self.state.lines[self.state.cursor_line] = format!("{before}{text}{after}");
            let col = self.state.cursor_col + text.len();
            self.set_cursor_col(col);
        } else {
            let current_line = self.state.lines[self.state.cursor_line].clone();
            let before = &current_line[..self.state.cursor_col];
            let after = &current_line[self.state.cursor_col..];
            self.state.lines[self.state.cursor_line] = format!("{before}{}", lines[0]);
            for (offset, line) in lines[1..lines.len() - 1].iter().enumerate() {
                self.state
                    .lines
                    .insert(self.state.cursor_line + 1 + offset, (*line).to_string());
            }
            let last_line_index = self.state.cursor_line + lines.len() - 1;
            let last = lines[lines.len() - 1];
            self.state.lines.insert(last_line_index, format!("{last}{after}"));
            self.state.cursor_line = last_line_index;
            self.set_cursor_col(last.len());
        }
        self.emit_change();
    }

    fn delete_yanked_text(&mut self) {
        let Some(yanked_text) = self.kill_ring.peek().map(str::to_string) else {
            return;
        };
        let yank_lines: Vec<&str> = yanked_text.split('\n').collect();
        if yank_lines.len() == 1 {
            let current_line = self.state.lines[self.state.cursor_line].clone();
            let delete_length = yanked_text.len();
            let before = &current_line[..self.state.cursor_col - delete_length];
            let after = &current_line[self.state.cursor_col..];
            self.state.lines[self.state.cursor_line] = format!("{before}{after}");
            let col = self.state.cursor_col - delete_length;
            self.set_cursor_col(col);
        } else {
            let start_line = self.state.cursor_line - (yank_lines.len() - 1);
            let start_col = self.state.lines[start_line].len() - yank_lines[0].len();
            let after_cursor = self.state.lines[self.state.cursor_line][self.state.cursor_col..].to_string();
            let before_yank = self.state.lines[start_line][..start_col].to_string();
            self.state.lines.splice(
                start_line..start_line + yank_lines.len(),
                [format!("{before_yank}{after_cursor}")],
            );
            self.state.cursor_line = start_line;
            self.set_cursor_col(start_col);
        }
        self.emit_change();
    }

    fn push_undo_snapshot(&mut self) {
        self.undo_stack.push(&EditorSnapshot {
            state: self.state.clone(),
            pastes: self.pastes.clone(),
            paste_counter: self.paste_counter,
        });
    }

    fn undo(&mut self) {
        self.exit_history_browsing();
        let Some(snapshot) = self.undo_stack.pop() else {
            return;
        };
        self.state = snapshot.state;
        self.pastes = snapshot.pastes;
        self.paste_counter = snapshot.paste_counter;
        self.last_action = None;
        self.preferred_visual_col = None;
        self.emit_change();
    }

    fn jump_to_char(&mut self, character: &str, mode: JumpMode) {
        self.last_action = None;
        let forward = mode == JumpMode::Forward;
        let lines = self.state.lines.clone();
        let mut line_index = self.state.cursor_line as isize;
        let end = if forward { lines.len() as isize } else { -1 };
        let step = if forward { 1 } else { -1 };
        while line_index != end {
            let line = &lines[line_index as usize];
            let is_current = line_index as usize == self.state.cursor_line;
            let index = if forward {
                let mut start = if is_current {
                    (self.state.cursor_col + 1).min(line.len())
                } else {
                    0
                };
                while start < line.len() && !line.is_char_boundary(start) {
                    start += 1;
                }
                line.get(start..)
                    .and_then(|rest| rest.find(character))
                    .map(|found| start + found)
            } else {
                let start = if is_current {
                    self.state.cursor_col.saturating_sub(1)
                } else {
                    line.len()
                };
                if line.get(start..).is_some_and(|rest| rest.starts_with(character)) {
                    Some(start)
                } else {
                    line.get(..start).and_then(|rest| rest.rfind(character))
                }
            };
            if let Some(index) = index {
                self.state.cursor_line = line_index as usize;
                self.set_cursor_col(index);
                return;
            }
            line_index += step;
        }
    }

    fn is_slash_menu_allowed(&self) -> bool {
        self.state.cursor_line == 0
    }

    fn is_at_start_of_message(&self) -> bool {
        if !self.is_slash_menu_allowed() {
            return false;
        }
        let current_line = &self.state.lines[self.state.cursor_line];
        let before = &current_line[..self.state.cursor_col];
        let trimmed = before.trim();
        trimmed.is_empty() || trimmed == "/"
    }

    fn is_in_slash_command_context(&self, text_before_cursor: &str) -> bool {
        self.is_slash_menu_allowed() && text_before_cursor.trim_start().starts_with('/')
    }

    fn get_best_autocomplete_match_index(&self, items: &[AutocompleteItem], prefix: &str) -> isize {
        if prefix.is_empty() {
            return -1;
        }
        let mut first_prefix_index = -1;
        for (index, item) in items.iter().enumerate() {
            if item.value == prefix {
                return index as isize;
            }
            if first_prefix_index == -1 && item.value.starts_with(prefix) {
                first_prefix_index = index as isize;
            }
        }
        first_prefix_index
    }

    fn create_autocomplete_list(&self, prefix: &str, items: &[AutocompleteItem]) -> SelectList {
        let layout = if prefix.starts_with('/') {
            SelectListLayoutOptions {
                min_primary_column_width: Some(12),
                max_primary_column_width: Some(32),
                truncate_primary: None,
            }
        } else {
            SelectListLayoutOptions::default()
        };
        let items = items
            .iter()
            .map(|item| {
                let mut select = SelectItem::new(item.value.clone(), item.label.clone());
                if let Some(description) = &item.description {
                    select = select.with_description(description.clone());
                }
                select
            })
            .collect();
        SelectList::new(
            items,
            self.autocomplete_max_visible,
            self.theme.select_list.clone(),
            layout,
        )
    }

    fn try_trigger_autocomplete(&mut self, explicit_tab: bool) {
        self.request_autocomplete(AutocompleteRequestOptions {
            force: false,
            explicit_tab,
        });
    }

    fn handle_tab_completion(&mut self) {
        if self.autocomplete_provider.is_none() {
            return;
        }
        let current_line = &self.state.lines[self.state.cursor_line];
        let before = &current_line[..self.state.cursor_col];
        if self.is_in_slash_command_context(before) && !before.trim_start().contains(' ') {
            self.handle_slash_command_completion();
        } else {
            self.force_file_autocomplete(true);
        }
    }

    fn handle_slash_command_completion(&mut self) {
        self.request_autocomplete(AutocompleteRequestOptions {
            force: false,
            explicit_tab: true,
        });
    }

    fn force_file_autocomplete(&mut self, explicit_tab: bool) {
        self.request_autocomplete(AutocompleteRequestOptions {
            force: true,
            explicit_tab,
        });
    }

    fn request_autocomplete(&mut self, options: AutocompleteRequestOptions) {
        let Some(provider) = self.autocomplete_provider.clone() else {
            return;
        };
        if options.force
            && let Some(should_trigger) = provider.should_trigger_file_completion(
                &self.state.lines,
                self.state.cursor_line,
                self.state.cursor_col,
            )
            && !should_trigger
        {
            return;
        }

        self.cancel_autocomplete_request();
        self.autocomplete_start_token += 1;
        let start_token = self.autocomplete_start_token;
        let debounce_ms = self.get_autocomplete_debounce_ms(options);
        self.autocomplete_deferred = Some(DeferredAutocomplete {
            start_token,
            deadline: Instant::now() + Duration::from_millis(debounce_ms),
            options,
        });
    }

    fn start_autocomplete_request(&mut self, start_token: u64, options: AutocompleteRequestOptions) {
        if start_token != self.autocomplete_start_token {
            return;
        }
        let Some(provider) = self.autocomplete_provider.clone() else {
            return;
        };
        let controller = tokio_util::sync::CancellationToken::new();
        self.autocomplete_abort = Some(controller.clone());
        self.autocomplete_request_id += 1;
        let request_id = self.autocomplete_request_id;
        let snapshot_text = self.get_text();
        let snapshot_line = self.state.cursor_line;
        let snapshot_col = self.state.cursor_col;
        let future = provider.get_suggestions(
            self.state.lines.clone(),
            snapshot_line,
            snapshot_col,
            AutocompleteOptions {
                signal: controller.clone(),
                force: options.force,
            },
        );
        self.autocomplete_pending = Some(PendingAutocomplete {
            request_id,
            controller,
            snapshot_text,
            snapshot_line,
            snapshot_col,
            options: AutocompleteOptions {
                signal: tokio_util::sync::CancellationToken::new(),
                force: options.force,
            },
            explicit_tab: options.explicit_tab,
            future,
        });
    }

    /// Start deferred autocomplete work whose debounce deadline has elapsed,
    /// leaving the request future pending for [`Editor::poll_autocomplete`].
    pub fn begin_autocomplete_request(&mut self) {
        if let Some(deferred) = self.autocomplete_deferred.take() {
            if Instant::now() < deferred.deadline {
                self.autocomplete_deferred = Some(deferred);
                return;
            }
            self.start_autocomplete_request(deferred.start_token, deferred.options);
        }
    }

    /// Resolve pending autocomplete work. Driver-owned counterpart of the
    /// source's debounce timer and async request task.
    pub async fn poll_autocomplete(&mut self) {
        let cancelled = std::mem::take(&mut self.autocomplete_cancelled);
        for pending in cancelled {
            let _ = pending.future.await;
        }
        self.begin_autocomplete_request();
        let Some(pending) = self.autocomplete_pending.take() else {
            return;
        };
        let suggestions = pending.future.await;
        if pending.controller.is_cancelled()
            || pending.request_id != self.autocomplete_request_id
            || self.get_text() != pending.snapshot_text
            || self.state.cursor_line != pending.snapshot_line
            || self.state.cursor_col != pending.snapshot_col
        {
            return;
        }
        self.autocomplete_abort = None;
        let Some(suggestions) = suggestions else {
            self.cancel_autocomplete();
            self.tui.request_render();
            return;
        };
        if suggestions.items.is_empty() {
            self.cancel_autocomplete();
            self.tui.request_render();
            return;
        }
        if pending.options.force && pending.explicit_tab && suggestions.items.len() == 1 {
            let item = suggestions.items[0].clone();
            self.push_undo_snapshot();
            self.last_action = None;
            if let Some(provider) = self.autocomplete_provider.clone() {
                let result = provider.apply_completion(
                    &self.state.lines,
                    self.state.cursor_line,
                    self.state.cursor_col,
                    &item,
                    &suggestions.prefix,
                );
                self.state.lines = result.lines;
                self.state.cursor_line = result.cursor_line;
                self.set_cursor_col(result.cursor_col);
                self.emit_change();
                self.tui.request_render();
            }
            return;
        }
        self.apply_autocomplete_suggestions(&suggestions, pending.options.force);
        self.tui.request_render();
    }

    fn set_autocomplete_trigger_characters(&mut self, trigger_characters: Vec<String>) {
        let mut next: Vec<String> = DEFAULT_AUTOCOMPLETE_TRIGGER_CHARACTERS
            .iter()
            .map(|value| (*value).to_string())
            .collect();
        for character in trigger_characters {
            if character.encode_utf16().count() != 1
                || character == "/"
                || is_whitespace_char(&character)
                || next.contains(&character)
            {
                continue;
            }
            next.push(character);
        }
        self.autocomplete_trigger_characters = next;
        self.autocomplete_trigger_pattern = build_trigger_pattern(&self.autocomplete_trigger_characters);
        self.autocomplete_debounce_pattern = build_debounce_pattern(&self.autocomplete_trigger_characters);
    }

    fn get_autocomplete_debounce_ms(&self, options: AutocompleteRequestOptions) -> u64 {
        if options.explicit_tab || options.force {
            return 0;
        }
        let current_line = &self.state.lines[self.state.cursor_line];
        let before = &current_line[..self.state.cursor_col];
        if self.autocomplete_debounce_pattern.is_match(before) {
            ATTACHMENT_AUTOCOMPLETE_DEBOUNCE_MS
        } else {
            0
        }
    }

    fn apply_autocomplete_suggestions(&mut self, suggestions: &AutocompleteSuggestions, force: bool) {
        self.autocomplete_prefix = suggestions.prefix.clone();
        self.autocomplete_list = Some(self.create_autocomplete_list(&suggestions.prefix, &suggestions.items));
        let best = self.get_best_autocomplete_match_index(&suggestions.items, &suggestions.prefix);
        if best >= 0
            && let Some(list) = self.autocomplete_list.as_mut()
        {
            list.set_selected_index(best);
        }
        self.autocomplete_state = Some(if force {
            AutocompleteState::Force
        } else {
            AutocompleteState::Regular
        });
    }

    fn cancel_autocomplete_request(&mut self) {
        self.autocomplete_start_token += 1;
        self.autocomplete_deferred = None;
        if let Some(abort) = self.autocomplete_abort.take() {
            abort.cancel();
        }
        if let Some(pending) = self.autocomplete_pending.take() {
            self.autocomplete_cancelled.push(pending);
        }
    }

    fn clear_autocomplete_ui(&mut self) {
        self.autocomplete_state = None;
        self.autocomplete_list = None;
        self.autocomplete_prefix.clear();
    }

    fn cancel_autocomplete(&mut self) {
        self.cancel_autocomplete_request();
        self.clear_autocomplete_ui();
    }

    fn update_autocomplete(&mut self) {
        let Some(state) = self.autocomplete_state else {
            return;
        };
        self.request_autocomplete(AutocompleteRequestOptions {
            force: state == AutocompleteState::Force,
            explicit_tab: false,
        });
    }

    fn update_autocomplete_after_edit(&mut self) {
        if self.autocomplete_state.is_some() {
            self.update_autocomplete();
        } else if self.autocomplete_provider.is_some() {
            let current_line = &self.state.lines[self.state.cursor_line];
            let before = &current_line[..self.state.cursor_col];
            if self.is_in_slash_command_context(before) || self.autocomplete_trigger_pattern.is_match(before) {
                self.try_trigger_autocomplete(false);
            }
        }
    }

    fn segment_graphemes(&self, text: &str) -> Vec<TextSegment> {
        let base: Vec<TextSegment> = text
            .grapheme_indices(true)
            .map(|(index, grapheme)| TextSegment {
                text: grapheme.to_string(),
                index,
            })
            .collect();
        merge_paste_markers(text, base, &self.valid_paste_ids())
    }

    fn word_segments(&self, text: &str) -> Vec<WordSegment> {
        let base = crate::word_navigation::word_segments(text);
        merge_word_paste_markers(text, base, &self.valid_paste_ids())
    }

    fn valid_paste_ids(&self) -> BTreeSet<i64> {
        self.pastes.keys().copied().collect()
    }

    fn layout_text(&self, content_width: usize) -> Vec<LayoutLine> {
        let mut layout_lines = Vec::new();
        if self.state.lines.is_empty() || (self.state.lines.len() == 1 && self.state.lines[0].is_empty()) {
            layout_lines.push(LayoutLine {
                text: String::new(),
                has_cursor: true,
                cursor_pos: Some(0),
            });
            return layout_lines;
        }
        for (index, line) in self.state.lines.iter().enumerate() {
            let is_current_line = index == self.state.cursor_line;
            if visible_width(line) <= content_width {
                layout_lines.push(LayoutLine {
                    text: line.clone(),
                    has_cursor: is_current_line,
                    cursor_pos: is_current_line.then_some(self.state.cursor_col),
                });
                continue;
            }
            let segments = self.segment_graphemes(line);
            let chunks = word_wrap_line(line, content_width, Some(&segments));
            for (chunk_index, chunk) in chunks.iter().enumerate() {
                let is_last_chunk = chunk_index + 1 == chunks.len();
                let cursor_pos = self.state.cursor_col;
                let (has_cursor_in_chunk, adjusted_cursor_pos) = if is_current_line {
                    if is_last_chunk {
                        (
                            cursor_pos >= chunk.start_index,
                            cursor_pos.saturating_sub(chunk.start_index),
                        )
                    } else {
                        let has = cursor_pos >= chunk.start_index && cursor_pos < chunk.end_index;
                        let adjusted = if has {
                            (cursor_pos - chunk.start_index).min(chunk.text.len())
                        } else {
                            0
                        };
                        (has, adjusted)
                    }
                } else {
                    (false, 0)
                };
                layout_lines.push(LayoutLine {
                    text: chunk.text.clone(),
                    has_cursor: has_cursor_in_chunk,
                    cursor_pos: has_cursor_in_chunk.then_some(adjusted_cursor_pos),
                });
            }
        }
        layout_lines
    }
}

impl Component for Editor {
    fn render(&mut self, width: usize) -> Vec<String> {
        let max_padding = width.saturating_sub(1) / 2;
        let padding_x = self.padding_x.min(max_padding);
        let content_width = 1.max(width.saturating_sub(padding_x * 2));
        let prompt_prefix_width = (visible_width(&self.prompt_prefix) + usize::from(!self.prompt_prefix.is_empty()))
            .min(content_width.saturating_sub(1));
        let editor_content_width = 1.max(content_width.saturating_sub(prompt_prefix_width));
        let layout_width = 1.max(editor_content_width - usize::from(padding_x == 0));
        self.last_width = layout_width;

        let horizontal = (self.border_color)("─");
        let layout_lines = self.layout_text(layout_width);

        let terminal_rows = self.tui.terminal_rows();
        let max_visible_lines = 5.max(terminal_rows * 3 / 10);
        let cursor_line_index = layout_lines.iter().position(|line| line.has_cursor).unwrap_or(0);
        if cursor_line_index < self.scroll_offset {
            self.scroll_offset = cursor_line_index;
        } else if cursor_line_index >= self.scroll_offset + max_visible_lines {
            self.scroll_offset = cursor_line_index - max_visible_lines + 1;
        }
        let max_scroll_offset = layout_lines.len().saturating_sub(max_visible_lines);
        self.scroll_offset = self.scroll_offset.min(max_scroll_offset);

        let visible_lines: Vec<LayoutLine> = layout_lines
            .iter()
            .skip(self.scroll_offset)
            .take(max_visible_lines)
            .cloned()
            .collect();

        let mut result = Vec::new();
        let mut prompt_position = None;
        let left_padding = " ".repeat(padding_x);
        let right_padding = left_padding.clone();

        if self.history_search_draft.is_some() {
            for line in self.render_history_search_lines(width) {
                result.push((self.border_color)(&line));
            }
        }

        if self.scroll_offset > 0 {
            result.push((self.border_color)(&create_scroll_border(
                "↑",
                self.scroll_offset,
                width,
            )));
        } else {
            result.push(horizontal.repeat(width));
        }

        let emit_cursor_marker = self.focused;
        for (index, layout_line) in visible_lines.iter().enumerate() {
            let mut display_text = layout_line.text.clone();
            let mut line_visible_width = visible_width(&layout_line.text);
            let mut cursor_in_padding = false;
            if layout_line.has_cursor
                && let Some(cursor_pos) = layout_line.cursor_pos
            {
                let before = &display_text[..cursor_pos];
                let after = &display_text[cursor_pos..];
                let marker = if emit_cursor_marker { CURSOR_MARKER } else { "" };
                if !after.is_empty() {
                    let graphemes = self.segment_graphemes(after);
                    let first = graphemes.first().map_or("", |segment| segment.text.as_str());
                    let rest = &after[first.len()..];
                    let cursor = format!("\x1b[7m{first}\x1b[0m");
                    display_text = format!("{before}{marker}{cursor}{rest}");
                } else {
                    let cursor = "\x1b[7m \x1b[0m";
                    display_text = format!("{before}{marker}{cursor}");
                    line_visible_width += 1;
                    if line_visible_width > editor_content_width && padding_x > 0 {
                        cursor_in_padding = true;
                    }
                }
            }
            let padding = " ".repeat(editor_content_width.saturating_sub(line_visible_width));
            let line_right_padding = if cursor_in_padding {
                &right_padding[1..]
            } else {
                &right_padding
            };
            let (prompt_text, position) =
                self.render_prompt_for_line(prompt_prefix_width, padding_x, index, result.len());
            if position.is_some() {
                prompt_position = position;
            }
            result.push(format!(
                "{left_padding}{prompt_text}{display_text}{padding}{line_right_padding}"
            ));
        }

        let lines_below = layout_lines
            .len()
            .saturating_sub(self.scroll_offset + visible_lines.len());
        if lines_below > 0 {
            result.push((self.border_color)(&create_scroll_border("↓", lines_below, width)));
        } else {
            result.push(horizontal.repeat(width));
        }

        if let (Some(_), Some(list)) = (self.autocomplete_state, self.autocomplete_list.as_mut()) {
            for line in list.render(editor_content_width) {
                let line_width = visible_width(&line);
                let prompt_prefix_padding = " ".repeat(prompt_prefix_width);
                let line_padding = " ".repeat(editor_content_width.saturating_sub(line_width));
                result.push(format!(
                    "{left_padding}{prompt_prefix_padding}{line}{line_padding}{right_padding}"
                ));
            }
        }

        self.prompt_position = prompt_position;
        self.update_prompt_cell_placement();
        result
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = get_keybindings();
        if self.handle_history_search_input(data, &keybindings) {
            return;
        }

        if let Some(jump_mode) = self.jump_mode {
            if keybindings.matches(data, "tui.editor.jumpForward")
                || keybindings.matches(data, "tui.editor.jumpBackward")
            {
                self.jump_mode = None;
                return;
            }
            let printable = decode_printable_key(data).or_else(|| {
                if data.chars().next().is_some_and(|character| character >= ' ') {
                    Some(data.to_string())
                } else {
                    None
                }
            });
            if let Some(printable) = printable {
                self.jump_mode = None;
                self.jump_to_char(&printable, jump_mode);
                return;
            }
            self.jump_mode = None;
        }

        let mut data = data.to_string();
        if data.contains("\x1b[200~") {
            self.is_in_paste = true;
            self.paste_buffer.clear();
            data = data.replacen("\x1b[200~", "", 1);
        }
        if self.is_in_paste {
            self.paste_buffer.push_str(&data);
            if let Some(end_index) = self.paste_buffer.find("\x1b[201~") {
                let paste_content = self.paste_buffer[..end_index].to_string();
                if !paste_content.is_empty() {
                    self.handle_paste(&paste_content);
                }
                self.is_in_paste = false;
                let remaining = self.paste_buffer[end_index + 6..].to_string();
                self.paste_buffer.clear();
                if !remaining.is_empty() {
                    self.handle_input(&remaining);
                }
                return;
            }
            return;
        }

        if keybindings.matches(&data, "tui.input.copy") {
            return;
        }
        if keybindings.matches(&data, "tui.editor.undo") {
            self.undo();
            return;
        }

        if self.autocomplete_state.is_some() && self.autocomplete_list.is_some() {
            if keybindings.matches(&data, "tui.select.cancel") {
                self.cancel_autocomplete();
                return;
            }
            if keybindings.matches(&data, "tui.select.up") || keybindings.matches(&data, "tui.select.down") {
                if let Some(list) = self.autocomplete_list.as_mut() {
                    list.handle_input(&data);
                }
                return;
            }
            if keybindings.matches(&data, "tui.input.tab") {
                let selected = self
                    .autocomplete_list
                    .as_ref()
                    .and_then(|list| list.get_selected_item().cloned());
                if let (Some(selected), Some(provider)) = (selected, self.autocomplete_provider.clone()) {
                    let item = AutocompleteItem {
                        value: selected.value.clone(),
                        label: selected.label.clone(),
                        description: selected.description.clone(),
                    };
                    self.push_undo_snapshot();
                    self.last_action = None;
                    let result = provider.apply_completion(
                        &self.state.lines,
                        self.state.cursor_line,
                        self.state.cursor_col,
                        &item,
                        &self.autocomplete_prefix,
                    );
                    self.state.lines = result.lines;
                    self.state.cursor_line = result.cursor_line;
                    self.set_cursor_col(result.cursor_col);
                    self.cancel_autocomplete();
                    self.emit_change();
                }
                return;
            }
            if keybindings.matches(&data, "tui.select.confirm") {
                let selected = self
                    .autocomplete_list
                    .as_ref()
                    .and_then(|list| list.get_selected_item().cloned());
                if let (Some(selected), Some(provider)) = (selected, self.autocomplete_provider.clone()) {
                    let item = AutocompleteItem {
                        value: selected.value.clone(),
                        label: selected.label.clone(),
                        description: selected.description.clone(),
                    };
                    self.push_undo_snapshot();
                    self.last_action = None;
                    let result = provider.apply_completion(
                        &self.state.lines,
                        self.state.cursor_line,
                        self.state.cursor_col,
                        &item,
                        &self.autocomplete_prefix,
                    );
                    self.state.lines = result.lines;
                    self.state.cursor_line = result.cursor_line;
                    self.set_cursor_col(result.cursor_col);
                    if self.autocomplete_prefix.starts_with('/') {
                        self.cancel_autocomplete();
                    } else {
                        self.cancel_autocomplete();
                        self.emit_change();
                        return;
                    }
                }
            }
        }

        if keybindings.matches(&data, "tui.input.tab") && self.autocomplete_state.is_none() {
            self.handle_tab_completion();
            return;
        }

        if keybindings.matches(&data, "tui.editor.deleteToLineEnd") {
            self.delete_to_end_of_line();
            return;
        }
        if keybindings.matches(&data, "tui.editor.deleteToLineStart") {
            self.delete_to_start_of_line();
            return;
        }
        if keybindings.matches(&data, "tui.editor.deleteWordBackward") {
            self.delete_word_backwards();
            return;
        }
        if keybindings.matches(&data, "tui.editor.deleteWordForward") {
            self.delete_word_forward();
            return;
        }
        if keybindings.matches(&data, "tui.editor.deleteCharBackward") || matches_key(&data, "shift+backspace") {
            self.handle_backspace();
            return;
        }
        if keybindings.matches(&data, "tui.editor.deleteCharForward") || matches_key(&data, "shift+delete") {
            self.handle_forward_delete();
            return;
        }
        if keybindings.matches(&data, "tui.editor.yank") {
            self.yank();
            return;
        }
        if keybindings.matches(&data, "tui.editor.yankPop") {
            self.yank_pop();
            return;
        }
        if keybindings.matches(&data, "tui.editor.historyPrevious") {
            self.cancel_autocomplete();
            self.navigate_history(-1);
            return;
        }
        if keybindings.matches(&data, "tui.editor.historyNext") {
            self.cancel_autocomplete();
            self.navigate_history(1);
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorLineStart") {
            self.move_to_line_start();
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorLineEnd") {
            self.move_to_line_end();
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorWordLeft") {
            self.move_word_backwards();
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorWordRight") {
            self.move_word_forwards();
            return;
        }

        if keybindings.matches(&data, "tui.input.newLine")
            || (data.starts_with('\n') && data.len() > 1)
            || data == "\x1b\r"
            || data == "\x1b[13;2~"
            || (data.len() > 1 && data.contains('\x1b') && data.contains('\r'))
            || data == "\n"
        {
            if self.should_submit_on_backslash_enter(&data, &keybindings) {
                self.handle_backspace();
                self.submit_value();
                return;
            }
            self.add_new_line();
            return;
        }

        if keybindings.matches(&data, "tui.input.submit") {
            if self.disable_submit {
                return;
            }
            let current_line = &self.state.lines[self.state.cursor_line];
            if self.state.cursor_col > 0 && current_line[..self.state.cursor_col].ends_with('\\') {
                self.handle_backspace();
                self.add_new_line();
                return;
            }
            self.submit_value();
            return;
        }

        if keybindings.matches(&data, "tui.editor.cursorUp") {
            if self.is_on_first_visual_line()
                && (self.is_editor_empty() || self.history_index > -1 || self.state.cursor_col == 0)
            {
                self.navigate_history(-1);
            } else if self.is_on_first_visual_line() {
                self.move_to_line_start();
            } else {
                self.move_cursor(-1, 0);
            }
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorDown") {
            if self.history_index > -1 && self.is_on_last_visual_line() {
                self.navigate_history(1);
            } else if self.is_on_last_visual_line() {
                self.move_to_line_end();
            } else {
                self.move_cursor(1, 0);
            }
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorRight") {
            self.move_cursor(0, 1);
            return;
        }
        if keybindings.matches(&data, "tui.editor.cursorLeft") {
            self.move_cursor(0, -1);
            return;
        }
        if keybindings.matches(&data, "tui.editor.pageUp") {
            self.page_scroll(-1);
            return;
        }
        if keybindings.matches(&data, "tui.editor.pageDown") {
            self.page_scroll(1);
            return;
        }
        if keybindings.matches(&data, "tui.editor.jumpForward") {
            self.jump_mode = Some(JumpMode::Forward);
            return;
        }
        if keybindings.matches(&data, "tui.editor.jumpBackward") {
            self.jump_mode = Some(JumpMode::Backward);
            return;
        }
        if matches_key(&data, "shift+space") {
            self.insert_character(" ", false);
            return;
        }
        if let Some(printable) = decode_printable_key(&data) {
            self.insert_character(&printable, false);
            return;
        }
        if data.chars().next().is_some_and(|character| character >= ' ') {
            self.insert_character(&data, false);
        }
    }

    fn invalidate(&mut self) {}

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for Editor {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ")
}

fn escape_character_class(value: &str) -> String {
    regex::escape(value)
}

fn build_trigger_pattern(trigger_characters: &[String]) -> Regex {
    let class = trigger_characters
        .iter()
        .map(|character| escape_character_class(character))
        .collect::<String>();
    Regex::new(&format!(r"(?:^|[\s])[{class}][^\s]*$")).expect("autocomplete trigger pattern")
}

fn build_debounce_pattern(trigger_characters: &[String]) -> Regex {
    let class = trigger_characters
        .iter()
        .filter(|character| character.as_str() != "@")
        .map(|character| escape_character_class(character))
        .collect::<String>();
    Regex::new(&format!(r#"(?:^|[ \t])(?:@(?:"[^"]*|[^\s]*)|[{class}][^\s]*)$"#))
        .expect("autocomplete debounce pattern")
}

fn is_word_character(character: &str) -> bool {
    character
        .chars()
        .any(|value| value.is_ascii_alphanumeric() || matches!(value, '.' | '-' | '_'))
}

fn is_paste_marker(segment: &str) -> bool {
    segment.len() >= 10 && PASTE_MARKER_SINGLE.is_match(segment)
}

fn decode_csi_u_ctrl_letters(text: &str) -> String {
    static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[(\d+);5u").expect("CSI-u ctrl-letter regex"));
    REGEX
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let code: u32 = captures[1].parse().unwrap_or(0);
            if (97..=122).contains(&code) {
                return char::from_u32(code - 96).unwrap_or('\0').to_string();
            }
            if (65..=90).contains(&code) {
                return char::from_u32(code - 64).unwrap_or('\0').to_string();
            }
            captures[0].to_string()
        })
        .into_owned()
}

fn renumber_paste_markers(line: &str, target_id: i64) -> String {
    PASTE_MARKER_REGEX
        .replace_all(line, |captures: &regex::Captures<'_>| {
            let id: i64 = captures[1].parse().unwrap_or(0);
            if id <= target_id {
                return captures[0].to_string();
            }
            let suffix = captures.get(2).map_or("", |value| value.as_str());
            if suffix.is_empty() {
                format!("[paste #{}]", id - 1)
            } else {
                format!("[paste #{} {}]", id - 1, suffix.trim_start())
            }
        })
        .into_owned()
}

fn create_scroll_border(direction: &str, hidden_line_count: usize, width: usize) -> String {
    let available_width = width;
    let indicator = format!("─── {direction} {hidden_line_count} more ");
    let remaining = available_width.saturating_sub(visible_width(&indicator));
    if available_width >= visible_width(&indicator) {
        return indicator + &"─".repeat(remaining);
    }
    let ellipsis: String = "...".chars().take(available_width).collect();
    let indicator_width = available_width.saturating_sub(visible_width(&ellipsis));
    slice_by_column(&indicator, 0, indicator_width, true) + &ellipsis
}

fn merge_paste_markers(text: &str, base: Vec<TextSegment>, valid_ids: &BTreeSet<i64>) -> Vec<TextSegment> {
    if valid_ids.is_empty() || !text.contains("[paste #") {
        return base;
    }
    let markers = paste_marker_spans(text, valid_ids);
    if markers.is_empty() {
        return base;
    }
    let mut result = Vec::new();
    let mut marker_index = 0;
    for segment in base {
        while marker_index < markers.len() && markers[marker_index].1 <= segment.index {
            marker_index += 1;
        }
        let marker = markers.get(marker_index).copied();
        if let Some((start, end)) = marker
            && segment.index >= start
            && segment.index < end
        {
            if segment.index == start {
                result.push(TextSegment {
                    text: text[start..end].to_string(),
                    index: start,
                });
            }
            continue;
        }
        result.push(segment);
    }
    result
}

fn merge_word_paste_markers(text: &str, base: Vec<WordSegment>, valid_ids: &BTreeSet<i64>) -> Vec<WordSegment> {
    if valid_ids.is_empty() || !text.contains("[paste #") {
        return base;
    }
    let markers = paste_marker_spans(text, valid_ids);
    if markers.is_empty() {
        return base;
    }
    let mut result = Vec::new();
    let mut marker_index = 0;
    let mut index = 0;
    for segment in base {
        let start = index;
        index += segment.segment.len();
        while marker_index < markers.len() && markers[marker_index].1 <= start {
            marker_index += 1;
        }
        let marker = markers.get(marker_index).copied();
        if let Some((marker_start, marker_end)) = marker
            && start >= marker_start
            && start < marker_end
        {
            if start == marker_start {
                result.push(WordSegment {
                    segment: text[marker_start..marker_end].to_string(),
                    is_word_like: false,
                });
            }
            continue;
        }
        result.push(segment);
    }
    result
}

fn paste_marker_spans(text: &str, valid_ids: &BTreeSet<i64>) -> Vec<(usize, usize)> {
    PASTE_MARKER_REGEX
        .captures_iter(text)
        .filter_map(|captures| {
            let id: i64 = captures[1].parse().ok()?;
            if !valid_ids.contains(&id) {
                return None;
            }
            let matched = captures.get(0)?;
            Some((matched.start(), matched.end()))
        })
        .collect()
}

/// Split a line into word-wrapped chunks.
pub fn word_wrap_line(line: &str, max_width: usize, pre_segmented: Option<&[TextSegment]>) -> Vec<TextChunk> {
    if line.is_empty() || max_width == 0 {
        return vec![TextChunk {
            text: String::new(),
            start_index: 0,
            end_index: 0,
        }];
    }
    if visible_width(line) <= max_width {
        return vec![TextChunk {
            text: line.to_string(),
            start_index: 0,
            end_index: line.len(),
        }];
    }
    let segments: Vec<TextSegment> = pre_segmented.map_or_else(
        || {
            line.grapheme_indices(true)
                .map(|(index, grapheme)| TextSegment {
                    text: grapheme.to_string(),
                    index,
                })
                .collect()
        },
        <[TextSegment]>::to_vec,
    );
    let mut chunks = Vec::new();
    let mut current_width = 0;
    let mut chunk_start = 0;
    let mut wrap_opportunity: Option<(usize, usize)> = None;

    for index in 0..segments.len() {
        let segment = &segments[index];
        let grapheme = &segment.text;
        let grapheme_width = visible_width(grapheme);
        let char_index = segment.index;
        let is_whitespace = !is_paste_marker(grapheme) && is_whitespace_char(grapheme);

        if current_width + grapheme_width > max_width {
            if let Some((opportunity_index, opportunity_width)) = wrap_opportunity
                && current_width - opportunity_width + grapheme_width <= max_width
            {
                chunks.push(TextChunk {
                    text: line[chunk_start..opportunity_index].to_string(),
                    start_index: chunk_start,
                    end_index: opportunity_index,
                });
                chunk_start = opportunity_index;
                current_width -= opportunity_width;
            } else if chunk_start < char_index {
                chunks.push(TextChunk {
                    text: line[chunk_start..char_index].to_string(),
                    start_index: chunk_start,
                    end_index: char_index,
                });
                chunk_start = char_index;
                current_width = 0;
            }
            wrap_opportunity = None;
        }

        if grapheme_width > max_width {
            let sub_chunks = word_wrap_line(grapheme, max_width, None);
            for sub_chunk in &sub_chunks[..sub_chunks.len() - 1] {
                chunks.push(TextChunk {
                    text: sub_chunk.text.clone(),
                    start_index: char_index + sub_chunk.start_index,
                    end_index: char_index + sub_chunk.end_index,
                });
            }
            let last = sub_chunks.last().expect("word wrap always returns a chunk");
            chunk_start = char_index + last.start_index;
            current_width = visible_width(&last.text);
            wrap_opportunity = None;
            continue;
        }

        current_width += grapheme_width;

        if let Some(next) = segments.get(index + 1) {
            let next_is_whitespace = is_whitespace_char(&next.text);
            if is_whitespace && (is_paste_marker(&next.text) || !next_is_whitespace) {
                wrap_opportunity = Some((next.index, current_width));
            } else if !is_whitespace && !next_is_whitespace {
                let is_cjk = !is_paste_marker(grapheme) && grapheme.chars().next().is_some_and(is_cjk_break);
                let next_is_cjk = !is_paste_marker(&next.text) && next.text.chars().next().is_some_and(is_cjk_break);
                if is_cjk || next_is_cjk {
                    wrap_opportunity = Some((next.index, current_width));
                }
            }
        }
    }

    chunks.push(TextChunk {
        text: line[chunk_start..].to_string(),
        start_index: chunk_start,
        end_index: line.len(),
    });
    chunks
}

impl Default for EditorState {
    fn default() -> Self {
        Self::empty()
    }
}
