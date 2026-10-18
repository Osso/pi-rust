//! Ported from `packages/tui/src/tui.ts`.
//!
//! The TypeScript class hierarchy (`Container` → `TuiBase` → `TUIRenderer`) is
//! represented by a shared [`TuiBase`] core plus two renderer structs
//! ([`TuiRenderer`], [`TuiMainScreen`]) that implement [`RendererApi`]. Components
//! are `Rc<RefCell<dyn Component>>` handles, which preserves the object identity
//! the source relies on for focus, overlays, fixed cells, and render regions.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::keys::{is_key_release, is_key_repeat, matches_key};
use crate::terminal::{Terminal, TerminalEvent};
use crate::terminal_colors::{
    RgbColor, TerminalColorScheme, is_osc11_background_color_response, parse_osc11_background_color,
    parse_terminal_color_scheme_report,
};
use crate::terminal_image::{delete_kitty_image, get_capabilities, is_image_line, set_cell_dimensions};
use crate::utils::{extract_segments, normalize_terminal_output, slice_by_column, slice_with_width};

const TERMINAL_FOCUS_IN: &str = "\x1b[I";
const TERMINAL_FOCUS_OUT: &str = "\x1b[O";
const ENABLE_TERMINAL_FOCUS_REPORTING: &str = "\x1b[?1004h";
const DISABLE_TERMINAL_FOCUS_REPORTING: &str = "\x1b[?1004l";
const KITTY_SEQUENCE_PREFIX: &str = "\x1b_G";
const SEGMENT_RESET: &str = "\x1b[0m\x1b]8;;\x07";

pub use crate::tui_main_screen::TuiMainScreen;

/// Cursor position marker emitted by focused components.
pub const CURSOR_MARKER: &str = "\x1b_pi:c\x07";

/// Handle to a component owned by a container or the TUI.
pub type ComponentRef = Rc<RefCell<dyn Component>>;

/// Create a shareable component handle.
pub fn component<T: Component + 'static>(value: T) -> ComponentRef {
    Rc::new(RefCell::new(value))
}

pub(crate) fn component_id(component: &ComponentRef) -> usize {
    Rc::as_ptr(component) as *const () as usize
}
fn same_component(left: &ComponentRef, right: &ComponentRef) -> bool {
    Rc::ptr_eq(left, right)
}

/// Component interface: render to lines and optionally handle input.
pub trait Component {
    fn render(&mut self, width: usize) -> Vec<String>;

    fn handle_input(&mut self, _data: &str) {}

    /// Whether the component implements input handling. Containers override this
    /// to `false`, matching the source's `handleInput?` presence check.
    fn wants_input(&self) -> bool {
        true
    }

    fn wants_key_release(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        None
    }

    fn as_container(&self) -> Option<&Container> {
        None
    }

    /// Layout node exposed to `layout.rs` (TypeScript's `LAYOUT_NODE` symbol).
    fn layout_node(&self) -> Option<crate::layout_node::LayoutNode> {
        None
    }

    /// Downcast hook for callers holding a trait-object component handle.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

/// Components that can receive focus and display a hardware cursor.
pub trait Focusable {
    fn focused(&self) -> bool;
    fn set_focused(&mut self, focused: bool);
}

fn set_component_focused_now(component: &ComponentRef, focused: bool) -> bool {
    match component.try_borrow_mut() {
        Ok(mut component) => {
            if let Some(focusable) = component.as_focusable_mut() {
                focusable.set_focused(focused);
            }
            true
        }
        Err(_) => false,
    }
}

fn component_is_focusable(component: &ComponentRef) -> bool {
    component.borrow_mut().as_focusable_mut().is_some()
}

/// Type guard matching `isFocusable`.
pub fn is_focusable(component: Option<&ComponentRef>) -> bool {
    component.is_some_and(component_is_focusable)
}

/// Container of child components.
#[derive(Default)]
pub struct Container {
    children: Vec<ComponentRef>,
}

impl Container {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_children(children: Vec<ComponentRef>) -> Self {
        Self { children }
    }

    pub fn add_child(&mut self, component: ComponentRef) {
        self.children.push(component);
    }

    pub fn remove_child(&mut self, component: &ComponentRef) {
        self.children.retain(|child| !same_component(child, component));
    }

    pub fn clear(&mut self) {
        self.children.clear();
    }

    pub fn children(&self) -> &[ComponentRef] {
        &self.children
    }

    pub fn render_children(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for child in &self.children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    pub fn invalidate_children(&mut self) {
        for child in &self.children {
            child.borrow_mut().invalidate();
        }
    }
}

impl Component for Container {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.render_children(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.invalidate_children();
    }

    fn as_container(&self) -> Option<&Container> {
        Some(self)
    }
}

type InputListener = Rc<dyn Fn(&str) -> TuiInputListenerResult>;
type ColorSchemeListener = Rc<dyn Fn(TerminalColorScheme)>;

/// Input listener result: optional consumption and optional data rewrite.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuiInputListenerResult {
    pub consume: bool,
    pub data: Option<String>,
}

/// Overlay anchor positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAnchor {
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    LeftCenter,
    RightCenter,
}

/// Absolute number or percentage string value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeValue {
    Absolute(f64),
    Percent(f64),
}

impl SizeValue {
    /// Parse a percentage string like `"50%"`. Absolute numbers are constructed directly.
    pub fn parse_percent(value: &str) -> Option<Self> {
        let body = value.strip_suffix('%')?;
        let parsed = parse_decimal(body)?;
        Some(SizeValue::Percent(parsed))
    }
}

fn parse_decimal(value: &str) -> Option<f64> {
    if value.is_empty() {
        return None;
    }
    let mut dots = 0;
    for character in value.chars() {
        if character == '.' {
            dots += 1;
            if dots > 1 {
                return None;
            }
        } else if !character.is_ascii_digit() {
            return None;
        }
    }
    if value.starts_with('.') || value.ends_with('.') {
        return None;
    }
    value.parse().ok()
}

/// Margin configuration for overlays.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayMargin {
    pub top: Option<i64>,
    pub right: Option<i64>,
    pub bottom: Option<i64>,
    pub left: Option<i64>,
}

/// Overlay positioning and sizing options.
#[derive(Clone, Default)]
pub struct OverlayOptions {
    pub width: Option<SizeValue>,
    pub min_width: Option<i64>,
    pub max_height: Option<SizeValue>,
    pub anchor: Option<OverlayAnchor>,
    pub offset_x: Option<i64>,
    pub offset_y: Option<i64>,
    pub row: Option<SizeValue>,
    pub col: Option<SizeValue>,
    pub margin: Option<OverlayMargin>,
    pub visible: Option<Rc<dyn Fn(usize, usize) -> bool>>,
    pub non_capturing: bool,
}

impl std::fmt::Debug for OverlayOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OverlayOptions")
            .field("width", &self.width)
            .field("min_width", &self.min_width)
            .field("max_height", &self.max_height)
            .field("anchor", &self.anchor)
            .field("offset_x", &self.offset_x)
            .field("offset_y", &self.offset_y)
            .field("row", &self.row)
            .field("col", &self.col)
            .field("margin", &self.margin)
            .field("visible", &self.visible.is_some())
            .field("non_capturing", &self.non_capturing)
            .finish()
    }
}

/// TUI mode. Fullscreen renderers are a later slice; the core supports regular.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Regular,
    Fullscreen,
}

/// Stop options.
#[derive(Debug, Clone, Copy, Default)]
pub struct TuiStopOptions {
    pub preserve_screen: bool,
}

/// Rect used by render regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderRegionRect {
    pub row: i64,
    pub col: i64,
    pub width: usize,
    pub height: usize,
}

/// Flow render region layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlowRenderRegionLayout {
    pub rect: RenderRegionRect,
    pub flow_end_row: i64,
    pub bottom_row: i64,
    pub bottom_height: i64,
    pub viewport_height: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorPosition {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone)]
struct OverlayEntry {
    component: ComponentRef,
    options: Option<OverlayOptions>,
    pre_focus: Option<ComponentRef>,
    hidden: bool,
    focus_order: u64,
}

type OverlayEntryRef = Rc<RefCell<OverlayEntry>>;

#[derive(Clone)]
enum OverlayBlockedFocusResume {
    RestoreOverlay,
    FocusTarget(Option<ComponentRef>),
}

