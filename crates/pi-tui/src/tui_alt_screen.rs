//! Ported from `packages/tui/src/tui-alt-screen.ts`.
//!
//! Alternate-screen TUI with an application-owned scrollable viewport, mouse
//! selection, transcript search, scrollbar dragging, flash messages, and Kitty
//! image placement caching. Timers are driver-owned: call [`TuiAltScreen::poll`]
//! to advance flash expiry, scrollbar auto-hide, and one selection auto-scroll
//! tick.

use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::alt_screen_search::{
    AltScreenSearchComponent, AltScreenSearchMatch, find_alt_screen_search_matches, get_alt_screen_search_match_key,
};
use crate::components::alt_screen_flash::{AltScreenFlashContainer, DEFAULT_DURATION_MS};
use crate::components::scroll_view::{
    ScrollFollow, ScrollState, ScrollView, ScrollViewOptions, ScrollViewScrollToOptions,
};
use crate::components::text::StyleFn;
use crate::keybindings::get_keybindings;
use crate::keys::is_key_release;
use crate::layout::{
    LayoutBox, LayoutFrame, ScrollbarGeometry, get_scroll_view_box, get_scroll_views_at, get_scrollbar_geometry,
    render_layout_frame,
};
use crate::terminal::Terminal;
use crate::terminal_image::{
    ImageProtocol, TerminalCapabilities, delete_all_kitty_images, delete_all_kitty_placements, delete_kitty_image,
    get_capabilities, get_kitty_image_placement, is_image_line, set_capabilities,
};
use crate::tui::{
    CURSOR_MARKER, Component, ComponentRef, OverlayAnchor, OverlayHandle, OverlayMargin, OverlayOptions, RendererApi,
    SizeValue, TuiBase, TuiExt, TuiInputListenerResult, TuiMode, TuiStopOptions, ViewportTui, component,
};
use crate::utils::{
    extract_ansi_code, get_grapheme_cell_range, get_osc8_link_at_column, slice_by_column, strip_terminal_sequences,
    visible_width,
};
use crate::word_navigation::word_segments;

