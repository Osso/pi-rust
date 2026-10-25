use crate::shared_virtual_terminal as virtual_terminal;
use pi_js_compat::js_json::JsString;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use pi_tui::TUI;
use pi_tui::components::loader::{DEFAULT_INTERVAL_MS, Loader, LoaderIndicatorOptions};
use pi_tui::tui::{Component, ComponentRef, TuiExt, component};
use virtual_terminal::concrete;

struct RenderCounter {
    renders: Rc<Cell<usize>>,
}

impl Component for RenderCounter {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        self.renders.set(self.renders.get() + 1);
        vec!["unrelated".into()]
    }
}

fn identity() -> pi_tui::components::StyleFn {
    Rc::new(|text: &JsString| text.clone())
}

#[test]
fn advances_the_default_spinner_no_more_than_four_times_per_second() {
    let renders = Rc::new(Cell::new(0));
    let counter = Rc::clone(&renders);
    let mut loader = Loader::new(
        Rc::new(move || counter.set(counter.get() + 1)),
        identity(),
        identity(),
        "Loading...",
        None,
    );

    assert_eq!(renders.get(), 1, "construction renders the initial frame");
    assert_eq!(loader.interval_ms(), DEFAULT_INTERVAL_MS);
    let deadline = loader.next_deadline().expect("animation is scheduled");

    assert!(!loader.poll(deadline - Duration::from_millis(1)));
    assert_eq!(renders.get(), 1, "default spinner should not advance before 250 ms");
    assert!(loader.poll(deadline));
    assert_eq!(renders.get(), 2, "default spinner should advance at 250 ms");
    loader.stop();
    assert!(!loader.is_running());
}

#[test]
fn empty_frames_hide_the_indicator_and_disable_animation() {
    let mut loader = Loader::new(
        Rc::new(|| {}),
        identity(),
        identity(),
        "Loading",
        Some(LoaderIndicatorOptions {
            frames: Some(Vec::new()),
            interval_ms: None,
        }),
    );
    loader.set_message("Updated");

    assert!(!loader.is_running());
    assert!(loader.next_deadline().is_none());
    let lines = loader.render(20);
    assert_eq!(lines[0], "");
    assert_eq!(lines[1].trim(), "Updated");
}

#[test]
fn single_frame_indicator_renders_verbatim_without_animation() {
    let mut loader = Loader::new(
        Rc::new(|| {}),
        Rc::new(|text: &JsString| JsString::from("<") + text + ">"),
        identity(),
        "Working",
        Some(LoaderIndicatorOptions {
            frames: Some(vec!["*".into()]),
            interval_ms: Some(10),
        }),
    );

    assert!(!loader.is_running());
    assert_eq!(loader.render(20)[1].trim(), "* Working");
}

#[test]
fn styled_frames_and_messages_apply_callbacks() {
    let mut loader = Loader::new(
        Rc::new(|| {}),
        Rc::new(|text: &JsString| JsString::from("<") + text + ">"),
        Rc::new(|text: &JsString| JsString::from("{") + text + "}"),
        "Working",
        Some(LoaderIndicatorOptions {
            frames: Some(vec!["a".into(), "b".into()]),
            interval_ms: Some(10),
        }),
    );

    let first = loader.render(20)[1].clone();
    assert_eq!(first.trim(), "a {Working}");
    let deadline = loader.next_deadline().unwrap();
    assert!(loader.poll(deadline));
    assert_eq!(loader.current_frame(), 1);
    let second = loader.render(20)[1].clone();
    assert_eq!(second.trim(), "b {Working}");
    loader.stop();
}

#[test]
fn falls_back_to_a_normal_root_render_when_no_region_is_registered() {
    let (terminal, handle) = concrete(30, 6);
    let tui = TUI::new(handle);
    let renders = Rc::new(Cell::new(0));
    let counter = Rc::clone(&renders);
    let slot: Rc<RefCell<Option<ComponentRef>>> = Rc::new(RefCell::new(None));
    let tui_ref = Rc::clone(&tui);
    let slot_ref = Rc::clone(&slot);
    let loader = Loader::new(
        Rc::new(move || {
            if let Some(component) = slot_ref.borrow().as_ref() {
                tui_ref.request_component_render(component).unwrap();
            }
        }),
        identity(),
        identity(),
        "Loading",
        Some(LoaderIndicatorOptions {
            frames: Some(Vec::new()),
            interval_ms: None,
        }),
    );
    let loader: ComponentRef = component(loader);
    *slot.borrow_mut() = Some(Rc::clone(&loader));

    tui.add_child(Rc::clone(&loader));
    tui.add_child(component(RenderCounter {
        renders: Rc::clone(&counter),
    }));
    tui.start().unwrap();
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
    let unrelated_render_count = renders.get();

    {
        let mut loader_ref = loader.borrow_mut();
        let loader_ref = loader_ref
            .as_any_mut()
            .expect("loader exposes a downcast hook")
            .downcast_mut::<Loader>()
            .expect("loader component");
        loader_ref.set_message("Updated");
    }
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();

    assert_eq!(renders.get(), unrelated_render_count + 1);
    assert!(terminal.borrow().viewport().join("\n").contains("Updated"));
    tui.stop().unwrap();
}