#[derive(Clone)]
enum OverlayFocusRestoreState {
    Inactive,
    Eligible {
        overlay: OverlayEntryRef,
    },
    Blocked {
        overlay: OverlayEntryRef,
        blocked_by: ComponentRef,
        resume: OverlayBlockedFocusResume,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayFocusRestorePolicy {
    Clear,
    Preserve,
}

#[derive(Debug, Default)]
pub(crate) struct FixedCellState {
    pub(crate) row: Option<i64>,
    pub(crate) col: Option<i64>,
    pub(crate) value: String,
    pub(crate) disposed: bool,
}

/// Handle for updating one terminal cell without re-rendering the tree.
#[derive(Clone)]
pub struct FixedCell {
    pub(crate) state: Rc<RefCell<FixedCellState>>,
    pub(crate) writer: FixedCellWriter,
}

#[derive(Clone)]
pub(crate) enum FixedCellWriter {
    Immediate(Weak<TuiRenderer>),
    Deferred(Weak<TuiMainScreen>),
}

impl FixedCell {
    pub fn place(&self, row: i64, col: i64) {
        let mut state = self.state.borrow_mut();
        if state.disposed {
            return;
        }
        state.row = Some(row);
        state.col = Some(col);
    }

    pub fn clear(&self) {
        let mut state = self.state.borrow_mut();
        state.row = None;
        state.col = None;
    }

    pub fn update(&self, value: &str) -> bool {
        {
            let mut state = self.state.borrow_mut();
            if state.disposed {
                return false;
            }
            state.value = value.to_string();
        }
        match &self.writer {
            FixedCellWriter::Immediate(renderer) => match renderer.upgrade() {
                Some(renderer) => renderer.write_fixed_cell(&self.state),
                None => false,
            },
            FixedCellWriter::Deferred(screen) => {
                if let Some(screen) = screen.upgrade() {
                    screen.base.request_render(false);
                }
                false
            }
        }
    }

    pub fn dispose(&self) {
        let mut state = self.state.borrow_mut();
        state.disposed = true;
        let state_ptr = Rc::as_ptr(&self.state);
        drop(state);
        let owner = match &self.writer {
            FixedCellWriter::Immediate(renderer) => renderer.upgrade().map(|renderer| renderer.base.clone()),
            FixedCellWriter::Deferred(screen) => screen.upgrade().map(|screen| screen.base.clone()),
        };
        if let Some(base) = owner {
            base.unregister_fixed_cell(&self.state);
        }
        let _ = state_ptr;
    }
}

#[derive(Default)]
pub(crate) struct RenderRegionState {
    pub(crate) component: Option<ComponentRef>,
    pub(crate) rect: Option<RenderRegionRect>,
    pub(crate) flow_layout: Option<FlowRenderRegionLayout>,
    pub(crate) generation: Option<u64>,
    pub(crate) components: Vec<ComponentRef>,
    pub(crate) disposed: bool,
}

pub(crate) type RenderRegionStateRef = Rc<RefCell<RenderRegionState>>;

/// Full-width region whose contents can be rewritten without a root render.
pub struct RenderRegion {
    pub(crate) state: RenderRegionStateRef,
    pub(crate) writer: RegionWriter,
}

pub(crate) enum RegionWriter {
    Immediate(Weak<TuiRenderer>),
    Deferred(Weak<TuiMainScreen>),
}

impl RenderRegion {
    pub fn place(&self, rect: RenderRegionRect) {
        match &self.writer {
            RegionWriter::Immediate(renderer) => {
                if let Some(renderer) = renderer.upgrade() {
                    renderer.place_render_region(&self.state, rect);
                }
            }
            RegionWriter::Deferred(screen) => {
                if let Some(screen) = screen.upgrade() {
                    screen.place_deferred_render_region(&self.state, rect);
                }
            }
        }
    }

    pub fn clear(&self) {
        match &self.writer {
            RegionWriter::Immediate(renderer) => {
                if let Some(renderer) = renderer.upgrade() {
                    renderer.clear_render_region(&self.state);
                }
            }
            RegionWriter::Deferred(screen) => {
                if let Some(screen) = screen.upgrade() {
                    screen.clear_deferred_render_region(&self.state);
                }
            }
        }
    }

    pub fn request_render(&self) -> bool {
        match &self.writer {
            RegionWriter::Immediate(renderer) => match renderer.upgrade() {
                Some(renderer) => renderer.request_render_region(&self.state),
                None => false,
            },
            RegionWriter::Deferred(screen) => {
                if let Some(screen) = screen.upgrade() {
                    screen.base.request_render(false);
                }
                false
            }
        }
    }

    pub fn dispose(&self) {
        match &self.writer {
            RegionWriter::Immediate(renderer) => {
                if let Some(renderer) = renderer.upgrade() {
                    renderer.dispose_render_region(&self.state);
                }
            }
            RegionWriter::Deferred(screen) => {
                if let Some(screen) = screen.upgrade() {
                    screen.dispose_deferred_render_region(&self.state);
                }
            }
        }
    }
}

/// Flow render region with viewport-relative bottom tracking.
pub struct FlowRenderRegion {
    pub(crate) state: RenderRegionStateRef,
    pub(crate) writer: RegionWriter,
}

impl FlowRenderRegion {
    pub fn place(&self, layout: FlowRenderRegionLayout) {
        match &self.writer {
            RegionWriter::Immediate(renderer) => {
                if let Some(renderer) = renderer.upgrade() {
                    renderer.place_flow_render_region(&self.state, layout);
                }
            }
            RegionWriter::Deferred(screen) => {
                if let Some(screen) = screen.upgrade() {
                    screen.place_deferred_render_region(&self.state, layout.rect);
                }
            }
        }
    }

    pub fn clear(&self) {
        RenderRegion {
            state: Rc::clone(&self.state),
            writer: match &self.writer {
                RegionWriter::Immediate(renderer) => RegionWriter::Immediate(renderer.clone()),
                RegionWriter::Deferred(screen) => RegionWriter::Deferred(screen.clone()),
            },
        }
        .clear();
    }

    pub fn request_render(&self) -> bool {
        RenderRegion {
            state: Rc::clone(&self.state),
            writer: match &self.writer {
                RegionWriter::Immediate(renderer) => RegionWriter::Immediate(renderer.clone()),
                RegionWriter::Deferred(screen) => RegionWriter::Deferred(screen.clone()),
            },
        }
        .request_render()
    }

    pub fn dispose(&self) {
        RenderRegion {
            state: Rc::clone(&self.state),
            writer: match &self.writer {
                RegionWriter::Immediate(renderer) => RegionWriter::Immediate(renderer.clone()),
                RegionWriter::Deferred(screen) => RegionWriter::Deferred(screen.clone()),
            },
        }
        .dispose();
    }
}

struct Osc11QueryState {
    settled: bool,
    result: Option<Option<RgbColor>>,
    deadline: Instant,
}

/// Pending OSC 11 background-color query.
#[derive(Clone)]
pub struct TerminalBackgroundQuery {
    state: Rc<RefCell<Osc11QueryState>>,
}

impl TerminalBackgroundQuery {
    pub fn is_settled(&self) -> bool {
        self.state.borrow().settled
    }

    /// `Some(color)` once settled; the inner value is `None` on timeout.
    pub fn result(&self) -> Option<Option<RgbColor>> {
        let state = self.state.borrow();
        state.settled.then_some(state.result.flatten())
    }
}

struct ColorSchemeQueryState {
    settled: bool,
    result: Option<TerminalColorScheme>,
    deadline: Instant,
    listener_id: Option<usize>,
}

/// Pending color-scheme query.
#[derive(Clone)]
pub struct TerminalColorSchemeQuery {
    state: Rc<RefCell<ColorSchemeQueryState>>,
}

impl TerminalColorSchemeQuery {
    pub fn is_settled(&self) -> bool {
        self.state.borrow().settled
    }

    pub fn result(&self) -> Option<Option<TerminalColorScheme>> {
        let state = self.state.borrow();
        state.settled.then_some(state.result)
    }
}

/// Renderer hooks used by the shared TUI core.
pub trait RendererApi {
    fn do_render(&self);
    fn reset_render_state(&self);
    fn before_terminal_stop(&self, options: &TuiStopOptions);
    fn mode(&self) -> TuiMode {
        TuiMode::Regular
    }
}

struct TuiBaseInner {
    terminal: Rc<RefCell<dyn Terminal>>,
    self_weak: std::cell::OnceCell<Weak<TuiBaseInner>>,
    renderer: RefCell<Option<Weak<dyn RendererApi>>>,
    children: RefCell<Vec<ComponentRef>>,
    focused_component: RefCell<Option<ComponentRef>>,
    deferred_focus_updates: RefCell<Vec<(ComponentRef, bool)>>,
    input_listeners: RefCell<Vec<(usize, InputListener)>>,
    next_listener_id: Cell<usize>,
    on_debug: RefCell<Option<Rc<dyn Fn()>>>,
    render_requested: Cell<bool>,
    force_render_requested: Cell<bool>,
    immediate_render_scheduled: Cell<bool>,
    render_timer_scheduled: Cell<bool>,
    terminal_focused: Cell<bool>,
    focus_reporting_enabled: Cell<bool>,
    show_hardware_cursor: Cell<bool>,
    clear_on_shrink: Cell<bool>,
    termux_session: Cell<bool>,
    debug_redraw: Cell<bool>,
    full_redraw_count: Cell<usize>,
    stopped: Cell<bool>,
    pending_osc11_queries: RefCell<VecDeque<Rc<RefCell<Osc11QueryState>>>>,
    pending_osc11_replies: Cell<usize>,
    pending_color_scheme_queries: RefCell<Vec<Rc<RefCell<ColorSchemeQueryState>>>>,
    terminal_color_scheme_listeners: RefCell<Vec<(usize, ColorSchemeListener)>>,
    terminal_color_scheme_notifications_enabled: Cell<bool>,
    log_directory: PathBuf,
    focus_order_counter: Cell<u64>,
    overlay_stack: RefCell<Vec<OverlayEntryRef>>,
    overlay_focus_restore: RefCell<OverlayFocusRestoreState>,
    fixed_cells: RefCell<Vec<Rc<RefCell<FixedCellState>>>>,
    render_generation: Cell<u64>,
    rendering: Cell<bool>,
    component_render_regions: RefCell<Vec<(usize, RenderRegionStateRef)>>,
}

/// Shared TUI core. Cheap to clone; all clones refer to the same state.
#[derive(Clone)]
pub struct TuiBase {
    inner: Rc<TuiBaseInner>,
}

impl TuiBase {
    pub(crate) fn new(
        terminal: Rc<RefCell<dyn Terminal>>,
        show_hardware_cursor: Option<bool>,
        log_directory: Option<PathBuf>,
    ) -> Self {
        let inner = Rc::new(TuiBaseInner {
            terminal,
            self_weak: std::cell::OnceCell::new(),
            renderer: RefCell::new(None),
            children: RefCell::new(Vec::new()),
            focused_component: RefCell::new(None),
            deferred_focus_updates: RefCell::new(Vec::new()),
            input_listeners: RefCell::new(Vec::new()),
            next_listener_id: Cell::new(0),
            on_debug: RefCell::new(None),
            render_requested: Cell::new(false),
            force_render_requested: Cell::new(false),
            immediate_render_scheduled: Cell::new(false),
            render_timer_scheduled: Cell::new(false),
            terminal_focused: Cell::new(true),
            focus_reporting_enabled: Cell::new(false),
            show_hardware_cursor: Cell::new(
                show_hardware_cursor.unwrap_or_else(|| environment_flag("PI_HARDWARE_CURSOR")),
            ),
            clear_on_shrink: Cell::new(environment_flag("PI_CLEAR_ON_SHRINK")),
            termux_session: Cell::new(std::env::var("TERMUX_VERSION").is_ok()),
            debug_redraw: Cell::new(environment_flag("PI_DEBUG_REDRAW")),
            full_redraw_count: Cell::new(0),
            stopped: Cell::new(false),
            pending_osc11_queries: RefCell::new(VecDeque::new()),
            pending_osc11_replies: Cell::new(0),
            pending_color_scheme_queries: RefCell::new(Vec::new()),
            terminal_color_scheme_listeners: RefCell::new(Vec::new()),
            terminal_color_scheme_notifications_enabled: Cell::new(false),
            log_directory: log_directory.unwrap_or_else(default_log_directory),
            focus_order_counter: Cell::new(0),
            overlay_stack: RefCell::new(Vec::new()),
            overlay_focus_restore: RefCell::new(OverlayFocusRestoreState::Inactive),
            fixed_cells: RefCell::new(Vec::new()),
            render_generation: Cell::new(0),
            rendering: Cell::new(false),
            component_render_regions: RefCell::new(Vec::new()),
        });
        let _ = inner.self_weak.set(Rc::downgrade(&inner));
        Self { inner }
    }

    pub(crate) fn install_renderer(&self, renderer: Weak<dyn RendererApi>) {
        *self.inner.renderer.borrow_mut() = Some(renderer);
    }

    fn renderer(&self) -> Option<Rc<dyn RendererApi>> {
        self.inner.renderer.borrow().as_ref().and_then(Weak::upgrade)
    }

    pub fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        Rc::clone(&self.inner.terminal)
    }

    pub fn full_redraws(&self) -> usize {
        self.inner.full_redraw_count.get()
    }

    pub fn get_show_hardware_cursor(&self) -> bool {
        self.inner.show_hardware_cursor.get()
    }

    pub fn set_show_hardware_cursor(&self, enabled: bool) {
        if self.inner.show_hardware_cursor.get() == enabled {
            return;
        }
        self.inner.show_hardware_cursor.set(enabled);
        if !enabled {
            self.inner.terminal.borrow_mut().hide_cursor();
        }
        self.request_render(false);
    }

    pub fn get_clear_on_shrink(&self) -> bool {
        self.inner.clear_on_shrink.get()
    }

    pub fn set_clear_on_shrink(&self, enabled: bool) {
        self.inner.clear_on_shrink.set(enabled);
    }

    /// Explicit override for the source's `TERMUX_VERSION` environment check.
    pub fn set_termux_session(&self, enabled: bool) {
        self.inner.termux_session.set(enabled);
    }

    /// Explicit override for the source's `PI_DEBUG_REDRAW` environment check.
    pub fn set_debug_redraw(&self, enabled: bool) {
        self.inner.debug_redraw.set(enabled);
    }

    fn is_termux_session(&self) -> bool {
        self.inner.termux_session.get()
    }

    fn debug_redraw(&self) -> bool {
        self.inner.debug_redraw.get()
    }

    pub fn get_focused_component(&self) -> Option<ComponentRef> {
        self.inner.focused_component.borrow().clone()
    }

    pub fn set_on_debug(&self, handler: Rc<dyn Fn()>) {
        *self.inner.on_debug.borrow_mut() = Some(handler);
    }

    pub fn add_child(&self, component: ComponentRef) {
        self.inner.children.borrow_mut().push(component);
    }

    pub fn remove_child(&self, component: &ComponentRef) {
        self.inner
            .children
            .borrow_mut()
            .retain(|child| !same_component(child, component));
    }

    pub fn clear_children(&self) {
        self.inner.children.borrow_mut().clear();
    }

    pub fn children(&self) -> Vec<ComponentRef> {
        self.inner.children.borrow().clone()
    }

    pub fn has_overlay_entries(&self) -> bool {
        !self.inner.overlay_stack.borrow().is_empty()
    }

    pub fn has_overlay(&self) -> bool {
        self.inner
            .overlay_stack
            .borrow()
            .iter()
            .any(|entry| self.is_overlay_visible(entry))
    }

    pub fn start(&self) -> Result<(), crate::terminal::TerminalRawModeError> {
        self.inner.stopped.set(false);
        self.inner.terminal_focused.set(true);
        self.inner.render_requested.set(false);
        self.inner.terminal.borrow_mut().start()?;
        if self
            .renderer()
            .is_some_and(|renderer| renderer.mode() == TuiMode::Regular)
            && !self.inner.focus_reporting_enabled.get()
        {
            self.inner.terminal.borrow_mut().write(ENABLE_TERMINAL_FOCUS_REPORTING);
            self.inner.focus_reporting_enabled.set(true);
        }
        self.inner.terminal.borrow_mut().hide_cursor();
        if self.inner.terminal_color_scheme_notifications_enabled.get() {
            self.inner.terminal.borrow_mut().write("\x1b[?2031h");
        }
        self.query_cell_size();
        self.request_render(false);
        Ok(())
    }

    pub fn stop(&self, options: TuiStopOptions) {
        self.inner.stopped.set(true);
        self.cancel_render_timer();
        self.inner.render_requested.set(false);
        self.inner.force_render_requested.set(true);
        if self
            .renderer()
            .is_some_and(|renderer| renderer.mode() == TuiMode::Regular)
            && self.inner.focus_reporting_enabled.get()
        {
            self.inner.terminal.borrow_mut().write(DISABLE_TERMINAL_FOCUS_REPORTING);
            self.inner.focus_reporting_enabled.set(false);
        }
        if self.inner.terminal_color_scheme_notifications_enabled.get() {
            self.inner.terminal.borrow_mut().write("\x1b[?2031l");
        }
        if let Some(renderer) = self.renderer() {
            renderer.before_terminal_stop(&options);
        }
        self.inner.terminal.borrow_mut().show_cursor();
        self.inner.terminal.borrow_mut().stop();
    }

    pub fn add_input_listener(&self, listener: InputListener) -> usize {
        let id = self.inner.next_listener_id.get();
        self.inner.next_listener_id.set(id + 1);
        self.inner.input_listeners.borrow_mut().push((id, listener));
        id
    }

    pub fn remove_input_listener(&self, id: usize) {
        self.inner
            .input_listeners
            .borrow_mut()
            .retain(|(listener_id, _)| *listener_id != id);
    }

    pub fn on_terminal_color_scheme_change(&self, listener: ColorSchemeListener) -> usize {
        let id = self.inner.next_listener_id.get();
        self.inner.next_listener_id.set(id + 1);
        self.inner
            .terminal_color_scheme_listeners
            .borrow_mut()
            .push((id, listener));
        id
    }

    pub fn remove_terminal_color_scheme_listener(&self, id: usize) {
        self.inner
            .terminal_color_scheme_listeners
            .borrow_mut()
            .retain(|(listener_id, _)| *listener_id != id);
    }

    pub fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        if self.inner.terminal_color_scheme_notifications_enabled.get() == enabled {
            return;
        }
        self.inner.terminal_color_scheme_notifications_enabled.set(enabled);
        if !self.inner.stopped.get() {
            self.inner
                .terminal
                .borrow_mut()
                .write(if enabled { "\x1b[?2031h" } else { "\x1b[?2031l" });
        }
    }

    fn query_cell_size(&self) {
        if get_capabilities().images.is_none() {
            return;
        }
        self.inner.terminal.borrow_mut().write("\x1b[16t");
    }

    pub fn query_terminal_background_color(&self, timeout: Duration) -> TerminalBackgroundQuery {
        let state = Rc::new(RefCell::new(Osc11QueryState {
            settled: false,
            result: None,
            deadline: Instant::now() + timeout,
        }));
        self.inner
            .pending_osc11_queries
            .borrow_mut()
            .push_back(Rc::clone(&state));
        self.inner
            .pending_osc11_replies
            .set(self.inner.pending_osc11_replies.get() + 1);
        self.inner.terminal.borrow_mut().write("\x1b]11;?\x07");
        TerminalBackgroundQuery { state }
    }

    pub fn query_terminal_color_scheme(&self, timeout: Duration) -> TerminalColorSchemeQuery {
        let state = Rc::new(RefCell::new(ColorSchemeQueryState {
            settled: false,
            result: None,
            deadline: Instant::now() + timeout,
            listener_id: None,
        }));
        let state_clone = Rc::clone(&state);
        let listener_id = self.on_terminal_color_scheme_change(Rc::new(move |scheme| {
            let mut query = state_clone.borrow_mut();
            if query.settled {
                return;
            }
            query.settled = true;
            query.result = Some(scheme);
        }));
        state.borrow_mut().listener_id = Some(listener_id);
        self.inner
            .pending_color_scheme_queries
            .borrow_mut()
            .push(Rc::clone(&state));
        self.inner.terminal.borrow_mut().write("\x1b[?996n");
        TerminalColorSchemeQuery { state }
    }

    pub fn request_render(&self, force: bool) {
        self.inner
            .force_render_requested
            .set(self.inner.force_render_requested.get() || force);
        if !self.inner.terminal_focused.get() {
            self.inner.render_requested.set(true);
            return;
        }
        if force {
            if let Some(renderer) = self.renderer() {
                renderer.reset_render_state();
            }
            self.request_immediate_render();
            return;
        }
        if self.inner.render_requested.get() {
            return;
        }
        self.inner.render_requested.set(true);
        self.schedule_render();
    }

    fn request_immediate_render(&self) {
        self.cancel_render_timer();
        self.inner.render_requested.set(true);
        if self.inner.immediate_render_scheduled.get() {
            return;
        }
        self.inner.immediate_render_scheduled.set(true);
    }

    fn cancel_render_timer(&self) {
        self.inner.render_timer_scheduled.set(false);
    }

    fn schedule_render(&self) {
        if self.inner.stopped.get()
            || !self.inner.terminal_focused.get()
            || self.inner.render_timer_scheduled.get()
            || !self.inner.render_requested.get()
        {
            return;
        }
        self.inner.render_timer_scheduled.set(true);
    }

