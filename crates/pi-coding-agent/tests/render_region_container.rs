use crate::shared_oracle as oracle;
use crate::shared_tui_virtual_terminal as virtual_terminal;

use pi_coding_agent::modes::interactive::components::render_region_container::RenderRegionContainer;
use pi_js_compat::js_json::JsString;
use pi_tui::tui::{RenderRegionRect, TuiExt, TuiRenderer, component};
use pi_tui::{Component, RootCompositor, RootCompositorOptions, RootLayoutEntry};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Line {
    text: Rc<RefCell<JsString>>,
    renders: Rc<Cell<usize>>,
    disposed: Rc<Cell<usize>>,
}
impl Component for Line {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        self.renders.set(self.renders.get() + 1);
        vec![self.text.borrow().clone()]
    }
    fn dispose(&mut self) {
        self.disposed.set(self.disposed.get() + 1);
    }
}
struct Retired {
    id: usize,
    retired: Rc<RefCell<Vec<usize>>>,
}
impl Component for Retired {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        vec![format!("child {}", self.id).into()]
    }
    fn dispose(&mut self) {
        self.retired.borrow_mut().push(self.id);
    }
}

#[test]
fn scoped_membership_and_clear_match_frozen() {
    use serde_json::json;
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let expected = oracle.request(&json!({"id":1,"module":"modes/interactive/components/render-region-container.ts","operation":"renderRegionContainer","width":20}));
    assert_eq!(expected["ok"], true, "{expected}");
    let (_, handle) = virtual_terminal::concrete(20, 8);
    let tui = TuiRenderer::new(handle);
    let mut container = RenderRegionContainer::new(Rc::new(move |child| tui.create_render_region(child)));
    let retired = Rc::new(RefCell::new(Vec::new()));
    let children: Vec<_> = (0..3)
        .map(|id| {
            component(Retired {
                id,
                retired: retired.clone(),
            })
        })
        .collect();
    let mut results = Vec::new();
    container.add_child(children[0].clone());
    results.push(json!(container.request_child_render(&children[0]).unwrap()));
    container.track_scoped_child(children[0].clone());
    container.track_scoped_child(children[0].clone());
    container.track_scoped_child(children[1].clone());
    results.push(json!(container.request_child_render(&children[1]).unwrap()));
    results.push(json!(container.request_child_render(&children[0]).unwrap()));
    results.push(json!(container.render(20)));
    container.place(RenderRegionRect {
        row: 3,
        col: 1,
        width: 20,
        height: 1,
    });
    results.push(json!(container.request_child_render(&children[0]).unwrap()));
    container.remove_child(&children[0]);
    container.remove_child(&children[0]);
    results.push(json!(container.request_child_render(&children[0]).unwrap()));
    container.add_child(children[2].clone());
    results.push(json!(container.render(20)));
    container.clear();
    container.clear();
    results.push(json!(container.render(20)));
    assert_eq!(
        json!({"results":results,"retired":*retired.borrow()}),
        expected["output"]
    );
}

struct Shared(Rc<RefCell<RenderRegionContainer>>);
impl Component for Shared {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }
}

#[test]
fn scoped_chat_removal_retires_a_native_retry_timer() {
    use pi_coding_agent::modes::interactive::components::status_indicator::RetryStatusIndicator;
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let (_, handle) = virtual_terminal::concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let mut chat = RenderRegionContainer::new(Rc::new(move |child| tui.create_render_region(child)));
    let requests = Rc::new(Cell::new(0));
    let render_requests = requests.clone();
    let retry = Rc::new(RefCell::new(RetryStatusIndicator::new(
        Rc::new(move || render_requests.set(render_requests.get() + 1)),
        1.0,
        3.0,
        1000.0,
    )));
    let child: pi_tui::tui::ComponentRef = retry.clone();
    chat.track_scoped_child(child.clone());
    chat.add_child(child.clone());
    chat.remove_child(&child);
    let before = requests.get();
    let frame = retry.borrow_mut().render(80);
    assert!(
        !retry
            .borrow_mut()
            .poll_countdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
    );
    assert_eq!(requests.get(), before, "retired timer still requests screen writes");
    assert_eq!(retry.borrow_mut().render(80), frame);
}

#[test]
fn scoped_chat_updates_do_not_render_neighbors_and_retire_once() {
    let (terminal, handle) = virtual_terminal::concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let region = Rc::new(RefCell::new(RenderRegionContainer::new({
        let tui = tui.clone();
        Rc::new(move |child| tui.create_render_region(child))
    })));
    let text = Rc::new(RefCell::new(JsString::from("before")));
    let renders = Rc::new(Cell::new(0));
    let disposed = Rc::new(Cell::new(0));
    let child = component(Line {
        text: text.clone(),
        renders: renders.clone(),
        disposed: disposed.clone(),
    });
    region.borrow_mut().add_child(child.clone());
    assert!(!region.borrow_mut().request_child_render(&child).unwrap());
    region.borrow_mut().track_scoped_child(child.clone());
    assert!(!region.borrow_mut().request_child_render(&child).unwrap());
    let neighbor_renders = Rc::new(Cell::new(0));
    let neighbor = component(Line {
        text: Rc::new(RefCell::new("neighbor".into())),
        renders: neighbor_renders.clone(),
        disposed: Rc::new(Cell::new(0)),
    });
    let placed = region.clone();
    tui.add_child(component(RootCompositor::new(RootCompositorOptions {
        get_height: Rc::new(|| 8),
        flow: vec![
            RootLayoutEntry::new(component(Shared(region.clone())))
                .with_on_layout(Rc::new(move |rect| placed.borrow_mut().place(rect))),
            RootLayoutEntry::new(neighbor),
        ],
        bottom: vec![],
    })));
    tui.start().unwrap();
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
    let neighbor_count = neighbor_renders.get();
    *text.borrow_mut() = "after".into();
    assert!(region.borrow_mut().request_child_render(&child).unwrap());
    terminal.borrow_mut().flush();
    assert_eq!(&terminal.borrow().viewport()[..2], ["after", "neighbor"]);
    assert_eq!(neighbor_renders.get(), neighbor_count);
    region.borrow_mut().place(RenderRegionRect {
        row: 2,
        col: 0,
        width: 24,
        height: 1,
    });
    region.borrow_mut().remove_child(&child);
    assert_eq!(disposed.get(), 1);
    assert!(!region.borrow_mut().request_child_render(&child).unwrap());
    region.borrow_mut().clear();
    assert_eq!(disposed.get(), 1);
    tui.stop().unwrap();
}
