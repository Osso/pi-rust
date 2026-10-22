//! Ported from `packages/tui/src/root-compositor.ts`.

use std::rc::Rc;

use pi_js_compat::js_json::JsString;

use crate::tui::{Component, ComponentRef, FlowRenderRegionLayout, RenderRegionRect};

/// One flow or bottom layout entry.
#[derive(Clone)]
pub struct RootLayoutEntry {
    pub component: ComponentRef,
    pub on_layout: Option<Rc<dyn Fn(RenderRegionRect)>>,
    pub on_flow_layout: Option<Rc<dyn Fn(FlowRenderRegionLayout)>>,
}

impl RootLayoutEntry {
    pub fn new(component: ComponentRef) -> Self {
        Self {
            component,
            on_layout: None,
            on_flow_layout: None,
        }
    }

    pub fn with_on_layout(mut self, on_layout: Rc<dyn Fn(RenderRegionRect)>) -> Self {
        self.on_layout = Some(on_layout);
        self
    }

    pub fn with_on_flow_layout(mut self, on_flow_layout: Rc<dyn Fn(FlowRenderRegionLayout)>) -> Self {
        self.on_flow_layout = Some(on_flow_layout);
        self
    }
}

/// Options for [`RootCompositor`].
pub struct RootCompositorOptions {
    pub get_height: Rc<dyn Fn() -> usize>,
    pub flow: Vec<RootLayoutEntry>,
    pub bottom: Vec<RootLayoutEntry>,
}

struct RenderedEntry {
    entry: RootLayoutEntry,
    lines: Vec<JsString>,
}

fn render_entries(entries: &[RootLayoutEntry], width: usize) -> Vec<RenderedEntry> {
    entries
        .iter()
        .map(|entry| RenderedEntry {
            entry: entry.clone(),
            lines: entry.component.borrow_mut().render(width),
        })
        .collect()
}

fn total_height(entries: &[RenderedEntry]) -> i64 {
    entries.iter().map(|entry| entry.lines.len() as i64).sum()
}

fn notify_layout(entries: &[RenderedEntry], start_row: i64, width: usize) {
    let mut row = start_row;
    for entry in entries {
        if let Some(on_layout) = &entry.entry.on_layout {
            on_layout(RenderRegionRect {
                row,
                col: 0,
                width,
                height: entry.lines.len(),
            });
        }
        row += entry.lines.len() as i64;
    }
}

fn notify_flow_layout(
    entries: &[RenderedEntry],
    width: usize,
    flow_end_row: i64,
    bottom_row: i64,
    bottom_height: i64,
    viewport_height: i64,
) {
    let mut row = 0;
    for entry in entries {
        let rect = RenderRegionRect {
            row,
            col: 0,
            width,
            height: entry.lines.len(),
        };
        if let Some(on_layout) = &entry.entry.on_layout {
            on_layout(rect);
        }
        if let Some(on_flow_layout) = &entry.entry.on_flow_layout {
            on_flow_layout(FlowRenderRegionLayout {
                rect,
                flow_end_row,
                bottom_row,
                bottom_height,
                viewport_height,
            });
        }
        row += entry.lines.len() as i64;
    }
}

/// Composes flow and bottom entries with a viewport-sized gap.
pub struct RootCompositor {
    get_height: Rc<dyn Fn() -> usize>,
    flow: Vec<RootLayoutEntry>,
    bottom: Vec<RootLayoutEntry>,
}

impl RootCompositor {
    pub fn new(options: RootCompositorOptions) -> Self {
        Self {
            get_height: options.get_height,
            flow: options.flow,
            bottom: options.bottom,
        }
    }
}

impl Component for RootCompositor {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let flow = render_entries(&self.flow, width);
        let bottom = render_entries(&self.bottom, width);
        let flow_height = total_height(&flow);
        let bottom_height = total_height(&bottom);
        let viewport_height = (self.get_height)() as i64;
        let gap_height = (viewport_height - flow_height - bottom_height).max(0);
        let bottom_row = flow_height + gap_height;

        notify_flow_layout(&flow, width, flow_height, bottom_row, bottom_height, viewport_height);
        notify_layout(&bottom, bottom_row, width);

        let mut lines = Vec::new();
        for entry in &flow {
            lines.extend(entry.lines.iter().cloned());
        }
        lines.extend(std::iter::repeat_n(JsString::default(), gap_height as usize));
        for entry in &bottom {
            lines.extend(entry.lines.iter().cloned());
        }
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        for entry in self.flow.iter().chain(self.bottom.iter()) {
            entry.component.borrow_mut().invalidate();
        }
    }
}
