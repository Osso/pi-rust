#[path = "support/oracle.rs"]
mod oracle;
#[path = "../examples/extensions/overlay_qa_tests.rs"]
mod overlay_qa_tests;
#[path = "support/extensions.rs"]
mod support;
#[path = "support/ui_thread.rs"]
mod ui_thread;

use futures_util::FutureExt;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
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
    fn unfocus_to(&self, _: Option<pi_tui::tui::ComponentRef>) {
        panic!("fixture does not exercise explicit focus targets")
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
    fn unfocus_to(&self, _: Option<pi_tui::tui::ComponentRef>) {
        panic!("fixture does not exercise explicit focus targets")
    }
    fn is_focused(&self) -> bool {
        true
    }
}
struct Terminal;
impl ExtensionUiTuiBackend for Terminal {
    fn request_component_render(&self, _identity: usize) {}
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("no editors")
    }
    fn request_render(&self) {
        REQUESTS.with(|count| count.set(count.get() + 1));
    }
    fn request_render_force(&self) {
        panic!("no forced redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("no terminal release")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("no terminal start")
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        120
    }

    fn show_overlay(
        &self,
        component: Box<dyn ExtensionUiComponent>,
        options: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        let panel = Rc::new(Panel {
            component,
            options: options.unwrap_or_default(),
            hidden: Cell::new(false),
            focused: Cell::new(false),
        });
        PANELS.with(|ps| ps.borrow_mut().push(panel.clone()));
        Rc::new(Handle(panel))
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
                .map(|(i, r)| {
                    (
                        JsString::from(r),
                        ColorValue::Text(JsString::from(format!("#{:02x}2020", 50 + i))),
                    )
                })
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
    mount_clock: Option<tokio::time::Instant>,
}
impl ExtensionUiContext for Ui {
    fn set_editor_text(&self, text: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        self.calls.lock().unwrap().push(json!(["editorText", text]));
        Ok(())
    }
    fn notify(
        &self,
        message: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.calls.lock().unwrap().push(json!(["notify", message, level]));
        Ok(())
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        opts: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let script = self.scripts.lock().unwrap().pop_front().unwrap_or(json!({}));
        let delay = script["delay"].as_u64().unwrap_or(0);
        let calls = self.calls.clone();
        if let Some(started) = self.mount_clock {
            calls
                .lock()
                .unwrap()
                .push(json!(["mount", started.elapsed().as_millis()]));
            let value = Rc::new(RefCell::new(None));
            let saved = value.clone();
            let component = factory(
                ExtensionUiFactoryContext {
                    tui: ExtensionUiTui::new(Arc::new(Terminal)),
                    theme: theme(),
                    keybindings: KeybindingsManager::new(Vec::new(), None),
                },
                ExtensionUiDone::new(Rc::new(move |result| *saved.borrow_mut() = result)),
            )
            .unwrap()
            .now_or_never()
            .expect("stack factory resolves immediately")
            .unwrap();
            component.handle_input(&JsString::from("\r"));
            component.dispose();
            let value = value.borrow().clone();
            return Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                Ok(value)
            });
        }
        let work = ui_thread::run(move || {
            Box::pin(async move {
                PANELS.with(|ps| ps.borrow_mut().clear());
                REQUESTS.with(|count| count.set(0));
                let result = Rc::new(RefCell::new(None));
                let saved = result.clone();
                let done_count = Rc::new(Cell::new(0));
                let count = done_count.clone();
                let done = ExtensionUiDone::new(Rc::new(move |v| {
                    count.set(count.get() + 1);
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
                .unwrap()
                .await
                .unwrap();
                component.set_focused(true);
                component.on_overlay_handle(Rc::new(MainHandle(calls.clone(), false)));
                let cloned = script["clone"].as_bool().unwrap_or(false).then(|| {
                    let cloned = factory(
                        ExtensionUiFactoryContext {
                            tui: ExtensionUiTui::new(Arc::new(Terminal)),
                            theme: theme(),
                            keybindings: KeybindingsManager::new(Vec::new(), None),
                        },
                        ExtensionUiDone::new(Rc::new(|_| {})),
                    )
                    .unwrap()
                    .now_or_never()
                    .expect("clone factory resolves immediately")
                    .unwrap();
                    cloned.on_overlay_handle(Rc::new(MainHandle(calls.clone(), true)));
                    cloned
                });
                if script["rawPanelProbe"] == true {
                    let panel = PANELS.with(|ps| ps.borrow()[0].clone());
                    for unit in [0xd800, 0xdc00] {
                        let raw = JsString::from_utf16(vec![unit]);
                        panel.component.handle_input(&raw);
                        let frame = JsString::join(panel.component.render(35), "\n");
                        assert!(frame.contains(&raw), "typed unit lost in panel frame: {frame:?}");
                        assert!(!frame.contains("\u{fffd}"), "typed unit replaced: {frame:?}");
                        panel.component.handle_input(&JsString::from("\x7f"));
                        let frame = JsString::join(panel.component.render(35), "\n");
                        assert!(!frame.contains(&raw), "backspace retained typed unit: {frame:?}");
                    }
                    component.dispose();
                    PANELS.with(|ps| ps.borrow_mut().clear());
                    return Ok(None);
                }
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
                    if step["ready"] == true {
                        wait_for_process_ready().await;
                    }
                    if let Some(event) = step.get("proc") {
                        use std::io::Write;
                        let path = pi_js_compat::process_env::get("OVERLAY_QA_CONTROL").unwrap();
                        let mut control = std::fs::OpenOptions::new().append(true).open(path).unwrap();
                        writeln!(control, "{event}").unwrap();
                    }
                    if step["exited"] == true {
                        wait_for_process_exit().await;
                        tokio::task::yield_now().await;
                    }
                    if step["running"] == true {
                        let frame = JsString::join(component.render(width), "\n");
                        assert!(
                            frame.contains("● Running"),
                            "exit must not finish before stdio closes: {frame:?}"
                        );
                        assert!(!frame.contains("✓ Done"));
                    }
                    if step["releaseTail"] == true {
                        let path = pi_js_compat::process_env::get("OVERLAY_QA_CONTROL").unwrap();
                        std::fs::write(format!("{path}.release"), "release").unwrap();
                    }
                    if let Some(text) = step["waitFor"].as_str() {
                        let mut ready = false;
                        for _ in 0..2000 {
                            if JsString::join(component.render(width), "\n").contains(text) {
                                ready = true;
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            tokio::task::yield_now().await;
                        }
                        assert!(ready, "process frame never contained {text}");
                    }
                    if step["dispose"] == true {
                        component.dispose();
                        if step["reap"] == true {
                            wait_for_process_exit().await;
                        }
                    }

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
                            p.component.handle_input(&JsString::from(key));
                        } else {
                            component.handle_input(&JsString::from(key));
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
                calls.lock().unwrap().push(json!([
                    "done",
                    result.is_some(),
                    result.clone().flatten(),
                    done_count.get()
                ]));
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
    if request["command"] == "overlay-overflow"
        && let Some(control) = pi_js_compat::process_env::get("OVERLAY_QA_CONTROL")
    {
        std::fs::write(&control, "").unwrap();
        let _ = std::fs::remove_file(format!("{control}.pid"));
        let release = format!("{control}.release");
        if std::path::Path::new(&release).exists() {
            std::fs::remove_file(release).unwrap();
        }
    }
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
        registry
            .get(&JsString::from("<example:overlay-qa-tests>"))
            .unwrap()
            .clone(),
        &JsString::from("/fixture"),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:overlay-qa-tests>"),
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
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
    let ui = Arc::new(Ui {
        scripts: Mutex::new(request["scripts"].as_array().unwrap().iter().cloned().collect()),
        calls: Arc::new(Mutex::new(Vec::new())),
        mount_clock: (request["mountClock"] == true).then(tokio::time::Instant::now),
    });
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Tui);
    let command = runner.get_command(request["command"].as_str().unwrap()).unwrap();
    (command.handler)(JsString::default(), runner.create_command_context())
        .await
        .unwrap();
    json!({"commands":commands,"calls":ui.calls.lock().unwrap().clone()})
}
#[tokio::test]
async fn streaming_panel_retains_raw_typed_units_until_backspace() {
    native(&json!({"command":"overlay-streaming","scripts":[{"rawPanelProbe":true}]})).await;
}

#[tokio::test]
async fn thirteen_commands_match_pinned_runtime_options_keys_and_ansi_frames() {
    let process_fixture = process_fixture();
    assert!(process_fixture.directory.path().is_dir());
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
    let output = format!(
        "{}{}partial",
        (0..20).map(|i| format!("line-{i}\n")).collect::<String>(),
        "\x1b]8;;file:///very-long-path\x07🚀中文".repeat(12)
    );
    fixtures.extend([
        json!({"command":"overlay-overflow","scripts":[{"steps":[{"ready":true},{"proc":{"stdout":output},"waitFor":"Streaming Output (21 lines)"},{"proc":{"stdout":"tail\n"},"waitFor":"Streaming Output (22 lines)"},{"proc":{"stderr":"  stderr fixture \n"},"waitFor":"Streaming Output (23 lines)"},{"key":"\x1b[A"},{"key":"\x1b[B"},{"width":9},{"proc":{"close":true},"waitFor":"Done"},{"key":"\x1b"}]}]}),
        json!({"command":"overlay-overflow","scripts":[{"steps":[{"ready":true},{"proc":{"stdout":"one\n"},"waitFor":"Streaming Output (1 lines)"},{"dispose":true,"reap":true},{"proc":{"stdout":"late-after-dispose\n"}},{"key":"\x1b"}]}]}),
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

#[tokio::test]
async fn focus_dispose_then_ctrl_c_does_not_complete() {
    let request = json!({"command":"overlay-focus","scripts":[{"steps":[{"dispose":true},{"key":"\x03"}]}]});
    let mut oracle = oracle::TsOracle::spawn(
        "example-overlay-qa-tests-oracle.ts",
        "packages/coding-agent/examples/extensions/overlay-qa-tests.ts",
    );
    let expected = oracle.request(&request);
    assert!(
        expected["calls"]
            .as_array()
            .unwrap()
            .contains(&json!(["done", false, null, 0]))
    );
    assert_eq!(native(&request).await, expected);
}

#[tokio::test]
async fn controller_close_and_dispose_lifecycle_matches_all_thirteen_commands() {
    let _process = process_fixture();
    let mut oracle = oracle::TsOracle::spawn(
        "example-overlay-qa-tests-oracle.ts",
        "packages/coding-agent/examples/extensions/overlay-qa-tests.ts",
    );
    for command in [
        "animation",
        "anchors",
        "margins",
        "stack",
        "overflow",
        "edge",
        "percent",
        "maxheight",
        "sidepanel",
        "toggle",
        "passive",
        "focus",
        "streaming",
    ] {
        for dispose in [false, true] {
            let mut steps = Vec::new();
            if command == "overflow" {
                steps.push(json!({"ready":true}));
            }
            if dispose {
                steps.push(json!({"dispose":true,"reap":command == "overflow"}));
            }
            steps.extend([json!({"key":"\x03"}), json!({"key":"\x03"})]);
            let script = json!({"steps":steps});
            let scripts = if command == "stack" {
                vec![script; 3]
            } else {
                vec![script]
            };
            let request = json!({"command":format!("overlay-{command}"),"scripts":scripts});
            let expected = oracle.request(&request);
            let actual = native(&request).await;
            assert_eq!(actual, expected, "lifecycle {command} dispose={dispose}");
        }
    }
}

#[tokio::test]
async fn overflow_done_waits_for_final_output_after_child_exit() {
    let _process = process_fixture();
    let request = json!({"command":"overlay-overflow","scripts":[{"steps":[
        {"ready":true}, {"proc":{"exitBeforeTail":"final-output-before-close\n"}},
        {"exited":true,"running":true}, {"releaseTail":true,"waitFor":"✓ Done"}, {"key":"\x1b"}
    ]}]});
    let mut oracle = oracle::TsOracle::spawn(
        "example-overlay-qa-tests-oracle.ts",
        "packages/coding-agent/examples/extensions/overlay-qa-tests.ts",
    );
    let immediate = json!({"command":"overlay-overflow","scripts":[{"steps":[
        {"ready":true}, {"proc":{"stdout":"final-output-before-close\n","close":true},"waitFor":"✓ Done"}, {"key":"\x1b"}
    ]}]});
    for request in [request, immediate] {
        let expected = oracle.request(&request);
        let actual = native(&request).await;
        for frame in actual["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call[0] == "frames")
        {
            let text = frame[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| line.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            if text.contains("✓ Done") {
                assert!(
                    text.contains("final-output-before-close"),
                    "Done frame must contain final output"
                );
            }
        }
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn sidepanel_saturates_at_both_menu_ends_against_pinned_source() {
    let mut steps = vec![json!({"key":"\x1b[A"})];
    steps.extend((0..6).map(|_| json!({"key":"\x1b[B"})));
    steps.extend((0..6).map(|_| json!({"key":"\x1b[A"})));
    steps.push(json!({"key":"\x1b"}));
    let request = json!({"command":"overlay-sidepanel","scripts":[{"steps":steps}]});
    let mut oracle = oracle::TsOracle::spawn(
        "example-overlay-qa-tests-oracle.ts",
        "packages/coding-agent/examples/extensions/overlay-qa-tests.ts",
    );
    assert_eq!(native(&request).await, oracle.request(&request));
}

#[test]
fn timestamp_normalization_preserves_exact_padding_and_hour_width() {
    let border = "\x1b[38;2;59;32;32m│\x1b[39m";
    for hour in 1..=12 {
        let mut short = json!(format!("[{hour}:02:03 AM]   {border}"));
        let mut padded = json!(format!(
            "[{hour:02}:59:58 PM]{}{}",
            " ".repeat(if hour < 10 { 2 } else { 3 }),
            border
        ));
        normalize_timestamps(&mut short);
        normalize_timestamps(&mut padded);
        assert_eq!(short, padded, "hour {hour}");
        assert_eq!(
            short,
            json!(format!(
                "[<local-time>]{}{border}",
                " ".repeat(if hour < 10 { 2 } else { 3 })
            ))
        );
        let mut wrong = json!(format!("[{hour:02}:59:58 PM]         {border}"));
        normalize_timestamps(&mut wrong);
        assert_ne!(short, wrong, "extra padding must remain observable");
    }
}

#[test]
#[should_panic(expected = "one-digit timestamp needs one padding space")]
fn timestamp_normalization_rejects_missing_padding() {
    normalize_timestamps(&mut json!("[1:02:03 AM]\x1b[38;2;59;32;32m│\x1b[39m"));
}

struct ProcessFixture {
    _environment_guard: std::sync::MutexGuard<'static, ()>,
    directory: tempfile::TempDir,
    path: String,
}
impl Drop for ProcessFixture {
    fn drop(&mut self) {
        pi_js_compat::process_env::set("PATH", &self.path);
        pi_js_compat::process_env::remove("OVERLAY_QA_CONTROL");
    }
}
fn process_fixture() -> ProcessFixture {
    use std::os::unix::fs::PermissionsExt;
    static ENVIRONMENT: Mutex<()> = Mutex::new(());
    let environment_guard = ENVIRONMENT.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("bash");
    std::fs::write(
        &program,
        r#"#!/usr/bin/python3
import os,sys,time,json,signal
control=os.environ['OVERLAY_QA_CONTROL']
def terminate(*_):
 sys.stdout.write('late-after-dispose\n');sys.stdout.flush();sys.exit(0)
signal.signal(signal.SIGTERM,terminate)
open(control+'.pid','w').write(str(os.getpid()))
position=0
while True:
 with open(control) as stream:
  stream.seek(position)
  for line in stream:
   event=json.loads(line)
   if 'stdout' in event:sys.stdout.write(event['stdout']);sys.stdout.flush()
   if 'stderr' in event:sys.stderr.write(event['stderr']);sys.stderr.flush()
   if 'exitBeforeTail' in event:
    if os.fork():sys.exit(0)
    deadline=time.monotonic()+2
    while not os.path.exists(control+'.release'):
     if time.monotonic()>deadline:os._exit(1)
     time.sleep(0.001)
    sys.stdout.write(event['exitBeforeTail']);sys.stdout.flush();os._exit(0)
   if event.get('close'):sys.exit(0)
  position=stream.tell()
 time.sleep(0.001)
"#,
    )
    .unwrap();
    std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = pi_js_compat::process_env::get("PATH").unwrap();
    pi_js_compat::process_env::set("PATH", &format!("{}:{path}", directory.path().display()));
    let control = directory.path().join("control");
    std::fs::write(&control, "").unwrap();
    pi_js_compat::process_env::set("OVERLAY_QA_CONTROL", control.to_str().unwrap());
    ProcessFixture {
        _environment_guard: environment_guard,
        directory,
        path,
    }
}
async fn wait_for_process_ready() {
    let control = pi_js_compat::process_env::get("OVERLAY_QA_CONTROL").unwrap();
    for _ in 0..2000 {
        if std::fs::read_to_string(format!("{control}.pid")).is_ok_and(|s| !s.is_empty()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        tokio::task::yield_now().await;
    }
    panic!("fixture process did not start");
}
async fn wait_for_process_exit() {
    let control = pi_js_compat::process_env::get("OVERLAY_QA_CONTROL").unwrap();
    let pid = std::fs::read_to_string(format!("{control}.pid")).unwrap();
    for _ in 0..2000 {
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        tokio::task::yield_now().await;
    }
    panic!("fixture process {pid} was not reaped after dispose");
}
fn normalize_timestamps(value: &mut Value) {
    match value {
        Value::String(s) => {
            let pattern = regex::Regex::new(r"\[\d{1,2}:\d{2}:\d{2} [AP]M\]").unwrap();
            if pattern.is_match(s) {
                let short_hour = regex::Regex::new(r"\[(\d):(\d{2}:\d{2} [AP]M)\]").unwrap();
                if short_hour.is_match(s) {
                    let padding = regex::Regex::new(" (\x1b\\[[0-9;]*m│\x1b\\[39m)$").unwrap();
                    assert!(padding.is_match(s), "one-digit timestamp needs one padding space: {s}");
                    *s = short_hour.replace_all(s, "[0$1:$2]").into_owned();
                    *s = padding.replace(s, "$1").into_owned();
                }
                *s = pattern.replace_all(s, "[<local-time>]").into_owned();
            }
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

#[tokio::test(start_paused = true)]
async fn stack_mounts_later_layers_at_absolute_four_hundred_ms_deadlines() {
    let request = json!({"command":"overlay-stack","mountClock":true,"scripts":[{"delay":200},{"delay":100},{}]});
    let result = native(&request).await;
    let mounts = result["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call[0] == "mount")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        mounts,
        vec![json!(["mount", 0]), json!(["mount", 400]), json!(["mount", 800])]
    );
    assert_eq!(
        result["calls"].as_array().unwrap().last().unwrap(),
        &json!(["notify", "Closed in order: Overlay 1, Overlay 2, Overlay 3", "info"])
    );
}

// Post-pin TS 4f9e6f8a4; pinned TS throws RangeError for the negative fill repeat at widths <= 3.
#[tokio::test]
async fn stack_renders_clamped_fill_rows_at_width_three() {
    let script = json!({"width":3,"steps":[{"key":"\r"}]});
    let request = json!({"command":"overlay-stack","scripts":[script, script, script]});
    let result = native(&request).await;
    let sgr = regex::Regex::new("\x1b\\[[0-9;]*m").unwrap();
    let frames = result["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call[0] == "frames")
        .map(|call| {
            call[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| sgr.replace_all(line.as_str().unwrap(), "").into_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let (truncated, empty) = ("│.│", "│ │");
    let mut frame = vec!["╭─╮", truncated, truncated, empty];
    frame.extend([truncated; 5]);
    frame.extend([empty, truncated, "╰─╯"]);
    assert_eq!(frames, vec![frame; 6]);
}
