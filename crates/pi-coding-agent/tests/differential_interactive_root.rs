use crate::shared_oracle as oracle;

use pi_coding_agent::modes::interactive::interactive_root_compositor::{
    InteractiveRootCompositorOptions, create_interactive_root_compositor,
};
use pi_js_compat::js_json::JsString;
use pi_tui::tui::{ComponentRef, FlowRenderRegionLayout, component};
use pi_tui::{Component, RenderRegionRect};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Line(&'static str);
impl Component for Line {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        vec![self.0.into()]
    }
}
fn line(text: &'static str) -> ComponentRef {
    component(Line(text))
}
fn rect(value: RenderRegionRect) -> Value {
    json!([value.row, value.col, value.width, value.height])
}

#[test]
fn root_flow_and_bottom_callbacks_match_frozen_typescript() {
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for widget_layout in [false, true] {
        let height = Rc::new(Cell::new(12));
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let on_layout = |name: &'static str| {
            let events = Rc::clone(&events);
            Rc::new(move |value| events.borrow_mut().push(json!([name, rect(value)]))) as Rc<dyn Fn(RenderRegionRect)>
        };
        let on_tail = {
            let events = Rc::clone(&events);
            Rc::new(move |layout: FlowRenderRegionLayout| {
                events.borrow_mut().push(json!([
                    "tail",
                    rect(layout.rect),
                    layout.flow_end_row,
                    layout.bottom_row,
                    layout.bottom_height,
                    layout.viewport_height
                ]))
            })
        };
        let mut root = create_interactive_root_compositor(InteractiveRootCompositorOptions {
            get_height: {
                let height = Rc::clone(&height);
                Rc::new(move || height.get())
            },
            header: line("header"),
            loaded_resources: line("resources"),
            chat: line("chat"),
            transcript_tail: line("tail"),
            pending_messages: line("pending"),
            on_transcript_tail_layout: on_tail,
            status: line("status"),
            widget_above: line("above"),
            editor: line("editor"),
            widget_below: line("below"),
            footer: line("footer"),
            on_chat_layout: on_layout("chat"),
            on_widget_above_layout: widget_layout.then(|| on_layout("above")),
            on_status_layout: on_layout("status"),
            on_editor_layout: on_layout("editor"),
        });
        let sizes = [12, 14, 8, 4];
        let actual: Vec<_> = sizes
            .into_iter()
            .map(|size| {
                height.set(size);
                events.borrow_mut().clear();
                let frame = root.render(20);
                json!({"frame":frame,"events":events.borrow().clone()})
            })
            .collect();
        let expected=oracle.request(&json!({"id":"root","module":"modes/interactive/interactive-root-compositor.ts","operation":"rootCompositor","sizes":sizes,"width":20,"widgetLayout":widget_layout}));
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(json!(actual), expected["output"]);
    }
}
