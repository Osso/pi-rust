//! Whole pinned program oracle plus the exact shipped Rust example on the real SDK.
#[path = "../examples/sdk/01_minimal.rs"]
mod minimal;
use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxResponseStep, RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
    faux_text, faux_tool_call,
};
use pi_ai::session_resources::{SessionResourceCleanupRegistration, register_session_resource_cleanup};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::loader::ExtensionRegistry;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::CreateAgentSessionOptions;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::SettingsManager;
use serde_json::{Value, json};

const PROMPT: &str = "What files are in the current directory?";
const TEXT: &str = "files: α.rs\n";

#[derive(Clone, Default)]
struct Output(Arc<Mutex<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Output {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

struct Fixture {
    _home: tempfile::TempDir,
    options: CreateAgentSessionOptions,
    session_file: JsString,
    cleaned: Arc<Mutex<bool>>,
    cleanup: SessionResourceCleanupRegistration,
}

async fn fixture(authenticated: bool) -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let cwd = home.path().join("project");
    let agent_dir = home.path().join("agent");
    let sessions = home.path().join("sessions");
    for path in [&cwd, &agent_dir, &sessions] {
        std::fs::create_dir_all(path).unwrap();
    }
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let mut model = faux.get_model(None).unwrap();
    if !authenticated {
        // Faux providers intentionally need no auth. Use an unregistered local
        // provider name to reject at SDK validation, before any stream starts.
        model.provider = "sdk-minimal-missing-auth".to_owned();
    }
    let credentials = if authenticated {
        serde_json::Map::from_iter([(model.provider.clone(), json!({"type":"api_key","key":"fixture"}))])
    } else {
        serde_json::Map::new()
    };
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(credentials)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(
        |context, _options, _state, _model| {
            assert_eq!(
                serde_json::to_value(&context.messages).unwrap()[0]["content"],
                json!([{"type":"text","text":PROMPT}])
            );
            Box::pin(async {
                Ok(faux_assistant_message(
                    vec![
                        faux_text(TEXT),
                        faux_tool_call(
                            "end_turn",
                            serde_json::Map::from_iter([("reason".into(), json!("done"))])
                                .into_iter()
                                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                                .collect::<pi_js_compat::js_json::JsObject>(),
                            Some("done".into()),
                        ),
                    ],
                    FauxAssistantMessageOptions {
                        timestamp: Some(42.0),
                        ..Default::default()
                    },
                ))
            })
        },
    ))]);
    // Explicit empty registry: never load bundled host hooks, even with private HOME.
    let extensions = ExtensionRegistry::new()
        .load_extensions(&[], &JsString::from(&cwd.display().to_string()), None, None)
        .await
        .unwrap();
    let loader = Arc::new(StaticResourceLoader {
        extensions: Some(extensions),
        ..Default::default()
    });
    let control_db = home.path().join("control.sqlite").display().to_string();
    let mut manager = SessionManager::create(
        &JsString::from(&cwd.display().to_string()),
        Some(&JsString::from(&sessions.display().to_string())),
        None,
    )
    .unwrap();
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .unwrap();
    let session_id = manager.get_session_id().to_owned();
    let session_file = manager.get_session_file().unwrap().to_owned();
    let cleaned = Arc::new(Mutex::new(false));
    let cleaned_resource = Arc::clone(&cleaned);
    let cleanup = register_session_resource_cleanup(Box::new(move |id| {
        if id == Some(&session_id) {
            *cleaned_resource.lock().unwrap() = true;
        }
        Ok(())
    }));
    Fixture {
        _home: home,
        options: CreateAgentSessionOptions {
            cwd: Some(JsString::from(cwd.display().to_string())),
            agent_dir: Some(JsString::from(agent_dir.display().to_string())),
            model: Some(model),
            model_registry: Some(runtime),
            resource_loader: Some(loader),
            session_manager: Some(manager),
            control_db_path: Some(JsString::from(control_db)),
            settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(
                serde_json::Map::new()
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
                Default::default(),
            )))),
            disable_runtime_coordination_inbound: true,
            ..Default::default()
        },
        session_file,
        cleaned,
        cleanup,
    }
}

fn source(request: Value) -> Value {
    let mut oracle = oracle::TsOracle::spawn(
        "sdk-minimal-oracle.ts",
        "packages/coding-agent/examples/sdk/01-minimal.ts",
    );
    oracle.request(&request)
}

