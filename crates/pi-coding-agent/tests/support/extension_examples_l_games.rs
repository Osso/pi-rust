#[path = "oracle.rs"]
mod oracle;
#[path = "extensions.rs"]
mod support;
use pi_coding_agent::core::{
    extensions::*,
    model_registry::ModelRegistry,
    model_runtime::{CreateModelRuntimeOptions, ModelRuntime},
    session_manager::SessionManager,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
type Trace = Arc<Mutex<Vec<Value>>>;
struct Backend(Trace);
impl ExtensionUiTuiBackend for Backend {
    fn request_render(&self) {
        self.0.lock().unwrap().push(json!(["requestRender"]));
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        80
    }
    fn mode(&self) -> pi_tui::tui::TuiMode {
        pi_tui::tui::TuiMode::Regular
    }
}
struct Ui {
    trace: Trace,
    item: Value,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<pi_coding_agent::modes::interactive::theme::Theme> {
        Arc::new(
            pi_coding_agent::modes::interactive::theme::load_theme_from_path(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json"),
                Some(pi_coding_agent::modes::interactive::theme::ColorMode::Truecolor),
            )
            .unwrap(),
        )
    }

    fn notify(&self, text: &str, kind: Option<NotifyType>) {
        self.trace.lock().unwrap().push(json!(["notify", text, kind]));
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        Box::pin(async move {
            let completed = Arc::new(Mutex::new(false));
            let done = completed.clone();
            let trace = self.trace.clone();
            let component = factory(
                ExtensionUiFactoryContext {
                    tui: ExtensionUiTui::new(Arc::new(Backend(self.trace.clone()))),
                    theme: self.theme(),
                    keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
                },
                ExtensionUiDone::new(Arc::new(move |_| {
                    *done.lock().unwrap() = true;
                    trace.lock().unwrap().push(json!(["done"]));
                })),
            );
            tokio::task::yield_now().await;
            let render = |width| {
                let frame = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| component.render(width))) {
                    Ok(lines) => json!(lines),
                    Err(error) => json!({"error":pi_coding_agent::utils::uncaught::format_uncaught(error.as_ref())}),
                };
                self.trace.lock().unwrap().push(json!(["render", width, frame]));
            };
            for width in self.item["widths"].as_array().unwrap() {
                render(width.as_u64().unwrap() as usize);
            }
            for step in self.item["steps"].as_array().unwrap() {
                match step["op"].as_str().unwrap() {
                    "key" => component.handle_input(step["key"].as_str().unwrap()),
                    "ticks" => {
                        for _ in 0..step["count"].as_u64().unwrap() {
                            tokio::time::advance(Duration::from_millis(step["ms"].as_u64().unwrap_or(100))).await;
                            tokio::task::yield_now().await;
                        }
                    }
                    "invalidate" => component.invalidate(),
                    other => panic!("unknown game fixture op {other}"),
                }
                for width in self.item["widths"].as_array().unwrap() {
                    render(width.as_u64().unwrap() as usize);
                }
            }
            assert!(*completed.lock().unwrap(), "script must close custom component");
            component.dispose();
            Ok(None)
        })
    }
}
pub async fn compare(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        factory,
        "/fixture",
        support::event_bus(),
        runtime.clone(),
        "<example:l-game>",
    )
    .await
    .unwrap();
    let metadata = json!(
        extension
            .commands
            .values()
            .map(|c| json!([c.name, c.description]))
            .collect::<Vec<_>>()
    );
    let manager = Arc::new(Mutex::new(SessionManager::in_memory(Some("/fixture"), None).unwrap()));
    for entry in request["entries"].as_array().into_iter().flatten() {
        manager
            .lock()
            .unwrap()
            .append_custom_entry(entry["customType"].as_str().unwrap(), Some(entry["data"].clone()))
            .unwrap();
    }
    let models = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            Default::default(),
        ))),
        models_path: Some(None),
        allow_model_network: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        "/fixture",
        manager.clone(),
        Arc::new(ModelRegistry::new(models)),
        None,
    );
    let trace: Trace = Arc::default();
    let log = trace.clone();
    let saved = manager.clone();
    let mut actions = support::actions();
    actions.append_entry = Arc::new(move |kind, data| {
        saved.lock().unwrap().append_custom_entry(kind, data.clone()).unwrap();
        log.lock().unwrap().push(json!(["append", kind, data]));
        Ok(())
    });
    runner.bind_core(actions, support::context_actions(), None);
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
        (command.handler)(String::new(), runner.create_command_context())
            .await
            .unwrap();
    }
    let before = trace.lock().unwrap().len();
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        trace.lock().unwrap().len(),
        before,
        "disposed game must not leave active intervals"
    );
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
