//! Batch E extends the shared real-runner/oracle fixtures with trust, resources,
//! compaction callbacks and private filesystem/git scenarios.
use crate::shared_extensions as support;
use crate::shared_oracle as oracle;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_js_compat::js_json::JsString;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Ui {
    calls: Mutex<Vec<Value>>,
    selected: Mutex<Option<String>>,
    note: Mutex<Option<String>>,
}
impl ExtensionUiContext for Ui {
    fn notify(
        &self,
        text: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.calls.lock().unwrap().push(json!(["notify", text, level]));
        Ok(())
    }
    fn select<'a>(
        &'a self,
        title: &JsString,
        items: &[JsString],
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.calls.lock().unwrap().push(json!(["select", title, items]));
        let value = self.selected.lock().unwrap().clone();
        Box::pin(async move { Ok(value.map(JsString::from)) })
    }
    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.calls.lock().unwrap().push(json!(["input", title, placeholder]));
        let value = self.note.lock().unwrap().clone();
        Box::pin(async move { Ok(value.map(JsString::from)) })
    }
}

pub fn factory(register: fn(&ExtensionApi) -> Result<(), HandlerError>) -> ExtensionFactory {
    Arc::new(move |api| {
        let result = register(api);
        Box::pin(async move { result })
    })
}

pub async fn compare(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    setup(cwd, &request);
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(name, factory);
    let extension = load_extension_from_factory(
        registry.get(&JsString::from(name)).unwrap().clone(),
        &JsString::from(cwd),
        support::event_bus(),
        runtime.clone(),
        &JsString::from(name),
    )
    .await
    .unwrap();
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let manager = support::session_manager(cwd);
    let mut runner = ExtensionRunner::new(
        vec![extension.clone()],
        runtime.clone(),
        cwd,
        manager.clone(),
        Arc::new(model_registry),
        None,
    );
    let ui = Arc::new(Ui::default());
    let usage = Arc::new(Mutex::new(None));
    let mut context = support::context_actions();
    let tokens = usage.clone();
    context.get_context_usage = Arc::new(move || tokens.lock().unwrap().clone());
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let compact_calls = calls.clone();
    let outcome = request["compactOutcome"].as_str().unwrap_or("complete").to_owned();
    context.compact = Arc::new(move |options| {
        let options = options.unwrap_or_default();
        compact_calls
            .lock()
            .unwrap()
            .push(json!(["compact", options.custom_instructions]));
        if outcome == "error" {
            if let Some(callback) = options.on_error {
                callback(&pi_js_compat::callback_error::CallbackError::new(
                    "fixture compaction failure",
                ))
                .expect("fixture error callback");
            }
        } else if let Some(callback) = options.on_complete {
            callback(
                &serde_json::from_value(
                    json!({"summary":"fixture summary","firstKeptEntryId":"kept","tokensBefore":120001}),
                )
                .unwrap(),
            )
            .expect("fixture completion callback");
        }
    });
    runner.bind_core(support::actions(), context, None).unwrap();
    let errors = Arc::new(Mutex::new(Vec::<Value>::new()));
    let error_log = errors.clone();
    runner.on_error(Arc::new(move |error| {
        error_log.lock().unwrap().push(json!([error.event, error.error]))
    }));
    let mut cases = Vec::new();
    for step in request["steps"].as_array().unwrap() {
        ui.calls.lock().unwrap().clear();
        calls.lock().unwrap().clear();
        *ui.selected.lock().unwrap() = step["selected"].as_str().map(str::to_owned);
        *ui.note.lock().unwrap() = step["note"].as_str().map(str::to_owned);
        let has_ui = step["hasUI"].as_bool().unwrap_or(true);
        let mode = serde_json::from_value(step.get("mode").cloned().unwrap_or(json!("tui"))).unwrap();
        runner.set_ui_context(if has_ui { Some(ui.clone()) } else { None }, mode);
        let result = match step["op"].as_str().unwrap() {
            "before" => json!(
                runner
                    .emit_before_agent_start(
                        &JsString::from("task"),
                        None,
                        &JsString::from(step["prompt"].as_str().unwrap_or("base")),
                        serde_json::from_value(step.get("options").cloned().unwrap_or(json!({}))).unwrap()
                    )
                    .await
            ),
            "trust" => {
                let (result, trust_errors) = emit_project_trust_event(
                    &LoadExtensionsResult {
                        extensions: vec![extension.clone()],
                        errors: vec![],
                        runtime: runtime.clone(),
                    },
                    ProjectTrustEvent { cwd: cwd.into() },
                    &ProjectTrustContext {
                        cwd: cwd.into(),
                        mode,
                        has_ui,
                        ui: if has_ui {
                            ui.clone()
                        } else {
                            runner.create_context().ui().unwrap()
                        },
                    },
                )
                .await;
                for error in trust_errors {
                    errors.lock().unwrap().push(json!([error.event, error.error]));
                }
                json!(result)
            }
            "resources" => json!(
                runner
                    .emit_resources_discover(
                        &JsString::from(cwd),
                        serde_json::from_value(step["reason"].clone()).unwrap()
                    )
                    .await
            ),
            "command" => {
                match (runner.get_command(step["name"].as_str().unwrap()).unwrap().handler)(
                    step["args"].as_str().unwrap_or("").into(),
                    runner.create_command_context(),
                )
                .await
                {
                    Ok(()) => Value::Null,
                    Err(error) => json!({"error":error.message}),
                }
            }
            "usage" => {
                *usage.lock().unwrap() = step["tokens"].as_f64().map(|tokens| ContextUsage {
                    tokens: Some(tokens),
                    context_window: 200000.0,
                    percent: Some(tokens / 2000.0),
                });
                Value::Null
            }
            "append" => {
                manager
                    .lock()
                    .unwrap()
                    .append_message(serde_json::from_value(step["message"].clone()).unwrap())
                    .unwrap();
                Value::Null
            }
            "write" => {
                write_files(cwd, &step["files"]);
                Value::Null
            }
            "git" => json!(git(
                cwd,
                step["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .as_slice()
            )),
            "snapshot" => snapshot(cwd, &step["files"]),
            "tool_result" => json!(
                runner
                    .emit_tool_result(serde_json::from_value(step["event"].clone()).unwrap())
                    .await
            ),
            "event" => {
                let mut value = step["event"].clone();
                if value["entryId"] == "$leaf" {
                    value["entryId"] = json!(manager.lock().unwrap().get_leaf_id());
                }
                if let Some(index) = value["entryId"]
                    .as_str()
                    .and_then(|s| s.strip_prefix('$'))
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    value["entryId"] = json!(manager.lock().unwrap().get_entries()[index].id());
                }
                let mut event: ExtensionEvent = serde_json::from_value(value).unwrap();
                let result = runner.emit(&mut event).await;
                json!(result.session_before_fork())
            }
            other => panic!("unknown fixture op {other}"),
        };
        cases.push(
            json!({"result":result,"ui":ui.calls.lock().unwrap().clone(),"actions":calls.lock().unwrap().clone()}),
        );
    }
    let commands = runner
        .get_registered_commands()
        .iter()
        .map(|command| json!([command.name, command.description]))
        .collect::<Vec<_>>();
    let actual = json!({"commands":commands,"cases":cases,"errors":errors.lock().unwrap().clone()});
    let actual = support::normalize_with_token(
        &actual,
        &format!("{}/examples/extensions/dynamic-resources", env!("CARGO_MANIFEST_DIR")),
        "<module>",
    );
    let actual = support::normalize_with_token(
        &actual,
        cwd,
        if name == "dynamic-resources/index.ts" {
            "<cwd>"
        } else {
            "<tmp>"
        },
    );
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "extension-examples-e-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}"),
        &[
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
            ("GIT_CONFIG_SYSTEM", "/dev/null".into()),
        ],
    );
    let expected = oracle.request(&request);
    assert_eq!(actual, expected, "example {name}: {request}");
    actual
}

