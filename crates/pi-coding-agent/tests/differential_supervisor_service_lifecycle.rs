use super::oracle;
use pi_ai::{
    compat::{ApiProvider, register_api_provider},
    request::CallbackError,
    types::{AssistantContent, AssistantMessage, AssistantMessageEvent, DoneReason, StopReason, TextContent, ToolCall},
    utils::event_stream::create_assistant_message_event_stream,
};
use pi_coding_agent::core::session_control_db::{
    PostSupervisorRequestInput, SupervisorRequestKind, SupervisorRequestStatus, post_supervisor_request,
    read_supervisor_request, with_control_db,
};
use pi_js_compat::{js_json as json, js_json::JsValue};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
const RESULT_PREFIX: &str = "SUPERVISOR_SERVICE_RESULT ";
fn environment(root: &Path) -> Vec<(&'static str, OsString)> {
    vec![
        ("HOME", root.join("home").into_os_string()),
        ("XDG_CONFIG_HOME", root.join("config").into_os_string()),
        ("XDG_STATE_HOME", root.join("state").into_os_string()),
        ("PI_KB_DIR", root.join("kb").into_os_string()),
        ("PI_CODING_AGENT_DIR", root.join("config/pi/agent").into_os_string()),
        ("PI_CODING_AGENT_STATE_DIR", root.join("state/pi").into_os_string()),
    ]
}
fn populate(root: &Path) {
    for name in ["home", "config/pi/agent", "state", "kb"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    super::differential_supervisor_instruction_evaluator::write_models(&root.join("config/pi/agent"));
}
fn register_service_fixture(calls: Arc<Mutex<Vec<&'static str>>>, api: &'static str) {
    register_api_provider(
        ApiProvider::new(
            api,
            |_, _, _| Err(CallbackError::new("unexpected raw stream")),
            move |model, context, _| {
                let review = context.system_prompt.as_ref().is_some_and(|prompt| {
                    prompt.starts_with("You are Pi's stateless Supervisor instruction veto gate.")
                });
                let compaction = !review && context.tools.is_none();
                calls.lock().unwrap().push(if review {
                    "review"
                } else if compaction {
                    "compaction"
                } else {
                    "request"
                });
                let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, 0.0);
                let reason = if review || compaction {
                    message.content = vec![AssistantContent::Text(TextContent::new(if review {
                        "ACCEPT"
                    } else {
                        "Frozen local compaction summary."
                    }))];
                    message.stop_reason = StopReason::Stop;
                    DoneReason::Stop
                } else {
                    let response =
                        json!({"kind":"continue","instructions":"Evidence is available.","reason":"bounded"});
                    message.content = vec![AssistantContent::ToolCall(ToolCall::new(
                        "decision",
                        "supervisor_response",
                        response.as_object().unwrap().clone(),
                    ))];
                    message.stop_reason = StopReason::ToolUse;
                    DoneReason::ToolUse
                };
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Start {
                    partial: message.clone(),
                });
                sender.push(AssistantMessageEvent::Done { reason, message });
                sender.end(None);
                Ok(stream)
            },
        ),
        Some("supervisor-service-test".into()),
    );
}
async fn spawn_compaction_http_and_seed_transcript(remote: Arc<AtomicUsize>) -> tokio::task::JoinHandle<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = stream.read(&mut bytes).await.unwrap();
            assert!(bytes[..count].starts_with(b"POST /codex/responses/compact "));
            remote.fetch_add(1, Ordering::SeqCst);
            let body = "{\"error\":{\"message\":\"unexpected remote compaction\"}}";
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let agent = PathBuf::from(std::env::var_os("PI_CODING_AGENT_DIR").unwrap());
    let model_path = agent.join("models.json");
    let mut models = pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(&model_path).unwrap()).unwrap();
    for provider in models["providers"].as_object_mut().unwrap().values_mut() {
        provider["api"] = json!("openai-codex-responses");
        provider["baseUrl"] = json!(endpoint);
    }
    std::fs::write(model_path, pi_js_compat::js_json::stringify_lossless(&models)).unwrap();
    let auth_path = agent.join("auth.json");
    let mut auth = pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(&auth_path).unwrap()).unwrap();
    for record in auth.as_object_mut().unwrap().values_mut() {
        record["key"] =
            json!("x.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiZml4dHVyZSJ9fQ.x");
    }
    std::fs::write(auth_path, pi_js_compat::js_json::stringify_lossless(&auth)).unwrap();
    let kb = pi_js_compat::js_json::JsString::from(std::env::var("PI_KB_DIR").unwrap());
    let directory = pi_js_compat::node_fs::decode_node_path(agent.join("supervisor-sessions").as_os_str());
    let mut manager = pi_coding_agent::core::session_manager::SessionManager::create(
        &kb,
        Some(&directory),
        Some(pi_coding_agent::core::session_manager::NewSessionOptions {
            id: Some("supervisor".into()),
            ..Default::default()
        }),
    )
    .unwrap();
    let text = "history ".repeat(10000);
    for _ in 0..2 {
        let user = json!({"role":"user","content":text,"timestamp":0});
        manager
            .append_message(pi_js_compat::js_json::from_value(&user).unwrap())
            .unwrap();
        let mut assistant = AssistantMessage::empty("openai-codex-responses", "openai-codex", "gpt-6.1-sol", 0.0);
        assistant.content = vec![AssistantContent::Text(TextContent::new(text.clone()))];
        assistant.stop_reason = StopReason::Stop;
        assistant.usage.input = 180000.0;
        assistant.usage.output = 20000.0;
        assistant.usage.total_tokens = 200000.0;
        manager
            .append_message(pi_agent_core::agent::AgentMessage::Assistant(Box::new(assistant)))
            .unwrap();
    }
    task
}

