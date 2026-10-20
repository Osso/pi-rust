//! Behavioral tests for `extensions/llama/index.ts` and `extensions/index.ts`.

#[path = "support/llama_fetch.rs"]
mod llama_fetch;
#[path = "support/tool_context.rs"]
mod tool_context;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{ApiKeyCredential, Credential, CredentialStore};
use pi_ai::types::ProviderEnv;
use pi_coding_agent::bundled_extensions::llama::index::{
    connection_error_message, is_connection_error, llama_extension_with_fetch, model_is_loaded,
    parse_hugging_face_model, register_builtin_extensions,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::event_bus::EventBusController;
use pi_coding_agent::core::extensions::loader::{ExtensionRegistry, load_extension_from_factory};
use pi_coding_agent::core::extensions::types::{
    ExtensionCommandContext, ExtensionMode, ExtensionUiComponent, ExtensionUiContext, ExtensionUiCustomFactory,
    ExtensionUiCustomOptions, ExtensionUiDone, ExtensionUiFactoryContext, ExtensionUiTui, NotifyType, ProviderActions,
};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_coding_agent::utils::ansi::strip_ansi;
use pi_tui::tui::TuiMode;
use serde_json::{Map, Value, json};

use llama_fetch::{HandlerFetch, VecBody, header, json_response};
use tool_context::{TestContextBuilder, in_memory_session};

fn sse_response(frames: &[&str]) -> pi_ai::request::HttpResponse {
    pi_ai::request::HttpResponse {
        status: 200,
        status_text: String::new(),
        headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        body: Box::new(VecBody {
            chunks: frames.iter().map(|frame| frame.as_bytes().to_vec()).collect(),
        }),
    }
}

struct TestBackend {
    renders: AtomicUsize,
}

impl pi_coding_agent::core::extensions::types::ExtensionUiTuiBackend for TestBackend {
    fn show_overlay(
        &self,
        _component: Box<dyn ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Arc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }
    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("this component fixture must not construct an editor")
    }

    fn request_render(&self) {
        self.renders.fetch_add(1, Ordering::SeqCst);
    }

    fn terminal_rows(&self) -> usize {
        40
    }

    fn terminal_columns(&self) -> usize {
        80
    }

    fn mode(&self) -> TuiMode {
        TuiMode::Regular
    }
}

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(Arc::clone(&theme));
    theme
}

enum FixtureRequest {
    Custom(ExtensionUiCustomFactory, tokio::sync::oneshot::Sender<()>),
    Render(std::sync::mpsc::Sender<Option<Vec<String>>>),
    Input(String),
    Stop,
}

struct FakeUi {
    sender: tokio::sync::mpsc::UnboundedSender<FixtureRequest>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    notifications: Mutex<Vec<(String, Option<NotifyType>)>>,
}

impl Default for FakeUi {
    fn default() -> Self {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
                let mut component: Option<Box<dyn ExtensionUiComponent>> = None;
                while let Some(request) = receiver.recv().await {
                    match request {
                        FixtureRequest::Custom(factory, sender) => {
                            let sender = std::cell::RefCell::new(Some(sender));
                            let done = ExtensionUiDone::new(Arc::new(move |_| {
                                if let Some(sender) = sender.borrow_mut().take() {
                                    let _ = sender.send(());
                                }
                            }));
                            component = Some(factory(
                                ExtensionUiFactoryContext {
                                    tui: ExtensionUiTui::new(Arc::new(TestBackend {
                                        renders: AtomicUsize::new(0),
                                    })),
                                    theme: install_theme(),
                                    keybindings: KeybindingsManager::new(Vec::new(), None),
                                },
                                done,
                            ));
                        }
                        FixtureRequest::Render(sender) => {
                            sender.send(component.as_ref().map(|c| c.render(80))).unwrap();
                        }
                        FixtureRequest::Input(data) => component.as_ref().expect("component").handle_input(&data),
                        FixtureRequest::Stop => break,
                    }
                }
            }));
        });
        Self {
            sender,
            worker: Mutex::new(Some(worker)),
            notifications: Mutex::new(Vec::new()),
        }
    }
}

impl Drop for FakeUi {
    fn drop(&mut self) {
        self.sender.send(FixtureRequest::Stop).unwrap();
        self.worker.get_mut().unwrap().take().unwrap().join().unwrap();
    }
}

impl FakeUi {
    fn render(&self) -> Option<Vec<String>> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender.send(FixtureRequest::Render(sender)).unwrap();
        receiver.recv().unwrap()
    }
}