    fn render_pending_state(&self) {
        if self.inner.stopped.get() || !self.inner.terminal_focused.get() || !self.inner.render_requested.get() {
            return;
        }
        if self.inner.force_render_requested.get() {
            if let Some(renderer) = self.renderer() {
                renderer.reset_render_state();
            }
            self.inner.force_render_requested.set(false);
        }
        self.inner.render_requested.set(false);
        if let Some(renderer) = self.renderer() {
            renderer.do_render();
        }
    }

    pub fn render_now(&self, force: bool) {
        if force && let Some(renderer) = self.renderer() {
            renderer.reset_render_state();
        }
        self.inner.render_requested.set(false);
        self.cancel_render_timer();
        if let Some(renderer) = self.renderer() {
            renderer.do_render();
        }
    }

    /// Drain queued terminal events and dispatch them to listeners/components.
    pub fn process_events(&self) {
        let events = self.inner.terminal.borrow_mut().poll_events();
        for event in events {
            match event {
                TerminalEvent::Input(data) => self.handle_terminal_input(&data),
                TerminalEvent::Resize => self.request_render(false),
            }
        }
        self.settle_terminal_queries();
    }

    /// Run all pending renders (the deterministic replacement for nextTick timers).
    pub fn flush_pending(&self) {
        self.process_events();
        loop {
            if self.inner.immediate_render_scheduled.get() {
                self.inner.immediate_render_scheduled.set(false);
                if !self.inner.stopped.get() && self.inner.render_requested.get() {
                    self.cancel_render_timer();
                    self.render_pending_state();
                }
            } else if self.inner.render_timer_scheduled.get() {
                self.inner.render_timer_scheduled.set(false);
                self.render_pending_state();
                if self.inner.render_requested.get() {
                    self.schedule_render();
                }
            } else {
                break;
            }
        }
    }

    fn settle_terminal_queries(&self) {
        let now = Instant::now();
        loop {
            let expired = {
                let mut queries = self.inner.pending_osc11_queries.borrow_mut();
                match queries.front() {
                    Some(query) if now >= query.borrow().deadline => queries.pop_front(),
                    _ => None,
                }
            };
            let Some(query) = expired else {
                break;
            };
            let mut query = query.borrow_mut();
            if !query.settled {
                query.settled = true;
                query.result = Some(None);
            }
            self.inner
                .pending_osc11_replies
                .set(self.inner.pending_osc11_replies.get().saturating_sub(1));
        }
        let now = Instant::now();
        let expired: Vec<Rc<RefCell<ColorSchemeQueryState>>> = {
            let mut queries = self.inner.pending_color_scheme_queries.borrow_mut();
            let mut expired = Vec::new();
            let mut index = 0;
            while index < queries.len() {
                let is_expired = {
                    let query = queries[index].borrow();
                    !query.settled && now >= query.deadline
                };
                if is_expired {
                    expired.push(queries.remove(index));
                } else {
                    index += 1;
                }
            }
            expired
        };
        for query in expired {
            let listener_id = {
                let mut query = query.borrow_mut();
                query.settled = true;
                query.listener_id.take()
            };
            if let Some(listener_id) = listener_id {
                self.remove_terminal_color_scheme_listener(listener_id);
            }
        }
    }

    fn prune_settled_color_scheme_queries(&self) {
        let settled: Vec<Rc<RefCell<ColorSchemeQueryState>>> = {
            let mut queries = self.inner.pending_color_scheme_queries.borrow_mut();
            let mut settled = Vec::new();
            let mut index = 0;
            while index < queries.len() {
                if queries[index].borrow().settled {
                    settled.push(queries.remove(index));
                } else {
                    index += 1;
                }
            }
            settled
        };
        for query in settled {
            let listener_id = query.borrow_mut().listener_id.take();
            if let Some(listener_id) = listener_id {
                self.remove_terminal_color_scheme_listener(listener_id);
            }
        }
    }

    fn consume_terminal_focus_report(&self, data: &str) -> bool {
        if self.renderer().map(|renderer| renderer.mode()) != Some(TuiMode::Regular) {
            return false;
        }
        if data == TERMINAL_FOCUS_OUT {
            self.inner.terminal_focused.set(false);
            self.cancel_render_timer();
            return true;
        }
        if data != TERMINAL_FOCUS_IN {
            return false;
        }
        if !self.inner.terminal_focused.get() {
            self.inner.terminal_focused.set(true);
            if self.inner.render_requested.get() {
                let force = self.inner.force_render_requested.get();
                self.inner.render_requested.set(false);
                self.request_render(force);
            }
        }
        true
    }

    fn consume_osc11_background_response(&self, data: &str) -> bool {
        if self.inner.pending_osc11_replies.get() == 0 {
            return false;
        }
        if !is_osc11_background_color_response(data) {
            return false;
        }
        let rgb = parse_osc11_background_color(data);
        self.inner
            .pending_osc11_replies
            .set(self.inner.pending_osc11_replies.get() - 1);
        let query = self.inner.pending_osc11_queries.borrow_mut().pop_front();
        if let Some(query) = query {
            let mut query = query.borrow_mut();
            if !query.settled {
                query.settled = true;
                query.result = Some(rgb);
            }
        }
        true
    }

    fn consume_terminal_color_scheme_report(&self, data: &str) -> bool {
        let Some(scheme) = parse_terminal_color_scheme_report(data) else {
            return false;
        };
        let listeners = self.inner.terminal_color_scheme_listeners.borrow().clone();
        for (_, listener) in listeners {
            listener(scheme);
        }
        self.prune_settled_color_scheme_queries();
        true
    }

