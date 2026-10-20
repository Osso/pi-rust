//! Ported from `packages/tui/src/tui-main-screen.ts`.
//!
//! The main-screen renderer renders into the terminal's main screen and
//! scrollback. It shares the frame commit path in [`crate::tui`].

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::terminal::Terminal;
use crate::terminal_image::is_image_line;
use crate::tui::{
    CommitVariant, ComponentRef, FixedCell, FixedCellState, FixedCellWriter, FlowRenderRegion, RegionWriter,
    RenderFrameState, RenderRegion, RenderRegionRect, RenderRegionState, RenderRegionStateRef, RendererApi, TuiBase,
    TuiExt, TuiStopOptions, commit_rendered_frame,
};

/// Main-screen TUI (`TuiMainScreen` in the source).
pub struct TuiMainScreen {
    pub(crate) base: TuiBase,
    frame: RefCell<RenderFrameState>,
}

impl TuiMainScreen {
    pub fn new(terminal: Rc<RefCell<dyn Terminal>>) -> Rc<Self> {
        Self::with_options(terminal, None, None)
    }

    pub fn with_options(
        terminal: Rc<RefCell<dyn Terminal>>,
        show_hardware_cursor: Option<bool>,
        log_directory: Option<PathBuf>,
    ) -> Rc<Self> {
        let base = TuiBase::new(terminal, show_hardware_cursor, log_directory);
        let screen = Rc::new(Self {
            base,
            frame: RefCell::new(RenderFrameState::default()),
        });
        let trait_object: Rc<dyn RendererApi> = screen.clone();
        screen.base.install_renderer(Rc::downgrade(&trait_object));
        screen
    }

    pub fn create_fixed_cell(self: &Rc<Self>) -> FixedCell {
        let state = Rc::new(RefCell::new(FixedCellState::default()));
        self.base.register_fixed_cell(Rc::clone(&state));
        FixedCell {
            state,
            writer: FixedCellWriter::Deferred(Rc::downgrade(self)),
        }
    }

    pub fn create_render_region(self: &Rc<Self>, component: ComponentRef) -> RenderRegion {
        let state = Rc::new(RefCell::new(RenderRegionState {
            component: Some(component),
            ..RenderRegionState::default()
        }));
        RenderRegion {
            state,
            writer: RegionWriter::Deferred(Rc::downgrade(self)),
        }
    }

    pub fn create_flow_render_region(self: &Rc<Self>, component: ComponentRef) -> FlowRenderRegion {
        let state = Rc::new(RefCell::new(RenderRegionState {
            component: Some(component),
            ..RenderRegionState::default()
        }));
        FlowRenderRegion {
            state,
            writer: RegionWriter::Deferred(Rc::downgrade(self)),
        }
    }

    pub fn request_component_render(&self, _component: &ComponentRef) -> bool {
        self.base.request_render(false);
        false
    }

    pub(crate) fn place_deferred_render_region(&self, state: &RenderRegionStateRef, rect: RenderRegionRect) {
        if state.borrow().disposed {
            return;
        }
        let mut state = state.borrow_mut();
        state.rect = Some(rect);
        state.flow_layout = None;
    }

    pub(crate) fn clear_deferred_render_region(&self, state: &RenderRegionStateRef) {
        let mut state = state.borrow_mut();
        state.rect = None;
        state.flow_layout = None;
    }

    pub(crate) fn dispose_deferred_render_region(&self, state: &RenderRegionStateRef) {
        state.borrow_mut().disposed = true;
        self.clear_deferred_render_region(state);
    }

    /// Capture the render frame for suspend/resume.
    pub fn capture_render_state(&self) -> MainScreenRenderState {
        let frame = self.frame.borrow();
        MainScreenRenderState {
            previous_lines: frame.previous_lines.clone(),
            previous_width: frame.previous_width,
            previous_height: frame.previous_height,
            cursor_row: frame.cursor_row,
            hardware_cursor_row: frame.hardware_cursor_row,
            max_lines_rendered: frame.max_lines_rendered,
            previous_viewport_top: frame.previous_viewport_top,
        }
    }

    /// Restore a captured render frame, dropping image lines.
    pub fn restore_render_state(&self, state: MainScreenRenderState) {
        let mut frame = self.frame.borrow_mut();
        frame.previous_lines = state
            .previous_lines
            .into_iter()
            .map(|line| if is_image_line(&line) { String::new() } else { line })
            .collect();
        frame.previous_kitty_image_ids.clear();
        frame.previous_width = state.previous_width;
        frame.previous_height = state.previous_height;
        frame.cursor_row = state.cursor_row;
        frame.hardware_cursor_row = state.hardware_cursor_row;
        frame.max_lines_rendered = state.max_lines_rendered;
        frame.previous_viewport_top = state.previous_viewport_top;
    }
}

/// Captured main-screen render frame.
#[derive(Debug, Clone)]
pub struct MainScreenRenderState {
    pub previous_lines: Vec<String>,
    pub previous_width: i64,
    pub previous_height: i64,
    pub cursor_row: i64,
    pub hardware_cursor_row: i64,
    pub max_lines_rendered: usize,
    pub previous_viewport_top: i64,
}

impl RendererApi for TuiMainScreen {
    fn do_render(&self) {
        if self.base.is_stopped() {
            return;
        }
        let width = self.base.terminal().borrow().columns();
        let height = self.base.terminal().borrow().rows();
        let mut new_lines = self.base.render_children(width);
        if self.base.has_overlay_entries() {
            new_lines = self.base.composite_overlays(new_lines, width, height);
        }
        let cursor_pos = self.base.extract_cursor_position(&mut new_lines, height);
        self.base.apply_line_resets(&mut new_lines);
        let mut frame = self.frame.borrow_mut();
        commit_rendered_frame(
            &self.base,
            &mut frame,
            crate::tui::CommitFrame {
                variant: CommitVariant {
                    check_upward_shrink: false,
                    check_deleted_viewport: true,
                },
                new_lines: &new_lines,
                cursor_pos,
                width,
                height,
                allow_full_render: true,
            },
        );
    }

    fn reset_render_state(&self) {
        self.frame.borrow_mut().reset_for_main_screen();
    }

    fn before_terminal_stop(&self, options: &TuiStopOptions) {
        let sequence = crate::tui::frame_stop_sequence(&self.frame.borrow(), options, true);
        self.base.terminal().borrow_mut().write(&sequence);
    }

    fn terminal_stop_output(&self) -> (String, String) {
        (
            crate::tui::frame_stop_sequence(&self.frame.borrow(), &TuiStopOptions::default(), true),
            String::new(),
        )
    }
}

impl TuiExt for TuiMainScreen {
    fn base(&self) -> &TuiBase {
        &self.base
    }
}
