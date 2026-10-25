use futures_util::FutureExt;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
#[path = "extensions.rs"]
mod extensions;
#[path = "oracle.rs"]
mod oracle;
#[path = "../../../pi-tui/tests/support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme};
use pi_tui::components::editor::{EditorFixedCell, EditorTheme};
use pi_tui::tui::TuiRenderer;
use serde_json::{Map, Value, json};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

thread_local! {
    static TUI: Rc<TuiRenderer> = TuiRenderer::new(virtual_terminal::concrete(80, 24).1);
    static RENDER_REQUESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct Backend;
impl ExtensionUiTuiBackend for Backend {
    fn request_component_render(&self, _identity: usize) {}
    fn show_overlay(
        &self,
        _component: Box<dyn pi_coding_agent::core::extensions::ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Rc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }

    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not acquire terminal")
    }

    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        TUI.with(|tui| Rc::new(tui.create_fixed_cell()))
    }
    fn request_render(&self) {
        RENDER_REQUESTS.with(|count| count.set(count.get() + 1));
        TUI.with(|tui| pi_tui::tui::TuiExt::request_render(tui.as_ref(), false));
    }
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        80
    }
}
fn tui() -> ExtensionUiTui {
    ExtensionUiTui::new(Arc::new(Backend))
}
struct Ui {
    theme: Arc<Theme>,
    effects: Mutex<Vec<Value>>,
    renders: Mutex<Vec<Value>>,
    editor: Mutex<Option<ExtensionEditorFactory>>,
    request: Value,
    result: Mutex<Value>,
}
fn snapshot(component: &mut dyn pi_tui::EditorComponent, widths: &[Value]) -> Value {
    let text = component.get_text().unwrap();
    let focused = component.as_focusable_mut().map(|f| f.focused());
    json!({"text":text,"focused":focused,"lines": widths.iter().map(|w| component.render(w.as_u64().unwrap() as usize)).collect::<Vec<_>>()})
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Result<Arc<Theme>, pi_ai::request::CallbackError> {
        Ok(self.theme.clone())
    }
    fn set_editor_component(&self, factory: Option<ExtensionEditorFactory>) {
        *self.editor.lock().unwrap() = factory;
        self.effects.lock().unwrap().push(json!(["editor"]));
    }
    fn set_working_visible(&self, visible: bool) {
        self.effects.lock().unwrap().push(json!(["workingVisible", visible]));
    }
    fn set_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        let footer = factory.unwrap()(ExtensionUiFooterContext {
            tui: tui(),
            theme: self.theme.clone(),
            footer_data: pi_coding_agent::core::footer_data_provider::FooterDataProvider::new("/fixture"),
        });
        self.effects.lock().unwrap().push(json!(["footer", footer.render(80)]));
    }
    fn notify(
        &self,
        message: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.effects.lock().unwrap().push(json!(["notify", message, level]));
        Ok(())
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        Box::pin(async move {
            self.effects
                .lock()
                .unwrap()
                .push(json!(["custom",{"overlay":options.unwrap().overlay}]));
            let result = Arc::new(Mutex::new(None));
            let captured = result.clone();
            let component = factory(
                ExtensionUiFactoryContext {
                    tui: tui(),
                    theme: self.theme.clone(),
                    keybindings: KeybindingsManager::new(Vec::new(), None),
                },
                ExtensionUiDone::new(Rc::new(move |value| {
                    *captured.lock().unwrap() = value;
                })),
            )
            .expect("custom component fixture factory")
            .now_or_never()
            .expect("fixture factory must resolve immediately")
            .expect("custom component fixture factory");
            component.set_focused(true);
            let widths = self.request["widths"].as_array().unwrap();
            let record = || json!({"text":null,"focused":component.focused(),"lines":widths.iter().map(|w|component.render(w.as_u64().unwrap() as usize)).collect::<Vec<_>>()});
            self.renders.lock().unwrap().push(record());
            for step in self.request["steps"].as_array().unwrap() {
                if let Some(input) = step["input"].as_str() {
                    component.handle_input(&JsString::from(input));
                }
                if let Some(focused) = step["focus"].as_bool() {
                    component.set_focused(focused);
                }
                self.renders.lock().unwrap().push(record());
            }
            component.set_focused(false);
            component.dispose();
            let value = result.lock().unwrap().clone();
            *self.result.lock().unwrap() = json!(value);
            Ok(value)
        })
    }
}

