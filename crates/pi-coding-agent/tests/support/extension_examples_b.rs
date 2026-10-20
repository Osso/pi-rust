use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
#[path = "oracle.rs"]
mod oracle;
#[path = "extensions.rs"]
mod support;

#[derive(Default)]
struct Ui {
    calls: Mutex<Vec<Value>>,
    selected: Mutex<Option<String>>,
}
impl ExtensionUiContext for Ui {
    fn notify(&self, message: &str, level: Option<NotifyType>) {
        self.calls.lock().unwrap().push(json!(["notify", message, level]));
    }
    fn select<'a>(
        &'a self,
        title: &str,
        items: &[String],
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        self.calls.lock().unwrap().push(json!(["select", title, items]));
        let selected = self.selected.lock().unwrap().clone();
        Box::pin(async move { Ok(selected) })
    }
    fn set_widget(&self, key: &str, content: Option<&[String]>, options: Option<&ExtensionWidgetOptions>) {
        self.calls
            .lock()
            .unwrap()
            .push(json!(["widget", key, content, options]));
    }
    fn set_working_message(&self, message: Option<&str>) {
        self.calls.lock().unwrap().push(json!(["workingMessage", message]));
    }
    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.calls.lock().unwrap().push(json!(["workingIndicator", options]));
    }
}

pub async fn compare(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    let cwd = request["cwd"].as_str().unwrap_or("/fixture");
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(name, factory);
    let extension = load_extension_from_factory(
        registry.get(name).unwrap().clone(),
        cwd,
        support::event_bus(),
        runtime.clone(),
        name,
    )
    .await
    .unwrap();
    let metadata = json!({"commands":extension.commands.keys().collect::<Vec<_>>(),"tools":extension.tools.keys().collect::<Vec<_>>()});
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(Map::new())) as Arc<dyn CredentialStore>),
        models_path: Some(None),
        allow_model_network: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut runner = support::runner_for(
        vec![extension],
        cwd,
        runtime,
        Arc::new(ModelRegistry::new(model_runtime)),
    );
    support::bind(&mut runner);
    let errors = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_errors = errors.clone();
    let _subscription = runner.on_error(Arc::new(move |error| {
        captured_errors.lock().unwrap().push(json!([error.event, error.error]));
    }));
    let ui = Arc::new(Ui::default());
    let mut cases = Vec::new();
    for case in request["cases"].as_array().unwrap() {
        ui.calls.lock().unwrap().clear();
        *ui.selected.lock().unwrap() = case["selected"].as_str().map(str::to_owned);
        runner.set_ui_context(
            if case["hasUI"].as_bool().unwrap_or(true) {
                Some(ui.clone())
            } else {
                None
            },
            ExtensionMode::Tui,
        );
        let result = match case["event"].as_str().unwrap() {
            "session_start" => {
                runner
                    .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                        reason: SessionStartReason::Startup,
                        previous_session_file: None,
                    }))
                    .await;
                Value::Null
            }
            "tool_call" => {
                let mut event=serde_json::from_value::<ToolCallEvent>(json!({"toolName":case["toolName"],"toolCallId":"fixture-call","input":case["input"],"bypassPermissions":false})).unwrap();
                json!(runner.emit_tool_call(&mut event).await.unwrap())
            }
            "input" => json!(
                runner
                    .emit_input(
                        case["text"].as_str().unwrap(),
                        serde_json::from_value(case["images"].clone()).unwrap(),
                        serde_json::from_value(case["source"].clone()).unwrap(),
                        serde_json::from_value(case["streamingBehavior"].clone()).unwrap()
                    )
                    .await
            ),
            "tool_metadata" => {
                let tool = runner
                    .get_tool_definition(case["toolName"].as_str().unwrap_or("bash"))
                    .unwrap();
                json!({"name":tool.name,"label":tool.label,"description":tool.description,"parameters":tool.parameters,"promptSnippet":tool.prompt_snippet,"promptGuidelines":tool.prompt_guidelines})
            }
            "execute" => {
                let tool = runner
                    .get_tool_definition(case["toolName"].as_str().unwrap_or("bash"))
                    .unwrap();
                match (tool.execute)(
                    "fixture-call".into(),
                    case["params"].clone(),
                    None,
                    None,
                    runner.create_context(),
                )
                .await
                {
                    Ok(result) => json!({"value":result}),
                    Err(error) => json!({"error":error.message}),
                }
            }
            other => panic!("unknown fixture event: {other}"),
        };
        cases.push(json!({"result":result,"ui":ui.calls.lock().unwrap().clone()}));
    }
    let actual = json!({"metadata":metadata,"cases":cases,"errors":errors.lock().unwrap().clone()});
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "extension-examples-b-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
        &[("LANG", "en_US.UTF-8".into()), ("LC_ALL", "en_US.UTF-8".into())],
    );
    let expected = oracle.request(&request);
    assert_eq!(actual, expected, "example {name}");
    actual
}