#[test]
fn pinned_program_defaults_observer_order_and_finally() {
    let messages =
        json!([{"role":"user","content":PROMPT},{"role":"assistant","content":[{"type":"text","text":"α"}]}]);
    let events = json!([
        {"type":"message_start"},
        {"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"hidden"}},
        {"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"α"}},
        {"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"\nβ"}},
        {"type":"message_end"}
    ]);
    let prefix = json!([
        ["createAgentSession", []],
        ["subscribe"],
        ["prompt", [PROMPT]],
        ["write", "α"],
        ["write", "\nβ"]
    ]);
    for failure in [Value::Null, json!("prompt failed")] {
        let result = source(json!({"messages":messages,"events":events,"failure":failure}));
        let mut expected = prefix.as_array().unwrap().clone();
        if failure.is_null() {
            for message in messages.as_array().unwrap() {
                expected.push(json!(["log", [message]]));
            }
            expected.push(json!(["log", []]));
        }
        expected.push(json!(["dispose"]));
        assert_eq!(result["events"], json!(expected));
        assert_eq!(result["error"], failure);
        assert_eq!(
            result["errorName"],
            if failure.is_null() { Value::Null } else { json!("Error") }
        );
    }
}

#[tokio::test]
async fn native_example_streams_then_prints_every_message_and_releases_resources() {
    let _entrypoint = minimal::main;
    let Fixture {
        options,
        session_file,
        cleaned,
        cleanup,
        _home,
    } = fixture(true).await;
    let output = Output::default();
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        minimal::run(options, output.clone()),
    )
    .await
    .expect("bounded prompt")
    .expect("example succeeds");
    assert!(*cleaned.lock().unwrap(), "dispose releases the session-scoped resource");
    cleanup.unregister();
    let output = output.text();
    let messages: Vec<Value> = output
        .strip_prefix(TEXT)
        .expect("text deltas precede message output")
        .strip_suffix("\n\n")
        .expect("final blank line")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON, not Rust Debug"))
        .collect();
    let persisted = SessionManager::open(&JsString::from(&session_file), None, None)
        .unwrap()
        .build_session_context();
    assert_eq!(json!(messages), serde_json::to_value(persisted.messages).unwrap());
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], json!([{"type":"text","text":PROMPT}]));
    assert_eq!(messages[1]["content"][0], json!({"type":"text","text":TEXT}));
    assert_eq!(messages[1]["timestamp"], 42);
    assert_eq!(messages[2]["role"], "toolResult");
    let source = source(
        json!({"messages":messages,"events":[{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":TEXT}}]}),
    );
    let mut normalized = String::new();
    for event in source["events"].as_array().unwrap() {
        match event[0].as_str().unwrap() {
            "write" => normalized.push_str(event[1].as_str().unwrap()),
            "log" => {
                if let Some(message) = event[1].as_array().unwrap().first() {
                    normalized.push_str(&serde_json::to_string(message).unwrap());
                }
                normalized.push('\n');
            }
            _ => {}
        }
    }
    assert_eq!(output, normalized, "Node log values normalized to native JSON lines");
    // Approved 2026-10-20: compare every message value as JSON, not Node inspect.
    // Preserve the unmodified source output in the proof log; do not normalize it.
    eprintln!("Full pinned source stdout:\n{}", source["stdout"].as_str().unwrap());
}

#[test]
fn entrypoint_error_keeps_name_message_and_failure_status_without_debug_quotes() {
    use std::process::{ExitCode, Termination};

    let message = "prompt failed: \"quoted\"\nnext line";
    if let Ok(case) = std::env::var("PI_SDK_MINIMAL_DIAGNOSTIC_FIXTURE") {
        let result = if case == "failure" {
            Err(minimal::ExampleError::new(JsString::from(message)))
        } else {
            Ok(())
        };
        let status = result.report();
        std::process::exit(if status == ExitCode::SUCCESS { 0 } else { 1 });
    }
    for (case, status, stderr) in [
        ("failure", 1, format!("Error: {message}\n")),
        ("success", 0, String::new()),
    ] {
        // Exercise Rust termination in an isolated test process, never default main.
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "entrypoint_error_keeps_name_message_and_failure_status_without_debug_quotes",
                "--nocapture",
            ])
            .env("PI_SDK_MINIMAL_DIAGNOSTIC_FIXTURE", case)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(status));
        assert_eq!(String::from_utf8(child.stderr).unwrap(), stderr);
    }
}

#[tokio::test]
async fn native_example_prompt_failure_releases_resources_without_printing_messages() {
    let Fixture {
        options,
        cleaned,
        cleanup,
        _home,
        ..
    } = fixture(false).await;
    let output = Output::default();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        minimal::run(options, output.clone()),
    )
    .await
    .expect("bounded prompt failure")
    .expect_err("missing fixture auth");
    assert_eq!(
        error.message,
        pi_coding_agent::core::auth_guidance::format_no_api_key_found_message("sdk-minimal-missing-auth").unwrap()
    );
    assert_eq!(error.name, "Error");
    assert!(error.original.is_some(), "native prompt source remains attached");
    let source = source(json!({"messages":[],"events":[],"failure":error.message}));
    assert_eq!(
        JsValue::from(source["error"].clone()),
        JsValue::String(error.message.clone())
    );
    assert_eq!(source["errorName"], "Error");
    assert_eq!(
        format!("{:?}", minimal::ExampleError(error.clone())),
        encode_node_utf8(&error.message)
    );
    assert_eq!(output.text(), "");
    assert!(
        *cleaned.lock().unwrap(),
        "finally releases resource after prompt failure"
    );
    cleanup.unregister();
}

use crate::shared_scripted_registry as scripted_registry;

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