fn assert_json_equal(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_json_equal(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (key, a) in a {
                assert_json_equal(a, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

pub async fn compare(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    run(name, factory, request, true).await
}

pub async fn native(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    run(name, factory, request, false).await
}

async fn run(name: &str, factory: ExtensionFactory, mut request: Value, compare_oracle: bool) -> Value {
    if name != "overlay-test" {
        tokio::time::pause();
    }
    let temp = if request["homeCwd"] == true {
        tempfile::tempdir_in(std::env::var_os("HOME").expect("sandbox HOME")).unwrap()
    } else {
        tempfile::tempdir().unwrap()
    };
    let cwd = temp.path().to_str().unwrap();
    let repository = request["repository"] != false;
    let branch_name = request["branchName"].as_str().unwrap_or("example").to_owned();
    if repository {
        let git = std::process::Command::new("git")
            .args(["init", "-q", &format!("--initial-branch={branch_name}")])
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(git.status.success());
    }
    request["cwd"] = json!(cwd);
    request["branch"] = json!(if repository { &branch_name } else { "" });
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(i, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(JsString::from(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * i) % 256,
                    (32 + 11 * i) % 256,
                    (48 + 13 * i) % 256
                ))),
            )
        })
        .collect();
    request["colors"] = json!(
        REQUIRED_COLORS
            .iter()
            .enumerate()
            .map(|(i, key)| (
                key.to_string(),
                json!([(16 + 7 * i) % 256, (32 + 11 * i) % 256, (48 + 13 * i) % 256])
            ))
            .collect::<Map<_, _>>()
    );
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap());
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(name, factory);
    let extension = load_extension_from_factory(
        registry.get(&JsString::from(name)).unwrap().clone(),
        &JsString::from(cwd),
        extensions::event_bus(),
        runtime.clone(),
        &JsString::from(name),
    )
    .await
    .unwrap();
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let mut runner = extensions::runner_for(vec![extension], cwd, runtime, Arc::new(model_registry));
    let live = Arc::new(Mutex::new(request.clone()));
    let mut actions = extensions::actions();
    let thinking = live.clone();
    actions.get_thinking_level = Arc::new(move || {
        let state = thinking.lock().unwrap();
        Ok(serde_json::from_value(state.get("thinking").cloned().unwrap_or(json!("off"))).unwrap())
    });
    let mut context = extensions::context_actions();
    let model = live.clone();
    context.get_model = Arc::new(move || {
        let state = model.lock().unwrap();
        let fields = state.get("model").filter(|m| !m.is_null())?;
        let mut model = json!({"id":"fixture","provider":"fixture","contextWindow":128000,"api":"openai-responses","name":"Fixture","baseUrl":"https://invalid.test","reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"maxTokens":4096});
        model
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        Some(serde_json::from_value(model).unwrap())
    });
    let usage = live.clone();
    context.get_context_usage = Arc::new(move || {
        usage
            .lock()
            .unwrap()
            .get("usage")
            .filter(|u| !u.is_null())
            .map(|u| serde_json::from_value(u.clone()).unwrap())
    });
    runner.bind_core(actions, context, None).unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = errors.clone();
    let _errors = runner.on_error(Arc::new(move |error| {
        captured.lock().unwrap().push(error.error.clone())
    }));
    let ui = Arc::new(Ui {
        theme,
        effects: Mutex::new(Vec::new()),
        renders: Mutex::new(Vec::new()),
        editor: Mutex::new(None),
        request: request.clone(),
        result: Mutex::new(Value::Null),
    });
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Tui);
    if name == "overlay-test" {
        let command = runner.get_command("overlay-test").unwrap();
        (command.handler)(JsString::default(), runner.create_command_context())
            .await
            .unwrap();
    } else {
        runner
            .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                reason: SessionStartReason::Startup,
                previous_session_file: None,
            }))
            .await;
        let requests_before_factory = RENDER_REQUESTS.with(|count| count.get());
        let factory = ui.editor.lock().unwrap().clone().expect("registered editor");
        let editor = factory(tui(), EditorTheme::plain(), KeybindingsManager::new(Vec::new(), None));
        if request["wireClear"] == true {
            let weak = Rc::downgrade(&editor);
            let mut component = editor.borrow_mut();
            let base = component
                .as_any_mut()
                .unwrap()
                .downcast_mut::<pi_coding_agent::modes::interactive::components::custom_editor::CustomEditor>()
                .expect("composed editor exposes its base for host callbacks");
            base.on_action(
                "app.clear",
                Rc::new(move || {
                    weak.upgrade()
                        .unwrap()
                        .borrow_mut()
                        .set_text(&JsString::from(""))
                        .unwrap();
                    true
                }),
            );
        }
        editor.borrow_mut().as_focusable_mut().unwrap().set_focused(true);
        if name == "border-status-editor" {
            // The real git subprocess completes on wall time, not the paused Tokio clock.
            for _ in 0..2000 {
                tokio::task::yield_now().await;
                if RENDER_REQUESTS.with(|count| count.get()) > requests_before_factory {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(
                RENDER_REQUESTS.with(|count| count.get()) > requests_before_factory,
                "branch refresh must request rendering"
            );
        }
        let widths = request["widths"].as_array().unwrap();
        ui.renders
            .lock()
            .unwrap()
            .push(snapshot(&mut *editor.borrow_mut(), widths));
        for step in request["steps"].as_array().unwrap() {
            for key in ["model", "usage", "thinking"] {
                if let Some(value) = step.get(key) {
                    live.lock().unwrap()[key] = value.clone();
                }
            }
            if let Some(input) = step["input"].as_str() {
                let input = editor.borrow_mut().handle_input_deferred(&JsString::from(input));
                input.run(|resume| resume(&mut *editor.borrow_mut()));
            }
            if let Some(focused) = step["focus"].as_bool() {
                editor.borrow_mut().as_focusable_mut().unwrap().set_focused(focused);
            }
            if let Some(event) = step["event"].as_str() {
                match event {
                    "agent_start" => {
                        runner.emit(&mut ExtensionEvent::AgentStart(AgentStartEvent {})).await;
                    }
                    "agent_end" => {
                        runner
                            .emit(&mut ExtensionEvent::AgentEnd(AgentEndEvent {
                                messages: Vec::new(),
                                session_continuation: None,
                            }))
                            .await;
                    }
                    "session_shutdown" => {
                        runner
                            .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
                                reason: SessionShutdownReason::Quit,
                                target_session_file: None,
                            }))
                            .await;
                    }
                    other => panic!("unknown event {other}"),
                }
            }
            tokio::task::yield_now().await;
            if let Some(ms) = step["tick"].as_u64() {
                let before = RENDER_REQUESTS.with(|count| count.get());
                // Tokio's timer wheel services millisecond deadlines on the next tick.
                for _ in 0..=ms {
                    tokio::time::advance(std::time::Duration::from_millis(1)).await;
                    tokio::task::yield_now().await;
                }
                if let Some(expected) = step["repaint"].as_bool() {
                    let after = RENDER_REQUESTS.with(|count| count.get());
                    assert_eq!(
                        after > before,
                        expected,
                        "{name} timer repaint: {step}; before={before}, after={after}"
                    );
                }
            }
            ui.renders
                .lock()
                .unwrap()
                .push(snapshot(&mut *editor.borrow_mut(), widths));
        }
    }
    assert!(errors.lock().unwrap().is_empty(), "{:?}", errors.lock().unwrap());
    let actual = json!({"effects":*ui.effects.lock().unwrap(),"renders":*ui.renders.lock().unwrap(),"result":*ui.result.lock().unwrap()});
    if !compare_oracle {
        return actual;
    }
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-editors-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    let response = oracle.request(&request);
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    assert_json_equal(&actual, &response["output"], name);
    actual
}