impl ExtensionUiContext for FakeUi {
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _options: Option<ExtensionUiCustomOptions>,
    ) -> pi_ai::request::BoxFuture<'a, Result<Option<Value>, String>> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.sender.send(FixtureRequest::Custom(factory, sender)).unwrap();
        Box::pin(async move {
            receiver.await.unwrap();
            Ok(None)
        })
    }

    fn notify(&self, message: &str, notify_type: Option<NotifyType>) {
        self.notifications
            .lock()
            .unwrap()
            .push((message.to_owned(), notify_type));
    }
}

fn text(lines: &[String]) -> Vec<String> {
    lines.iter().map(|line| strip_ansi(line)).collect()
}

async fn wait_for_component(ui: &Arc<FakeUi>) -> () {
    for _ in 0..200 {
        if ui.render().is_some() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("custom UI factory was never invoked");
}

async fn wait_for_text(ui: &Arc<FakeUi>, needle: &str) {
    for _ in 0..200 {
        if let Some(lines) = ui.render() {
            if text(&lines).iter().any(|line| line.contains(needle)) {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("view never rendered {needle:?}");
}

fn press(ui: &Arc<FakeUi>, data: &str) {
    ui.sender.send(FixtureRequest::Input(data.to_owned())).unwrap();
}

#[test]
fn parses_hugging_face_model_specs() {
    assert_eq!(parse_hugging_face_model("owner/repo"), ("owner/repo".to_owned(), None));
    assert_eq!(
        parse_hugging_face_model("owner/repo:Q4_K_M"),
        ("owner/repo".to_owned(), Some("Q4_K_M".to_owned()))
    );
    assert_eq!(
        parse_hugging_face_model("owner:tag"),
        ("owner".to_owned(), Some("tag".to_owned()))
    );
    assert_eq!(
        parse_hugging_face_model("owner/repo:q:x"),
        ("owner/repo".to_owned(), Some("q:x".to_owned()))
    );
}

#[test]
fn classifies_connection_errors_and_loaded_models() {
    assert!(is_connection_error("TypeError: fetch failed"));
    assert!(is_connection_error("TimeoutError"));
    assert!(is_connection_error("network error"));
    assert!(!is_connection_error("boom"));
    assert_eq!(
        connection_error_message("fetch failed"),
        "Could not connect to the server."
    );
    assert_eq!(connection_error_message("boom"), "boom");

    let loaded: pi_coding_agent::bundled_extensions::llama::client::LlamaModelInfo =
        serde_json::from_value(json!({"id": "m", "status": {"value": "loaded"}})).unwrap();
    assert!(model_is_loaded(&loaded));
    let sleeping: pi_coding_agent::bundled_extensions::llama::client::LlamaModelInfo =
        serde_json::from_value(json!({"id": "m", "status": {"value": "sleeping"}})).unwrap();
    assert!(model_is_loaded(&sleeping));
    let unloaded: pi_coding_agent::bundled_extensions::llama::client::LlamaModelInfo =
        serde_json::from_value(json!({"id": "m", "status": {"value": "unloaded"}})).unwrap();
    assert!(!model_is_loaded(&unloaded));
}

#[tokio::test]
async fn registers_the_native_provider_and_command() {
    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"data": []}))));
    let runtime = pi_coding_agent::core::extensions::create_extension_runtime();
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let fetch = Arc::clone(&fetch);
            let result = llama_extension_with_fetch(pi, fetch);
            Box::pin(async move { result })
        }),
        "/tmp",
        Arc::new(EventBusController::new()),
        runtime.clone(),
        "<inline:llama.cpp>",
    )
    .await
    .unwrap();
    assert_eq!(
        extension.read().commands.get("llama").unwrap().description.as_deref(),
        Some("Manage llama.cpp router models")
    );
    let registrations = runtime.pending_native_provider_registrations();
    assert_eq!(
        registrations
            .iter()
            .map(|registration| registration.provider.id().to_owned())
            .collect::<Vec<_>>(),
        vec!["llama.cpp".to_owned()]
    );

    let mut registry = ExtensionRegistry::new();
    register_builtin_extensions(&mut registry);
    assert!(registry.get("<inline:llama.cpp>").is_some());
}

