#[path = "support/oracle.rs"]
mod oracle;
#[path = "../examples/extensions/overlay_qa_tests.rs"]
mod overlay_qa_tests;
#[path = "support/extensions.rs"]
mod support;
#[path = "support/ui_thread.rs"]
mod ui_thread;

use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    sync::{Arc, Mutex},
};

struct Panel {
    component: Box<dyn ExtensionUiComponent>,
    options: ExtensionUiOverlayOptions,
    hidden: Cell<bool>,
    focused: Cell<bool>,
}
thread_local! {static REQUESTS:Cell<usize>=const {Cell::new(0)};}
thread_local! {static PANELS:RefCell<Vec<Rc<Panel>>>=const {RefCell::new(Vec::new())};}
struct Handle(Rc<Panel>);
impl ExtensionUiOverlayHandle for Handle {
    fn hide(&self) {
        self.0.hidden.set(true);
        self.unfocus();
    }
    fn set_hidden(&self, hidden: bool) {
        self.0.hidden.set(hidden);
    }
    fn is_hidden(&self) -> bool {
        self.0.hidden.get()
    }
    fn focus(&self) {
        PANELS.with(|ps| {
            for p in ps.borrow().iter() {
                p.focused.set(false);
                p.component.set_focused(false);
            }
        });
        self.0.focused.set(true);
        self.0.component.set_focused(true);
    }
    fn unfocus(&self) {
        self.0.focused.set(false);
        self.0.component.set_focused(false);
    }
    fn is_focused(&self) -> bool {
        self.0.focused.get()
    }
}
struct MainHandle(Arc<Mutex<Vec<Value>>>, bool);
impl ExtensionUiOverlayHandle for MainHandle {
    fn hide(&self) {}
    fn set_hidden(&self, hidden: bool) {
        self.0
            .lock()
            .unwrap()
            .push(json!([if self.1 { "hiddenClone" } else { "hidden" }, hidden]));
    }
    fn is_hidden(&self) -> bool {
        false
    }
    fn focus(&self) {}
    fn unfocus(&self) {}
    fn is_focused(&self) -> bool {
        true
    }
}
struct Terminal;
impl ExtensionUiTuiBackend for Terminal {
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("no editors")
    }
    fn request_render(&self) {
        REQUESTS.with(|count| count.set(count.get() + 1));
    }
    fn request_render_force(&self) {
        panic!("no forced redraw")
    }
    fn stop(&self) {
        panic!("no terminal release")
    }
    fn start(&self) -> Result<(), String> {
        panic!("no terminal start")
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        120
    }
    fn mode(&self) -> pi_tui::tui::TuiMode {
        pi_tui::tui::TuiMode::Regular
    }
    fn show_overlay(
        &self,
        component: Box<dyn ExtensionUiComponent>,
        options: Option<ExtensionUiOverlayOptions>,
    ) -> Arc<dyn ExtensionUiOverlayHandle> {
        let panel = Rc::new(Panel {
            component,
            options: options.unwrap_or_default(),
            hidden: Cell::new(false),
            focused: Cell::new(false),
        });
        PANELS.with(|ps| ps.borrow_mut().push(panel.clone()));
        Arc::new(Handle(panel))
    }
}
fn theme() -> Arc<Theme> {
    let roles = [
        "accent",
        "dim",
        "muted",
        "text",
        "success",
        "error",
        "warning",
        "toolTitle",
        "borderMuted",
        "border",
        "selectedBg",
    ];
    Arc::new(
        Theme::new(
            &roles
                .into_iter()
                .enumerate()
                .map(|(i, r)| (r.to_owned(), ColorValue::Text(format!("#{:02x}2020", 50 + i))))
                .collect::<Vec<_>>(),
            &[],
            ColorMode::Truecolor,
            None,
            None,
        )
        .unwrap(),
    )
}
fn options(o: ExtensionUiOverlayOptions) -> Value {
    use pi_tui::tui::{OverlayAnchor, SizeValue};
    fn size(v: SizeValue) -> Value {
        match v {
            SizeValue::Absolute(n) => json!(n as i64),
            SizeValue::Percent(n) => json!(format!("{n}%")),
        }
    }
    let mut value = json!({});
    for (key, v) in [
        ("width", o.width),
        ("maxHeight", o.max_height),
        ("row", o.row),
        ("col", o.col),
    ] {
        if let Some(v) = v {
            value[key] = size(v);
        }
    }
    for (key, v) in [
        ("minWidth", o.min_width),
        ("offsetX", o.offset_x),
        ("offsetY", o.offset_y),
    ] {
        if let Some(v) = v {
            value[key] = json!(v);
        }
    }
    if let Some(anchor) = o.anchor {
        value["anchor"] = json!(match anchor {
            OverlayAnchor::Center => "center",
            OverlayAnchor::TopLeft => "top-left",
            OverlayAnchor::TopCenter => "top-center",
            OverlayAnchor::TopRight => "top-right",
            OverlayAnchor::LeftCenter => "left-center",
            OverlayAnchor::RightCenter => "right-center",
            OverlayAnchor::BottomLeft => "bottom-left",
            OverlayAnchor::BottomCenter => "bottom-center",
            OverlayAnchor::BottomRight => "bottom-right",
        });
    }
    if let Some(m) = o.margin {
        let mut margin = json!({});
        for (key, v) in [
            ("top", m.top),
            ("right", m.right),
            ("bottom", m.bottom),
            ("left", m.left),
        ] {
            if let Some(v) = v {
                margin[key] = json!(v);
            }
        }
        value["margin"] = margin;
    }
    if let Some(visible) = o.visible {
        value["visible"] = json!([visible(99, 40), visible(100, 40)]);
    }
    if o.non_capturing {
        value["nonCapturing"] = json!(true);
    }
    value
}
fn frames(component: &dyn ExtensionUiComponent, width: usize) -> Value {
    let panels = PANELS.with(|ps| ps.borrow().clone());
    json!(["frames",component.render(width),panels.iter().map(|p|json!({"options":options(p.options.clone()),"hidden":p.hidden.get(),"lines":p.component.render(match p.options.width {Some(pi_tui::tui::SizeValue::Absolute(w))=>w as usize,_=>34})})).collect::<Vec<_>>()])
}
struct Ui {
    scripts: Mutex<VecDeque<Value>>,
    calls: Arc<Mutex<Vec<Value>>>,
}
impl ExtensionUiContext for Ui {
    fn set_editor_text(&self, text: &str) {
        self.calls.lock().unwrap().push(json!(["editorText", text]));
    }
    fn notify(&self, message: &str, level: Option<NotifyType>) {
        self.calls.lock().unwrap().push(json!(["notify", message, level]));
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        opts: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        let script = self.scripts.lock().unwrap().pop_front().unwrap_or(json!({}));
        let delay = script["delay"].as_u64().unwrap_or(0);
        let calls = self.calls.clone();
        let work = ui_thread::run(move || {
            Box::pin(async move {
                PANELS.with(|ps| ps.borrow_mut().clear());
                REQUESTS.with(|count| count.set(0));
                let result = Rc::new(RefCell::new(None));
                let saved = result.clone();
                let done = ExtensionUiDone::new(Arc::new(move |v| {
                    *saved.borrow_mut() = Some(v);
                }));
                let component = factory(
                    ExtensionUiFactoryContext {
                        tui: ExtensionUiTui::new(Arc::new(Terminal)),
                        theme: theme(),
                        keybindings: KeybindingsManager::new(Vec::new(), None),
                    },
                    done,
                )
                .unwrap();
                component.set_focused(true);
                component.on_overlay_handle(Arc::new(MainHandle(calls.clone(), false)));
                let cloned = script["clone"].as_bool().unwrap_or(false).then(|| {
                    let cloned = factory(
                        ExtensionUiFactoryContext {
                            tui: ExtensionUiTui::new(Arc::new(Terminal)),
                            theme: theme(),
                            keybindings: KeybindingsManager::new(Vec::new(), None),
                        },
                        ExtensionUiDone::new(Arc::new(|_| {})),
                    )
                    .unwrap();
                    cloned.on_overlay_handle(Arc::new(MainHandle(calls.clone(), true)));
                    cloned
                });
                let opts = opts.unwrap();
                assert_eq!(opts.overlay, Some(true));
                calls
                    .lock()
                    .unwrap()
                    .push(json!(["options", options(opts.overlay_options.unwrap()())]));
                let width = script["width"].as_u64().unwrap_or(60) as usize;
                calls.lock().unwrap().push(frames(component.as_ref(), width));
                tokio::task::yield_now().await;
                for step in script["steps"].as_array().into_iter().flatten() {
                    if let Some(ms) = step["ms"].as_u64() {
                        for _ in 0..ms {
                            tokio::time::advance(std::time::Duration::from_millis(1)).await;
                            tokio::task::yield_now().await;
                        }
                    }
                    let projected = step["units"].as_array().map(|units| {
                        String::from_utf16_lossy(&units.iter().map(|u| u.as_u64().unwrap() as u16).collect::<Vec<_>>())
                    });
                    if let Some(key) = step["key"].as_str().or(projected.as_deref()) {
                        if let Some(index) = step["panel"].as_u64() {
                            let p = PANELS.with(|ps| ps.borrow()[index as usize].clone());
                            p.component.handle_input(key);
                        } else {
                            component.handle_input(key);
                        }
                    }
                    tokio::task::yield_now().await;
                    calls.lock().unwrap().push(frames(
                        component.as_ref(),
                        step["width"].as_u64().unwrap_or(width as u64) as usize,
                    ));
                }
                component.invalidate();
                component.dispose();
                if let Some(cloned) = cloned {
                    cloned.dispose();
                }
                calls
                    .lock()
                    .unwrap()
                    .push(json!(["requests", REQUESTS.with(Cell::get)]));
                let result = result.borrow().clone();
                calls
                    .lock()
                    .unwrap()
                    .push(json!(["done", result.is_some(), result.clone().flatten()]));
                PANELS.with(|ps| ps.borrow_mut().clear());
                Ok(result.flatten())
            })
        });
        Box::pin(async move {
            let result = work.await;
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            result
        })
    }
}
async fn native(request: &Value) -> Value {
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(
        "<example:overlay-qa-tests>",
        Arc::new(|api| {
            let result = overlay_qa_tests::overlay_qa_tests_extension(api);
            Box::pin(async move { result })
        }),
    );
    let extension = load_extension_from_factory(
        registry.get("<example:overlay-qa-tests>").unwrap().clone(),
        "/fixture",
        support::event_bus(),
        runtime.clone(),
        "<example:overlay-qa-tests>",
    )
    .await
    .unwrap();
    let commands = extension
        .read()
        .commands
        .iter()
        .map(|(name, c)| json!([name, c.description]))
        .collect::<Vec<_>>();
    let mut runner = support::runner_for(vec![extension], "/fixture", runtime, support::model_registry().await);
    runner.bind_core(support::actions(), support::context_actions(), None);
    let ui = Arc::new(Ui {
        scripts: Mutex::new(request["scripts"].as_array().unwrap().iter().cloned().collect()),
        calls: Arc::new(Mutex::new(Vec::new())),
    });
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Tui);
    let command = runner.get_command(request["command"].as_str().unwrap()).unwrap();
    (command.handler)(String::new(), runner.create_command_context())
        .await
        .unwrap();
    json!({"commands":commands,"calls":ui.calls.lock().unwrap().clone()})
}
#[tokio::test]
async fn thirteen_commands_match_pinned_runtime_options_keys_and_ansi_frames() {
    let mut oracle = oracle::TsOracle::spawn(
        "example-overlay-qa-tests-oracle.ts",
        "packages/coding-agent/examples/extensions/overlay-qa-tests.ts",
    );
    let mut fixtures = vec![
        json!({"command":"overlay-animation","scripts":[{"steps":[{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-anchors","scripts":[{"steps":[{"key":" "}]},{"steps":[{"key":"\r"}]}]}),
        json!({"command":"overlay-margins","scripts":[{"steps":[{"key":" "}]},{"steps":[{"key":" "}]},{"steps":[{"key":" "}]},{"steps":[{"key":" "}]},{"steps":[{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-stack","scripts":[{"steps":[{"key":"\r"}]},{"steps":[{"key":"\u{001b}"}]},{"steps":[{"key":"\u{0003}"}]}]}),
        json!({"command":"overlay-overflow","scripts":[{"steps":[{"key":"\u{001b}[A"},{"key":"\u{001b}[B"},{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-edge","scripts":[{"steps":[{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-percent","scripts":[{"steps":[{"key":" "}]},{"steps":[{"key":" "}]},{"steps":[{"key":" "}]},{"steps":[{"key":" "}]},{"steps":[{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-maxheight","scripts":[{"steps":[{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-sidepanel","scripts":[{"steps":[{"key":"\u{001b}[A"},{"key":"\u{001b}[B"},{"key":"\r"},{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-toggle","scripts":[{"steps":[{"key":"t"},{"key":"t"},{"ms":1000},{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-passive","scripts":[{"steps":[{"key":"x"},{"key":"é"},{"key":"🚀"},{"key":"\u{007f}"},{"ms":1000},{"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-focus","scripts":[{"steps":[{"panel":0,"key":"hello"},{"panel":0,"key":"\u{001b}[D"},{"panel":0,"key":"\r"},{"panel":0,"key":"\t"},{"panel":1,"key":"\u{001b}[Z"},{"panel":0,"key":"\u{001b}"},{"panel":1,"key":"\u{0004}"},{"panel":2,"key":"\u{001b}"}]}]}),
        json!({"command":"overlay-streaming","scripts":[{"steps":[{"key":"\t"},{"panel":0,"key":"z"},{"panel":0,"key":"\t"},{"panel":1,"key":"\t"},{"panel":2,"key":"\t"},{"key":"\u{001b}"}]}]}),
    ];
    for (command, count) in [("overlay-anchors", 9), ("overlay-margins", 5), ("overlay-percent", 5)] {
        let mut scripts = (0..count)
            .map(|_| json!({"width":9,"steps":[{"key":"\x1b[C"}]}))
            .collect::<Vec<_>>();
        scripts.push(json!({"steps":[{"key":"\x1b"}]}));
        fixtures.push(json!({"command":command,"scripts":scripts}));
    }
    fixtures.extend([
        json!({"command":"overlay-toggle","scripts":[{"clone":true,"steps":[{"key":"t"},{"ms":1000},{"key":"\x1b"}]}]}),
        json!({"command":"overlay-animation","scripts":[{"steps":[{"ms":1100},{"width":1},{"width":7},{"key":"\x03"},{"ms":100}]}]}),
        json!({"command":"overlay-passive","scripts":[{"steps":[{"ms":61000},{"units":[55296]},{"key":"\x7f"},{"key":"\x03"},{"ms":1000}]}]}),
        json!({"command":"overlay-streaming","scripts":[{"steps":[{"ms":5000},{"key":"\t"},{"panel":0,"key":"A"},{"panel":0,"key":"é"},{"panel":0,"key":"🚀"},{"panel":0,"key":"\x7f"},{"panel":0,"key":"\x03"},{"ms":500}]}]}),
        json!({"command":"overlay-focus","scripts":[{"width":12,"steps":[{"panel":0,"key":"é🚀中"},{"panel":0,"key":"\x7f"},{"panel":0,"key":"\x1b[C"},{"panel":0,"key":"\x1b[A"},{"panel":0,"key":"\x1b[B"},{"panel":0,"key":"\x1b[Z"},{"panel":2,"key":"\x03"}]}]}),
        json!({"command":"overlay-stack","scripts":[{"delay":1100,"steps":[{"key":"\r"}]},{"delay":600,"steps":[{"key":"\r"}]},{"delay":100,"steps":[{"key":"\r"}]}]}),
    ]);
    for request in fixtures {
        let mut expected = oracle.request(&request);
        let mut actual = native(&request).await;
        normalize_timestamps(&mut expected);
        normalize_timestamps(&mut actual);
        if actual != expected {
            std::fs::write(
                std::env::temp_dir().join("overlay-qa-actual.json"),
                serde_json::to_vec_pretty(&actual).unwrap(),
            )
            .unwrap();
            std::fs::write(
                std::env::temp_dir().join("overlay-qa-expected.json"),
                serde_json::to_vec_pretty(&expected).unwrap(),
            )
            .unwrap();
            panic!("differential mismatch: {request}; /tmp/overlay-qa-{{actual,expected}}.json");
        }
    }
}

fn normalize_timestamps(value: &mut Value) {
    match value {
        Value::String(s) => {
            let pattern = regex::Regex::new(r"\[\d{1,2}:\d{2}:\d{2} [AP]M\]").unwrap();
            *s = pattern.replace_all(s, "[<local-time>]").into_owned();
        }
        Value::Array(items) => {
            for item in items {
                normalize_timestamps(item);
            }
        }
        Value::Object(items) => {
            for item in items.values_mut() {
                normalize_timestamps(item);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn stack_mounts_later_layers_at_absolute_four_hundred_ms_deadlines() {
    let request = json!({"command":"overlay-stack","scripts":[{"delay":200,"steps":[{"key":"\r"}]},{"delay":100,"steps":[{"key":"\r"}]},{"steps":[{"key":"\r"}]}]});
    let started = std::time::Instant::now();
    let result = native(&request).await;
    let elapsed = started.elapsed();
    assert_eq!(
        result["calls"].as_array().unwrap().last().unwrap(),
        &json!(["notify", "Closed in order: Overlay 1, Overlay 2, Overlay 3", "info"])
    );
    assert!(
        elapsed >= std::time::Duration::from_millis(790) && elapsed < std::time::Duration::from_millis(1050),
        "stack elapsed {elapsed:?}"
    );
}