fn write_files(cwd: &str, files: &Value) {
    if let Some(files) = files.as_object() {
        for (name, content) in files {
            let path = Path::new(cwd).join(name);
            if content.is_null() {
                if path.exists() {
                    std::fs::remove_file(path).unwrap();
                }
            } else {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content.as_str().unwrap()).unwrap();
            }
        }
    }
}
fn git(cwd: &str, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn setup(cwd: &str, request: &Value) {
    if request["git"] == true {
        git(cwd, &["init", "-q"]);
        git(cwd, &["config", "user.name", "Fixture"]);
        git(cwd, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(Path::new(cwd).join("tracked.txt"), "initial\n").unwrap();
        git(cwd, &["add", "-A"]);
        git(cwd, &["commit", "-qm", "initial"]);
        if request["rejectCommit"] == true {
            std::os::unix::fs::symlink("/usr/bin/false", Path::new(cwd).join(".git/hooks/pre-commit")).unwrap();
        }
    }
    write_files(cwd, &request["files"]);
}
fn snapshot(cwd: &str, files: &Value) -> Value {
    let files = files
        .as_array()
        .unwrap()
        .iter()
        .map(|name| {
            let name = name.as_str().unwrap();
            (name.to_owned(), std::fs::read_to_string(Path::new(cwd).join(name)).ok())
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    json!({"files":files,"status":git(cwd,&["status","--porcelain"]),"messages":git(cwd,&["log","--format=%B%x00"])})
}
