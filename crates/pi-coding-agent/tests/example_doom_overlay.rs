#[path = "../examples/extensions/doom_overlay/index.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;
#[path = "support/ui_thread.rs"]
mod ui_thread;

use futures_util::FutureExt;
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::tui::TuiExt;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};
type Trace = Arc<Mutex<Vec<Value>>>;

struct Module {
    trace: Trace,
    current: Arc<Mutex<Value>>,
}
impl example::doom_engine::DoomModule for Module {
    fn create_path(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn create_data_file(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn malloc(&mut self, _: u32) -> Result<u32, String> {
        Ok(64)
    }
    fn set_value(&mut self, _: u32, _: u32, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn create(&mut self, _: u32, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn free(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn frame_buffer(&self) -> Result<u32, String> {
        Ok(1024)
    }
    fn width(&self) -> Result<usize, String> {
        Ok(if self.current.lock().unwrap()["emptyFrame"] == true {
            0
        } else {
            2
        })
    }
    fn height(&self) -> Result<usize, String> {
        Ok(if self.current.lock().unwrap()["emptyFrame"] == true {
            0
        } else {
            2
        })
    }
    fn get_i32(&self, ptr: u32) -> Result<i32, String> {
        if self.current.lock().unwrap()["frameError"] == true {
            return Err("fixture frame failed".into());
        }
        Ok([0x00ff0000, 0x0000ff00, 0x000000ff, 0x00ffffff][((ptr - 1024) / 4) as usize])
    }
    fn tick(&mut self) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["tick"]));
        if self.current.lock().unwrap()["tickError"] == true {
            Err("fixture tick failed".into())
        } else {
            Ok(())
        }
    }
    fn push_key(&mut self, pressed: bool, key: u32) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["key", u32::from(pressed), key]));
        let mut current = self.current.lock().unwrap();
        let count = current["keyCalls"].as_u64().unwrap_or(0) + 1;
        current["keyCalls"] = json!(count);
        if current["keyErrorAt"].as_u64() == Some(count) {
            Err("fixture key failed".into())
        } else {
            Ok(())
        }
    }
}
struct Backend(Trace, bool);
impl ExtensionUiTuiBackend for Backend {
    fn request_component_render(&self, _identity: usize) {}
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("Doom does not create an editor")
    }
    fn request_render(&self) {
        self.0.lock().unwrap().push(json!(["requestRender"]));
        if self.1 {
            pi_coding_agent::utils::uncaught::throw_uncaught("Error", "fixture render request failed");
        }
    }
    fn request_render_force(&self) {
        panic!("no force render")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("no terminal stop")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("no terminal start")
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        90
    }

    fn show_overlay(
        &self,
        _: Box<dyn ExtensionUiComponent>,
        _: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        panic!("no nested overlay")
    }
}
#[derive(Default)]
struct Terminal {
    events: Vec<pi_tui::terminal::TerminalEvent>,
}
impl pi_tui::terminal::Terminal for Terminal {
    fn start(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }
    fn stop(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }
    fn drain_input(&mut self, _: u64, _: u64) {}
    fn write(&mut self, _: &JsString) {}
    fn columns(&self) -> usize {
        90
    }
    fn rows(&self) -> usize {
        40
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, _: isize) {}
    fn hide_cursor(&mut self) {}
    fn show_cursor(&mut self) {}
    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _: &JsString) {}
    fn set_progress(&mut self, _: bool) {}
    fn poll_events(&mut self) -> Vec<pi_tui::terminal::TerminalEvent> {
        std::mem::take(&mut self.events)
    }
}
struct Input(Rc<dyn ExtensionUiComponent>);
impl pi_tui::tui::Component for Input {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.render(width)
    }
    fn handle_input(&mut self, data: &JsString) {
        self.0.handle_input(&JsString::from(data));
    }
    fn invalidate(&mut self) {
        self.0.invalidate();
    }
    fn wants_key_release(&self) -> bool {
        self.0.wants_key_release()
    }
}
struct Ui {
    trace: Trace,
    current: Arc<Mutex<Value>>,
    index: bool,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Result<Arc<pi_coding_agent::modes::interactive::theme::Theme>, pi_ai::request::CallbackError> {
        Ok(Arc::new(
            pi_coding_agent::modes::interactive::theme::load_theme_from_path(
                &JsString::from(
                    (&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json"))
                        .to_str()
                        .expect("fixture path must be UTF-8"),
                ),
                Some(pi_coding_agent::modes::interactive::theme::ColorMode::Truecolor),
            )
            .unwrap(),
        ))
    }
    fn notify(
        &self,
        text: &JsString,
        kind: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.trace.lock().unwrap().push(json!(["notify", text, kind]));
        Ok(())
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let trace = self.trace.clone();
        let current = self.current.lock().unwrap().clone();
        let theme = self.theme().unwrap();
        let index = self.index;
        ui_thread::run(move || {
            Box::pin(async move {
                if index {
                    let options = options.unwrap();
                    let overlay = options.overlay_options.unwrap()();
                    trace.lock().unwrap().push(json!(["custom",{"overlay":options.overlay,"overlayOptions":{
                    "width":match overlay.width.unwrap(){pi_tui::tui::SizeValue::Percent(s)=>json!(format!("{s}%")),_=>panic!("expected percent")},
                    "maxHeight":match overlay.max_height.unwrap(){pi_tui::tui::SizeValue::Percent(s)=>json!(format!("{s}%")),_=>panic!("expected percent")},
                    "anchor":"center","margin":{"top":1}
                }}]));
                    assert_eq!(overlay.anchor, Some(pi_tui::tui::OverlayAnchor::Center));
                    assert_eq!(overlay.margin.unwrap().top, Some(1));
                }
                if let Some(error) = current["customError"].as_str() {
                    return Err(JsString::from(error));
                }
                let value = Arc::new(Mutex::new(None));
                let captured = value.clone();
                let actions = trace.clone();
                let component = match factory(
                    ExtensionUiFactoryContext {
                        tui: ExtensionUiTui::new(Arc::new(Backend(trace.clone(), current["renderError"] == true))),
                        theme,
                        keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
                    },
                    ExtensionUiDone::new(Rc::new(move |result| {
                        *captured.lock().unwrap() = result.clone();
                        actions.lock().unwrap().push(json!(["done", result]));
                    })),
                ) {
                    Ok(pending) => pending.await.expect("custom component fixture continuation"),
                    Err(error) => {
                        if index {
                            return Err(error);
                        }
                        trace
                            .lock()
                            .unwrap()
                            .push(json!(["constructorError", js_format!("Error: {}", error)]));
                        return Ok(None);
                    }
                };
                let component: Rc<dyn ExtensionUiComponent> = component.into();
                trace
                    .lock()
                    .unwrap()
                    .push(json!(["wantsKeyRelease", component.wants_key_release()]));
                let terminal = Rc::new(RefCell::new(Terminal::default()));
                let host = pi_tui::tui::TuiRenderer::new(terminal.clone());
                let input = pi_tui::tui::component(Input(component.clone()));
                host.add_child(input.clone());
                host.set_focus(Some(input));
                tokio::task::yield_now().await;
                let steps = current["steps"]
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| vec![json!({"input":"q"})]);
                for step in steps {
                    if let Some(width) = step["render"].as_u64() {
                        let frame = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            component.render(width as usize)
                        })) {
                            Ok(frame) => json!(frame),
                            Err(error) => {
                                json!({"error":pi_coding_agent::utils::uncaught::format_uncaught(error.as_ref())})
                            }
                        };
                        trace.lock().unwrap().push(json!(["render", width, frame]));
                    }
                    if let Some(data) = step["input"].as_str() {
                        let data = step["units"]
                            .as_array()
                            .map(|u| {
                                String::from_utf16_lossy(
                                    &u.iter().map(|u| u.as_u64().unwrap() as u16).collect::<Vec<_>>(),
                                )
                            })
                            .unwrap_or_else(|| data.to_owned());
                        terminal
                            .borrow_mut()
                            .events
                            .push(pi_tui::terminal::TerminalEvent::Input(JsString::from(data)));
                        if let Err(error) =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.process_events()))
                        {
                            trace.lock().unwrap().push(json!([
                                "uncaught",
                                pi_coding_agent::utils::uncaught::format_uncaught(error.as_ref())
                            ]));
                        }
                    }
                    if let Some(ms) = step["ms"].as_u64() {
                        tokio::time::advance(Duration::from_millis(ms)).await;
                        tokio::task::yield_now().await;
                    }
                    if step["invalidate"] == true {
                        component.invalidate();
                    }
                    if step["dispose"] == true {
                        component.dispose();
                    }
                }
                component.dispose();
                tokio::time::advance(Duration::from_secs(2)).await;
                tokio::task::yield_now().await;
                let result = value.lock().unwrap().clone();
                Ok(result)
            })
        }).map(|result| result.map_err(pi_js_compat::callback_error::CallbackError::from)).boxed()
    }
}
fn engine(path: &JsString, trace: Trace, current: Arc<Mutex<Value>>) -> example::doom_engine::DoomEngine {
    let mut engine = example::doom_engine::DoomEngine::new(pi_js_compat::js_json::encode_node_utf8(path).into());
    engine
        .init_with_module(Box::new(Module { trace, current }))
        .map_err(|error| error.message())
        .unwrap();
    assert!(engine.is_initialized());
    engine
}
async fn compare(mut request: Value) {
    let temp = tempfile::tempdir().unwrap();
    let wad = temp.path().join("fixture.wad");
    std::fs::write(&wad, b"IWAD-fixture").unwrap();
    let other = temp.path().join("other.wad");
    std::fs::write(&other, b"IWAD-other").unwrap();
    let replace = |s: &str| {
        s.replace("$wad", wad.to_str().unwrap())
            .replace("$other", other.to_str().unwrap())
    };
    if let Some(commands) = request["commands"].as_array_mut() {
        for item in commands {
            for field in ["args", "wad"] {
                if let Some(s) = item[field].as_str() {
                    item[field] = json!(replace(s));
                }
            }
        }
    }
    let trace: Trace = Arc::default();
    let current = Arc::new(Mutex::new(request.clone()));
    let index = request["kind"] == "index";
    let factory: ExtensionFactory = if index {
        let trace = trace.clone();
        let current = current.clone();
        Arc::new(move |api| {
            let ensure_trace = trace.clone();
            let ensure_current = current.clone();
            let ensure = Arc::new(move || {
                let trace = ensure_trace.clone();
                let current = ensure_current.clone();
                Box::pin(async move {
                    trace.lock().unwrap().push(json!(["ensureWad"]));
                    current.lock().unwrap()["wad"].as_str().map(JsString::from)
                }) as BoxFuture<'static, Option<JsString>>
            });
            let load_trace = trace.clone();
            let load_current = current.clone();
            let load = Arc::new(move |path: JsString| {
                let trace = load_trace.clone();
                let current = load_current.clone();
                Box::pin(async move {
                    trace
                        .lock()
                        .unwrap()
                        .extend([json!(["new", path]), json!(["init", path])]);
                    if let Some(error) = current.lock().unwrap()["initError"].as_str() {
                        return Err(JsString::from(error));
                    }
                    Ok(engine(&path, trace, current))
                }) as BoxFuture<'static, Result<example::doom_engine::DoomEngine, JsString>>
            });
            let result = example::doom_overlay_extension(api, ensure, load);
            Box::pin(async move { result })
        })
    } else {
        let engine = Arc::new(Mutex::new(engine(
            &JsString::from(wad.to_str().unwrap()),
            trace.clone(),
            current.clone(),
        )));
        let resume = request["resume"] == true;
        Arc::new(move |api| {
            let engine = engine.clone();
            let result = api.register_command(
                "fixture",
                None,
                None,
                Arc::new(move |_, ctx| {
                    let engine = engine.clone();
                    Box::pin(async move {
                        ctx.ui()?
                            .custom(
                                Arc::new(move |context, done| {
                                    pi_coding_agent::core::extensions::ready_custom_component(Box::new(
                                        example::doom_component::DoomOverlayComponent::new(
                                            context.tui,
                                            engine.clone(),
                                            done,
                                            resume,
                                        )?,
                                    ))
                                }),
                                None,
                            )
                            .await?;
                        Ok(())
                    })
                }),
            );
            Box::pin(async move { result.map_err(HandlerError::new) })
        })
    };
    let runtime = support::runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register("<doom-fixture>", factory);
    let extension = load_extension_from_factory(
        registry.get(&JsString::from("<doom-fixture>")).unwrap().clone(),
        &JsString::from(temp.path().to_str().unwrap()),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<doom-fixture>"),
    )
    .await
    .unwrap();
    let name = if index { "doom-overlay" } else { "fixture" };
    let command = extension.read().commands[name].clone();
    if index {
        trace
            .lock()
            .unwrap()
            .push(json!(["command", command.name, command.description]));
    }
    let models = support::model_registry().await;
    let mut runner = support::runner_for(vec![extension], temp.path().to_str().unwrap(), runtime, models);
    support::bind(&mut runner);
    let ui = Arc::new(Ui {
        trace: trace.clone(),
        current: current.clone(),
        index,
    });
    let commands = if index {
        request["commands"].as_array().unwrap().clone()
    } else {
        vec![request.clone()]
    };
    for item in commands {
        *current.lock().unwrap() = item.clone();
        let mode = match item["mode"].as_str().unwrap_or("tui") {
            "tui" => ExtensionMode::Tui,
            "rpc" => ExtensionMode::Rpc,
            "json" => ExtensionMode::Json,
            _ => ExtensionMode::Print,
        };
        runner.set_ui_context(Some(ui.clone()), mode);
        (command.handler)(
            JsString::from(item["args"].as_str().unwrap_or("").to_owned()),
            runner.create_command_context(),
        )
        .await
        .unwrap();
    }
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-overlay-oracle.ts",
        &format!(
            "packages/coding-agent/examples/extensions/doom-overlay/{}",
            if index { "index.ts" } else { "doom-component.ts" }
        ),
    );
    let expected = ts.request(&request);
    assert_eq!(json!(trace.lock().unwrap().clone()), expected, "request {request}");
}

