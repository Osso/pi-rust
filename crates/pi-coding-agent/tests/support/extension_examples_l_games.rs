use futures_util::FutureExt;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
#[path = "extensions.rs"]
mod fixture;
#[path = "oracle.rs"]
mod oracle;
#[path = "ui_thread.rs"]
mod ui_thread;
use pi_coding_agent::core::{extensions::*, model_registry::ModelRegistry, session_manager::SessionManager};
use serde_json::{Value, json};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
type Trace = Arc<Mutex<Vec<Value>>>;
struct Backend(Trace);
impl ExtensionUiTuiBackend for Backend {
    fn request_component_render(&self, _identity: usize) {}
    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not acquire terminal")
    }

    fn show_overlay(
        &self,
        _component: Box<dyn pi_coding_agent::core::extensions::ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Rc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("this component fixture must not construct an editor")
    }
    fn request_render(&self) {
        self.0.lock().unwrap().push(json!(["requestRender"]));
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        80
    }
}
// Drive inputs through the real TUI release filter, not directly into the game.
#[derive(Default)]
struct GameTerminal {
    events: Vec<pi_tui::terminal::TerminalEvent>,
}
impl pi_tui::terminal::Terminal for GameTerminal {
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
struct GameHost {
    terminal: std::rc::Rc<std::cell::RefCell<GameTerminal>>,
    host: std::rc::Rc<pi_tui::tui::TuiRenderer>,
}
struct GameInput(std::rc::Rc<dyn ExtensionUiComponent>);
impl pi_tui::tui::Component for GameInput {
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
    item: Value,
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
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let trace = self.trace.clone();
        let item = self.item.clone();
        let theme = self.theme().unwrap();
        ui_thread::run(move || {
            Box::pin(async move {
                let completed = Arc::new(Mutex::new(false));
                let done = completed.clone();
                let done_trace = trace.clone();
                let component = factory(
                    ExtensionUiFactoryContext {
                        tui: ExtensionUiTui::new(Arc::new(Backend(trace.clone()))),
                        theme,
                        keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
                    },
                    ExtensionUiDone::new(Rc::new(move |_| {
                        *done.lock().unwrap() = true;
                        done_trace.lock().unwrap().push(json!(["done"]));
                    })),
                )
                .expect("custom component fixture factory")
                .now_or_never()
                .expect("fixture factory must resolve immediately")
                .expect("custom component fixture factory");
                let component: std::rc::Rc<dyn ExtensionUiComponent> = component.into();
                use pi_tui::tui::TuiExt;
                let terminal = std::rc::Rc::new(std::cell::RefCell::new(GameTerminal::default()));
                let host = pi_tui::tui::TuiRenderer::new(terminal.clone());
                let input = pi_tui::tui::component(GameInput(component.clone()));
                host.add_child(input.clone());
                host.set_focus(Some(input));
                let game = GameHost { terminal, host };
                tokio::task::yield_now().await;
                let render = |width| {
                    let frame = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| component.render(width)))
                    {
                        Ok(lines) => json!(lines),
                        Err(error) => {
                            json!({"error":pi_coding_agent::utils::uncaught::format_uncaught(error.as_ref())})
                        }
                    };
                    trace.lock().unwrap().push(json!(["render", width, frame]));
                };
                for width in item["widths"].as_array().unwrap() {
                    render(width.as_u64().unwrap() as usize);
                }
                for step in item["steps"].as_array().unwrap() {
                    match step["op"].as_str().unwrap() {
                        "key" => {
                            game.terminal
                                .borrow_mut()
                                .events
                                .push(pi_tui::terminal::TerminalEvent::Input(
                                    step["key"].as_str().unwrap().into(),
                                ));
                            game.host.process_events();
                        }
                        "ticks" => {
                            for _ in 0..step["count"].as_u64().unwrap() {
                                tokio::time::advance(Duration::from_millis(step["ms"].as_u64().unwrap_or(100))).await;
                                tokio::task::yield_now().await;
                            }
                        }
                        "invalidate" => component.invalidate(),
                        other => panic!("unknown game fixture op {other}"),
                    }
                    for width in item["widths"].as_array().unwrap() {
                        render(width.as_u64().unwrap() as usize);
                    }
                }
                if item["disposeRunning"] == true {
                    assert!(!*completed.lock().unwrap(), "dispose must be tested before any Done");
                    let width = item["widths"][0].as_u64().unwrap() as usize;
                    let frame = component.render(width);
                    let requests = trace.lock().unwrap().len();
                    assert!(
                        trace.lock().unwrap().iter().any(|call| call[0] == "requestRender"),
                        "game must tick before disposal"
                    );
                    component.dispose();
                    for _ in 0..40 {
                        tokio::time::advance(Duration::from_millis(50)).await;
                        tokio::task::yield_now().await;
                    }
                    assert_eq!(
                        trace.lock().unwrap().len(),
                        requests,
                        "dispose must stop render requests on the game runtime"
                    );
                    assert_eq!(component.render(width), frame, "dispose must stop game ticks");
                    assert!(!*completed.lock().unwrap(), "dispose must not call Done");
                } else {
                    assert!(*completed.lock().unwrap(), "script must close custom component");
                    component.dispose();
                }
                Ok(None)
            })
        })
    }
}
pub async fn compare(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        factory,
        &JsString::from("/fixture"),
        fixture::event_bus(),
        runtime.clone(),
        &JsString::from("<example:l-game>"),
    )
    .await
    .unwrap();
    let metadata = json!(
        extension
            .read()
            .commands
            .values()
            .map(|c| json!([c.name, c.description]))
            .collect::<Vec<_>>()
    );
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from("/fixture")), None).unwrap(),
    ));
    for entry in request["entries"].as_array().into_iter().flatten() {
        manager
            .lock()
            .unwrap()
            .append_custom_entry(
                &JsString::from(entry["customType"].as_str().unwrap()),
                Some(JsValue::from(entry["data"].clone())),
            )
            .unwrap();
    }
    let models = ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
        pi_js_compat::js_json::JsObject::new(),
    ))
    .unwrap();
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        "/fixture",
        manager.clone(),
        Arc::new(models),
        None,
    );
    let trace: Trace = Arc::default();
    let log = trace.clone();
    let saved = manager.clone();
    let mut actions = fixture::actions();
    actions.append_entry = Arc::new(move |kind, data| {
        saved.lock().unwrap().append_custom_entry(kind, data.clone()).unwrap();
        log.lock().unwrap().push(json!(["append", kind, data]));
        Ok(())
    });
    runner.bind_core(actions, fixture::context_actions(), None).unwrap();
    let log = trace.clone();
    let _error = runner.on_error(Arc::new(move |e| {
        log.lock().unwrap().push(json!(["error", e.event, e.error]))
    }));
    for item in request["commands"].as_array().unwrap() {
        runner.set_ui_context(
            Some(Arc::new(Ui {
                trace: trace.clone(),
                item: item.clone(),
            })),
            serde_json::from_value(item.get("mode").cloned().unwrap_or(json!("tui"))).unwrap(),
        );
        let command = runner
            .get_command(if name == "snake" { "snake" } else { "invaders" })
            .unwrap();
        (command.handler)(JsString::default(), runner.create_command_context())
            .await
            .unwrap();
    }
    let entries: Vec<_> = manager
        .lock()
        .unwrap()
        .get_entries()
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap();
            for k in ["id", "parentId", "timestamp"] {
                v.as_object_mut().unwrap().remove(k);
            }
            v
        })
        .collect();
    let actual = json!({"metadata":metadata,"trace":trace.lock().unwrap().clone(),"entries":entries});
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-l-games-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    let expected = oracle.request(&request);
    let actual_trace = actual["trace"].as_array().unwrap();
    let expected_trace = expected["trace"].as_array().unwrap();
    assert_eq!(
        actual_trace.len(),
        expected_trace.len(),
        "game action counts: native {} TS {}",
        actual_trace.len(),
        expected_trace.len()
    );
    for (i, (a, e)) in actual_trace.iter().zip(expected_trace).enumerate() {
        assert_eq!(a, e, "{name} frame/action {i}");
    }
    assert_eq!(actual, expected, "{name} stored state/metadata");
    actual
}
