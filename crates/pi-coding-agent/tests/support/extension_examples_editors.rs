#[path = "extensions.rs"]
mod extensions;
#[path = "oracle.rs"]
mod oracle;
#[path = "../../../pi-tui/tests/support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme};
use pi_tui::components::editor::{EditorFixedCell, EditorTheme};
use pi_tui::tui::{TuiMode, TuiRenderer};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

thread_local! { static TUI: Rc<TuiRenderer> = TuiRenderer::new(virtual_terminal::concrete(80, 24).1, Some(true), None); }
struct Backend;
impl ExtensionUiTuiBackend for Backend {
    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        TUI.with(|tui| Rc::new(tui.create_fixed_cell()))
    }
    fn request_render(&self) {}
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        80
    }
    fn mode(&self) -> TuiMode {
        TuiMode::Regular
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
    let text = component.get_text();
    let focused = component.as_focusable_mut().map(|f| f.focused());
    json!({"text":text,"focused":focused,"lines": widths.iter().map(|w| component.render(w.as_u64().unwrap() as usize)).collect::<Vec<_>>()})
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<Theme> {
        self.theme.clone()
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
    fn notify(&self, message: &str, level: Option<NotifyType>) {
        self.effects.lock().unwrap().push(json!(["notify", message, level]));
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
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
                ExtensionUiDone::new(Arc::new(move |value| {
                    *captured.lock().unwrap() = value;
                })),
            );
            component.set_focused(true);
            let widths = self.request["widths"].as_array().unwrap();
            let record = || json!({"text":null,"focused":component.focused(),"lines":widths.iter().map(|w|component.render(w.as_u64().unwrap() as usize)).collect::<Vec<_>>()});
            self.renders.lock().unwrap().push(record());
            for step in self.request["steps"].as_array().unwrap() {
                if let Some(input) = step["input"].as_str() {
                    component.handle_input(input);
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

pub async fn compare(name: &str, factory: ExtensionFactory, mut request: Value) -> Value {
    if name != "overlay-test" {
        tokio::time::pause();
    }
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let git = std::process::Command::new("git")
        .args(["init", "-q", "--initial-branch=example"])
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(git.status.success());
    request["cwd"] = json!(cwd);
    request["branch"] = json!("example");
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(i, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * i) % 256,
                    (32 + 11 * i) % 256,
                    (48 + 13 * i) % 256
                )),
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
        registry.get(name).unwrap().clone(),
        cwd,
        extensions::event_bus(),
        runtime.clone(),
        name,
    )
    .await
    .unwrap();
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(Map::new())) as Arc<dyn CredentialStore>),
        models_path: Some(None),
        allow_model_network: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut runner = extensions::runner_for(
        vec![extension],
        cwd,
        runtime,
        Arc::new(ModelRegistry::new(model_runtime)),
    );
    extensions::bind(&mut runner);
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = errors.clone();
    let _errors = runner.on_error(Arc::new(move |error| captured.lock().unwrap().push(error.error)));
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
        (command.handler)(String::new(), runner.create_command_context())
            .await
            .unwrap();
    } else {
        runner
            .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                reason: SessionStartReason::Startup,
                previous_session_file: None,
            }))
            .await;
        // The example's git branch refresh is asynchronous, just like the source.
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        let factory = ui.editor.lock().unwrap().clone().expect("registered editor");
        let editor = factory(tui(), EditorTheme::plain(), KeybindingsManager::new(Vec::new(), None));
        editor.borrow_mut().as_focusable_mut().unwrap().set_focused(true);
        let widths = request["widths"].as_array().unwrap();
        ui.renders
            .lock()
            .unwrap()
            .push(snapshot(&mut *editor.borrow_mut(), widths));
        for step in request["steps"].as_array().unwrap() {
            if let Some(input) = step["input"].as_str() {
                editor.borrow_mut().handle_input(input);
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
                            .emit(&mut ExtensionEvent::AgentEnd(AgentEndEvent { messages: Vec::new() }))
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
                for _ in 0..ms {
                    tokio::time::advance(std::time::Duration::from_millis(1)).await;
                    tokio::task::yield_now().await;
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
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-editors-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    let expected = oracle.request(&request);
    assert_eq!(actual, expected, "example {name}");
    actual
}