    fn consume_cell_size_response(&self, data: &str) -> bool {
        let Some(body) = data.strip_prefix("\x1b[6;").and_then(|rest| rest.strip_suffix('t')) else {
            return false;
        };
        let mut parts = body.split(';');
        let (Some(height), Some(width), None) = (parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        let (Ok(height_px), Ok(width_px)) = (height.parse::<u32>(), width.parse::<u32>()) else {
            return false;
        };
        if height_px == 0 || width_px == 0 {
            return true;
        }
        set_cell_dimensions(crate::terminal_image::CellDimensions { width_px, height_px });
        self.invalidate_all();
        self.request_render(false);
        true
    }

    fn handle_terminal_input(&self, data: &str) {
        if self.consume_terminal_focus_report(data) {
            return;
        }
        if self.consume_osc11_background_response(data) {
            return;
        }
        if self.consume_terminal_color_scheme_report(data) {
            return;
        }

        let listeners = self.inner.input_listeners.borrow().clone();
        let mut current = data.to_string();
        if !listeners.is_empty() {
            for (_, listener) in listeners {
                let result = listener(&current);
                if result.consume {
                    return;
                }
                if let Some(rewritten) = result.data {
                    current = rewritten;
                }
            }
            if current.is_empty() {
                return;
            }
        }
        let data = current.as_str();

        if self.consume_cell_size_response(data) {
            return;
        }

        let on_debug = self.inner.on_debug.borrow().clone();
        if let Some(on_debug) = on_debug
            && matches_key(data, "shift+ctrl+d")
            && !is_key_release(data)
            && !is_key_repeat(data)
        {
            on_debug();
            return;
        }

        let focused_overlay = self
            .inner
            .overlay_stack
            .borrow()
            .iter()
            .find(|entry| {
                self.inner
                    .focused_component
                    .borrow()
                    .as_ref()
                    .is_some_and(|focused| same_component(&entry.borrow().component, focused))
            })
            .cloned();
        if let Some(entry) = focused_overlay
            && !self.is_overlay_visible(&entry)
        {
            if let Some(top) = self.get_topmost_visible_overlay() {
                let component = top.borrow().component.clone();
                self.set_focus(Some(component));
            } else {
                let pre_focus = entry.borrow().pre_focus.clone();
                self.set_focus_internal(pre_focus, OverlayFocusRestorePolicy::Preserve);
            }
        }

        let focus_is_overlay = {
            let focused = self.inner.focused_component.borrow();
            focused.as_ref().is_some_and(|focused| {
                self.inner
                    .overlay_stack
                    .borrow()
                    .iter()
                    .any(|entry| same_component(&entry.borrow().component, focused))
            })
        };
        if !focus_is_overlay {
            match self.get_visible_overlay_focus_restore() {
                OverlayFocusRestoreState::Eligible { overlay } => {
                    let component = overlay.borrow().component.clone();
                    self.set_focus(Some(component));
                }
                OverlayFocusRestoreState::Blocked {
                    overlay,
                    blocked_by,
                    resume,
                } => {
                    let focused = self.get_focused_component();
                    if !focused
                        .as_ref()
                        .is_some_and(|focused| same_component(focused, &blocked_by))
                    {
                        match resume {
                            OverlayBlockedFocusResume::RestoreOverlay => {
                                let component = overlay.borrow().component.clone();
                                self.set_focus(Some(component));
                            }
                            OverlayBlockedFocusResume::FocusTarget(target) => {
                                self.clear_overlay_focus_restore();
                                self.set_focus(target);
                            }
                        }
                    }
                }
                OverlayFocusRestoreState::Inactive => {}
            }
        }

        let focused = self.inner.focused_component.borrow().clone();
        if let Some(focused) = focused {
            let wants_input = focused.borrow().wants_input();
            if wants_input {
                let wants_release = focused.borrow().wants_key_release();
                if is_key_release(data) && !wants_release {
                    return;
                }
                focused.borrow_mut().handle_input(data);
                self.apply_deferred_focus_updates();
                self.request_immediate_render();
            }
        }
    }

    pub fn set_focus(&self, component: Option<ComponentRef>) {
        self.set_focus_internal(component, OverlayFocusRestorePolicy::Clear);
    }

    fn set_focus_internal(&self, component: Option<ComponentRef>, overlay_focus_restore: OverlayFocusRestorePolicy) {
        let previous_focus = self.inner.focused_component.borrow().clone();
        let mut next_focus = component;
        let previous_focused_overlay = previous_focus.as_ref().and_then(|previous| {
            self.inner
                .overlay_stack
                .borrow()
                .iter()
                .find(|entry| same_component(&entry.borrow().component, previous) && self.is_overlay_visible(entry))
                .cloned()
        });
        let next_focus_is_overlay = next_focus.as_ref().is_some_and(|next| {
            self.inner
                .overlay_stack
                .borrow()
                .iter()
                .any(|entry| same_component(&entry.borrow().component, next))
        });
        let restore_state = self.get_visible_overlay_focus_restore();
        if let Some(next) = next_focus.clone() {
            if !next_focus_is_overlay {
                if let OverlayFocusRestoreState::Blocked {
                    overlay,
                    blocked_by,
                    resume,
                } = &restore_state
                {
                    let blocked_by_is_previous = previous_focus
                        .as_ref()
                        .is_some_and(|previous| same_component(previous, blocked_by));
                    if blocked_by_is_previous {
                        let resume_target = matches!(resume, OverlayBlockedFocusResume::FocusTarget(_));
                        if resume_target || !self.is_component_mounted(blocked_by) {
                            next_focus = self.resolve_blocked_overlay_focus_resume(overlay, resume);
                        } else {
                            *self.inner.overlay_focus_restore.borrow_mut() = OverlayFocusRestoreState::Blocked {
                                overlay: Rc::clone(overlay),
                                blocked_by: next,
                                resume: resume.clone(),
                            };
                        }
                    }
                } else if let Some(previous_overlay) = &previous_focused_overlay
                    && !matches!(restore_state, OverlayFocusRestoreState::Inactive)
                {
                    let restore_overlay = match &restore_state {
                        OverlayFocusRestoreState::Eligible { overlay }
                        | OverlayFocusRestoreState::Blocked { overlay, .. } => Rc::clone(overlay),
                        OverlayFocusRestoreState::Inactive => unreachable!(),
                    };
                    if Rc::ptr_eq(&restore_overlay, previous_overlay)
                        && !self.is_overlay_focus_ancestor(previous_overlay, &next)
                    {
                        *self.inner.overlay_focus_restore.borrow_mut() = OverlayFocusRestoreState::Blocked {
                            overlay: Rc::clone(previous_overlay),
                            blocked_by: next,
                            resume: OverlayBlockedFocusResume::RestoreOverlay,
                        };
                    }
                }
            }
        } else if let OverlayFocusRestoreState::Blocked {
            overlay,
            blocked_by,
            resume,
        } = &restore_state
        {
            let blocked_by_is_previous = previous_focus
                .as_ref()
                .is_some_and(|previous| same_component(previous, blocked_by));
            if blocked_by_is_previous {
                next_focus = self.resolve_blocked_overlay_focus_resume(overlay, resume);
            } else if overlay_focus_restore == OverlayFocusRestorePolicy::Clear {
                self.clear_overlay_focus_restore();
            }
        } else if overlay_focus_restore == OverlayFocusRestorePolicy::Clear {
            self.clear_overlay_focus_restore();
        }

        if let Some(previous) = &previous_focus
            && !set_component_focused_now(previous, false)
        {
            self.inner
                .deferred_focus_updates
                .borrow_mut()
                .push((Rc::clone(previous), false));
        }
        if let Some(next) = &next_focus
            && !set_component_focused_now(next, true)
        {
            self.inner
                .deferred_focus_updates
                .borrow_mut()
                .push((Rc::clone(next), true));
        }
        *self.inner.focused_component.borrow_mut() = next_focus.clone();

        let focused_overlay = next_focus.as_ref().and_then(|next| {
            self.inner
                .overlay_stack
                .borrow()
                .iter()
                .find(|entry| same_component(&entry.borrow().component, next) && self.is_overlay_visible(entry))
                .cloned()
        });
        if let Some(overlay) = focused_overlay {
            *self.inner.overlay_focus_restore.borrow_mut() = OverlayFocusRestoreState::Eligible { overlay };
        }
    }

    fn apply_deferred_focus_updates(&self) {
        let updates = std::mem::take(&mut *self.inner.deferred_focus_updates.borrow_mut());
        for (component, focused) in updates {
            if !set_component_focused_now(&component, focused) {
                self.inner
                    .deferred_focus_updates
                    .borrow_mut()
                    .push((component, focused));
            }
        }
    }

    fn clear_overlay_focus_restore(&self) {
        *self.inner.overlay_focus_restore.borrow_mut() = OverlayFocusRestoreState::Inactive;
    }

    fn clear_overlay_focus_restore_for(&self, overlay: &OverlayEntryRef) {
        let matches = match &*self.inner.overlay_focus_restore.borrow() {
            OverlayFocusRestoreState::Inactive => false,
            OverlayFocusRestoreState::Eligible { overlay: current }
            | OverlayFocusRestoreState::Blocked { overlay: current, .. } => Rc::ptr_eq(current, overlay),
        };
        if matches {
            self.clear_overlay_focus_restore();
        }
    }

    fn resolve_blocked_overlay_focus_resume(
        &self,
        overlay: &OverlayEntryRef,
        resume: &OverlayBlockedFocusResume,
    ) -> Option<ComponentRef> {
        match resume {
            OverlayBlockedFocusResume::RestoreOverlay => Some(overlay.borrow().component.clone()),
            OverlayBlockedFocusResume::FocusTarget(target) => {
                self.clear_overlay_focus_restore();
                target.clone()
            }
        }
    }

    fn get_visible_overlay_focus_restore(&self) -> OverlayFocusRestoreState {
        let restore_state = self.inner.overlay_focus_restore.borrow().clone();
        match &restore_state {
            OverlayFocusRestoreState::Inactive => restore_state,
            OverlayFocusRestoreState::Eligible { overlay } | OverlayFocusRestoreState::Blocked { overlay, .. } => {
                let stack = self.inner.overlay_stack.borrow();
                let present = stack.iter().any(|entry| Rc::ptr_eq(entry, overlay));
                let visible = present && self.is_overlay_visible(overlay);
                drop(stack);
                if visible {
                    restore_state
                } else {
                    OverlayFocusRestoreState::Inactive
                }
            }
        }
    }

    fn is_overlay_focus_ancestor(&self, entry: &OverlayEntryRef, component: &ComponentRef) -> bool {
        let mut visited: Vec<ComponentRef> = Vec::new();
        let mut current = entry.borrow().pre_focus.clone();
        while let Some(candidate) = current {
            if visited
                .iter()
                .any(|visited_component| same_component(visited_component, &candidate))
            {
                break;
            }
            if same_component(&candidate, component) {
                return true;
            }
            visited.push(candidate.clone());
            current = self
                .inner
                .overlay_stack
                .borrow()
                .iter()
                .find(|overlay| same_component(&overlay.borrow().component, &candidate))
                .and_then(|overlay| overlay.borrow().pre_focus.clone());
        }
        false
    }

    fn retarget_overlay_pre_focus(&self, removed: &OverlayEntryRef) {
        let removed_component = removed.borrow().component.clone();
        let removed_pre_focus = removed.borrow().pre_focus.clone();
        for overlay in self.inner.overlay_stack.borrow().iter() {
            if Rc::ptr_eq(overlay, removed) {
                continue;
            }
            let mut entry = overlay.borrow_mut();
            let matches = entry
                .pre_focus
                .as_ref()
                .is_some_and(|pre| same_component(pre, &removed_component));
            if matches {
                entry.pre_focus = removed_pre_focus.clone();
            }
        }
    }

    fn is_component_mounted(&self, component: &ComponentRef) -> bool {
        self.inner
            .children
            .borrow()
            .iter()
            .any(|root| contains_component(root, component))
    }

    fn is_overlay_visible(&self, entry: &OverlayEntryRef) -> bool {
        let entry = entry.borrow();
        if entry.hidden {
            return false;
        }
        if let Some(options) = &entry.options
            && let Some(visible) = &options.visible
        {
            return visible(
                self.inner.terminal.borrow().columns(),
                self.inner.terminal.borrow().rows(),
            );
        }
        true
    }

    fn get_topmost_visible_overlay(&self) -> Option<OverlayEntryRef> {
        let stack = self.inner.overlay_stack.borrow().clone();
        let mut topmost: Option<OverlayEntryRef> = None;
        for overlay in stack {
            let (non_capturing, focus_order) = {
                let entry = overlay.borrow();
                (
                    entry.options.as_ref().is_some_and(|options| options.non_capturing),
                    entry.focus_order,
                )
            };
            if non_capturing || !self.is_overlay_visible(&overlay) {
                continue;
            }
            let replace = match &topmost {
                Some(current) => focus_order > current.borrow().focus_order,
                None => true,
            };
            if replace {
                topmost = Some(overlay);
            }
        }
        topmost
    }

    pub fn show_overlay(&self, component: ComponentRef, options: Option<OverlayOptions>) -> OverlayHandle {
        self.inner
            .focus_order_counter
            .set(self.inner.focus_order_counter.get() + 1);
        let entry = Rc::new(RefCell::new(OverlayEntry {
            component: component.clone(),
            options: options.clone(),
            pre_focus: self.get_focused_component(),
            hidden: false,
            focus_order: self.inner.focus_order_counter.get(),
        }));
        self.inner.overlay_stack.borrow_mut().push(Rc::clone(&entry));
        let non_capturing = options.as_ref().is_some_and(|options| options.non_capturing);
        if !non_capturing && self.is_overlay_visible(&entry) {
            self.set_focus(Some(component));
        }
        self.inner.terminal.borrow_mut().hide_cursor();
        self.request_render(false);
        OverlayHandle {
            base: self.clone(),
            entry,
        }
    }

    pub fn hide_overlay(&self) {
        let overlay = self.inner.overlay_stack.borrow().last().cloned();
        let Some(overlay) = overlay else {
            return;
        };
        self.clear_overlay_focus_restore_for(&overlay);
        self.retarget_overlay_pre_focus(&overlay);
        self.inner.overlay_stack.borrow_mut().pop();
        let overlay_component = overlay.borrow().component.clone();
        let focused = self.get_focused_component();
        if focused
            .as_ref()
            .is_some_and(|focused| same_component(focused, &overlay_component))
        {
            let fallback = self
                .get_topmost_visible_overlay()
                .map(|top| top.borrow().component.clone())
                .or_else(|| overlay.borrow().pre_focus.clone());
            self.set_focus(fallback);
        }
        if self.inner.overlay_stack.borrow().is_empty() {
            self.inner.terminal.borrow_mut().hide_cursor();
        }
        self.request_render(false);
    }

    fn remove_overlay(&self, entry: &OverlayEntryRef) {
        let index = self
            .inner
            .overlay_stack
            .borrow()
            .iter()
            .position(|candidate| Rc::ptr_eq(candidate, entry));
        let Some(index) = index else {
            return;
        };
        self.clear_overlay_focus_restore_for(entry);
        self.retarget_overlay_pre_focus(entry);
        self.inner.overlay_stack.borrow_mut().remove(index);
        let component = entry.borrow().component.clone();
        let focused = self.get_focused_component();
        if focused
            .as_ref()
            .is_some_and(|focused| same_component(focused, &component))
        {
            let fallback = self
                .get_topmost_visible_overlay()
                .map(|top| top.borrow().component.clone())
                .or_else(|| entry.borrow().pre_focus.clone());
            self.set_focus(fallback);
        }
        if self.inner.overlay_stack.borrow().is_empty() {
            self.inner.terminal.borrow_mut().hide_cursor();
        }
        self.request_render(false);
    }

    fn set_overlay_hidden(&self, entry: &OverlayEntryRef, hidden: bool) {
        if entry.borrow().hidden == hidden {
            return;
        }
        entry.borrow_mut().hidden = hidden;
        let component = entry.borrow().component.clone();
        if hidden {
            self.clear_overlay_focus_restore_for(entry);
            let focused = self.get_focused_component();
            if focused
                .as_ref()
                .is_some_and(|focused| same_component(focused, &component))
            {
                let fallback = self
                    .get_topmost_visible_overlay()
                    .map(|top| top.borrow().component.clone())
                    .or_else(|| entry.borrow().pre_focus.clone());
                self.set_focus(fallback);
            }
        } else {
            let non_capturing = entry
                .borrow()
                .options
                .as_ref()
                .is_some_and(|options| options.non_capturing);
            if !non_capturing && self.is_overlay_visible(entry) {
                self.inner
                    .focus_order_counter
                    .set(self.inner.focus_order_counter.get() + 1);
                entry.borrow_mut().focus_order = self.inner.focus_order_counter.get();
                self.set_focus(Some(component));
            }
        }
        self.request_render(false);
    }

    fn focus_overlay(&self, entry: &OverlayEntryRef) {
        let present = self
            .inner
            .overlay_stack
            .borrow()
            .iter()
            .any(|candidate| Rc::ptr_eq(candidate, entry));
        if !present || !self.is_overlay_visible(entry) {
            return;
        }
        self.inner
            .focus_order_counter
            .set(self.inner.focus_order_counter.get() + 1);
        entry.borrow_mut().focus_order = self.inner.focus_order_counter.get();
        let component = entry.borrow().component.clone();
        self.set_focus(Some(component));
        self.request_render(false);
    }

    fn unfocus_overlay(&self, entry: &OverlayEntryRef, target: Option<Option<ComponentRef>>) {
        let component = entry.borrow().component.clone();
        let focused = self.get_focused_component();
        let is_focused = focused
            .as_ref()
            .is_some_and(|focused| same_component(focused, &component));
        let restore_state = self.inner.overlay_focus_restore.borrow().clone();
        let has_pending_restore = match &restore_state {
            OverlayFocusRestoreState::Inactive => false,
            OverlayFocusRestoreState::Eligible { overlay } | OverlayFocusRestoreState::Blocked { overlay, .. } => {
                Rc::ptr_eq(overlay, entry)
            }
        };
        if !is_focused && !has_pending_restore {
            return;
        }
        if let OverlayFocusRestoreState::Blocked {
            overlay, blocked_by, ..
        } = &restore_state
        {
            let blocked_matches = Rc::ptr_eq(overlay, entry)
                && focused
                    .as_ref()
                    .is_some_and(|focused| same_component(focused, blocked_by));
            if blocked_matches {
                if let Some(target) = target {
                    *self.inner.overlay_focus_restore.borrow_mut() = OverlayFocusRestoreState::Blocked {
                        overlay: Rc::clone(entry),
                        blocked_by: Rc::clone(blocked_by),
                        resume: OverlayBlockedFocusResume::FocusTarget(target),
                    };
                } else {
                    self.clear_overlay_focus_restore();
                }
                self.request_render(false);
                return;
            }
        }
        self.clear_overlay_focus_restore_for(entry);
        if is_focused || target.is_some() {
            let top_visible = self.get_topmost_visible_overlay();
            let fallback = top_visible
                .filter(|top| !Rc::ptr_eq(top, entry))
                .map(|top| top.borrow().component.clone())
                .or_else(|| entry.borrow().pre_focus.clone());
            self.set_focus(target.unwrap_or(fallback));
        }
        self.request_render(false);
    }

    fn is_overlay_focused(&self, entry: &OverlayEntryRef) -> bool {
        let component = entry.borrow().component.clone();
        self.get_focused_component()
            .as_ref()
            .is_some_and(|focused| same_component(focused, &component))
    }

    pub fn invalidate_all(&self) {
        let children = self.children();
        for child in children {
            child.borrow_mut().invalidate();
        }
        let overlays = self.inner.overlay_stack.borrow().clone();
        for overlay in overlays {
            let component = overlay.borrow().component.clone();
            component.borrow_mut().invalidate();
        }
    }

    pub(crate) fn render_children(&self, width: usize) -> Vec<String> {
        let children = self.children();
        let mut lines = Vec::new();
        for child in children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    fn render_generation(&self) -> u64 {
        self.inner.render_generation.get()
    }

    fn advance_render_generation(&self) -> u64 {
        self.inner.render_generation.set(self.inner.render_generation.get() + 1);
        self.inner.render_generation.get()
    }

    fn is_rendering(&self) -> bool {
        self.inner.rendering.get()
    }

    fn set_rendering(&self, rendering: bool) {
        self.inner.rendering.set(rendering);
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.inner.stopped.get()
    }

    fn is_terminal_focused(&self) -> bool {
        self.inner.terminal_focused.get()
    }

    fn is_render_requested(&self) -> bool {
        self.inner.render_requested.get()
    }

    fn increment_full_redraws(&self) {
        self.inner.full_redraw_count.set(self.inner.full_redraw_count.get() + 1);
    }

    fn log_directory(&self) -> &PathBuf {
        &self.inner.log_directory
    }

    fn fixed_cells(&self) -> Vec<Rc<RefCell<FixedCellState>>> {
        self.inner.fixed_cells.borrow().clone()
    }

    pub(crate) fn register_fixed_cell(&self, state: Rc<RefCell<FixedCellState>>) {
        self.inner.fixed_cells.borrow_mut().push(state);
    }

    fn unregister_fixed_cell(&self, state: &Rc<RefCell<FixedCellState>>) {
        let pointer = Rc::as_ptr(state);
        self.inner
            .fixed_cells
            .borrow_mut()
            .retain(|candidate| Rc::as_ptr(candidate) != pointer);
    }

    fn region_state_for(&self, component: &ComponentRef) -> Option<RenderRegionStateRef> {
        let id = component_id(component);
        self.inner
            .component_render_regions
            .borrow()
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, state)| Rc::clone(state))
    }

    fn set_region_state(&self, component: &ComponentRef, state: RenderRegionStateRef) {
        let id = component_id(component);
        let mut regions = self.inner.component_render_regions.borrow_mut();
        if let Some((_, existing)) = regions.iter_mut().find(|(candidate, _)| *candidate == id) {
            *existing = state;
        } else {
            regions.push((id, state));
        }
    }

    fn remove_region_state(&self, component: &ComponentRef, state: &RenderRegionStateRef) {
        let id = component_id(component);
        let pointer = Rc::as_ptr(state);
        self.inner
            .component_render_regions
            .borrow_mut()
            .retain(|(candidate, candidate_state)| *candidate != id || Rc::as_ptr(candidate_state) != pointer);
    }

    fn region_states(&self) -> Vec<RenderRegionStateRef> {
        let mut states: Vec<RenderRegionStateRef> = Vec::new();
        for (_, state) in self.inner.component_render_regions.borrow().iter() {
            if !states.iter().any(|existing| Rc::ptr_eq(existing, state)) {
                states.push(Rc::clone(state));
            }
        }
        states
    }

    pub(crate) fn composite_overlays(&self, lines: Vec<String>, term_width: usize, term_height: usize) -> Vec<String> {
        if self.inner.overlay_stack.borrow().is_empty() {
            return lines;
        }
        let mut result = lines;
        let mut rendered: Vec<(Vec<String>, i64, i64, usize)> = Vec::new();
        let mut min_lines_needed = result.len();
        let mut visible_entries: Vec<OverlayEntryRef> = self
            .inner
            .overlay_stack
            .borrow()
            .iter()
            .filter(|entry| self.is_overlay_visible(entry))
            .cloned()
            .collect();
        visible_entries.sort_by_key(|entry| entry.borrow().focus_order);
        for entry in visible_entries {
            let (component, options) = {
                let entry = entry.borrow();
                (entry.component.clone(), entry.options.clone())
            };
            let first = self.resolve_overlay_layout(options.as_ref(), 0, term_width, term_height);
            let mut overlay_lines = component.borrow_mut().render(first.width);
            if let Some(max_height) = first.max_height
                && overlay_lines.len() > max_height
            {
                overlay_lines.truncate(max_height);
            }
            let layout = self.resolve_overlay_layout(options.as_ref(), overlay_lines.len(), term_width, term_height);
            min_lines_needed = min_lines_needed.max((layout.row as usize) + overlay_lines.len());
            rendered.push((overlay_lines, layout.row, layout.col, first.width));
        }
        let working_height = result.len().max(term_height).max(min_lines_needed);
        while result.len() < working_height {
            result.push(String::new());
        }
        let viewport_start = working_height.saturating_sub(term_height);
        for (overlay_lines, row, col, width) in rendered {
            for (index, line) in overlay_lines.into_iter().enumerate() {
                let target = viewport_start as i64 + row + index as i64;
                if target < 0 || target as usize >= result.len() {
                    continue;
                }
                let truncated = if visible_width(&line) > width {
                    slice_by_column(&line, 0, width, true)
                } else {
                    line
                };
                let target = target as usize;
                result[target] = composite_tui_line(&result[target], &truncated, col as usize, width, term_width);
            }
        }
        result
    }