fn read_compaction_records() -> Vec<JsValue> {
    let directory = PathBuf::from(std::env::var_os("PI_CODING_AGENT_DIR").unwrap()).join("supervisor-sessions");
    let mut compactions = Vec::new();
    for file in std::fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "jsonl") {
            for line in std::fs::read_to_string(path).unwrap().lines() {
                let entry = pi_js_compat::js_json::parse_lossless(line).unwrap();
                if entry["type"] == json!("compaction") {
                    compactions.push(json!({"summary":entry["summary"],"fromHook":entry["fromHook"]}));
                }
            }
        }
    }
    compactions
}

async fn wait_for(predicate: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("service fixture timed out");
}
#[test]
fn actual_supervisor_service_reviews_and_closes_like_frozen() {
    for blocked in [false, true] {
        compare_service_case(blocked, false);
    }
}
#[test]
fn supervisor_compaction_uses_frozen_default_path_without_remote_effect() {
    compare_service_case(false, true);
}
fn compare_service_case(blocked: bool, compaction: bool) {
    let directory = tempfile::tempdir().unwrap();
    let source_root = directory.path().join("source");
    let native_root = directory.path().join("native");
    populate(&source_root);
    populate(&native_root);
    let mut source = oracle::TsOracle::spawn_with_environment(
        "coding-agent-supervisor-main-oracle.ts",
        "packages/coding-agent/src/supervisor/main.ts",
        &environment(&source_root),
    );
    let expected = source
        .request(&json!({"id":1,"operation":"serviceLifecycle","blockCompletion":blocked,"compaction":compaction}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"]["calls"],
        if compaction {
            json!(["compaction", "request", "review"])
        } else {
            json!(["request", "review"])
        }
    );
    assert_eq!(expected["output"]["remoteCompactions"], json!(0));
    assert_eq!(
        expected["output"]["compactions"],
        if compaction {
            json!([{"summary":"Frozen local compaction summary.","fromHook":false}])
        } else {
            json!([])
        }
    );
    assert_eq!(expected["output"]["consoleClosed"], json!(true));
    assert_eq!(expected["output"]["wakeClosed"], json!(true));
    if blocked {
        assert_eq!(expected["output"]["error"], json!("blocked completion"));
    } else {
        assert_eq!(expected["output"]["error"], JsValue::Null);
        assert_eq!(
            expected["output"]["response"],
            json!({"kind":"continue","instructions":"Evidence is available.","reason":"bounded"})
        );
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "differential_supervisor_service_lifecycle::supervisor_service_child",
            "--ignored",
            "--nocapture",
        ])
        .envs(environment(&native_root))
        .env("PI_SERVICE_BLOCK", if blocked { "1" } else { "0" })
        .env("PI_SERVICE_COMPACTION", if compaction { "1" } else { "0" });
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "child failed: {} {}",
        String::from_utf8(output.stdout.clone()).unwrap(),
        String::from_utf8(output.stderr).unwrap()
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let result = text
        .lines()
        .find_map(|line| line.strip_prefix(RESULT_PREFIX))
        .expect("child result");
    let actual = pi_js_compat::js_json::parse_lossless(result).unwrap();
    assert_eq!(actual, expected["output"]);
}
#[tokio::test]
#[ignore = "invoked by the service lifecycle parent tests"]
async fn supervisor_service_child() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let compaction = std::env::var("PI_SERVICE_COMPACTION").unwrap() == "1";
    let remote = Arc::new(AtomicUsize::new(0));
    let http_fixture = if compaction {
        Some(spawn_compaction_http_and_seed_transcript(remote.clone()).await)
    } else {
        None
    };
    let state = pi_coding_agent::config::get_user_state_root().unwrap();
    let database = pi_coding_agent::core::session_control_db::get_control_db_path(&state);
    let console = PathBuf::from(format!("{}.supervisor-console.sock", database.to_utf8().unwrap()));
    let wake = PathBuf::from(format!("{}.supervisor.sock", database.to_utf8().unwrap()));
    let running = tokio::spawn(pi_coding_agent::supervisor::main::run_supervisor_service());
    wait_for(|| running.is_finished() || console.exists()).await;
    assert!(!running.is_finished(), "service ended before startup");
    assert!(wake.exists(), "wake socket did not start");
    register_service_fixture(
        calls.clone(),
        if compaction {
            "openai-codex-responses"
        } else {
            "supervisor-review-fixture"
        },
    );
    let id = post_supervisor_request(
        &database,
        &PostSupervisorRequestInput {
            sender_session_id: "main".into(),
            project_id: "fixture".into(),
            kind: SupervisorRequestKind::GoalIdleReview,
            payload: json!({"objective":"Verify the fixture."}).as_object().unwrap().clone(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .unwrap();
    if std::env::var("PI_SERVICE_BLOCK").unwrap() == "1" {
        with_control_db(&database,|connection| {
            connection.execute_batch("CREATE TRIGGER block_complete BEFORE UPDATE OF status ON supervisor_requests WHEN NEW.status = 'completed' BEGIN SELECT RAISE(ABORT, 'blocked completion'); END")?;
            Ok(())
        }).unwrap();
    }
    pi_coding_agent::supervisor::request_wake::notify_supervisor_request(&database);
    wait_for(|| {
        running.is_finished()
            || read_supervisor_request(&database, id).unwrap().unwrap().status == SupervisorRequestStatus::Completed
    })
    .await;
    if !running.is_finished() {
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(std::process::id() as i32).unwrap(),
            rustix::process::Signal::TERM,
        )
        .unwrap();
    }
    let result = tokio::time::timeout(Duration::from_secs(15), running)
        .await
        .unwrap()
        .unwrap();
    let error = result.err().map(|error| error.message);
    let stored = read_supervisor_request(&database, id).unwrap().unwrap();
    let output = json!({"error":error,"calls":*calls.lock().unwrap(),"status":stored.status,"response":stored.response,
        "consoleClosed":!console.exists(),"wakeClosed":!wake.exists(),"compactions":read_compaction_records(),
        "remoteCompactions":remote.load(Ordering::SeqCst)});
    if let Some(task) = http_fixture {
        task.abort();
    }
    println!("{RESULT_PREFIX}{}", pi_js_compat::js_json::stringify_lossless(&output));
}