#[tokio::test]
async fn warns_outside_interactive_mode() {
    let ui = Arc::new(FakeUi::default());
    let (_, context) = TestContextBuilder::new("/tmp", in_memory_session("/tmp", "llama-index-non-tui"))
        .mode(ExtensionMode::Print)
        .ui(ui.clone())
        .build()
        .await;
    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"data": []}))));
    let runtime = pi_coding_agent::core::extensions::create_extension_runtime();
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let fetch = Arc::clone(&fetch);
            let result = llama_extension_with_fetch(pi, fetch);
            Box::pin(async move { result })
        }),
        "/tmp",
        Arc::new(EventBusController::new()),
        runtime,
        "<inline:llama.cpp>",
    )
    .await
    .unwrap();
    let handler = extension.read().commands.get("llama").unwrap().handler.clone();
    handler(String::new(), ExtensionCommandContext { context })
        .await
        .unwrap();
    assert_eq!(
        ui.notifications.lock().unwrap().as_slice(),
        &[(
            "/llama is available in interactive mode".to_owned(),
            Some(NotifyType::Warning)
        )]
    );
}

#[tokio::test]
async fn manages_models_through_the_tui_flow() {
    let status_loaded = Arc::new(AtomicBool::new(true));
    let fetch = HandlerFetch::new(Arc::new({
        let status_loaded = Arc::clone(&status_loaded);
        move |request| {
            if request.url.ends_with("/models/sse") {
                return sse_response(&[]);
            }
            if request.url.ends_with("/models/unload") {
                status_loaded.store(false, Ordering::SeqCst);
                return json_response(200, &json!({"success": true}));
            }
            if request.url.ends_with("/models/load") {
                status_loaded.store(true, Ordering::SeqCst);
                return json_response(200, &json!({"success": true}));
            }
            let status = if status_loaded.load(Ordering::SeqCst) {
                "loaded"
            } else {
                "unloaded"
            };
            json_response(200, &json!({"data": [{"id": "m", "status": {"value": status}}]}))
        }
    }));

    let storage = Arc::new(AuthStorage::in_memory(Map::new()));
    let credential = Credential::ApiKey(ApiKeyCredential {
        key: Some("local".to_owned()),
        env: Some(ProviderEnv::from([(
            "LLAMA_BASE_URL".to_owned(),
            "http://127.0.0.1:8080".to_owned(),
        )])),
    });
    storage
        .modify(
            "llama.cpp",
            Arc::new(move |_| {
                let credential = credential.clone();
                Box::pin(async move { Ok(Some(credential)) })
            }),
            Default::default(),
        )
        .await
        .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage as Arc<dyn CredentialStore>),
        models_path: None,
        ..Default::default()
    })
    .await
    .unwrap();
    let registry = Arc::new(ModelRegistry::new(runtime));
    let register_registry = Arc::clone(&registry);
    let provider_actions = ProviderActions {
        register_provider: None,
        register_native_provider: Some(Arc::new(move |provider| {
            register_registry.register_native_provider(provider);
        })),
        unregister_provider: None,
    };

    let ui = Arc::new(FakeUi::default());
    let (_, context, extension_runtime) = TestContextBuilder::new("/tmp", in_memory_session("/tmp", "llama-index-tui"))
        .mode(ExtensionMode::Tui)
        .ui(ui.clone())
        .model_registry(registry)
        .provider_actions(provider_actions)
        .build_with_runtime()
        .await;
    let extension = load_extension_from_factory(
        Arc::new({
            let fetch = Arc::clone(&fetch);
            move |pi| {
                let fetch = Arc::clone(&fetch);
                let result = llama_extension_with_fetch(pi, fetch);
                Box::pin(async move { result })
            }
        }),
        "/tmp",
        Arc::new(EventBusController::new()),
        extension_runtime,
        "<inline:llama.cpp>",
    )
    .await
    .unwrap();
    let handler = extension.read().commands.get("llama").unwrap().handler.clone();
    let task = tokio::spawn(handler(String::new(), ExtensionCommandContext { context }));
    wait_for_component(&ui).await;
    wait_for_text(&ui, "Download model…").await;

    press(&ui, "\r");
    wait_for_text(&ui, "Unload model?").await;
    press(&ui, "\r");
    wait_for_text(&ui, "llama.cpp models").await;
    press(&ui, "\x1b");
    task.await.unwrap().unwrap();

    let unload = fetch
        .requests()
        .into_iter()
        .find(|request| request.url.ends_with("/models/unload"))
        .expect("unload request");
    assert_eq!(header(&unload, "authorization").as_deref(), Some("Bearer local"));
    assert!(
        ui.notifications
            .lock()
            .unwrap()
            .iter()
            .any(|(message, _)| message == "Unloaded m")
    );
}