#[tokio::test]
async fn component_frames_input_releases_timer_and_disposal_match_pinned() {
    compare(json!({"kind":"component","steps":[{"render":0},{"render":1},{"render":10},{"render":33},{"render":80},{"invalidate":true},{"render":33},{"input":"w"},{"input":"\u{1b}[119;1:3u"},{"input":"\u{1b}[113;1:3u"},{"input":"W"},{"input":"?"},{"input":"12"},{"input":"q"},{"ms":28}]})).await;
    compare(json!({"kind":"component","resume":true,"steps":[{"ms":27},{"ms":1},{"ms":28},{"dispose":true},{"ms":28},{"input":"Q"}]})).await;
}
#[tokio::test]
async fn component_timer_errors_exit_but_key_errors_remain_uncaught() {
    compare(json!({"kind":"component","renderError":true,"steps":[{"ms":28},{"ms":28}]})).await;
    compare(json!({"kind":"component","tickError":true,"steps":[{"ms":28},{"ms":28}]})).await;
    for (resume, n, steps) in [
        (true, 1, json!([])),
        (true, 2, json!([])),
        (false, 1, json!([{"input":"w"},{"ms":28}])),
        (false, 2, json!([{"input":"q"},{"ms":28}])),
    ] {
        compare(json!({"kind":"component","resume":resume,"keyErrorAt":n,"steps":steps})).await;
    }
}
#[tokio::test]
async fn component_frame_errors_propagate_without_poisoning_and_empty_frames_render_black() {
    compare(json!({"kind":"component","frameError":true,"steps":[{"render":80},{"ms":28},{"input":"q"}]})).await;
    compare(json!({"kind":"component","emptyFrame":true,"steps":[{"render":5},{"input":"q"}]})).await;
}

#[tokio::test]
async fn component_projects_lone_surrogates_at_string_input_boundary() {
    assert_eq!(String::from_utf16_lossy(&[0xd800]), "\u{fffd}");
    compare(json!({"kind":"component","steps":[{"input":"","units":[55296]},{"input":"q"}]})).await;
}
#[tokio::test]
async fn command_registration_mode_wad_reuse_and_errors_match_pinned() {
    compare(json!({"kind":"index","commands":[{"mode":"rpc"},{"mode":"json"},{"mode":"text"},{},{"wad":"$wad"},{"args":" \u{feff}$wad\u{a0} "},{"args":"$other"},{"args":"$other","keyErrorAt":1},{"args":"$other"},{"args":"$wad","initError":"fixture init failed"},{"args":"$wad"},{"args":"$wad","customError":"fixture UI failed"},{"args":"$wad"}]})).await;
}
