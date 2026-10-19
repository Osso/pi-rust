//! Differential: `supervisor/main.ts` request loop against the pinned TypeScript module.

#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxContent, FauxModelDefinition, RegisterFauxProviderOptions, faux_assistant_message,
    faux_provider, faux_tool_call,
};
use pi_ai::types::StopReason;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
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
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

const SOURCE_MODULE: &str = "packages/coding-agent/src/supervisor/main.ts";

fn normalize_session_file(path: &str) -> Vec<Value> {
    let content = std::fs::read_to_string(path).expect("session file");
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
            for (key, child) in object.iter_mut() {
                if key == "id" || key == "parentId" || key == "toolCallId" {
                    *child = Value::String("<id>".to_owned());
                } else if key == "timestamp" {
                    *child = Value::String("<timestamp>".to_owned());
                } else if key == "cwd" {
                    *child = Value::String("<cwd>".to_owned());
                } else if matches!(
                    key.as_str(),
                    "input" | "output" | "cacheRead" | "cacheWrite" | "totalTokens"
                ) {
                    *child = Value::String("<number>".to_owned());
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
            id: "gpt-5.6-sol".to_owned(),
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
                serde_json::Map::from_iter([
                    ("kind".to_owned(), Value::String("approve".to_owned())),
                    ("reason".to_owned(), Value::String("ok".to_owned())),
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
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(serde_json::Map::new()))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    let mut manager = SessionManager::create(
        dir,
        Some(&session_dir),
        Some(NewSessionOptions {
            id: Some("supervisor".to_owned()),
            ..Default::default()
        }),
    )
    .expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&control_db_path), Default::default())
        .expect("metadata control db");
    post_supervisor_request(
        &control_db_path,
        &pi_coding_agent::core::session_control_db::PostSupervisorRequestInput {
            sender_session_id: "requester".to_owned(),
            project_id: "pi".to_owned(),
            kind: SupervisorRequestKind::ApprovalReview,
            payload: Map::from_iter([("toolName".to_owned(), Value::String("read".to_owned()))]),
            deadline_at: "2099-01-01T00:00:00.000Z".to_owned(),
        },
    )
    .expect("post request");
    let session_file = manager.get_session_file().expect("session file").to_owned();
    let session = Arc::new(
        create_supervisor_session(CreateSupervisorSessionOptions {
            agent_dir: dir.to_owned(),
            kb_dir: dir.to_owned(),
            model_runtime: runtime,
            resource_loader: StaticResourceLoader::new(),
            session_manager: manager,
            settings_manager: Arc::new(Mutex::new(create_supervisor_settings_manager())),
        })
        .await
        .expect("session")
        .session,
    );
    let signal = CancellationToken::new();
    let wake_server = Arc::new(SupervisorRequestWakeServer::new(&control_db_path));
    wake_server.start().await.expect("wake server");
    let process_signal = signal.clone();
    run_supervisor_request_loop(RunSupervisorRequestLoopInput {
        claim_token: "claim-token".to_owned(),
        console_prompts: Arc::new(Mutex::new(SupervisorConsolePromptQueue::new())),
        control_db_path: control_db_path.clone(),
        session: session.clone(),
        signal: signal.clone(),
        wake_server: Arc::clone(&wake_server),
        claim_next_request: None,
        process_request: Some(Arc::new(move |db_path, request, session| {
            let process_signal = process_signal.clone();
            Box::pin(async move {
                process_supervisor_request(&db_path, request, session).await;
                process_signal.cancel();
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
    assert_eq!(response["ok"], true, "{response}");
    let ts = &response["output"];
    assert_eq!(rust["controlRows"], ts["controlRows"]);
    assert_eq!(rust["session"], ts["session"]);
}
