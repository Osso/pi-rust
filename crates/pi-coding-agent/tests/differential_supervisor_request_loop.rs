//! Differential: `supervisor/main.ts` request loop against the pinned TypeScript module.

use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxContent, FauxModelDefinition, RegisterFauxProviderOptions, faux_assistant_message,
    faux_provider, faux_tool_call,
};
use pi_ai::types::StopReason;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::session_control_db::{
    SupervisorRequestKind, post_supervisor_request, read_supervisor_request,
};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use pi_coding_agent::supervisor::main::{
    CreateSupervisorSessionOptions, RunSupervisorRequestLoopInput, SupervisorConsolePromptQueue,
    create_supervisor_session, create_supervisor_settings_manager, process_supervisor_request,
    run_supervisor_request_loop,
};
use pi_coding_agent::supervisor::request_wake::SupervisorRequestWakeServer;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsObject as Map;
use pi_js_compat::js_json::JsValue as Value;

const SOURCE_MODULE: &str = "packages/coding-agent/src/supervisor/main.ts";

fn normalize_session_file(path: &JsString) -> Vec<Value> {
    let content = std::fs::read_to_string(path.to_utf8().expect("utf-8 test path")).expect("session file");
    content
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).expect("session line");
            normalize_entry(&mut value);
            value
        })
        .collect()
}

fn normalize_entry(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_entry(item);
            }
        }
        Value::Object(object) => {
            if object
                .get("customType")
                .and_then(Value::as_string)
                .is_some_and(|value| value == "codex_restriction_context")
            {
                let data = object.get_mut("data").unwrap().as_object_mut().unwrap();
                let request_id = data.get_mut("requestId").expect("restriction context requestId");
                let text = request_id.as_string().unwrap().to_utf8().expect("UUID is scalar");
                let uuid =
                    regex::Regex::new(r"(?i)\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z")
                        .unwrap();
                assert!(uuid.is_match(&text), "restriction requestId must be a v4 UUID: {text}");
                *request_id = json!("<requestId>");
            }
            for (key, child) in object.iter_mut() {
                if key == "id" || key == "parentId" || key == "toolCallId" {
                    *child = Value::String("<id>".to_owned().into());
                } else if key == "timestamp" {
                    *child = Value::String("<timestamp>".to_owned().into());
                } else if key == "cwd" {
                    *child = Value::String("<cwd>".to_owned().into());
                } else if matches!(
                    key.to_utf8().expect("fixture field name").as_str(),
                    "input" | "output" | "cacheRead" | "cacheWrite" | "totalTokens"
                ) {
                    *child = Value::String("<number>".to_owned().into());
                } else {
                    normalize_entry(child);
                }
            }
        }
        _ => {}
    }
}

async fn run_rust(dir: &str) -> Value {
    let control_db_path = format!("{dir}/control.sqlite");
    let session_dir = format!("{dir}/sessions");
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("openai-codex".to_owned()),
        api: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "gpt-6.1-sol".to_owned(),
            name: Some("Sol".to_owned()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(200_000.0),
            max_tokens: Some(8192.0),
        }]),
        token_size: Some(pi_ai::providers::faux::FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        tokens_per_second: Some(1_000_000.0),
        ..Default::default()
    });
    faux.set_responses(vec![
        faux_assistant_message(
            FauxContent::Blocks(vec![faux_tool_call(
                "supervisor_response",
                pi_js_compat::js_json::JsObject::from_iter([
                    (JsString::from("kind"), Value::String("approve".to_owned().into())),
                    (JsString::from("reason"), Value::String("ok".to_owned().into())),
                ]),
                Some("call-1".to_owned()),
            )]),
            FauxAssistantMessageOptions {
                stop_reason: Some(StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
    ]);
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(pi_js_compat::js_json::JsObject::new())).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    let mut manager = SessionManager::create(
        &JsString::from(dir),
        Some(&JsString::from(session_dir)),
        Some(NewSessionOptions {
            id: Some("supervisor".to_owned().into()),
            ..Default::default()
        }),
    )
    .expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
        .expect("metadata control db");
    post_supervisor_request(
        &control_db_path,
        &pi_coding_agent::core::session_control_db::PostSupervisorRequestInput {
            sender_session_id: "requester".to_owned().into(),
            project_id: "pi".to_owned().into(),
            kind: SupervisorRequestKind::ApprovalReview,
            payload: Map::from_iter([(JsString::from("toolName"), Value::String("read".to_owned().into()))]),
            deadline_at: "2099-01-01T00:00:00.000Z".to_owned(),
        },
    )
    .expect("post request");
    let session_file = manager.get_session_file().expect("session file").to_owned();
    let session = Arc::new(
        create_supervisor_session(CreateSupervisorSessionOptions {
            agent_dir: dir.to_owned().into(),
            kb_dir: dir.to_owned().into(),
            model_registry: runtime,
            resource_loader: StaticResourceLoader::new(),
            session_manager: manager,
            settings_manager: Arc::new(Mutex::new(create_supervisor_settings_manager())),
        })
        .await
        .expect("session")
        .session,
    );
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let wake_server = Arc::new(SupervisorRequestWakeServer::new(&JsString::from(&control_db_path)));
    wake_server.start().await.expect("wake server");
    let controller_process_signal = controller_signal.clone();
    run_supervisor_request_loop(RunSupervisorRequestLoopInput {
        claim_token: "claim-token".to_owned().into(),
        console_prompts: Arc::new(Mutex::new(SupervisorConsolePromptQueue::new())),
        control_db_path: control_db_path.clone().into(),
        session: session.clone(),
        signal: signal.clone(),
        wake_server: Arc::clone(&wake_server),
        claim_next_request: None,
        process_request: Some(Arc::new(move |db_path, request, session| {
            let controller_process_signal = controller_process_signal.clone();
            Box::pin(async move {
                process_supervisor_request(&db_path, request, session, None).await?;
                controller_process_signal.abort(None);
                Ok(())
            })
        })),
    })
    .await
    .expect("request loop");
    wake_server.close().await.expect("close wake");

    let connection = rusqlite::Connection::open(&control_db_path).expect("control db");
    let mut statement = connection
        .prepare("SELECT id, kind, status, response_json FROM supervisor_requests ORDER BY id")
        .expect("select");
    let rows: Vec<Value> = statement
        .query_map([], |row| {
            let response: Option<String> = row.get(3)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "kind": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "response_json": response
            }))
        })
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("row values");
    assert!(read_supervisor_request(&control_db_path, 1).unwrap().is_some());
    json!({
        "controlRows": rows,
        "session": normalize_session_file(&session_file)
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn supervisor_request_loop_matches_typescript() {
    let rust_dir = tempfile::tempdir().expect("rust temp");
    let ts_dir = tempfile::tempdir().expect("ts temp");
    let rust = run_rust(&rust_dir.path().display().to_string()).await;
    let mut oracle = oracle::TsOracle::spawn("coding-agent-supervisor-request-loop-oracle.ts", SOURCE_MODULE);
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "supervisor-request-loop",
        "dir": ts_dir.path().display().to_string()
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let ts = &response["output"];
    assert_eq!(rust["controlRows"], ts["controlRows"]);
    assert_eq!(rust["session"], ts["session"]);
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