    fn resolve_overlay_layout(
        &self,
        options: Option<&OverlayOptions>,
        overlay_height: usize,
        term_width: usize,
        term_height: usize,
    ) -> OverlayLayout {
        let default = OverlayOptions::default();
        let options = options.unwrap_or(&default);
        let margin = options.margin.unwrap_or_default();
        let margin_top = margin.top.unwrap_or(0).max(0);
        let margin_right = margin.right.unwrap_or(0).max(0);
        let margin_bottom = margin.bottom.unwrap_or(0).max(0);
        let margin_left = margin.left.unwrap_or(0).max(0);
        let avail_width = (term_width as i64 - margin_left - margin_right).max(1);
        let avail_height = (term_height as i64 - margin_top - margin_bottom).max(1);

        let mut width = parse_size_value(options.width, term_width).unwrap_or_else(|| 80_f64.min(avail_width as f64));
        if let Some(min_width) = options.min_width {
            width = width.max(min_width as f64);
        }
        width = width.max(1.0).min(avail_width as f64);

        let mut max_height = parse_size_value(options.max_height, term_height);
        if let Some(limit) = max_height {
            max_height = Some(limit.max(1.0).min(avail_height as f64));
        }
        let effective_height = match max_height {
            Some(limit) => (overlay_height as f64).min(limit),
            None => overlay_height as f64,
        };

        let row = match options.row {
            Some(SizeValue::Percent(percent)) => {
                let max_row = (avail_height as f64 - effective_height).max(0.0);
                margin_top + (max_row * percent / 100.0).floor() as i64
            }
            Some(SizeValue::Absolute(value)) => value as i64,
            None => resolve_anchor_row(
                options.anchor.unwrap_or(OverlayAnchor::Center),
                effective_height as i64,
                avail_height,
                margin_top,
            ),
        };
        let col = match options.col {
            Some(SizeValue::Percent(percent)) => {
                let max_col = (avail_width as f64 - width).max(0.0);
                margin_left + (max_col * percent / 100.0).floor() as i64
            }
            Some(SizeValue::Absolute(value)) => value as i64,
            None => resolve_anchor_col(
                options.anchor.unwrap_or(OverlayAnchor::Center),
                width as i64,
                avail_width,
                margin_left,
            ),
        };
        let row = row + options.offset_y.unwrap_or(0);
        let col = col + options.offset_x.unwrap_or(0);
        let row = row
            .max(margin_top)
            .min(term_height as i64 - margin_bottom - effective_height as i64);
        let col = col
            .max(margin_left)
            .min(term_width as i64 - margin_right - width as i64);

        OverlayLayout {
            width: width as usize,
            row,
            col,
            max_height: max_height.map(|limit| limit as usize),
        }
    }

    pub(crate) fn apply_line_resets(&self, lines: &mut [String]) {
        for line in lines.iter_mut() {
            if !is_image_line(line) {
                *line = format!("{}{}", normalize_terminal_output(line), SEGMENT_RESET);
            }
        }
    }

    pub(crate) fn extract_cursor_position(&self, lines: &mut [String], height: usize) -> Option<CursorPosition> {
        let viewport_top = lines.len().saturating_sub(height);
        for row in (viewport_top..lines.len()).rev() {
            let line = &lines[row];
            let Some(marker_index) = line.find(CURSOR_MARKER) else {
                continue;
            };
            let before_marker = &line[..marker_index];
            let col = visible_width(before_marker);
            lines[row] = format!("{}{}", before_marker, &line[marker_index + CURSOR_MARKER.len()..]);
            return Some(CursorPosition { row, col });
        }
        None
    }

    fn apply_fixed_cells(&self, lines: &[String]) -> Vec<String> {
        let cells = self.fixed_cells();
        if cells.is_empty() {
            return lines.to_vec();
        }
        let mut result = lines.to_vec();
        for state in cells {
            let state = state.borrow();
            if state.disposed || state.value.is_empty() {
                continue;
            }
            let (Some(row), Some(col)) = (state.row, state.col) else {
                continue;
            };
            if row < 0 || row as usize >= result.len() {
                continue;
            }
            let row = row as usize;
            if let Some(patched) = patch_fixed_cell_line(&result[row], col, &state.value, false) {
                result[row] = patched;
            }
        }
        result
    }
}

struct OverlayLayout {
    width: usize,
    row: i64,
    col: i64,
    max_height: Option<usize>,
}

fn parse_size_value(value: Option<SizeValue>, reference_size: usize) -> Option<f64> {
    match value? {
        SizeValue::Absolute(value) => Some(value),
        SizeValue::Percent(percent) => Some((reference_size as f64 * percent / 100.0).floor()),
    }
}

fn resolve_anchor_row(anchor: OverlayAnchor, height: i64, avail_height: i64, margin_top: i64) -> i64 {
    match anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::TopCenter | OverlayAnchor::TopRight => margin_top,
        OverlayAnchor::BottomLeft | OverlayAnchor::BottomCenter | OverlayAnchor::BottomRight => {
            margin_top + avail_height - height
        }
        OverlayAnchor::LeftCenter | OverlayAnchor::Center | OverlayAnchor::RightCenter => {
            margin_top + (avail_height - height).div_euclid(2)
        }
    }
}

fn resolve_anchor_col(anchor: OverlayAnchor, width: i64, avail_width: i64, margin_left: i64) -> i64 {
    match anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::LeftCenter | OverlayAnchor::BottomLeft => margin_left,
        OverlayAnchor::TopRight | OverlayAnchor::RightCenter | OverlayAnchor::BottomRight => {
            margin_left + avail_width - width
        }
        OverlayAnchor::TopCenter | OverlayAnchor::Center | OverlayAnchor::BottomCenter => {
            margin_left + (avail_width - width).div_euclid(2)
        }
    }
}

/// Handle returned by [`TuiBase::show_overlay`].
#[derive(Clone)]
pub struct OverlayHandle {
    base: TuiBase,
    entry: OverlayEntryRef,
}

impl OverlayHandle {
    pub fn hide(&self) {
        self.base.remove_overlay(&self.entry);
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.base.set_overlay_hidden(&self.entry, hidden);
    }

    pub fn is_hidden(&self) -> bool {
        self.entry.borrow().hidden
    }

    pub fn focus(&self) {
        self.base.focus_overlay(&self.entry);
    }

    /// Release focus without an explicit target.
    pub fn unfocus(&self) {
        self.base.unfocus_overlay(&self.entry, None);
    }

    /// Release focus to an explicit target (`None` clears focus).
    pub fn unfocus_to(&self, target: Option<ComponentRef>) {
        self.base.unfocus_overlay(&self.entry, Some(target));
    }

    pub fn is_focused(&self) -> bool {
        self.base.is_overlay_focused(&self.entry)
    }
}

/// Composite overlay content into a terminal line at a fixed column.
pub fn composite_tui_line(
    base_line: &str,
    overlay_line: &str,
    start_col: usize,
    overlay_width: usize,
    total_width: usize,
) -> String {
    if is_image_line(base_line) {
        return base_line.to_string();
    }
    let after_start = start_col + overlay_width;
    let base = extract_segments(
        base_line,
        start_col,
        after_start,
        total_width.saturating_sub(after_start),
        true,
    );
    let overlay = slice_with_width(overlay_line, 0, overlay_width, true);
    let before_pad = start_col.saturating_sub(base.before_width);
    let overlay_pad = overlay_width.saturating_sub(overlay.width);
    let actual_before_width = start_col.max(base.before_width);
    let actual_overlay_width = overlay_width.max(overlay.width);
    let after_target = total_width
        .saturating_sub(actual_before_width)
        .saturating_sub(actual_overlay_width);
    let after_pad = after_target.saturating_sub(base.after_width);
    let result = format!(
        "{}{}{}{}{}{}{}{}",
        base.before,
        " ".repeat(before_pad),
        SEGMENT_RESET,
        overlay.text,
        " ".repeat(overlay_pad),
        SEGMENT_RESET,
        base.after,
        " ".repeat(after_pad)
    );
    if visible_width(&result) <= total_width {
        result
    } else {
        slice_by_column(&result, 0, total_width, true)
    }
}

fn contains_component(root: &ComponentRef, target: &ComponentRef) -> bool {
    if same_component(root, target) {
        return true;
    }
    let children = root
        .borrow()
        .as_container()
        .map(|container| container.children().to_vec());
    children.is_some_and(|children| children.iter().any(|child| contains_component(child, target)))
}

fn environment_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1")
}

