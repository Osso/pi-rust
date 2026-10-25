//! Ported from `coding-agent/src/modes/interactive/interactive-root-compositor.ts`.

use std::rc::Rc;

use pi_tui::tui::{ComponentRef, FlowRenderRegionLayout};
use pi_tui::{RenderRegionRect, RootCompositor, RootCompositorOptions, RootLayoutEntry};

/// `InteractiveRootFlowLayout`.
pub type InteractiveRootFlowLayout = FlowRenderRegionLayout;

/// `InteractiveRootCompositorOptions`.
pub struct InteractiveRootCompositorOptions {
    pub get_height: Rc<dyn Fn() -> usize>,
    pub header: ComponentRef,
    pub loaded_resources: ComponentRef,
    pub chat: ComponentRef,
    pub on_chat_layout: Rc<dyn Fn(RenderRegionRect)>,
    pub transcript_tail: ComponentRef,
    pub pending_messages: ComponentRef,
    pub on_transcript_tail_layout: Rc<dyn Fn(InteractiveRootFlowLayout)>,
    pub status: ComponentRef,
    pub widget_above: ComponentRef,
    pub editor: ComponentRef,
    pub widget_below: ComponentRef,
    pub footer: ComponentRef,
    pub on_status_layout: Rc<dyn Fn(RenderRegionRect)>,
    pub on_widget_above_layout: Option<Rc<dyn Fn(RenderRegionRect)>>,
    pub on_editor_layout: Rc<dyn Fn(RenderRegionRect)>,
}

/// `createInteractiveRootCompositor`.
pub fn create_interactive_root_compositor(options: InteractiveRootCompositorOptions) -> RootCompositor {
    let mut widget_above = RootLayoutEntry::new(options.widget_above);
    if let Some(on_layout) = options.on_widget_above_layout {
        widget_above = widget_above.with_on_layout(on_layout);
    }
    RootCompositor::new(RootCompositorOptions {
        get_height: options.get_height,
        flow: vec![
            RootLayoutEntry::new(options.header),
            RootLayoutEntry::new(options.loaded_resources),
            RootLayoutEntry::new(options.chat).with_on_layout(options.on_chat_layout),
            RootLayoutEntry::new(options.transcript_tail).with_on_flow_layout(options.on_transcript_tail_layout),
            RootLayoutEntry::new(options.pending_messages),
        ],
        bottom: vec![
            RootLayoutEntry::new(options.status).with_on_layout(options.on_status_layout),
            widget_above,
            RootLayoutEntry::new(options.editor).with_on_layout(options.on_editor_layout),
            RootLayoutEntry::new(options.widget_below),
            RootLayoutEntry::new(options.footer),
        ],
    })
}