const ENTER_ALT_SCREEN: &str = "\x1b[?1049h";
const EXIT_ALT_SCREEN: &str = "\x1b[?1049l";
const DISABLE_AUTOWRAP: &str = "\x1b[?7l";
const ENABLE_AUTOWRAP: &str = "\x1b[?7h";
const ENABLE_BUTTON_MOTION_MOUSE: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1004h\x1b[?1006h";
const ENABLE_ALL_MOTION_MOUSE: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1004h\x1b[?1006h";
const DISABLE_MOUSE: &str = "\x1b[?1006l\x1b[?1004l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
const FOCUS_IN: &str = "\x1b[I";
const FOCUS_OUT: &str = "\x1b[O";
const BEGIN_SYNCHRONIZED_OUTPUT: &str = "\x1b[?2026h";
const END_SYNCHRONIZED_OUTPUT: &str = "\x1b[?2026l";
const PAGE_SCROLL_OVERLAP: i64 = 4;
const MAX_CACHED_OFFSCREEN_KITTY_IMAGES: usize = 16;
const MAX_CACHED_OFFSCREEN_KITTY_TRANSMISSION_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CACHED_OFFSCREEN_KITTY_DECODED_BYTES: u64 = 64 * 1024 * 1024;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);

/// Environment values consulted when entering the alternate screen.
///
/// The TypeScript source reads `process.env` directly. Rust exposes the same
/// values through [`TuiAltScreen::set_environment`] so tests do not mutate
/// process environment.
#[derive(Debug, Clone, Default)]
pub struct AltScreenEnvironment {
    pub term: Option<String>,
    pub tmux: bool,
    pub zellij: bool,
    pub sty: bool,
    pub platform: String,
}

impl AltScreenEnvironment {
    pub fn detect() -> Self {
        let platform = match std::env::consts::OS {
            "windows" => "win32".to_string(),
            "macos" => "darwin".to_string(),
            other => other.to_string(),
        };
        Self {
            term: std::env::var("TERM").ok(),
            tmux: std::env::var("TMUX").is_ok(),
            zellij: std::env::var("ZELLIJ").is_ok(),
            sty: std::env::var("STY").is_ok(),
            platform,
        }
    }

    fn uses_multiplexer(&self) -> bool {
        let term = self.term.as_deref().unwrap_or("").to_lowercase();
        self.tmux || self.zellij || self.sty || term.starts_with("tmux") || term.starts_with("screen")
    }
}

/// Open an OSC 8 hyperlink activated by a primary-button click.
pub type OpenUrlFn = Rc<dyn Fn(&str)>;

/// Options for [`TuiAltScreen`].
#[derive(Clone, Default)]
pub struct TuiAltScreenOptions {
    /// Number of logical lines moved for each mouse-wheel event.
    pub wheel_scroll_lines: Option<i64>,
    /// Capture mouse events for viewport scrolling and text selection.
    pub mouse: Option<bool>,
    /// Style a non-current transcript search match.
    pub search_match_style: Option<StyleFn>,
    /// Style the current transcript search match.
    pub search_current_match_style: Option<StyleFn>,
    /// Open an OSC 8 hyperlink activated with a primary-button click.
    pub open_url: Option<OpenUrlFn>,
    /// Handle an unmodified secondary-button press for clipboard paste.
    pub on_right_click_paste: Option<Rc<dyn Fn()>>,
}

#[derive(Clone)]
struct SelectionPoint {
    row: i64,
    col: i64,
    scroll_view: Option<Rc<RefCell<ScrollState>>>,
    boundary: bool,
}

impl SelectionPoint {
    fn new(row: i64, col: i64, scroll_view: Option<Rc<RefCell<ScrollState>>>) -> Self {
        Self {
            row,
            col,
            scroll_view,
            boundary: false,
        }
    }
}

#[derive(Clone)]
struct SelectionRange {
    start: SelectionPoint,
    end: SelectionPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionGranularity {
    Character,
    Word,
    Line,
}

#[derive(Clone)]
struct ClickTarget {
    timestamp: Instant,
    count: u32,
    row: i64,
    scroll_view: Option<Rc<RefCell<ScrollState>>>,
    word_start: i64,
    word_end: i64,
}

#[derive(Debug, Clone, Copy)]
struct SgrMouseEvent {
    button: i64,
    x: i64,
    y: i64,
    release: bool,
}

#[derive(Debug, Clone, Copy)]
struct WheelEvent {
    direction: i64,
    x: i64,
    y: i64,
}

#[derive(Clone)]
struct ScrollbarDrag {
    scroll_view: Rc<RefCell<ScrollState>>,
    grab_offset: i64,
}

struct ScrollbarTarget {
    scroll_view: Rc<RefCell<ScrollState>>,
    geometry: ScrollbarGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchSelectionMode {
    Query,
    Retain,
    Next,
    Previous,
}

struct ActiveSearch {
    typed: Rc<RefCell<AltScreenSearchComponent>>,
    overlay: OverlayHandle,
    query: String,
    matches: Vec<AltScreenSearchMatch>,
    selected_index: i64,
    selected_key: Option<String>,
    anchor_row: i64,
    selection_mode: SearchSelectionMode,
}

#[derive(Debug, Clone, Copy)]
struct SearchHighlightRange {
    start_col: i64,
    end_col: i64,
    current: bool,
}

#[derive(Debug, Clone, Copy)]
struct CachedKittyImage {
    transmission_generation: u64,
    transmission_bytes: usize,
    estimated_decoded_bytes: u64,
}

struct AltScreenState {
    previous_screen: Vec<String>,
    last_document: Vec<String>,
    previous_screen_width: usize,
    previous_screen_height: usize,
    layout_root: Option<ComponentRef>,
    current_layout: Option<LayoutFrame>,
    alt_screen_active: bool,
    image_protocol: Option<ImageProtocol>,
    saved_capabilities: Option<TerminalCapabilities>,
    uploaded_kitty_images: Vec<(u32, CachedKittyImage)>,
    selection_anchor: Option<SelectionPoint>,
    selection_focus: Option<SelectionPoint>,
    selection_granularity: SelectionGranularity,
    selection_initial_range: Option<SelectionRange>,
    last_click: Option<ClickTarget>,
    selection_drag_pointer: Option<(i64, i64)>,
    selection_auto_scroll_direction: i64,
    selection_press_active: bool,
    scrollbar_drag: Option<ScrollbarDrag>,
    scrollbar_hover: Option<Rc<RefCell<ScrollState>>>,
    active_search: Option<ActiveSearch>,
    pressed_url: Option<String>,
    selection_dragged: bool,
}

impl Default for AltScreenState {
    fn default() -> Self {
        Self {
            previous_screen: Vec::new(),
            last_document: Vec::new(),
            previous_screen_width: 0,
            previous_screen_height: 0,
            layout_root: None,
            current_layout: None,
            alt_screen_active: false,
            image_protocol: None,
            saved_capabilities: None,
            uploaded_kitty_images: Vec::new(),
            selection_anchor: None,
            selection_focus: None,
            selection_granularity: SelectionGranularity::Character,
            selection_initial_range: None,
            last_click: None,
            selection_drag_pointer: None,
            selection_auto_scroll_direction: 0,
            selection_press_active: false,
            scrollbar_drag: None,
            scrollbar_hover: None,
            active_search: None,
            pressed_url: None,
            selection_dragged: false,
        }
    }
}

/// Implicit scroll-view child that renders the TUI's mounted children.
struct ImplicitDocument {
    base: TuiBase,
}

impl Component for ImplicitDocument {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.base.render_children(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        for child in self.base.children() {
            child.borrow_mut().invalidate();
        }
    }
}

/// Alternate-screen TUI (`TuiAltScreen` in the source).
pub struct TuiAltScreen {
    base: TuiBase,
    self_weak: OnceCell<Weak<TuiAltScreen>>,
    state: RefCell<AltScreenState>,
    implicit_scroll_view: ComponentRef,
    implicit_scroll_state: Rc<RefCell<ScrollState>>,
    flashes: RefCell<AltScreenFlashContainer>,
    wheel_scroll_lines: i64,
    mouse_enabled: bool,
    search_match_style: StyleFn,
    search_current_match_style: StyleFn,
    open_url: Option<OpenUrlFn>,
    on_right_click_paste: Option<Rc<dyn Fn()>>,
    environment: RefCell<AltScreenEnvironment>,
}

impl TuiAltScreen {
    pub fn new(terminal: Rc<RefCell<dyn Terminal>>) -> Rc<Self> {
        Self::with_options(terminal, None, None, TuiAltScreenOptions::default())
    }

    pub fn with_options(
        terminal: Rc<RefCell<dyn Terminal>>,
        show_hardware_cursor: Option<bool>,
        log_directory: Option<std::path::PathBuf>,
        options: TuiAltScreenOptions,
    ) -> Rc<Self> {
        let base = TuiBase::new(terminal, show_hardware_cursor, log_directory);
        let implicit_document: ComponentRef = component(ImplicitDocument { base: base.clone() });
        let scroll_view = ScrollView::new(
            implicit_document,
            ScrollViewOptions {
                follow: Some(ScrollFollow::End),
                primary: true,
                ..ScrollViewOptions::default()
            },
        );
        let implicit_scroll_state = Rc::clone(scroll_view.state());
        let implicit_scroll_view: ComponentRef = component(scroll_view);
        let flash_base = base.clone();
        let flashes = RefCell::new(AltScreenFlashContainer::new(Rc::new(move || {
            flash_base.request_render(false);
        })));
        let screen = Rc::new(Self {
            base,
            self_weak: OnceCell::new(),
            state: RefCell::new(AltScreenState::default()),
            implicit_scroll_view,
            implicit_scroll_state,
            flashes,
            wheel_scroll_lines: options.wheel_scroll_lines.unwrap_or(1).max(1),
            mouse_enabled: options.mouse.unwrap_or(true),
            search_match_style: options
                .search_match_style
                .unwrap_or_else(|| Rc::new(|text: &str| format!("\x1b[4m{text}\x1b[24m"))),
            search_current_match_style: options
                .search_current_match_style
                .unwrap_or_else(|| Rc::new(|text: &str| format!("\x1b[1;7m{text}\x1b[22;27m"))),
            open_url: options.open_url,
            on_right_click_paste: options.on_right_click_paste,
            environment: RefCell::new(AltScreenEnvironment::detect()),
        });
        let _ = screen.self_weak.set(Rc::downgrade(&screen));
        let weak = Rc::downgrade(&screen);
        screen
            .base
            .add_input_listener(Rc::new(move |data| match weak.upgrade() {
                Some(tui) => tui.handle_viewport_input(data),
                None => TuiInputListenerResult::default(),
            }));
        let renderer: Rc<dyn RendererApi> = screen.clone();
        screen.base.install_renderer(Rc::downgrade(&renderer));
        screen
    }

    /// Override the environment values used for multiplexer and platform checks.
    pub fn set_environment(&self, environment: AltScreenEnvironment) {
        *self.environment.borrow_mut() = environment;
    }

    pub fn viewport_top(&self) -> usize {
        self.primary_scroll_view().borrow().scroll_top()
    }

    pub fn is_following_output(&self) -> bool {
        self.primary_scroll_view().borrow().is_following_end()
    }

    pub fn set_layout_root(&self, layout_root: Option<ComponentRef>) {
        let mut state = self.state.borrow_mut();
        if same_optional_component(&state.layout_root, &layout_root) {
            return;
        }
        state.layout_root = layout_root;
        state.current_layout = None;
        drop(state);
        self.base.request_render(false);
    }

    /// Render the layout root when set, otherwise the mounted children.
    pub fn render(&self, width: usize) -> Vec<String> {
        let layout_root = self.state.borrow().layout_root.clone();
        match layout_root {
            Some(root) => root.borrow_mut().render(width),
            None => self.base.render_children(width),
        }
    }

    /// Invalidate the mounted root and every overlay.
    pub fn invalidate(&self) {
        let layout_root = self.state.borrow().layout_root.clone();
        match layout_root {
            Some(root) => root.borrow_mut().invalidate(),
            None => {
                for child in self.base.children() {
                    child.borrow_mut().invalidate();
                }
            }
        }
        self.base.invalidate_overlays();
    }

    pub fn scroll_by(&self, lines: i64) {
        self.primary_scroll_view().borrow_mut().scroll_by(lines);
        self.base.request_render(false);
    }

    pub fn scroll_to_top(&self) {
        self.primary_scroll_view().borrow_mut().scroll_to_start();
        self.base.request_render(false);
    }

    pub fn scroll_to_bottom(&self) {
        self.primary_scroll_view().borrow_mut().scroll_to_end();
        self.base.request_render(false);
    }

    /// Show a transient message in the alternate-screen flash stack.
    pub fn flash(&self, message: impl Into<String>, duration: Option<Duration>) {
        self.flashes.borrow_mut().flash(
            message,
            duration.unwrap_or_else(|| Duration::from_millis(DEFAULT_DURATION_MS)),
        );
    }

    /// Advance driver-owned timers: flash expiry, scrollbar auto-hide, and one
    /// selection auto-scroll tick (one 50 ms interval per call).
    pub fn poll(&self, now: Instant) {
        self.flashes.borrow_mut().poll(now);
        for scroll_view in self.all_scroll_views() {
            scroll_view.borrow_mut().poll(now);
        }
        if self.state.borrow().selection_auto_scroll_direction != 0 {
            self.auto_scroll_selection();
        }
    }

    fn primary_scroll_view(&self) -> Rc<RefCell<ScrollState>> {
        let state = self.state.borrow();
        state
            .current_layout
            .as_ref()
            .and_then(|layout| layout.primary_scroll_view.clone())
            .unwrap_or_else(|| Rc::clone(&self.implicit_scroll_state))
    }

    fn all_scroll_views(&self) -> Vec<Rc<RefCell<ScrollState>>> {
        let state = self.state.borrow();
        let mut result = vec![Rc::clone(&self.implicit_scroll_state)];
        if let Some(layout) = &state.current_layout {
            collect_scroll_views(&layout.root, &mut result);
        }
        result
    }

    fn scroll_to_prompt(&self, direction: i64) {
        let state = self.state.borrow();
        let Some(layout) = &state.current_layout else {
            return;
        };
        let scroll_view = layout
            .primary_scroll_view
            .clone()
            .unwrap_or_else(|| Rc::clone(&self.implicit_scroll_state));
        let Some(box_) = get_scroll_view_box(layout, &scroll_view) else {
            return;
        };
        let Some(lines) = &box_.scroll_content_lines else {
            return;
        };
        let scroll_top = scroll_view.borrow().scroll_top() as i64;
        let mut row = scroll_top + direction;
        while row >= 0 && (row as usize) < lines.len() {
            if is_prompt_start(&lines[row as usize]) {
                scroll_view
                    .borrow_mut()
                    .scroll_to(row, ScrollViewScrollToOptions::default());
                self.base.request_render(false);
                return;
            }
            row += direction;
        }
    }

    fn open_search(&self) {
        {
            let state = self.state.borrow();
            if let Some(search) = &state.active_search {
                search.overlay.focus();
                return;
            }
        }
        let weak = self.self_weak.get().cloned();
        let callback: Rc<dyn Fn(&str)> = Rc::new(move |query: &str| {
            if let Some(tui) = weak.as_ref().and_then(Weak::upgrade) {
                tui.update_search_query(query);
            }
        });
        let typed = Rc::new(RefCell::new(AltScreenSearchComponent::new(callback)));
        let component_ref: ComponentRef = typed.clone();
        let anchor_row = self.primary_scroll_view().borrow().scroll_top() as i64;
        let mut state = self.state.borrow_mut();
        let overlay = self.base.show_overlay(
            component_ref.clone(),
            Some(OverlayOptions {
                anchor: Some(OverlayAnchor::TopRight),
                width: Some(SizeValue::Percent(40.0)),
                min_width: Some(24),
                margin: Some(OverlayMargin {
                    top: Some(1),
                    right: Some(1),
                    bottom: Some(1),
                    left: Some(1),
                }),
                ..OverlayOptions::default()
            }),
        );
        state.active_search = Some(ActiveSearch {
            typed,
            overlay,
            query: String::new(),
            matches: Vec::new(),
            selected_index: -1,
            selected_key: None,
            anchor_row,
            selection_mode: SearchSelectionMode::Query,
        });
    }

    fn close_search(&self) {
        let search = self.state.borrow_mut().active_search.take();
        let Some(search) = search else {
            return;
        };
        search.overlay.hide();
        self.base.request_render(false);
    }

    fn update_search_query(&self, query: &str) {
        let selected_row = {
            let state = self.state.borrow();
            let Some(search) = &state.active_search else {
                return;
            };
            if query == search.query {
                return;
            }
            if search.selected_index >= 0 {
                search
                    .matches
                    .get(search.selected_index as usize)
                    .and_then(|search_match| search_match.segments.first())
                    .map(|segment| segment.row)
            } else {
                None
            }
        };
        let primary_top = self.primary_scroll_view().borrow().scroll_top() as i64;
        let mut state = self.state.borrow_mut();
        let Some(search) = &mut state.active_search else {
            return;
        };
        search.anchor_row = selected_row.unwrap_or(primary_top);
        search.query = query.to_string();
        search.selection_mode = SearchSelectionMode::Query;
        drop(state);
        self.base.request_render(false);
    }

    fn navigate_search(&self, direction: i64) {
        let mut state = self.state.borrow_mut();
        let Some(search) = &mut state.active_search else {
            return;
        };
        if search.query.is_empty() {
            return;
        }
        search.selection_mode = if direction < 0 {
            SearchSelectionMode::Previous
        } else {
            SearchSelectionMode::Next
        };
        drop(state);
        self.base.request_render(false);
    }

    fn refresh_search(&self, layout: &LayoutFrame) -> bool {
        let mut state = self.state.borrow_mut();
        let Some(search) = &mut state.active_search else {
            return false;
        };
        let scroll_view = layout
            .primary_scroll_view
            .clone()
            .unwrap_or_else(|| Rc::clone(&self.implicit_scroll_state));
        let box_ = get_scroll_view_box(layout, &scroll_view);
        let lines = box_.and_then(|box_| box_.scroll_content_lines.as_ref());
        let (Some(lines), false) = (lines, search.query.trim().is_empty()) else {
            search.matches.clear();
            search.selected_index = -1;
            search.selected_key = None;
            search.selection_mode = SearchSelectionMode::Retain;
            search.typed.borrow_mut().set_result(-1, 0);
            return false;
        };

        let should_reveal_selection = search.selection_mode != SearchSelectionMode::Retain;
        let matches = find_alt_screen_search_matches(lines, &search.query);
        let exact_index = search
            .selected_key
            .as_ref()
            .and_then(|key| {
                matches
                    .iter()
                    .position(|search_match| &get_alt_screen_search_match_key(search_match) == key)
            })
            .map(|index| index as i64)
            .unwrap_or(-1);
        let mut selected_index = -1_i64;
        if !matches.is_empty() {
            let count = matches.len() as i64;
            selected_index = match search.selection_mode {
                SearchSelectionMode::Query => {
                    let found = matches
                        .iter()
                        .position(|search_match| {
                            search_match.segments.first().map(|segment| segment.row).unwrap_or(0) >= search.anchor_row
                        })
                        .map(|index| index as i64)
                        .unwrap_or(-1);
                    if found < 0 { 0 } else { found }
                }
                SearchSelectionMode::Next => {
                    let base = if exact_index >= 0 {
                        exact_index
                    } else {
                        search.selected_index.min(count - 1)
                    };
                    if base < 0 { 0 } else { (base + 1) % count }
                }
                SearchSelectionMode::Previous => {
                    let base = if exact_index >= 0 {
                        exact_index
                    } else {
                        search.selected_index.min(count - 1)
                    };
                    if base < 0 {
                        count - 1
                    } else {
                        (base - 1 + count) % count
                    }
                }
                SearchSelectionMode::Retain => {
                    if exact_index >= 0 {
                        exact_index
                    } else {
                        search.selected_index.max(0).min(count - 1)
                    }
                }
            };
        }

        search.matches = matches;
        search.selected_index = selected_index;
        search.selected_key = if selected_index >= 0 {
            search
                .matches
                .get(selected_index as usize)
                .map(get_alt_screen_search_match_key)
        } else {
            None
        };
        search.selection_mode = SearchSelectionMode::Retain;
        search
            .typed
            .borrow_mut()
            .set_result(selected_index, search.matches.len());
        if !should_reveal_selection {
            return false;
        }

        let selected = search.matches.get(selected_index.max(0) as usize);
        let first_segment = selected.and_then(|search_match| search_match.segments.first());
        let last_segment = selected.and_then(|search_match| search_match.segments.last());
        let viewport_height = scroll_view.borrow().viewport_height() as i64;
        let (Some(box_), Some(first_segment), Some(last_segment)) = (box_, first_segment, last_segment) else {
            return false;
        };
        if viewport_height <= 0 {
            return false;
        }
        let before = scroll_view.borrow().scroll_top() as i64;
        let visible_bottom = before + viewport_height - 1;
        let mut target = before;
        if first_segment.row < before || last_segment.row > visible_bottom {
            target = first_segment.row - viewport_height / 3;
        }
        scroll_view
            .borrow_mut()
            .scroll_to(target, ScrollViewScrollToOptions { disable_follow: true });
        let after = scroll_view.borrow().scroll_top() as i64;
        let _ = box_;
        after != before
    }

    fn handle_viewport_input(&self, data: &str) -> TuiInputListenerResult {
        if data == FOCUS_OUT {
            let mut state = self.state.borrow_mut();
            let had_active_selection = state.selection_press_active;
            let had_non_empty_active_selection = had_active_selection && selection_bounds(&state).is_some();
            state.selection_press_active = false;
            stop_selection_auto_scroll(&mut state);
            self.set_scrollbar_hover(&mut state, None);
            state.scrollbar_drag = None;
            state.pressed_url = None;
            state.selection_dragged = false;
            if had_active_selection {
                state.selection_anchor = None;
                state.selection_focus = None;
                state.selection_granularity = SelectionGranularity::Character;
                state.selection_initial_range = None;
                if had_non_empty_active_selection {
                    self.base.request_render(false);
                }
            }
            state.last_click = None;
            return consume_input();
        }
        if data == FOCUS_IN {
            return consume_input();
        }

        if let Some(wheel_event) = parse_wheel_event(data) {
            self.route_wheel(wheel_event);
            return consume_input();
        }
        if let Some(mouse_event) = parse_sgr_mouse_event(data) {
            if self.handle_right_click_paste(mouse_event) {
                return consume_input();
            }
            let handled = self.handle_scrollbar_mouse_event(mouse_event);
            if !self.state.borrow().scrollbar_drag.is_some() {
                self.update_scrollbar_hover(mouse_event.x, mouse_event.y);
            }
            if !handled {
                self.handle_selection_mouse_event(mouse_event);
            }
            return consume_input();
        }
        if is_mouse_sequence(data) {
            return consume_input();
        }

        let keybindings = get_keybindings();
        let is_release = is_key_release(data);
        if keybindings.matches(data, "tui.altScreen.search") {
            if !is_release {
                self.open_search();
            }
            return consume_input();
        }
        let search_focused = self
            .state
            .borrow()
            .active_search
            .as_ref()
            .is_some_and(|search| search.overlay.is_focused());
        if search_focused {
            if keybindings.matches(data, "tui.altScreen.searchNext") {
                if !is_release {
                    self.navigate_search(1);
                }
                return consume_input();
            }
            if keybindings.matches(data, "tui.altScreen.searchPrevious") {
                if !is_release {
                    self.navigate_search(-1);
                }
                return consume_input();
            }
            if keybindings.matches(data, "tui.altScreen.searchClose") {
                if !is_release {
                    self.close_search();
                }
                return consume_input();
            }
        }
        if keybindings.matches(data, "tui.altScreen.pageUp") {
            if !is_release {
                let height = self.primary_scroll_view().borrow().viewport_height() as i64;
                self.scroll_by(-(height - PAGE_SCROLL_OVERLAP).max(1));
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.pageDown") {
            if !is_release {
                let height = self.primary_scroll_view().borrow().viewport_height() as i64;
                self.scroll_by((height - PAGE_SCROLL_OVERLAP).max(1));
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.halfPageUp") {
            if !is_release {
                let height = self.primary_scroll_view().borrow().viewport_height() as i64;
                self.scroll_by(-(height / 2).max(1));
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.halfPageDown") {
            if !is_release {
                let height = self.primary_scroll_view().borrow().viewport_height() as i64;
                self.scroll_by((height / 2).max(1));
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.lineUp") {
            if !is_release {
                self.scroll_by(-1);
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.lineDown") {
            if !is_release {
                self.scroll_by(1);
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.previousPrompt") {
            if !is_release {
                self.scroll_to_prompt(-1);
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.nextPrompt") {
            if !is_release {
                self.scroll_to_prompt(1);
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.top") {
            if !is_release {
                self.scroll_to_top();
            }
            return consume_input();
        }
        if keybindings.matches(data, "tui.altScreen.bottom") {
            if !is_release {
                self.scroll_to_bottom();
            }
            return consume_input();
        }
        TuiInputListenerResult::default()
    }

    fn route_wheel(&self, event: WheelEvent) {
        let mut state = self.state.borrow_mut();
        let mut remaining = event.direction * self.wheel_scroll_lines;
        let views = state
            .current_layout
            .as_ref()
            .map(|layout| get_scroll_views_at(layout, event.x, event.y))
            .unwrap_or_default();
        let mut seen: Vec<Rc<RefCell<ScrollState>>> = Vec::new();
        for scroll_view in views {
            seen.push(Rc::clone(&scroll_view));
            let (leftover, overscroll) = {
                let mut scroll = scroll_view.borrow_mut();
                (scroll.scroll_by(remaining), scroll.overscroll)
            };
            remaining = leftover;
            if remaining == 0 || overscroll == crate::components::scroll_view::ScrollOverscroll::Contain {
                break;
            }
        }
        let primary = state
            .current_layout
            .as_ref()
            .and_then(|layout| layout.primary_scroll_view.clone())
            .unwrap_or_else(|| Rc::clone(&self.implicit_scroll_state));
        let primary_seen = seen.iter().any(|view| Rc::ptr_eq(view, &primary));
        if remaining != 0 && !primary_seen {
            primary.borrow_mut().scroll_by(remaining);
        }
        let target = self.scrollbar_target_at(&state, event.x, event.y);
        self.set_scrollbar_hover(&mut state, target.map(|target| target.scroll_view));
        drop(state);
        self.base.request_render(false);
    }

    fn handle_right_click_paste(&self, event: SgrMouseEvent) -> bool {
        let Some(handler) = &self.on_right_click_paste else {
            return false;
        };
        if self.environment.borrow().platform != "win32" || event.release || event.button != 2 {
            return false;
        }
        let handler = Rc::clone(handler);
        let _ = catch_unwind(AssertUnwindSafe(|| handler()));
        true
    }

    fn scrollbar_target_at(&self, state: &AltScreenState, x: i64, y: i64) -> Option<ScrollbarTarget> {
        if self.base.has_overlay() {
            return None;
        }
        let layout = state.current_layout.as_ref()?;
        for scroll_view in get_scroll_views_at(layout, x, y) {
            let box_ = get_scroll_view_box(layout, &scroll_view)?;
            let Some(geometry) = get_scrollbar_geometry(box_) else {
                continue;
            };
            if x == geometry.column && y >= geometry.thumb_top && y < geometry.thumb_top + geometry.thumb_height {
                return Some(ScrollbarTarget { scroll_view, geometry });
            }
        }
        None
    }

    fn set_scrollbar_hover(&self, state: &mut AltScreenState, scroll_view: Option<Rc<RefCell<ScrollState>>>) {
        let unchanged = match (&state.scrollbar_hover, &scroll_view) {
            (None, None) => true,
            (Some(current), Some(next)) => Rc::ptr_eq(current, next),
            _ => false,
        };
        if unchanged {
            return;
        }
        if let Some(previous) = state.scrollbar_hover.take() {
            previous.borrow_mut().set_scrollbar_active(false);
        }
        state.scrollbar_hover = scroll_view.clone();
        if let Some(next) = scroll_view {
            next.borrow_mut().set_scrollbar_active(true);
        }
    }

    fn update_scrollbar_hover(&self, x: i64, y: i64) {
        let mut state = self.state.borrow_mut();
        let target = self.scrollbar_target_at(&state, x, y);
        self.set_scrollbar_hover(&mut state, target.map(|target| target.scroll_view));
    }

    fn handle_scrollbar_mouse_event(&self, event: SgrMouseEvent) -> bool {
        let mut state = self.state.borrow_mut();
        if let Some(drag) = state.scrollbar_drag.clone() {
            if event.release {
                state.scrollbar_drag = None;
                return true;
            }
            let geometry = state
                .current_layout
                .as_ref()
                .and_then(|layout| get_scroll_view_box(layout, &drag.scroll_view))
                .and_then(get_scrollbar_geometry);
            if let Some(geometry) = geometry {
                let max_thumb_offset = geometry.track_height - geometry.thumb_height;
                let thumb_offset = (event.y - geometry.track_top - drag.grab_offset).clamp(0, max_thumb_offset);
                let scroll_top = if max_thumb_offset == 0 {
                    0
                } else {
                    ((thumb_offset as f64 / max_thumb_offset as f64) * geometry.max_scroll_top as f64).round() as i64
                };
                drag.scroll_view
                    .borrow_mut()
                    .scroll_to(scroll_top, ScrollViewScrollToOptions::default());
            }
            return true;
        }

        if event.release || (event.button & 32) != 0 || (event.button & 3) != 0 {
            return false;
        }
        let Some(target) = self.scrollbar_target_at(&state, event.x, event.y) else {
            return false;
        };
        stop_selection_auto_scroll(&mut state);
        state.selection_press_active = false;
        state.selection_anchor = None;
        state.selection_focus = None;
        state.selection_granularity = SelectionGranularity::Character;
        state.selection_initial_range = None;
        state.last_click = None;
        state.pressed_url = None;
        state.selection_dragged = false;
        self.set_scrollbar_hover(&mut state, Some(Rc::clone(&target.scroll_view)));
        state.scrollbar_drag = Some(ScrollbarDrag {
            scroll_view: target.scroll_view,
            grab_offset: event.y - target.geometry.thumb_top,
        });
        true
    }

    fn handle_selection_mouse_event(&self, event: SgrMouseEvent) {
        if (event.button & 3) != 0 {
            return;
        }
        let rows = self.base.terminal().borrow().rows() as i64;
        let columns = self.base.terminal().borrow().columns() as i64;
        let mut state = self.state.borrow_mut();
        let anchor_scroll_view = state
            .selection_anchor
            .as_ref()
            .and_then(|point| point.scroll_view.clone());
        let point = selection_point(&state, event, anchor_scroll_view, rows, columns);
        if event.release {
            if !state.selection_press_active {
                return;
            }
            state.selection_press_active = false;
            stop_selection_auto_scroll(&mut state);
            if state.selection_anchor.is_none() {
                return;
            }
            update_selection_focus(&mut state, point.clone());
            let clicked_url = if !state.selection_dragged
                && same_optional_scroll_view(
                    state
                        .selection_anchor
                        .as_ref()
                        .and_then(|anchor| anchor.scroll_view.as_ref()),
                    point.scroll_view.as_ref(),
                )
                && state
                    .selection_anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.row == point.row)
                && state
                    .selection_anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.col == point.col)
            {
                state.pressed_url.clone()
            } else {
                None
            };
            state.pressed_url = None;
            if let (Some(url), Some(open_url)) = (clicked_url, &self.open_url) {
                state.selection_anchor = None;
                state.selection_focus = None;
                let open_url = Rc::clone(open_url);
                drop(state);
                let _ = catch_unwind(AssertUnwindSafe(|| open_url(&url)));
                self.base.request_render(false);
                return;
            }
            self.copy_selection_to_clipboard(&mut state);
            drop(state);
            self.base.request_render(false);
            return;
        }
        if (event.button & 32) != 0 {
            if !state.selection_press_active || state.selection_anchor.is_none() {
                return;
            }
            state.selection_dragged = true;
            state.last_click = None;
            state.pressed_url = None;
            update_selection_focus(&mut state, point);
            update_selection_auto_scroll(&mut state, event, rows);
            drop(state);
            self.base.request_render(false);
            return;
        }
        stop_selection_auto_scroll(&mut state);
        state.selection_press_active = true;
        let scroll_view = if !self.base.has_overlay() {
            state
                .current_layout
                .as_ref()
                .and_then(|layout| get_scroll_views_at(layout, event.x, event.y).into_iter().next())
        } else {
            None
        };
        let anchor = selection_point(&state, event, scroll_view, rows, columns);
        let word = word_selection(&state, &anchor);
        let click_count = click_count(&mut state, &anchor, word.as_ref());
        let range = match click_count {
            2 => word,
            3 => Some(line_selection(&state, &anchor)),
            _ => None,
        };
        state.selection_granularity = match (range.is_some(), click_count) {
            (true, 2) => SelectionGranularity::Word,
            (true, _) => SelectionGranularity::Line,
            (false, _) => SelectionGranularity::Character,
        };
        state.selection_initial_range = range.clone();
        state.selection_anchor = Some(
            range
                .as_ref()
                .map_or_else(|| anchor.clone(), |range| range.start.clone()),
        );
        state.selection_focus = Some(range.as_ref().map_or_else(|| anchor.clone(), |range| range.end.clone()));
        state.selection_dragged = false;
        state.pressed_url = if range.is_some() {
            None
        } else {
            let row = event.y.clamp(0, (rows - 1).max(0)) as usize;
            let col = event.x.clamp(0, (columns - 1).max(0)) as usize;
            state
                .previous_screen
                .get(row)
                .and_then(|line| get_osc8_link_at_column(line, col))
                .map(str::to_string)
        };
        drop(state);
        self.base.request_render(false);
    }

    fn copy_selection_to_clipboard(&self, state: &mut AltScreenState) {
        let Some(selection) = selection_bounds(state) else {
            return;
        };
        let source_lines: Vec<String> = match &selection.start.scroll_view {
            Some(scroll_view) => {
                let Some(layout) = state.current_layout.as_ref() else {
                    return;
                };
                let Some(box_) = get_scroll_view_box(layout, scroll_view) else {
                    return;
                };
                let Some(lines) = &box_.scroll_content_lines else {
                    return;
                };
                lines.clone()
            }
            None => state.previous_screen.clone(),
        };
        let mut lines = Vec::new();
        for row in selection.start.row..=selection.end.row {
            let line = source_lines.get(row.max(0) as usize).cloned().unwrap_or_default();
            let columns = get_selection_columns(&line, row, &selection, 0, visible_width(&line) as i64);
            lines.push(
                strip_terminal_sequences(&slice_by_column(
                    &line,
                    columns.0.max(0) as usize,
                    (columns.1 - columns.0).max(0) as usize,
                    true,
                ))
                .trim_end()
                .to_string(),
            );
        }
        let text = lines.join("\n");
        if text.is_empty() {
            return;
        }
        self.base.terminal().borrow_mut().write(&format!(
            "\x1b]52;c;{}\x07",
            pi_js_compat::base64::btoa(text.as_bytes())
        ));
        self.flash("Copied!", None);
    }

    fn apply_search_text_highlight(&self, text: &str, current: bool) -> String {
        let style = if current {
            &self.search_current_match_style
        } else {
            &self.search_match_style
        };
        let mut result = String::new();
        let mut plain_start = 0;
        let mut index = 0;
        while index < text.len() {
            match extract_ansi_code(text, index) {
                Some(ansi) => {
                    if index > plain_start {
                        result.push_str(&style(&text[plain_start..index]));
                    }
                    result.push_str(ansi.code);
                    index += ansi.length;
                    plain_start = index;
                }
                None => index += next_char_len(text, index),
            }
        }
        if plain_start < text.len() {
            result.push_str(&style(&text[plain_start..]));
        }
        result
    }

    fn apply_search_highlights(&self, screen: Vec<String>, layout: &LayoutFrame) -> Vec<String> {
        let state = self.state.borrow();
        let Some(search) = &state.active_search else {
            return screen;
        };
        if search.selected_index < 0 || search.matches.is_empty() {
            return screen;
        }
        let scroll_view = layout
            .primary_scroll_view
            .clone()
            .unwrap_or_else(|| Rc::clone(&self.implicit_scroll_state));
        let Some(box_) = get_scroll_view_box(layout, &scroll_view) else {
            return screen;
        };
        let scrollbar_column = get_scrollbar_geometry(box_).map(|geometry| geometry.column);
        let min_row = box_.rect.y.max(box_.clip.y).max(0);
        let max_row = (screen.len() as i64)
            .min(box_.rect.y + box_.rect.height)
            .min(box_.clip.y + box_.clip.height);
        let min_column = box_.rect.x.max(box_.clip.x).max(0);
        let max_column = (self.base.terminal().borrow().columns() as i64)
            .min(box_.rect.x + box_.rect.width)
            .min(box_.clip.x + box_.clip.width)
            .min(scrollbar_column.unwrap_or(i64::MAX));

        let scroll_top = scroll_view.borrow().scroll_top() as i64;
        let mut ranges_by_row: BTreeMap<i64, Vec<SearchHighlightRange>> = BTreeMap::new();
        for (match_index, search_match) in search.matches.iter().enumerate() {
            for segment in &search_match.segments {
                let row = box_.rect.y + segment.row - scroll_top;
                if row < min_row || row >= max_row {
                    continue;
                }
                let start_col = min_column.max(box_.rect.x + segment.start_col);
                let end_col = max_column.min(box_.rect.x + segment.end_col);
                if end_col <= start_col {
                    continue;
                }
                ranges_by_row.entry(row).or_default().push(SearchHighlightRange {
                    start_col,
                    end_col,
                    current: match_index as i64 == search.selected_index,
                });
            }
        }
        drop(state);

        let mut result = screen;
        for (row, mut ranges) in ranges_by_row {
            let index = row as usize;
            let Some(line) = result.get_mut(index) else {
                continue;
            };
            if is_image_line(line) {
                continue;
            }
            ranges.sort_by_key(|range| std::cmp::Reverse(range.start_col));
            let line_width = visible_width(line) as i64;
            for range in ranges {
                let start_col = range.start_col.min(line_width);
                let end_col = range.end_col.min(line_width);
                if end_col <= start_col {
                    continue;
                }
                let before = slice_by_column(line, 0, start_col as usize, true);
                let highlighted = slice_by_column(line, start_col as usize, (end_col - start_col) as usize, true);
                let after = slice_by_column(line, end_col as usize, (line_width - end_col).max(0) as usize, true);
                *line = format!(
                    "{before}{}{after}",
                    self.apply_search_text_highlight(&highlighted, range.current)
                );
            }
        }
        result
    }

    fn apply_selection_highlight(&self, text: &str) -> String {
        let mut result = String::from("\x1b[7m");
        let mut index = 0;
        while index < text.len() {
            match extract_ansi_code(text, index) {
                Some(ansi) => {
                    result.push_str(ansi.code);
                    if ansi.code.ends_with('m') {
                        result.push_str("\x1b[7m");
                    }
                    index += ansi.length;
                }
                None => {
                    let length = next_char_len(text, index);
                    result.push_str(&text[index..index + length]);
                    index += length;
                }
            }
        }
        result.push_str("\x1b[27m");
        result
    }

    fn composite_flashes(&self, screen: Vec<String>, width: usize, height: usize) -> Vec<String> {
        let flash_lines = self.flashes.borrow_mut().render(width);
        let skip = flash_lines.len().saturating_sub(height);
        let flash_lines = &flash_lines[skip..];
        if flash_lines.is_empty() {
            return screen;
        }
        let mut result = screen;
        result.resize(result.len().max(height), String::new());
        for (row, line) in flash_lines.iter().enumerate() {
            let flash_width = visible_width(line);
            if flash_width == 0 {
                continue;
            }
            let start_col = width.saturating_sub(flash_width);
            result[row] = crate::tui::composite_tui_line(&result[row], line, start_col, flash_width, width);
        }
        result
    }

    fn delete_kitty_images(&self, protocol: Option<ImageProtocol>) -> String {
        if protocol == Some(ImageProtocol::Kitty) {
            delete_all_kitty_images()
        } else {
            String::new()
        }
    }

    fn prepare_kitty_screen(&self, state: &mut AltScreenState, screen: Vec<String>) -> (Vec<String>, String) {
        let mut visible_image_ids: Vec<u32> = Vec::new();
        let mut lines = Vec::with_capacity(screen.len());
        for line in screen {
            let Some(placement) = get_kitty_image_placement(&line) else {
                lines.push(line);
                continue;
            };
            visible_image_ids.push(placement.image_id);
            let cached = state
                .uploaded_kitty_images
                .iter()
                .find(|(image_id, _)| *image_id == placement.image_id)
                .map(|(_, image)| *image);
            let next = CachedKittyImage {
                transmission_generation: placement.transmission_generation,
                transmission_bytes: placement.transmission_bytes,
                estimated_decoded_bytes: placement.estimated_decoded_bytes,
            };
            state
                .uploaded_kitty_images
                .retain(|(image_id, _)| *image_id != placement.image_id);
            state.uploaded_kitty_images.push((placement.image_id, next));
            let reuse = cached.is_some_and(|image| image.transmission_generation == placement.transmission_generation);
            lines.push(if reuse { placement.replacement_line } else { line });
        }

        let mut cached_offscreen_image_count = 0_usize;
        let mut cached_offscreen_transmission_bytes = 0_u64;
        let mut cached_offscreen_decoded_bytes = 0_u64;
        for (image_id, cached) in &state.uploaded_kitty_images {
            if visible_image_ids.contains(image_id) {
                continue;
            }
            cached_offscreen_image_count += 1;
            cached_offscreen_transmission_bytes += cached.transmission_bytes as u64;
            cached_offscreen_decoded_bytes += cached.estimated_decoded_bytes;
        }

        let mut evicted_image_deletion = String::new();
        let mut index = 0;
        while index < state.uploaded_kitty_images.len() {
            if cached_offscreen_image_count <= MAX_CACHED_OFFSCREEN_KITTY_IMAGES
                && cached_offscreen_transmission_bytes <= MAX_CACHED_OFFSCREEN_KITTY_TRANSMISSION_BYTES
                && cached_offscreen_decoded_bytes <= MAX_CACHED_OFFSCREEN_KITTY_DECODED_BYTES
            {
                break;
            }
            let (image_id, cached) = state.uploaded_kitty_images[index];
            if visible_image_ids.contains(&image_id) {
                index += 1;
                continue;
            }
            evicted_image_deletion.push_str(&delete_kitty_image(image_id));
            state.uploaded_kitty_images.remove(index);
            cached_offscreen_image_count -= 1;
            cached_offscreen_transmission_bytes -= cached.transmission_bytes as u64;
            cached_offscreen_decoded_bytes -= cached.estimated_decoded_bytes;
        }
        (lines, evicted_image_deletion)
    }

    fn do_render_impl(&self) {
        if self.base.is_stopped() || !self.state.borrow().alt_screen_active {
            return;
        }
        let width = self.base.terminal().borrow().columns().max(1);
        let height = self.base.terminal().borrow().rows().max(1);
        let layout_root = self
            .state
            .borrow()
            .layout_root
            .clone()
            .unwrap_or_else(|| Rc::clone(&self.implicit_scroll_view));
        let render_base = self.base.clone();
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            render_base.request_render(false);
        });
        let mut next_layout = render_layout_frame(&layout_root, width, height, Rc::clone(&request_render));
        if self.refresh_search(&next_layout) {
            next_layout = render_layout_frame(&layout_root, width, height, request_render);
        }

        let mut screen: Vec<String> = next_layout
            .lines
            .iter()
            .map(|line| strip_osc133_zone_prefix(line))
            .collect();
        screen = self.apply_search_highlights(screen, &next_layout);
        screen = self.base.composite_overlays(screen, width, height);
        if screen.len() > height {
            screen = screen.split_off(screen.len() - height);
        }
        {
            let state = self.state.borrow();
            screen = apply_selection(
                screen,
                &next_layout,
                &state,
                self.base.terminal().borrow().columns() as i64,
                &|text| self.apply_selection_highlight(text),
            );
        }
        screen = self.composite_flashes(screen, width, height);

        let cursor_position = self.base.extract_cursor_position(&mut screen, height);
        self.base.apply_line_resets(&mut screen);
        screen = screen
            .into_iter()
            .map(|line| {
                if is_image_line(&line) || visible_width(&line) <= width {
                    line
                } else {
                    slice_by_column(&line, 0, width, true)
                }
            })
            .collect();

        let mut state = self.state.borrow_mut();
        let full_redraw = state.previous_screen.is_empty()
            || state.previous_screen_width != width
            || state.previous_screen_height != height;
        let images_need_redraw = screen.iter().enumerate().any(|(row, line)| {
            let previous = state.previous_screen.get(row).map(String::as_str).unwrap_or("");
            line != previous && (is_image_line(line) || is_image_line(previous))
        });
        let redraw_images = full_redraw || images_need_redraw;
        let had_uploaded_kitty_images = !state.uploaded_kitty_images.is_empty();
        let (prepared_lines, evicted_image_deletion) =
            if redraw_images && state.image_protocol == Some(ImageProtocol::Kitty) {
                self.prepare_kitty_screen(&mut state, screen)
            } else {
                (screen, String::new())
            };

        let mut buffer = String::from(BEGIN_SYNCHRONIZED_OUTPUT);
        if full_redraw {
            self.base.increment_full_redraws();
            let clear_images = if state.image_protocol == Some(ImageProtocol::Kitty) && had_uploaded_kitty_images {
                delete_all_kitty_placements()
            } else {
                self.delete_kitty_images(state.image_protocol)
            };
            buffer.push_str(&clear_images);
            buffer.push_str("\x1b[2J");
        } else if images_need_redraw {
            if state.image_protocol == Some(ImageProtocol::Iterm2) {
                buffer.push_str("\x1b[2J");
            } else if state.image_protocol == Some(ImageProtocol::Kitty) {
                buffer.push_str(&delete_all_kitty_placements());
            }
        }
        buffer.push_str(&evicted_image_deletion);

        for row in 0..height {
            if !full_redraw && !images_need_redraw && prepared_lines.get(row) == state.previous_screen.get(row) {
                continue;
            }
            buffer.push_str(&format!("\x1b[{};1H\x1b[2K", row + 1));
            buffer.push_str(prepared_lines.get(row).map(String::as_str).unwrap_or(""));
        }

        match cursor_position {
            Some(cursor) => {
                buffer.push_str(&format!("\x1b[{};{}H", cursor.row + 1, width.min(cursor.col) + 1));
                buffer.push_str(if self.base.get_show_hardware_cursor() {
                    "\x1b[?25h"
                } else {
                    "\x1b[?25l"
                });
            }
            None => buffer.push_str("\x1b[?25l"),
        }
        buffer.push_str(END_SYNCHRONIZED_OUTPUT);
        self.base.terminal().borrow_mut().write(&buffer);

        state.previous_screen = prepared_lines;
        state.previous_screen_width = width;
        state.previous_screen_height = height;
        state.current_layout = Some(next_layout);
    }

    fn reset_render_state_impl(&self) {
        let mut state = self.state.borrow_mut();
        state.previous_screen.clear();
        state.previous_screen_width = 0;
        state.previous_screen_height = 0;
        state.current_layout = None;
    }

    fn before_terminal_start_impl(&self) {
        {
            let mut state = self.state.borrow_mut();
            stop_selection_auto_scroll(&mut state);
            state.selection_press_active = false;
            self.set_scrollbar_hover(&mut state, None);
            state.scrollbar_drag = None;
        }
        self.flashes.borrow_mut().dispose();
        let mut should_invalidate = false;
        {
            let mut state = self.state.borrow_mut();
            state.alt_screen_active = true;
            let capabilities = get_capabilities();
            state.image_protocol = capabilities.images;
            state.uploaded_kitty_images.clear();
            if capabilities.images == Some(ImageProtocol::Iterm2) {
                state.saved_capabilities = Some(capabilities);
                set_capabilities(TerminalCapabilities {
                    images: None,
                    ..capabilities
                });
                should_invalidate = true;
            }
            state.last_document.clear();
            state.selection_anchor = None;
            state.selection_focus = None;
            state.selection_granularity = SelectionGranularity::Character;
            state.selection_initial_range = None;
            state.last_click = None;
            state.pressed_url = None;
            state.selection_dragged = false;
        }
        if should_invalidate {
            self.invalidate();
        }
        self.reset_render_state_impl();
        let environment = self.environment.borrow();
        let mouse_sequence = if environment.uses_multiplexer() {
            ENABLE_BUTTON_MOTION_MOUSE
        } else {
            ENABLE_ALL_MOTION_MOUSE
        };
        let mouse = if self.mouse_enabled { mouse_sequence } else { "" };
        self.base.terminal().borrow_mut().write(&format!(
            "{ENTER_ALT_SCREEN}{DISABLE_AUTOWRAP}{mouse}\x1b[2J\x1b[H\x1b[?25l"
        ));
    }

    fn before_terminal_stop_impl(&self) {
        self.close_search();
        {
            let mut state = self.state.borrow_mut();
            stop_selection_auto_scroll(&mut state);
            state.selection_press_active = false;
            self.set_scrollbar_hover(&mut state, None);
            state.scrollbar_drag = None;
        }
        self.flashes.borrow_mut().dispose();
        let mut state = self.state.borrow_mut();
        if !state.alt_screen_active {
            return;
        }
        let mouse = if self.mouse_enabled { DISABLE_MOUSE } else { "" };
        self.base.terminal().borrow_mut().write(&format!(
            "{BEGIN_SYNCHRONIZED_OUTPUT}{}{mouse}{ENABLE_AUTOWRAP}{END_SYNCHRONIZED_OUTPUT}",
            self.delete_kitty_images(state.image_protocol)
        ));
        state.uploaded_kitty_images.clear();
    }

    fn after_terminal_stop_impl(&self, options: &TuiStopOptions) {
        {
            let state = self.state.borrow();
            if !state.alt_screen_active {
                return;
            }
        }
        self.state.borrow_mut().alt_screen_active = false;
        if options.preserve_screen {
            self.base.terminal().borrow_mut().write(&format!(
                "{BEGIN_SYNCHRONIZED_OUTPUT}{EXIT_ALT_SCREEN}\x1b[?25h{END_SYNCHRONIZED_OUTPUT}"
            ));
        } else {
            let width = self.base.terminal().borrow().columns().max(1);
            let document_lines: Vec<String> = self
                .render(width)
                .iter()
                .map(|line| strip_osc133_zone_prefix(line))
                .collect();
            let mut last_document: Vec<String> = document_lines
                .iter()
                .map(|line| line.replace(CURSOR_MARKER, ""))
                .collect();
            self.base.apply_line_resets(&mut last_document);
            last_document = last_document
                .into_iter()
                .map(|line| {
                    if is_image_line(&line) || visible_width(&line) <= width {
                        line
                    } else {
                        slice_by_column(&line, 0, width, true)
                    }
                })
                .collect();
            let mut buffer = format!("{BEGIN_SYNCHRONIZED_OUTPUT}{EXIT_ALT_SCREEN}{DISABLE_AUTOWRAP}");
            for (row, line) in last_document.iter().enumerate() {
                if row > 0 {
                    buffer.push_str("\r\n");
                }
                buffer.push_str(&format!("\r\x1b[2K{line}"));
            }
            buffer.push_str(&format!(
                "\x1b[0m{ENABLE_AUTOWRAP}\r\n\x1b[?25h{END_SYNCHRONIZED_OUTPUT}"
            ));
            self.base.terminal().borrow_mut().write(&buffer);
            self.state.borrow_mut().last_document = last_document;
        }
        let saved = self.state.borrow_mut().saved_capabilities.take();
        if let Some(saved) = saved {
            set_capabilities(saved);
        }
    }

    fn auto_scroll_selection(&self) {
        let mut state = self.state.borrow_mut();
        let scroll_view = state
            .selection_anchor
            .as_ref()
            .and_then(|point| point.scroll_view.clone());
        let pointer = state.selection_drag_pointer;
        let direction = state.selection_auto_scroll_direction;
        let (Some(scroll_view), Some((x, y)), true) = (scroll_view, pointer, direction != 0) else {
            stop_selection_auto_scroll(&mut state);
            return;
        };
        let remaining = scroll_view.borrow_mut().scroll_by(direction);
        if remaining == direction {
            stop_selection_auto_scroll(&mut state);
            return;
        }
        let rows = self.base.terminal().borrow().rows() as i64;
        let point = get_scroll_selection_point(&state, &scroll_view, x, y, rows);
        if let Some(point) = point {
            update_selection_focus(&mut state, point);
        }
        drop(state);
        self.base.request_render(false);
    }
}

impl RendererApi for TuiAltScreen {
    fn do_render(&self) {
        self.do_render_impl();
    }

    fn reset_render_state(&self) {
        self.reset_render_state_impl();
    }

    fn before_terminal_start(&self) {
        self.before_terminal_start_impl();
    }

    fn before_terminal_stop(&self, _options: &TuiStopOptions) {
        self.before_terminal_stop_impl();
    }

    fn after_terminal_stop(&self, options: &TuiStopOptions) {
        self.after_terminal_stop_impl(options);
    }

    fn mode(&self) -> TuiMode {
        TuiMode::Fullscreen
    }
}

impl TuiExt for TuiAltScreen {
    fn base(&self) -> &TuiBase {
        &self.base
    }

    fn as_viewport_tui(&self) -> Option<&dyn ViewportTui> {
        Some(self)
    }
}

impl ViewportTui for TuiAltScreen {
    fn set_layout_root(&self, component: Option<ComponentRef>) {
        TuiAltScreen::set_layout_root(self, component);
    }

    fn viewport_top(&self) -> usize {
        TuiAltScreen::viewport_top(self)
    }

    fn is_following_output(&self) -> bool {
        TuiAltScreen::is_following_output(self)
    }
}

fn consume_input() -> TuiInputListenerResult {
    TuiInputListenerResult {
        consume: true,
        data: None,
    }
}

fn same_optional_component(left: &Option<ComponentRef>, right: &Option<ComponentRef>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
        _ => false,
    }
}

fn same_optional_scroll_view(
    left: Option<&Rc<RefCell<ScrollState>>>,
    right: Option<&Rc<RefCell<ScrollState>>>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
        _ => false,
    }
}

fn next_char_len(text: &str, index: usize) -> usize {
    text[index..].chars().next().map(char::len_utf8).unwrap_or(1)
}

fn is_osc133_zone_prefix(line: &str) -> Option<usize> {
    for kind in ["A", "B", "C"] {
        let bel = format!("\x1b]133;{kind}\x07");
        if line.starts_with(&bel) {
            return Some(bel.len());
        }
        let st = format!("\x1b]133;{kind}\x1b\\");
        if line.starts_with(&st) {
            return Some(st.len());
        }
    }
    None
}

fn strip_osc133_zone_prefix(line: &str) -> String {
    let mut rest = line;
    while let Some(length) = is_osc133_zone_prefix(rest) {
        rest = &rest[length..];
    }
    rest.to_string()
}

fn is_prompt_start(line: &str) -> bool {
    line.starts_with("\x1b]133;A\x07") || line.starts_with("\x1b]133;A\x1b\\")
}

fn collect_scroll_views(box_: &LayoutBox, result: &mut Vec<Rc<RefCell<ScrollState>>>) {
    if let Some(scroll_view) = &box_.scroll_view {
        result.push(Rc::clone(scroll_view));
    }
    for child in &box_.children {
        collect_scroll_views(child, result);
    }
}

fn parse_sgr_fields(data: &str) -> Option<(i64, i64, i64, bool)> {
    let body = data.strip_prefix("\x1b[<")?;
    let (body, release) = match body.strip_suffix('M') {
        Some(body) => (body, false),
        None => (body.strip_suffix('m')?, true),
    };
    let parts: Vec<&str> = body.split(';').collect();
    if parts.len() != 3 {
        return None;
    }
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let button = parts[0].parse().ok()?;
    let x = parts[1].parse().ok()?;
    let y = parts[2].parse().ok()?;
    Some((button, x, y, release))
}

fn parse_wheel_event(data: &str) -> Option<WheelEvent> {
    if let Some((button, x, y, _release)) = parse_sgr_fields(data) {
        if button & 64 == 0 {
            return None;
        }
        let direction = button & 3;
        if direction != 0 && direction != 1 {
            return None;
        }
        return Some(WheelEvent {
            direction: if direction == 0 { -1 } else { 1 },
            x: x - 1,
            y: y - 1,
        });
    }
    if data.len() == 6 && data.starts_with("\x1b[M") {
        let bytes = data.as_bytes();
        let button = bytes[3] as i64 - 32;
        if button & 64 == 0 {
            return None;
        }
        let direction = button & 3;
        if direction != 0 && direction != 1 {
            return None;
        }
        return Some(WheelEvent {
            direction: if direction == 0 { -1 } else { 1 },
            x: bytes[4] as i64 - 33,
            y: bytes[5] as i64 - 33,
        });
    }
    None
}

fn parse_sgr_mouse_event(data: &str) -> Option<SgrMouseEvent> {
    let (button, x, y, release) = parse_sgr_fields(data)?;
    Some(SgrMouseEvent {
        button,
        x: x - 1,
        y: y - 1,
        release,
    })
}

fn is_mouse_sequence(data: &str) -> bool {
    parse_sgr_fields(data).is_some() || (data.len() == 6 && data.starts_with("\x1b[M"))
}

fn get_scroll_selection_point(
    state: &AltScreenState,
    scroll_view: &Rc<RefCell<ScrollState>>,
    x: i64,
    y: i64,
    terminal_rows: i64,
) -> Option<SelectionPoint> {
    let layout = state.current_layout.as_ref()?;
    let box_ = get_scroll_view_box(layout, scroll_view)?;
    if box_.rect.height <= 0 || box_.clip.height <= 0 {
        return None;
    }
    let visible_top = box_.rect.y.max(box_.clip.y).max(0);
    let visible_bottom = (terminal_rows - 1)
        .min(box_.rect.y + box_.rect.height - 1)
        .min(box_.clip.y + box_.clip.height - 1);
    if visible_bottom < visible_top {
        return None;
    }
    let pointer_row = y.clamp(visible_top, visible_bottom);
    let max_content_row = box_
        .scroll_content_lines
        .as_ref()
        .map(|lines| lines.len().max(1) as i64 - 1)
        .unwrap_or(0)
        .max(0);
    let scroll_top = scroll_view.borrow().scroll_top() as i64;
    Some(SelectionPoint::new(
        (scroll_top + pointer_row - box_.rect.y).clamp(0, max_content_row),
        (x - box_.rect.x).clamp(0, box_.rect.width - 1),
        Some(Rc::clone(scroll_view)),
    ))
}

fn selection_point(
    state: &AltScreenState,
    event: SgrMouseEvent,
    scroll_view: Option<Rc<RefCell<ScrollState>>>,
    terminal_rows: i64,
    terminal_columns: i64,
) -> SelectionPoint {
    if let Some(scroll_view) = &scroll_view
        && let Some(point) = get_scroll_selection_point(state, scroll_view, event.x, event.y, terminal_rows)
    {
        return point;
    }
    SelectionPoint::new(
        event.y.clamp(0, terminal_rows - 1),
        event.x.clamp(0, terminal_columns - 1),
        None,
    )
}

fn selection_source_line(state: &AltScreenState, point: &SelectionPoint) -> String {
    if let Some(scroll_view) = &point.scroll_view
        && let Some(layout) = state.current_layout.as_ref()
        && let Some(box_) = get_scroll_view_box(layout, scroll_view)
        && let Some(lines) = &box_.scroll_content_lines
    {
        return lines.get(point.row.max(0) as usize).cloned().unwrap_or_default();
    }
    state
        .previous_screen
        .get(point.row.max(0) as usize)
        .cloned()
        .unwrap_or_default()
}

fn word_selection(state: &AltScreenState, point: &SelectionPoint) -> Option<SelectionRange> {
    let line = strip_terminal_sequences(&selection_source_line(state, point));
    let mut start = 0_i64;
    for segment in word_segments(&line) {
        let end = start + visible_width(&segment.segment) as i64;
        if point.col >= start && point.col < end {
            return Some(SelectionRange {
                start: SelectionPoint {
                    col: start,
                    ..point.clone()
                },
                end: SelectionPoint {
                    col: end,
                    boundary: true,
                    ..point.clone()
                },
            });
        }
        start = end;
    }
    None
}

fn line_selection(state: &AltScreenState, point: &SelectionPoint) -> SelectionRange {
    SelectionRange {
        start: SelectionPoint {
            col: 0,
            ..point.clone()
        },
        end: SelectionPoint {
            col: visible_width(&selection_source_line(state, point)) as i64,
            boundary: true,
            ..point.clone()
        },
    }
}

fn update_selection_focus(state: &mut AltScreenState, point: SelectionPoint) {
    if state.selection_granularity == SelectionGranularity::Character || state.selection_initial_range.is_none() {
        state.selection_focus = Some(point);
        return;
    }
    let range = match state.selection_granularity {
        SelectionGranularity::Word => word_selection(state, &point),
        _ => Some(line_selection(state, &point)),
    };
    let Some(range) = range else {
        return;
    };
    let initial = state.selection_initial_range.clone().expect("checked above");
    let target_before_initial = range.start.row < initial.start.row
        || (range.start.row == initial.start.row && range.start.col < initial.start.col);
    if target_before_initial {
        state.selection_anchor = Some(initial.end);
        state.selection_focus = Some(range.start);
    } else {
        state.selection_anchor = Some(initial.start);
        state.selection_focus = Some(range.end);
    }
}

fn click_count(state: &mut AltScreenState, point: &SelectionPoint, word: Option<&SelectionRange>) -> u32 {
    let now = Instant::now();
    let previous = state.last_click.clone();
    let count = match (word, previous.as_ref()) {
        (Some(word), Some(previous))
            if now.duration_since(previous.timestamp) <= DOUBLE_CLICK_INTERVAL
                && previous.row == point.row
                && same_optional_scroll_view(previous.scroll_view.as_ref(), point.scroll_view.as_ref())
                && previous.word_start == word.start.col
                && previous.word_end == word.end.col =>
        {
            (previous.count % 3) + 1
        }
        _ => 1,
    };
    state.last_click = word.map(|word| ClickTarget {
        timestamp: now,
        count,
        row: point.row,
        scroll_view: point.scroll_view.clone(),
        word_start: word.start.col,
        word_end: word.end.col,
    });
    count
}

fn update_selection_auto_scroll(state: &mut AltScreenState, event: SgrMouseEvent, terminal_rows: i64) {
    let scroll_view = state
        .selection_anchor
        .as_ref()
        .and_then(|point| point.scroll_view.clone());
    let Some(scroll_view) = scroll_view else {
        stop_selection_auto_scroll(state);
        return;
    };
    let Some(layout) = state.current_layout.as_ref() else {
        stop_selection_auto_scroll(state);
        return;
    };
    let Some(box_) = get_scroll_view_box(layout, &scroll_view) else {
        stop_selection_auto_scroll(state);
        return;
    };
    if box_.rect.height <= 0 || box_.clip.height <= 0 {
        stop_selection_auto_scroll(state);
        return;
    }
    let visible_top = box_.rect.y.max(box_.clip.y).max(0);
    let visible_bottom = (terminal_rows - 1)
        .min(box_.rect.y + box_.rect.height - 1)
        .min(box_.clip.y + box_.clip.height - 1);
    state.selection_drag_pointer = Some((event.x, event.y));
    state.selection_auto_scroll_direction = if event.y <= visible_top {
        -1
    } else if event.y >= visible_bottom {
        1
    } else {
        0
    };
    if state.selection_auto_scroll_direction == 0 {
        stop_selection_auto_scroll(state);
    }
}

fn stop_selection_auto_scroll(state: &mut AltScreenState) {
    state.selection_auto_scroll_direction = 0;
    state.selection_drag_pointer = None;
}

fn selection_bounds(state: &AltScreenState) -> Option<SelectionRange> {
    let anchor = state.selection_anchor.as_ref()?;
    let focus = state.selection_focus.as_ref()?;
    if !same_optional_scroll_view(anchor.scroll_view.as_ref(), focus.scroll_view.as_ref()) {
        return None;
    }
    if anchor.row == focus.row && anchor.col == focus.col {
        return None;
    }
    let anchor_before_focus = anchor.row < focus.row || (anchor.row == focus.row && anchor.col < focus.col);
    if anchor_before_focus {
        Some(SelectionRange {
            start: anchor.clone(),
            end: focus.clone(),
        })
    } else {
        Some(SelectionRange {
            start: focus.clone(),
            end: anchor.clone(),
        })
    }
}

fn get_selection_columns(
    line: &str,
    row: i64,
    selection: &SelectionRange,
    min_column: i64,
    max_column: i64,
) -> (i64, i64) {
    let line_width = visible_width(line) as i64;
    let mut start = min_column.max(0);
    let mut end = max_column.min(line_width);
    if row == selection.start.row {
        start = get_grapheme_cell_range(line, selection.start.col.max(0) as usize)
            .map(|range| range.start as i64)
            .unwrap_or(selection.start.col.min(line_width));
    }
    if row == selection.end.row {
        end = if selection.end.boundary {
            selection.end.col.min(line_width)
        } else {
            get_grapheme_cell_range(line, selection.end.col.max(0) as usize)
                .map(|range| range.end as i64)
                .unwrap_or((selection.end.col + 1).min(line_width))
        };
    }
    (start.max(min_column), end.min(max_column))
}

fn apply_selection(
    screen: Vec<String>,
    layout: &LayoutFrame,
    state: &AltScreenState,
    terminal_columns: i64,
    highlight: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let Some(selection) = selection_bounds(state) else {
        return screen;
    };
    let mut screen_selection = selection.clone();
    let mut min_row = 0_i64;
    let mut max_row = screen.len() as i64 - 1;
    let mut min_column = 0_i64;
    let mut max_column = terminal_columns;
    if let Some(scroll_view) = &selection.start.scroll_view {
        let Some(box_) = get_scroll_view_box(layout, scroll_view) else {
            return screen;
        };
        min_row = box_.rect.y.max(box_.clip.y).max(0);
        max_row = (screen.len() as i64 - 1)
            .min(box_.rect.y + box_.rect.height - 1)
            .min(box_.clip.y + box_.clip.height - 1);
        min_column = box_.rect.x.max(box_.clip.x).max(0);
        max_column = terminal_columns
            .min(box_.rect.x + box_.rect.width)
            .min(box_.clip.x + box_.clip.width);
        let scroll_top = scroll_view.borrow().scroll_top() as i64;
        screen_selection = SelectionRange {
            start: SelectionPoint {
                row: box_.rect.y + selection.start.row - scroll_top,
                col: box_.rect.x + selection.start.col,
                ..selection.start.clone()
            },
            end: SelectionPoint {
                row: box_.rect.y + selection.end.row - scroll_top,
                col: box_.rect.x + selection.end.col,
                ..selection.end.clone()
            },
        };
    }
    screen
        .into_iter()
        .enumerate()
        .map(|(row, line)| {
            let row = row as i64;
            if row < min_row
                || row > max_row
                || row < screen_selection.start.row
                || row > screen_selection.end.row
                || is_image_line(&line)
            {
                return line;
            }
            let line_width = visible_width(&line) as i64;
            let columns = get_selection_columns(&line, row, &screen_selection, min_column, max_column);
            if columns.1 <= columns.0 {
                return line;
            }
            let before = slice_by_column(&line, 0, columns.0.max(0) as usize, true);
            let selected = slice_by_column(
                &line,
                columns.0.max(0) as usize,
                (columns.1 - columns.0).max(0) as usize,
                true,
            );
            let after = slice_by_column(
                &line,
                columns.1.max(0) as usize,
                (line_width - columns.1).max(0) as usize,
                true,
            );
            format!("{before}{}{after}", highlight(&selected))
        })
        .collect()
}