fn default_log_directory() -> PathBuf {
    if let Ok(directory) = std::env::var("PI_CODING_AGENT_DIR") {
        return PathBuf::from(directory);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pi").join("agent")
}

fn find_upward_viewport_top_after_shrink(
    previous_line_count: usize,
    next_line_count: usize,
    height: usize,
    current_viewport_top: i64,
) -> Option<i64> {
    if next_line_count >= previous_line_count {
        return None;
    }
    let next_viewport_top = (next_line_count as i64 - height as i64).max(0);
    (next_viewport_top < current_viewport_top).then_some(next_viewport_top)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KittyImageHeader {
    ids: Vec<u32>,
    rows: usize,
}

fn parse_kitty_image_header(line: &str) -> Option<KittyImageHeader> {
    let sequence_start = line.find(KITTY_SEQUENCE_PREFIX)?;
    let params_start = sequence_start + KITTY_SEQUENCE_PREFIX.len();
    let params_end = line[params_start..].find(';')? + params_start;
    let mut ids = Vec::new();
    let mut rows = 1;
    for parameter in line[params_start..params_end].split(',') {
        let mut parts = parameter.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let Some(value) = parts.next() else {
            continue;
        };
        let Ok(number) = value.parse::<u64>() else {
            continue;
        };
        if number == 0 || number > 0xffff_ffff {
            continue;
        }
        match key {
            "i" => ids.push(number as u32),
            "r" => rows = number as usize,
            _ => {}
        }
    }
    Some(KittyImageHeader { ids, rows })
}

fn extract_kitty_image_ids(line: &str) -> Vec<u32> {
    parse_kitty_image_header(line)
        .map(|header| header.ids)
        .unwrap_or_default()
}

fn extract_kitty_image_rows(line: &str) -> usize {
    parse_kitty_image_header(line).map(|header| header.rows).unwrap_or(1)
}

fn collect_kitty_image_ids(lines: &[String]) -> Vec<u32> {
    let mut ids = Vec::new();
    for line in lines {
        for id in extract_kitty_image_ids(line) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

fn delete_kitty_images(ids: &[u32]) -> String {
    ids.iter().map(|id| delete_kitty_image(*id)).collect::<String>()
}

fn get_kitty_image_reserved_rows(lines: &[String], index: usize, max_index: usize) -> usize {
    let rows = lines.get(index).map(|line| extract_kitty_image_rows(line)).unwrap_or(1);
    if rows <= 1 {
        return 1;
    }
    let max_rows = rows
        .min(max_index.saturating_sub(index) + 1)
        .min(lines.len().saturating_sub(index));
    let mut reserved_rows = 1;
    while reserved_rows < max_rows {
        let line = lines.get(index + reserved_rows).map(String::as_str).unwrap_or("");
        if is_image_line(line) || visible_width(line) > 0 {
            break;
        }
        reserved_rows += 1;
    }
    reserved_rows
}

fn expand_changed_range_for_kitty_images(
    first_changed: usize,
    last_changed: usize,
    previous_lines: &[String],
    new_lines: &[String],
) -> (usize, usize) {
    let mut expanded_first = first_changed;
    let mut expanded_last = last_changed;
    for lines in [previous_lines, new_lines] {
        for (index, line) in lines.iter().enumerate() {
            if extract_kitty_image_ids(line).is_empty() {
                continue;
            }
            let block_end = index + get_kitty_image_reserved_rows(lines, index, lines.len() - 1) - 1;
            if index >= first_changed || (index <= last_changed && block_end >= first_changed) {
                expanded_first = expanded_first.min(index);
                expanded_last = expanded_last.max(block_end);
            }
        }
    }
    (expanded_first, expanded_last)
}

fn delete_changed_kitty_images(first_changed: usize, last_changed: usize, previous_lines: &[String]) -> String {
    if last_changed < first_changed {
        return String::new();
    }
    let mut ids = Vec::new();
    let max_line = last_changed.min(previous_lines.len().saturating_sub(1));
    for line in &previous_lines[first_changed..=max_line] {
        for id in extract_kitty_image_ids(line) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    delete_kitty_images(&ids)
}

fn patch_fixed_cell_line(line: &str, col: i64, value: &str, has_line_reset: bool) -> Option<String> {
    if is_image_line(line) {
        return None;
    }
    let raw_line = if has_line_reset && line.ends_with(SEGMENT_RESET) {
        &line[..line.len() - SEGMENT_RESET.len()]
    } else {
        line
    };
    let line_width = visible_width(raw_line) as i64;
    if col < 0 || col >= line_width {
        return None;
    }
    let cell = slice_with_width(value, 0, 1, true);
    if cell.width != 1 {
        return None;
    }
    let after_length = (line_width - col - 1) as usize;
    let base = extract_segments(raw_line, col as usize, col as usize + 1, after_length, true);
    if base.before_width as i64 != col || base.after_width != after_length {
        return None;
    }
    let patched = format!(
        "{}{}{}{}{}",
        base.before, SEGMENT_RESET, cell.text, SEGMENT_RESET, base.after
    );
    Some(if has_line_reset {
        format!("{patched}{SEGMENT_RESET}")
    } else {
        patched
    })
}

/// Render-frame state shared by both renderer variants.
#[derive(Debug, Default)]
pub(crate) struct RenderFrameState {
    pub(crate) previous_lines: Vec<String>,
    pub(crate) previous_kitty_image_ids: Vec<u32>,
    pub(crate) previous_width: i64,
    pub(crate) previous_height: i64,
    pub(crate) cursor_row: i64,
    pub(crate) hardware_cursor_row: i64,
    pub(crate) max_lines_rendered: usize,
    pub(crate) previous_viewport_top: i64,
}

impl RenderFrameState {
    fn reset_for_renderer(&mut self) {
        self.previous_lines.clear();
        self.previous_kitty_image_ids.clear();
        self.previous_width = -1;
        self.previous_height = -1;
        self.cursor_row = 0;
        self.hardware_cursor_row = 0;
        self.max_lines_rendered = 0;
        self.previous_viewport_top = 0;
    }

    pub(crate) fn reset_for_main_screen(&mut self) {
        self.previous_lines.clear();
        self.previous_width = -1;
        self.previous_height = -1;
        self.cursor_row = 0;
        self.hardware_cursor_row = 0;
        self.max_lines_rendered = 0;
        self.previous_viewport_top = 0;
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CommitVariant {
    pub(crate) check_upward_shrink: bool,
    pub(crate) check_deleted_viewport: bool,
}

fn log_redraw(base: &TuiBase, reason: &str, previous: usize, new: usize, height: usize) {
    if !base.debug_redraw() {
        return;
    }
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let message = format!("[{seconds}] fullRender: {reason} (prev={previous}, new={new}, height={height})\n");
    let path = base.log_directory().join("pi-debug.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(message.as_bytes());
    }
}

fn write_crash_log(base: &TuiBase, width: usize, line_index: usize, new_lines: &[String]) -> PathBuf {
    let path = base.log_directory().join("pi-crash.log");
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let mut data = format!(
        "Crash at {seconds}\nTerminal width: {width}\nLine {line_index} visible width: {}\n\n=== All rendered lines ===\n",
        visible_width(&new_lines[line_index])
    );
    for (index, line) in new_lines.iter().enumerate() {
        data.push_str(&format!("[{index}] (w={}) {line}\n", visible_width(line)));
    }
    data.push('\n');
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, data);
    path
}

fn full_render(
    base: &TuiBase,
    state: &mut RenderFrameState,
    new_lines: &[String],
    cursor_pos: Option<CursorPosition>,
    width: usize,
    height: usize,
    clear: bool,
) {
    base.increment_full_redraws();
    let mut buffer = String::from("\x1b[?2026h");
    if clear {
        buffer.push_str(&delete_kitty_images(&state.previous_kitty_image_ids));
        buffer.push_str("\x1b[2J\x1b[H\x1b[3J");
    }
    let mut index = 0;
    while index < new_lines.len() {
        if index > 0 {
            buffer.push_str("\r\n");
        }
        let line = &new_lines[index];
        let image = is_image_line(line);
        let image_reserved_rows = if image {
            get_kitty_image_reserved_rows(new_lines, index, new_lines.len() - 1)
        } else {
            1
        };
        if image_reserved_rows > 1 && image_reserved_rows <= height {
            for _ in 1..image_reserved_rows {
                buffer.push_str("\r\n");
            }
            buffer.push_str(&format!("\x1b[{}A", image_reserved_rows - 1));
            buffer.push_str(line);
            buffer.push_str(&format!("\x1b[{}B", image_reserved_rows - 1));
            index += image_reserved_rows;
            continue;
        }
        buffer.push_str(line);
        index += 1;
    }
    buffer.push_str("\x1b[?2026l");
    base.terminal().borrow_mut().write(&buffer);
    state.cursor_row = (new_lines.len() as i64 - 1).max(0);
    state.hardware_cursor_row = state.cursor_row;
    if clear {
        state.max_lines_rendered = new_lines.len();
    } else {
        state.max_lines_rendered = state.max_lines_rendered.max(new_lines.len());
    }
    let buffer_length = height.max(new_lines.len());
    state.previous_viewport_top = (buffer_length as i64 - height as i64).max(0);
    position_hardware_cursor(base, state, cursor_pos, new_lines.len());
    state.previous_lines = new_lines.to_vec();
    state.previous_kitty_image_ids = collect_kitty_image_ids(new_lines);
    state.previous_width = width as i64;
    state.previous_height = height as i64;
}

pub(crate) struct CommitFrame<'a> {
    pub(crate) variant: CommitVariant,
    pub(crate) new_lines: &'a [String],
    pub(crate) cursor_pos: Option<CursorPosition>,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) allow_full_render: bool,
}

pub(crate) fn commit_rendered_frame(base: &TuiBase, state: &mut RenderFrameState, frame: CommitFrame<'_>) -> bool {
    let CommitFrame {
        variant,
        new_lines,
        cursor_pos,
        width,
        height,
        allow_full_render,
    } = frame;
    let width_changed = state.previous_width != 0 && state.previous_width != width as i64;
    let height_changed = state.previous_height != 0 && state.previous_height != height as i64;
    let previous_buffer_length = if state.previous_height > 0 {
        state.previous_viewport_top + state.previous_height
    } else {
        height as i64
    };
    let mut prev_viewport_top = if height_changed {
        (previous_buffer_length - height as i64).max(0)
    } else {
        state.previous_viewport_top
    };
    let mut viewport_top = prev_viewport_top;
    let mut hardware_cursor_row = state.hardware_cursor_row;
    let compute_line_diff = |hardware: i64, prev_top: i64, viewport: i64, target: i64| -> i64 {
        let current_screen_row = hardware - prev_top;
        let target_screen_row = target - viewport;
        target_screen_row - current_screen_row
    };

    if state.previous_lines.is_empty() && !width_changed && !height_changed {
        log_redraw(base, "first render", 0, new_lines.len(), height);
        full_render(base, state, new_lines, cursor_pos, width, height, false);
        return true;
    }
    if width_changed {
        log_redraw(
            base,
            &format!("terminal width changed ({} -> {width})", state.previous_width),
            state.previous_lines.len(),
            new_lines.len(),
            height,
        );
        full_render(base, state, new_lines, cursor_pos, width, height, true);
        return true;
    }
    if height_changed && !base.is_termux_session() {
        log_redraw(
            base,
            &format!("terminal height changed ({} -> {height})", state.previous_height),
            state.previous_lines.len(),
            new_lines.len(),
            height,
        );
        full_render(base, state, new_lines, cursor_pos, width, height, true);
        return true;
    }
    if base.get_clear_on_shrink() && new_lines.len() < state.max_lines_rendered && !base.has_overlay_entries() {
        log_redraw(
            base,
            &format!("clearOnShrink (maxLinesRendered={})", state.max_lines_rendered),
            state.previous_lines.len(),
            new_lines.len(),
            height,
        );
        full_render(base, state, new_lines, cursor_pos, width, height, true);
        return true;
    }
    if variant.check_upward_shrink
        && let Some(upward) = find_upward_viewport_top_after_shrink(
            state.previous_lines.len(),
            new_lines.len(),
            height,
            prev_viewport_top,
        )
    {
        log_redraw(
            base,
            &format!("content shrink moved viewport up ({prev_viewport_top} -> {upward})"),
            state.previous_lines.len(),
            new_lines.len(),
            height,
        );
        full_render(base, state, new_lines, cursor_pos, width, height, true);
        return true;
    }

    let mut first_changed: i64 = -1;
    let mut last_changed: i64 = -1;
    let max_lines = new_lines.len().max(state.previous_lines.len());
    for index in 0..max_lines {
        let old_line = state.previous_lines.get(index).map(String::as_str).unwrap_or("");
        let new_line = new_lines.get(index).map(String::as_str).unwrap_or("");
        if old_line != new_line {
            if first_changed == -1 {
                first_changed = index as i64;
            }
            last_changed = index as i64;
        }
    }
    let appended_lines = new_lines.len() > state.previous_lines.len();
    if appended_lines {
        if first_changed == -1 {
            first_changed = state.previous_lines.len() as i64;
        }
        last_changed = new_lines.len() as i64 - 1;
    }
    if first_changed != -1 {
        let (expanded_first, expanded_last) = expand_changed_range_for_kitty_images(
            first_changed as usize,
            last_changed as usize,
            &state.previous_lines,
            new_lines,
        );
        first_changed = expanded_first as i64;
        last_changed = expanded_last as i64;
    }
    let append_start = appended_lines && first_changed == state.previous_lines.len() as i64 && first_changed > 0;

    if first_changed == -1 {
        position_hardware_cursor(base, state, cursor_pos, new_lines.len());
        state.previous_viewport_top = prev_viewport_top;
        state.previous_height = height as i64;
        return true;
    }

    if first_changed >= new_lines.len() as i64 {
        if state.previous_lines.len() > new_lines.len() {
            let target_row = (new_lines.len() as i64 - 1).max(0);
            if variant.check_deleted_viewport && target_row < prev_viewport_top {
                if !allow_full_render {
                    return false;
                }
                log_redraw(
                    base,
                    &format!("deleted lines moved viewport up ({target_row} < {prev_viewport_top})"),
                    state.previous_lines.len(),
                    new_lines.len(),
                    height,
                );
                full_render(base, state, new_lines, cursor_pos, width, height, true);
                return true;
            }
            let mut buffer = String::from("\x1b[?2026h");
            buffer.push_str(&delete_changed_kitty_images(
                first_changed as usize,
                last_changed as usize,
                &state.previous_lines,
            ));
            let line_diff = compute_line_diff(hardware_cursor_row, prev_viewport_top, viewport_top, target_row);
            if line_diff > 0 {
                buffer.push_str(&format!("\x1b[{line_diff}B"));
            } else if line_diff < 0 {
                buffer.push_str(&format!("\x1b[{}A", -line_diff));
            }
            buffer.push('\r');
            let extra_lines = state.previous_lines.len() - new_lines.len();
            if extra_lines > height {
                if !allow_full_render {
                    return false;
                }
                log_redraw(
                    base,
                    &format!("extraLines > height ({extra_lines} > {height})"),
                    state.previous_lines.len(),
                    new_lines.len(),
                    height,
                );
                full_render(base, state, new_lines, cursor_pos, width, height, true);
                return true;
            }
            let clear_start_offset = usize::from(!new_lines.is_empty());
            if extra_lines > 0 && clear_start_offset > 0 {
                buffer.push_str(&format!("\x1b[{clear_start_offset}B"));
            }
            for index in 0..extra_lines {
                buffer.push_str("\r\x1b[2K");
                if index < extra_lines - 1 {
                    buffer.push_str("\x1b[1B");
                }
            }
            let move_back = (extra_lines as i64 - 1 + clear_start_offset as i64).max(0);
            if move_back > 0 {
                buffer.push_str(&format!("\x1b[{move_back}A"));
            }
            buffer.push_str("\x1b[?2026l");
            base.terminal().borrow_mut().write(&buffer);
            state.cursor_row = target_row;
            state.hardware_cursor_row = target_row;
        }
        position_hardware_cursor(base, state, cursor_pos, new_lines.len());
        state.previous_lines = new_lines.to_vec();
        state.previous_kitty_image_ids = collect_kitty_image_ids(new_lines);
        state.previous_width = width as i64;
        state.previous_height = height as i64;
        state.previous_viewport_top = prev_viewport_top;
        return true;
    }

    if first_changed < prev_viewport_top {
        if !allow_full_render {
            return false;
        }
        log_redraw(
            base,
            &format!("firstChanged < viewportTop ({first_changed} < {prev_viewport_top})"),
            state.previous_lines.len(),
            new_lines.len(),
            height,
        );
        full_render(base, state, new_lines, cursor_pos, width, height, true);
        return true;
    }

    let mut buffer = String::from("\x1b[?2026h");
    buffer.push_str(&delete_changed_kitty_images(
        first_changed as usize,
        last_changed as usize,
        &state.previous_lines,
    ));
    let prev_viewport_bottom = prev_viewport_top + height as i64 - 1;
    let move_target_row = if append_start { first_changed - 1 } else { first_changed };
    if move_target_row > prev_viewport_bottom {
        let current_screen_row = (hardware_cursor_row - prev_viewport_top).max(0).min(height as i64 - 1);
        let move_to_bottom = height as i64 - 1 - current_screen_row;
        if move_to_bottom > 0 {
            buffer.push_str(&format!("\x1b[{move_to_bottom}B"));
        }
        let scroll = move_target_row - prev_viewport_bottom;
        buffer.push_str(&"\r\n".repeat(scroll as usize));
        prev_viewport_top += scroll;
        viewport_top += scroll;
        hardware_cursor_row = move_target_row;
    }
    let line_diff = compute_line_diff(hardware_cursor_row, prev_viewport_top, viewport_top, move_target_row);
    if line_diff > 0 {
        buffer.push_str(&format!("\x1b[{line_diff}B"));
    } else if line_diff < 0 {
        buffer.push_str(&format!("\x1b[{}A", -line_diff));
    }
    buffer.push_str(if append_start { "\r\n" } else { "\r" });

    let render_end = last_changed.min(new_lines.len() as i64 - 1);
    let mut index = first_changed;
    while index <= render_end {
        if index > first_changed {
            buffer.push_str("\r\n");
        }
        let line = &new_lines[index as usize];
        let image = is_image_line(line);
        let image_reserved_rows = if image {
            get_kitty_image_reserved_rows(new_lines, index as usize, render_end as usize)
        } else {
            1
        };
        if image_reserved_rows > 1 {
            let image_start_screen_row = index - viewport_top;
            if image_start_screen_row < 0 || image_start_screen_row + image_reserved_rows as i64 > height as i64 {
                if !allow_full_render {
                    return false;
                }
                log_redraw(
                    base,
                    &format!(
                        "kitty image pre-clear would scroll ({image_start_screen_row} + {image_reserved_rows} > {height})"
                    ),
                    state.previous_lines.len(),
                    new_lines.len(),
                    height,
                );
                full_render(base, state, new_lines, cursor_pos, width, height, true);
                return true;
            }
            buffer.push_str("\x1b[2K");
            for _ in 1..image_reserved_rows {
                buffer.push_str("\r\n\x1b[2K");
            }
            buffer.push_str(&format!("\x1b[{}A", image_reserved_rows - 1));
            buffer.push_str(line);
            buffer.push_str(&format!("\x1b[{}B", image_reserved_rows - 1));
            index += image_reserved_rows as i64;
            continue;
        }
        buffer.push_str("\x1b[2K");
        if !image && visible_width(line) > width {
            let crash_path = write_crash_log(base, width, index as usize, new_lines);
            base.stop(TuiStopOptions::default());
            panic!(
                "Rendered line {index} exceeds terminal width ({} > {width}).\n\nThis is likely caused by a custom TUI component not truncating its output.\nUse visibleWidth() to measure and truncateToWidth() to truncate lines.\n\nDebug log written to: {}",
                visible_width(line),
                crash_path.display()
            );
        }
        buffer.push_str(line);
        index += 1;
    }

    let mut final_cursor_row = render_end;
    if state.previous_lines.len() > new_lines.len() {
        if render_end < new_lines.len() as i64 - 1 {
            let move_down = new_lines.len() as i64 - 1 - render_end;
            buffer.push_str(&format!("\x1b[{move_down}B"));
            final_cursor_row = new_lines.len() as i64 - 1;
        }
        let extra_lines = state.previous_lines.len() - new_lines.len();
        for _ in new_lines.len()..state.previous_lines.len() {
            buffer.push_str("\r\n\x1b[2K");
        }
        buffer.push_str(&format!("\x1b[{extra_lines}A"));
    }
    buffer.push_str("\x1b[?2026l");
    base.terminal().borrow_mut().write(&buffer);

    state.cursor_row = (new_lines.len() as i64 - 1).max(0);
    state.hardware_cursor_row = final_cursor_row;
    state.max_lines_rendered = state.max_lines_rendered.max(new_lines.len());
    state.previous_viewport_top = prev_viewport_top.max(final_cursor_row - height as i64 + 1);
    position_hardware_cursor(base, state, cursor_pos, new_lines.len());
    state.previous_lines = new_lines.to_vec();
    state.previous_kitty_image_ids = collect_kitty_image_ids(new_lines);
    state.previous_width = width as i64;
    state.previous_height = height as i64;
    true
}

fn position_hardware_cursor(
    base: &TuiBase,
    state: &mut RenderFrameState,
    cursor_pos: Option<CursorPosition>,
    total_lines: usize,
) {
    let Some(cursor_pos) = cursor_pos.filter(|_| total_lines > 0) else {
        base.terminal().borrow_mut().hide_cursor();
        return;
    };
    let target_row = cursor_pos.row.min(total_lines - 1) as i64;
    let target_col = cursor_pos.col;
    let row_delta = target_row - state.hardware_cursor_row;
    let mut buffer = String::new();
    if row_delta > 0 {
        buffer.push_str(&format!("\x1b[{row_delta}B"));
    } else if row_delta < 0 {
        buffer.push_str(&format!("\x1b[{}A", -row_delta));
    }
    buffer.push_str(&format!("\x1b[{}G", target_col + 1));
    if !buffer.is_empty() {
        base.terminal().borrow_mut().write(&buffer);
    }
    state.hardware_cursor_row = target_row;
    if base.get_show_hardware_cursor() {
        base.terminal().borrow_mut().show_cursor();
    } else {
        base.terminal().borrow_mut().hide_cursor();
    }
}

/// Common TUI surface shared by both renderer variants.
pub trait TuiExt: RendererApi + Sized {
    fn base(&self) -> &TuiBase;

    fn mode(&self) -> TuiMode {
        TuiMode::Regular
    }

    fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        self.base().terminal()
    }

    fn full_redraws(&self) -> usize {
        self.base().full_redraws()
    }

    fn add_child(&self, component: ComponentRef) {
        self.base().add_child(component);
    }

    fn remove_child(&self, component: &ComponentRef) {
        self.base().remove_child(component);
    }

    fn clear(&self) {
        self.base().clear_children();
    }

    fn invalidate(&self) {
        self.base().invalidate_all();
    }

    fn get_show_hardware_cursor(&self) -> bool {
        self.base().get_show_hardware_cursor()
    }

    fn set_show_hardware_cursor(&self, enabled: bool) {
        self.base().set_show_hardware_cursor(enabled);
    }

    fn get_clear_on_shrink(&self) -> bool {
        self.base().get_clear_on_shrink()
    }

    fn set_clear_on_shrink(&self, enabled: bool) {
        self.base().set_clear_on_shrink(enabled);
    }

    fn set_termux_session(&self, enabled: bool) {
        self.base().set_termux_session(enabled);
    }

    fn set_debug_redraw(&self, enabled: bool) {
        self.base().set_debug_redraw(enabled);
    }

    fn get_focused_component(&self) -> Option<ComponentRef> {
        self.base().get_focused_component()
    }

    fn set_on_debug(&self, handler: Rc<dyn Fn()>) {
        self.base().set_on_debug(handler);
    }

    fn set_focus(&self, component: Option<ComponentRef>) {
        self.base().set_focus(component);
    }

    fn show_overlay(&self, component: ComponentRef, options: Option<OverlayOptions>) -> OverlayHandle {
        self.base().show_overlay(component, options)
    }

    fn hide_overlay(&self) {
        self.base().hide_overlay();
    }

    fn has_overlay(&self) -> bool {
        self.base().has_overlay()
    }

    fn start(&self) -> Result<(), crate::terminal::TerminalRawModeError> {
        self.base().start()
    }

    fn stop(&self) {
        self.base().stop(TuiStopOptions::default());
    }

    fn stop_with(&self, options: TuiStopOptions) {
        self.base().stop(options);
    }

    fn render_now(&self, force: bool) {
        self.base().render_now(force);
    }

    fn request_render(&self, force: bool) {
        self.base().request_render(force);
    }

    fn flush_pending(&self) {
        self.base().flush_pending();
    }

    fn process_events(&self) {
        self.base().process_events();
    }

    fn add_input_listener(&self, listener: InputListener) -> usize {
        self.base().add_input_listener(listener)
    }

    fn remove_input_listener(&self, id: usize) {
        self.base().remove_input_listener(id);
    }

    fn on_terminal_color_scheme_change(&self, listener: ColorSchemeListener) -> usize {
        self.base().on_terminal_color_scheme_change(listener)
    }

    fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        self.base().set_terminal_color_scheme_notifications(enabled);
    }

    fn query_terminal_background_color(&self, timeout: Duration) -> TerminalBackgroundQuery {
        self.base().query_terminal_background_color(timeout)
    }

    fn query_terminal_color_scheme(&self, timeout: Duration) -> TerminalColorSchemeQuery {
        self.base().query_terminal_color_scheme(timeout)
    }
}

/// TUI renderer used by fixed cells and render regions (`TUI` in the source).
pub struct TuiRenderer {
    base: TuiBase,
    frame: RefCell<RenderFrameState>,
    cached_cursor_position: Cell<Option<CursorPosition>>,
}

impl TuiRenderer {
    pub fn new(terminal: Rc<RefCell<dyn Terminal>>) -> Rc<Self> {
        Self::with_options(terminal, None, None)
    }

    pub fn with_options(
        terminal: Rc<RefCell<dyn Terminal>>,
        show_hardware_cursor: Option<bool>,
        log_directory: Option<PathBuf>,
    ) -> Rc<Self> {
        let base = TuiBase::new(terminal, show_hardware_cursor, log_directory);
        let renderer = Rc::new(Self {
            base,
            frame: RefCell::new(RenderFrameState::default()),
            cached_cursor_position: Cell::new(None),
        });
        let trait_object: Rc<dyn RendererApi> = renderer.clone();
        renderer.base.install_renderer(Rc::downgrade(&trait_object));
        renderer
    }

    pub fn create_fixed_cell(self: &Rc<Self>) -> FixedCell {
        let state = Rc::new(RefCell::new(FixedCellState::default()));
        self.base.register_fixed_cell(Rc::clone(&state));
        FixedCell {
            state,
            writer: FixedCellWriter::Immediate(Rc::downgrade(self)),
        }
    }

    pub fn create_render_region(self: &Rc<Self>, component: ComponentRef) -> RenderRegion {
        let state = Rc::new(RefCell::new(RenderRegionState {
            component: Some(component),
            ..RenderRegionState::default()
        }));
        RenderRegion {
            state,
            writer: RegionWriter::Immediate(Rc::downgrade(self)),
        }
    }

    pub fn create_flow_render_region(self: &Rc<Self>, component: ComponentRef) -> FlowRenderRegion {
        let state = Rc::new(RefCell::new(RenderRegionState {
            component: Some(component),
            ..RenderRegionState::default()
        }));
        FlowRenderRegion {
            state,
            writer: RegionWriter::Immediate(Rc::downgrade(self)),
        }
    }

    pub fn request_component_render(&self, component: &ComponentRef) -> bool {
        match self.base.region_state_for(component) {
            Some(state) => self.request_render_region(&state),
            None => {
                self.base.request_render(false);
                false
            }
        }
    }

    fn place_render_region(&self, state: &RenderRegionStateRef, rect: RenderRegionRect) {
        if state.borrow().disposed {
            return;
        }
        self.unmap_render_region_components(state);
        {
            let mut state = state.borrow_mut();
            state.rect = Some(rect);
            state.flow_layout = None;
            state.generation = Some(self.base.render_generation());
        }
        let component = state.borrow().component.clone();
        if let Some(component) = component {
            self.map_render_region_components(state, &component);
        }
    }

    fn place_flow_render_region(&self, state: &RenderRegionStateRef, layout: FlowRenderRegionLayout) {
        if state.borrow().disposed {
            return;
        }
        self.unmap_render_region_components(state);
        {
            let mut state = state.borrow_mut();
            state.rect = Some(layout.rect);
            state.flow_layout = Some(layout);
            state.generation = Some(self.base.render_generation());
        }
        let component = state.borrow().component.clone();
        if let Some(component) = component {
            self.map_render_region_components(state, &component);
        }
    }

    fn clear_render_region(&self, state: &RenderRegionStateRef) {
        self.unmap_render_region_components(state);
        let mut state = state.borrow_mut();
        state.rect = None;
        state.flow_layout = None;
        state.generation = None;
    }

    fn dispose_render_region(&self, state: &RenderRegionStateRef) {
        if state.borrow().disposed {
            return;
        }
        state.borrow_mut().disposed = true;
        self.clear_render_region(state);
    }

    fn map_render_region_components(&self, state: &RenderRegionStateRef, component: &ComponentRef) {
        state.borrow_mut().components.push(component.clone());
        self.base.set_region_state(component, Rc::clone(state));
        let children = component
            .borrow()
            .as_container()
            .map(|container| container.children().to_vec());
        if let Some(children) = children {
            for child in children {
                self.map_render_region_components(state, &child);
            }
        }
    }

    fn unmap_render_region_components(&self, state: &RenderRegionStateRef) {
        let components = std::mem::take(&mut state.borrow_mut().components);
        for component in components {
            self.base.remove_region_state(&component, state);
        }
    }

    fn request_render_region(&self, state: &RenderRegionStateRef) -> bool {
        let flow = state.borrow().flow_layout;
        let rendered = match flow {
            Some(_) => self.write_flow_render_region(state),
            None => self.write_render_region(state),
        };
        if rendered {
            return true;
        }
        self.base.request_render(false);
        false
    }

    fn write_fixed_cell(&self, state: &Rc<RefCell<FixedCellState>>) -> bool {
        let snapshot = {
            let state = state.borrow();
            if state.disposed {
                return false;
            }
            (state.row, state.col, state.value.clone())
        };
        let (Some(row), Some(col), value) = snapshot else {
            return false;
        };
        if value.is_empty() {
            return false;
        }
        if self.base.is_stopped()
            || !self.base.is_terminal_focused()
            || self.base.is_render_requested()
            || self.base.has_overlay()
        {
            return false;
        }
        let terminal_columns = self.base.terminal().borrow().columns();
        let terminal_rows = self.base.terminal().borrow().rows();
        {
            let frame = self.frame.borrow();
            if frame.previous_width != terminal_columns as i64 || frame.previous_height != terminal_rows as i64 {
                return false;
            }
            let Some(line) = frame.previous_lines.get(row as usize) else {
                return false;
            };
            let viewport_bottom = frame.previous_viewport_top + terminal_rows as i64;
            let is_visible = row >= frame.previous_viewport_top && row < viewport_bottom;
            if !is_visible {
                return false;
            }
            if col < 0 || col >= terminal_columns as i64 {
                return false;
            }
            let patched = patch_fixed_cell_line(line, col, &value, true);
            let cell = slice_with_width(&value, 0, 1, true);
            let Some(patched) = patched.filter(|_| cell.width == 1) else {
                return false;
            };
            drop(frame);
            self.write_fixed_cell_to_terminal(row, col, &cell.text);
            let mut frame = self.frame.borrow_mut();
            frame.previous_lines[row as usize] = patched;
        }
        true
    }

    fn write_fixed_cell_to_terminal(&self, row: i64, col: i64, value: &str) {
        let screen_row = row - self.frame.borrow().previous_viewport_top;
        let buffer = format!(
            "\x1b[?2026h\x1b7\x1b[{};{}H{}{}\x1b8\x1b[?2026l",
            screen_row + 1,
            col + 1,
            value,
            SEGMENT_RESET
        );
        self.base.terminal().borrow_mut().write(&buffer);
    }

    fn write_flow_render_region(&self, state: &RenderRegionStateRef) -> bool {
        let Some(layout) = self.get_writable_flow_render_region_layout(state) else {
            return false;
        };
        let Some(frame) = self.build_flow_render_frame(state, layout) else {
            return false;
        };
        let Some(updates) = self.plan_flow_render_state_updates(state, layout, &frame) else {
            return false;
        };
        let mut render_frame = self.frame.borrow_mut();
        let width = render_frame.previous_width as usize;
        let height = render_frame.previous_height as usize;
        if !commit_rendered_frame(
            &self.base,
            &mut render_frame,
            crate::tui::CommitFrame {
                variant: CommitVariant {
                    check_upward_shrink: false,
                    check_deleted_viewport: false,
                },
                new_lines: &frame.lines,
                cursor_pos: updates.cursor_position,
                width,
                height,
                allow_full_render: false,
            },
        ) {
            return false;
        }
        drop(render_frame);
        self.apply_flow_render_state_updates(state, updates);
        true
    }

    fn build_flow_render_frame(
        &self,
        state: &RenderRegionStateRef,
        layout: FlowRenderRegionLayout,
    ) -> Option<FlowRenderFrame> {
        let rect = layout.rect;
        {
            let frame = self.frame.borrow();
            let start = rect.row.max(0) as usize;
            if frame.previous_lines.iter().skip(start).any(|line| is_image_line(line)) {
                return None;
            }
        }
        let component = state.borrow().component.clone()?;
        let rendered_lines = component.borrow_mut().render(rect.width);
        if rendered_lines.len() < rect.height {
            return None;
        }
        let tail_lines = self.prepare_flow_render_region_lines(&rendered_lines, rect.width)?;
        let height_delta = tail_lines.len() as i64 - rect.height as i64;
        let flow_end_row = layout.flow_end_row + height_delta;
        let bottom_row = flow_end_row.max(layout.viewport_height - layout.bottom_height);
        let lines = self.splice_flow_render_frame_lines(layout, &tail_lines, flow_end_row, bottom_row);
        Some(FlowRenderFrame {
            lines,
            rect: RenderRegionRect {
                height: tail_lines.len(),
                ..rect
            },
            bottom_row,
            height_delta,
            bottom_delta: bottom_row - layout.bottom_row,
        })
    }

    fn splice_flow_render_frame_lines(
        &self,
        layout: FlowRenderRegionLayout,
        tail_lines: &[String],
        flow_end_row: i64,
        bottom_row: i64,
    ) -> Vec<String> {
        let rect = layout.rect;
        let frame = self.frame.borrow();
        let start = rect.row.max(0) as usize;
        let tail_end = (rect.row + rect.height as i64).max(0) as usize;
        let flow_end = layout.flow_end_row.max(0) as usize;
        let bottom_start = layout.bottom_row.max(0) as usize;
        let mut result = Vec::new();
        result.extend_from_slice(&frame.previous_lines[..start.min(frame.previous_lines.len())]);
        result.extend_from_slice(tail_lines);
        result.extend_from_slice(
            &frame.previous_lines[tail_end.min(frame.previous_lines.len())..flow_end.min(frame.previous_lines.len())],
        );
        let mut gap_lines: Vec<String> = vec![String::new(); (bottom_row - flow_end_row).max(0) as usize];
        self.base.apply_line_resets(&mut gap_lines);
        result.extend(gap_lines);
        result.extend_from_slice(&frame.previous_lines[bottom_start.min(frame.previous_lines.len())..]);
        result
    }

    fn plan_flow_render_state_updates(
        &self,
        state: &RenderRegionStateRef,
        layout: FlowRenderRegionLayout,
        frame: &FlowRenderFrame,
    ) -> Option<FlowRenderStateUpdates> {
        let placements = self.plan_flow_render_region_placement_updates(state, layout, frame.rect, frame.bottom_row)?;
        let fixed_cells =
            self.plan_flow_fixed_cell_placement_updates(layout, frame.height_delta, frame.bottom_delta)?;
        let cursor_position = self.shift_flow_cursor_position(layout, frame.height_delta, frame.bottom_delta)?;
        Some(FlowRenderStateUpdates {
            placements,
            fixed_cells,
            cursor_position,
        })
    }

    fn shift_flow_cursor_position(
        &self,
        layout: FlowRenderRegionLayout,
        height_delta: i64,
        bottom_delta: i64,
    ) -> Option<Option<CursorPosition>> {
        let cursor_position = self.cached_cursor_position.get();
        let Some(cursor_position) = cursor_position else {
            return Some(None);
        };
        let row = self.shift_flow_row(cursor_position.row as i64, layout, height_delta, bottom_delta)?;
        Some(Some(CursorPosition {
            row: row.max(0) as usize,
            ..cursor_position
        }))
    }

    fn apply_flow_render_state_updates(&self, state: &RenderRegionStateRef, updates: FlowRenderStateUpdates) {
        self.cached_cursor_position.set(updates.cursor_position);
        for update in updates.placements {
            let mut region = update.state.borrow_mut();
            region.rect = Some(update.rect);
            region.flow_layout = update.flow_layout;
        }
        for update in updates.fixed_cells {
            update.state.borrow_mut().row = Some(update.row);
        }
        self.unmap_render_region_components(state);
        let component = state.borrow().component.clone();
        if let Some(component) = component {
            self.map_render_region_components(state, &component);
        }
    }

    fn plan_flow_render_region_placement_updates(
        &self,
        current_state: &RenderRegionStateRef,
        layout: FlowRenderRegionLayout,
        next_rect: RenderRegionRect,
        next_bottom_row: i64,
    ) -> Option<Vec<RenderRegionPlacementUpdate>> {
        let height_delta = next_rect.height as i64 - layout.rect.height as i64;
        let bottom_delta = next_bottom_row - layout.bottom_row;
        let states = self.base.region_states();
        let mut updates = Vec::new();
        for state in states {
            let (rect, generation, flow_layout) = {
                let state = state.borrow();
                (state.rect, state.generation, state.flow_layout)
            };
            let Some(rect) = rect else {
                continue;
            };
            if generation != Some(self.base.render_generation()) {
                continue;
            }
            let shifted_rect = if Rc::ptr_eq(&state, current_state) {
                next_rect
            } else {
                self.shift_render_region_rect(rect, layout, height_delta, bottom_delta)?
            };
            let shifted_flow_layout = flow_layout.map(|flow| FlowRenderRegionLayout {
                rect: shifted_rect,
                flow_end_row: flow.flow_end_row + height_delta,
                bottom_row: flow.bottom_row + bottom_delta,
                ..flow
            });
            updates.push(RenderRegionPlacementUpdate {
                state,
                rect: shifted_rect,
                flow_layout: shifted_flow_layout,
            });
        }
        Some(updates)
    }

    fn shift_render_region_rect(
        &self,
        rect: RenderRegionRect,
        layout: FlowRenderRegionLayout,
        height_delta: i64,
        bottom_delta: i64,
    ) -> Option<RenderRegionRect> {
        let rect_bottom = rect.row + rect.height as i64;
        let tail_start = layout.rect.row;
        let tail_bottom = tail_start + layout.rect.height as i64;
        if rect_bottom <= tail_start {
            return Some(rect);
        }
        if rect.row >= tail_bottom && rect_bottom <= layout.flow_end_row {
            return Some(RenderRegionRect {
                row: rect.row + height_delta,
                ..rect
            });
        }
        let bottom_end = layout.bottom_row + layout.bottom_height;
        if rect.row >= layout.bottom_row && rect_bottom <= bottom_end {
            return Some(RenderRegionRect {
                row: rect.row + bottom_delta,
                ..rect
            });
        }
        None
    }

    fn plan_flow_fixed_cell_placement_updates(
        &self,
        layout: FlowRenderRegionLayout,
        height_delta: i64,
        bottom_delta: i64,
    ) -> Option<Vec<FixedCellPlacementUpdate>> {
        let mut updates = Vec::new();
        for state in self.base.fixed_cells() {
            let (disposed, row) = {
                let state = state.borrow();
                (state.disposed, state.row)
            };
            if disposed {
                continue;
            }
            let Some(row) = row else {
                continue;
            };
            let shifted = self.shift_flow_row(row, layout, height_delta, bottom_delta)?;
            updates.push(FixedCellPlacementUpdate { state, row: shifted });
        }
        Some(updates)
    }

    fn shift_flow_row(
        &self,
        row: i64,
        layout: FlowRenderRegionLayout,
        height_delta: i64,
        bottom_delta: i64,
    ) -> Option<i64> {
        let tail_start = layout.rect.row;
        let tail_bottom = tail_start + layout.rect.height as i64;
        if row < tail_start {
            return Some(row);
        }
        if row < tail_bottom {
            return None;
        }
        if row < layout.flow_end_row {
            return Some(row + height_delta);
        }
        if row < layout.bottom_row {
            return None;
        }
        if row < layout.bottom_row + layout.bottom_height {
            return Some(row + bottom_delta);
        }
        None
    }

    fn get_writable_flow_render_region_layout(&self, state: &RenderRegionStateRef) -> Option<FlowRenderRegionLayout> {
        let (layout, disposed, generation) = {
            let state = state.borrow();
            (state.flow_layout, state.disposed, state.generation)
        };
        let has_current_placement = layout.is_some() && !disposed && generation == Some(self.base.render_generation());
        if !has_current_placement || !self.can_write_render_region_now() {
            return None;
        }
        let layout = layout?;
        let rect = layout.rect;
        let previous_width = self.frame.borrow().previous_width;
        let is_full_width = rect.col == 0 && rect.width as i64 == previous_width;
        let terminal_rows = self.base.terminal().borrow().rows() as i64;
        if !is_full_width || layout.viewport_height != terminal_rows {
            return None;
        }
        let region_bottom = rect.row + rect.height as i64;
        let frame_end = layout.bottom_row + layout.bottom_height;
        let frame_length = self.frame.borrow().previous_lines.len() as i64;
        let has_valid_rows = rect.row >= 0
            && region_bottom <= layout.flow_end_row
            && layout.flow_end_row <= layout.bottom_row
            && layout.bottom_height >= 0
            && frame_end == frame_length;
        has_valid_rows.then_some(layout)
    }

    fn prepare_flow_render_region_lines(&self, lines: &[String], width: usize) -> Option<Vec<String>> {
        if lines
            .iter()
            .any(|line| line.contains(CURSOR_MARKER) || is_image_line(line) || visible_width(line) > width)
        {
            return None;
        }
        let mut lines = lines.to_vec();
        self.base.apply_line_resets(&mut lines);
        Some(lines)
    }

    fn write_render_region(&self, state: &RenderRegionStateRef) -> bool {
        let Some(rect) = self.get_writable_render_region_rect(state) else {
            return false;
        };
        let previous_lines: Vec<String> = {
            let frame = self.frame.borrow();
            let start = rect.row as usize;
            let end = (rect.row as usize + rect.height).min(frame.previous_lines.len());
            frame.previous_lines[start.min(end)..end].to_vec()
        };
        if previous_lines.iter().any(|line| is_image_line(line)) {
            return false;
        }
        let Some(component) = state.borrow().component.clone() else {
            return false;
        };
        let rendered_lines = component.borrow_mut().render(rect.width);
        if rendered_lines.len() != rect.height {
            return false;
        }
        let Some(next_lines) = self.prepare_render_region_lines(&rendered_lines, rect) else {
            return false;
        };
        self.write_render_region_lines(rect.row, &previous_lines, &next_lines);
        {
            let mut frame = self.frame.borrow_mut();
            frame
                .previous_lines
                .splice(rect.row as usize..rect.row as usize + rect.height, next_lines);
        }
        self.unmap_render_region_components(state);
        self.map_render_region_components(state, &component);
        true
    }

    fn can_write_render_region_now(&self) -> bool {
        let (previous_width, previous_height) = {
            let frame = self.frame.borrow();
            (frame.previous_width, frame.previous_height)
        };
        let terminal = self.base.terminal();
        let terminal_columns = terminal.borrow().columns() as i64;
        let terminal_rows = terminal.borrow().rows() as i64;
        !self.base.is_stopped()
            && !self.base.is_rendering()
            && self.base.is_terminal_focused()
            && !self.base.is_render_requested()
            && !self.base.has_overlay()
            && previous_width == terminal_columns
            && previous_height == terminal_rows
    }

    fn get_writable_render_region_rect(&self, state: &RenderRegionStateRef) -> Option<RenderRegionRect> {
        let (rect, disposed, generation) = {
            let state = state.borrow();
            (state.rect, state.disposed, state.generation)
        };
        let has_current_placement = rect.is_some() && !disposed && generation == Some(self.base.render_generation());
        if !has_current_placement || !self.can_write_render_region_now() {
            return None;
        }
        let rect = rect?;
        let previous_width = self.frame.borrow().previous_width;
        if rect.col != 0 || rect.width as i64 != previous_width {
            return None;
        }
        let region_bottom = rect.row + rect.height as i64;
        let frame = self.frame.borrow();
        if rect.row < 0 || region_bottom > frame.previous_lines.len() as i64 {
            return None;
        }
        let terminal_rows = self.base.terminal().borrow().rows() as i64;
        let viewport_bottom = frame.previous_viewport_top + terminal_rows;
        if rect.row < frame.previous_viewport_top || region_bottom > viewport_bottom {
            return None;
        }
        Some(rect)
    }

    fn prepare_render_region_lines(&self, lines: &[String], rect: RenderRegionRect) -> Option<Vec<String>> {
        if lines
            .iter()
            .any(|line| line.contains(CURSOR_MARKER) || is_image_line(line) || visible_width(line) > rect.width)
        {
            return None;
        }
        let mut lines = self.apply_fixed_cells_to_region(lines, rect.row);
        self.base.apply_line_resets(&mut lines);
        Some(lines)
    }

    fn apply_fixed_cells_to_region(&self, lines: &[String], start_row: i64) -> Vec<String> {
        let mut result = lines.to_vec();
        let cells = self.base.fixed_cells();
        if cells.is_empty() {
            return result;
        }
        for state in cells {
            let state = state.borrow();
            if state.disposed || state.value.is_empty() {
                continue;
            }
            let (Some(row), Some(col)) = (state.row, state.col) else {
                continue;
            };
            let line_index = row - start_row;
            if line_index < 0 || line_index as usize >= result.len() {
                continue;
            }
            let line_index = line_index as usize;
            if let Some(patched) = patch_fixed_cell_line(&result[line_index], col, &state.value, false) {
                result[line_index] = patched;
            }
        }
        result
    }

    fn write_render_region_lines(&self, start_row: i64, previous_lines: &[String], next_lines: &[String]) {
        let mut buffer = String::from("\x1b[?2026h\x1b7");
        let mut changed = false;
        let previous_viewport_top = self.frame.borrow().previous_viewport_top;
        for (index, next_line) in next_lines.iter().enumerate() {
            if previous_lines.get(index) == Some(next_line) {
                continue;
            }
            let screen_row = start_row + index as i64 - previous_viewport_top;
            buffer.push_str(&format!("\x1b[{};1H\x1b[2K{next_line}", screen_row + 1));
            changed = true;
        }
        if !changed {
            return;
        }
        buffer.push_str("\x1b8\x1b[?2026l");
        self.base.terminal().borrow_mut().write(&buffer);
    }
}

impl RendererApi for TuiRenderer {
    fn do_render(&self) {
        if self.base.is_stopped() {
            return;
        }
        let width = self.base.terminal().borrow().columns();
        let height = self.base.terminal().borrow().rows();
        self.base.advance_render_generation();
        self.base.set_rendering(true);
        let mut new_lines = self.base.render_children(width);
        self.base.set_rendering(false);
        new_lines = self.base.apply_fixed_cells(&new_lines);
        if self.base.has_overlay_entries() {
            new_lines = self.base.composite_overlays(new_lines, width, height);
        }
        let cursor_pos = self.base.extract_cursor_position(&mut new_lines, height);
        self.base.apply_line_resets(&mut new_lines);
        let mut frame = self.frame.borrow_mut();
        if commit_rendered_frame(
            &self.base,
            &mut frame,
            crate::tui::CommitFrame {
                variant: CommitVariant {
                    check_upward_shrink: true,
                    check_deleted_viewport: false,
                },
                new_lines: &new_lines,
                cursor_pos,
                width,
                height,
                allow_full_render: true,
            },
        ) {
            self.cached_cursor_position.set(cursor_pos);
        }
    }

    fn reset_render_state(&self) {
        self.frame.borrow_mut().reset_for_renderer();
        self.cached_cursor_position.set(None);
    }

    fn before_terminal_stop(&self, options: &TuiStopOptions) {
        let frame = self.frame.borrow();
        if options.preserve_screen || frame.previous_lines.is_empty() {
            return;
        }
        let target_row = frame.previous_lines.len() as i64;
        let line_diff = target_row - frame.hardware_cursor_row;
        if line_diff > 0 {
            self.base.terminal().borrow_mut().write(&format!("\x1b[{line_diff}B"));
        } else if line_diff < 0 {
            self.base
                .terminal()
                .borrow_mut()
                .write(&format!("\x1b[{}A", -line_diff));
        }
        self.base.terminal().borrow_mut().write("\r\n");
    }
}

impl TuiExt for TuiRenderer {
    fn base(&self) -> &TuiBase {
        &self.base
    }
}

struct FlowRenderFrame {
    lines: Vec<String>,
    rect: RenderRegionRect,
    bottom_row: i64,
    height_delta: i64,
    bottom_delta: i64,
}

struct FlowRenderStateUpdates {
    placements: Vec<RenderRegionPlacementUpdate>,
    fixed_cells: Vec<FixedCellPlacementUpdate>,
    cursor_position: Option<CursorPosition>,
}

struct RenderRegionPlacementUpdate {
    state: RenderRegionStateRef,
    rect: RenderRegionRect,
    flow_layout: Option<FlowRenderRegionLayout>,
}

struct FixedCellPlacementUpdate {
    state: Rc<RefCell<FixedCellState>>,
    row: i64,
}

impl Component for TuiRenderer {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.base.render_children(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.base.invalidate_all();
    }
}

impl Component for TuiMainScreen {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.base.render_children(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.base.invalidate_all();
    }
}

// Re-export the visible-width helper as the source does.
pub use crate::utils::visible_width;
