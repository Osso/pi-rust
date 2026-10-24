//! Differential tests: session directory and session/coordination tools against the pinned TypeScript source.

#[path = "support/executable_script.rs"]
mod executable_script;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;

use std::path::Path;

use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, WritableSessionMetadata, archive_session, get_control_db_path,
    list_pending_architect_requests, list_runtime_mailbox_listeners, list_runtime_mailbox_messages,
    list_session_health, list_shared_channel_messages_after, read_shared_channel_cursor,
    register_runtime_mailbox_listener, write_session_goal, write_session_health, write_session_metadata,
};
use pi_coding_agent::core::session_directory::{
    BroadcastSessionFilters, BroadcastSessionsInput, SessionDirectoryOptions, broadcast_to_sessions, list_sessions,
};
use pi_coding_agent::core::session_health::SessionHealthRecord;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::{js_json, js_json as json};

struct Oracle {
    inner: oracle::TsOracle,
    next_id: u64,
}

impl Oracle {
    fn new() -> Self {
        Self {
            inner: oracle::TsOracle::spawn("coding-agent-session-tools-oracle.ts", "packages/coding-agent/src"),
            next_id: 0,
        }
    }

    fn request(&mut self, mut request: Value) -> Value {
        self.next_id += 1;
        request["id"] = json!(self.next_id);
        self.inner.request(&request)
    }
}

fn temp_root(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new().prefix(name).tempdir().expect("temp dir")
}

fn self_pid() -> u32 {
    std::process::id()
}

fn apply_setup(control_db_path: &str, setup: &[Value]) {
    for step in setup {
        match step["op"]
            .as_string()
            .expect("setup op")
            .to_utf8()
            .expect("ASCII fixture setup operation")
            .as_str()
        {
            "metadata" => write_session_metadata(
                control_db_path,
                &WritableSessionMetadata {
                    session_path: step["sessionPath"].as_string().expect("sessionPath").to_owned().into(),
                    id: step["id"].as_string().expect("id").to_owned().into(),
                    cwd: step["cwd"].as_string().expect("cwd").to_owned().into(),
                    name: step["name"].as_string().cloned(),
                    parent_session_path: step["parentSessionPath"].as_string().cloned(),
                    archived_at: None,
                    is_archived: None,
                    is_subagent: step["isSubagent"].as_bool(),
                    subagent_name: None,
                    model_provider: None,
                    model_id: None,
                    thinking_level: None,
                    created_at: step["createdAt"]
                        .as_string()
                        .unwrap_or(&JsString::from("2026-01-01T00:00:00.000Z"))
                        .to_owned()
                        .into(),
                    modified_at: step["modifiedAt"]
                        .as_string()
                        .unwrap_or(&JsString::from("2026-01-01T00:10:00.000Z"))
                        .to_owned()
                        .into(),
                    message_count: step["messageCount"].as_f64().map(|value| value as i64).unwrap_or(1),
                    first_message: step["firstMessage"]
                        .as_string()
                        .unwrap_or(&JsString::from("hello"))
                        .to_owned()
                        .into(),
                },
            )
            .expect("write metadata"),
            "goal" => write_session_goal(
                control_db_path,
                step["sessionPath"].as_string().expect("sessionPath"),
                step["goalJson"].as_string(),
            )
            .expect("write goal"),
            "health" => {
                let record: SessionHealthRecord = js_json::from_value(&step["record"].clone()).expect("health record");
                write_session_health(control_db_path, &record).expect("write health");
            }
            "listener" => {
                let pid = match &step["pid"] {
                    Value::String(value) if value == "self" => self_pid(),
                    value => u32::try_from(value.as_u64().expect("pid")).expect("pid fits"),
                };
                register_runtime_mailbox_listener(
                    control_db_path,
                    &RuntimeMailboxAddress {
                        agent_id: step["agentId"].as_string().cloned(),
                        session_id: step["sessionId"].as_string().expect("sessionId").to_owned().into(),
                    },
                    pid,
                    step["sessionPath"].as_string(),
                    Default::default(),
                )
                .expect("register listener");
            }
            "archive" => archive_session(control_db_path, step["sessionPath"].as_string().expect("sessionPath"))
                .expect("archive"),
            other => panic!("unknown setup op {other}"),
        }
    }
    for step in setup.iter().filter(|step| step["op"] == json!("listener")) {
        let updated_at = step["updatedAt"]
            .as_string()
            .cloned()
            .unwrap_or_else(|| "2026-01-01T00:10:00.000Z".into());
        let connection = rusqlite::Connection::open(control_db_path).expect("open control db");
        connection
            .execute(
                "UPDATE runtime_mailbox_listeners SET updated_at = ? WHERE recipient_session_id = ?",
                rusqlite::params![updated_at, step["sessionId"].as_string().expect("sessionId")],
            )
            .expect("update listener timestamp");
    }
}

fn directory_options(request: &Value) -> SessionDirectoryOptions {
    let alive_pids: Vec<Option<u32>> = request["alivePids"]
        .as_array()
        .expect("alivePids")
        .iter()
        .map(|value| match value {
            Value::String(text) if text == "self" => Some(std::process::id()),
            other => other.as_u64().map(|pid| u32::try_from(pid).expect("pid fits")),
        })
        .collect();
    SessionDirectoryOptions {
        now_ms: Some(
            pi_js_compat::date::date_parse_utf16(request["nowIso"].as_string().expect("nowIso").units()) as i64,
        ),
        include_ended: request["includeEnded"].as_bool(),
        is_runtime_process_alive: Some(std::sync::Arc::new(move |pid| alive_pids.contains(&Some(pid)))),
    }
}

fn stable_sessions(sessions: &[pi_coding_agent::core::session_health::SessionDirectoryEntry]) -> Value {
    Value::Array(
        sessions
            .iter()
            .map(|session| {
                json!({
                    "sessionId": session.session_id,
                    "sessionPath": session.session_path,
                    "pid": session.pid,
                    "status": session.status.as_str(),
                    "startedAt": session.started_at,
                    "lastActiveAt": session.last_active_at,
                    "name": session.name,
                    "goal": session.goal,
                    "cwd": session.cwd,
                    "lastCheckedAt": session.last_checked_at,
                    "checkStatus": session.check_status.as_str(),
                    "checkLatencyMs": session.check_latency_ms,
                    "agentGeneration": session.agent_generation,
                    "checkedGeneration": session.checked_generation,
                    "eligibleToReceive": session.eligible_to_receive,
                })
            })
            .collect(),
    )
}

fn listener_state(control_db_path: &str) -> Value {
    Value::Array(
        list_runtime_mailbox_listeners(control_db_path)
            .expect("listeners")
            .into_iter()
            .map(|listener| {
                let mut entry = js_json::JsObject::new();
                entry.insert("sessionId".into(), Value::String(listener.session_id));
                entry.insert(
                    "agentId".into(),
                    listener.agent_id.map(Value::String).unwrap_or(Value::Null),
                );
                entry.insert("pid".into(), json!(listener.pid));
                if let Some(session_path) = listener.session_path {
                    entry.insert("sessionPath".into(), Value::String(session_path.into()));
                }
                entry.insert("updatedAt".into(), Value::String(listener.updated_at.into()));
                Value::Object(entry)
            })
            .collect(),
    )
}

fn health_state(control_db_path: &str) -> Value {
    Value::Array(
        list_session_health(control_db_path)
            .expect("health")
            .into_iter()
            .map(|health| js_json::to_value(&health).expect("health serializes"))
            .collect(),
    )
}

fn mailbox_state(control_db_path: &str) -> Value {
    Value::Array(
        list_runtime_mailbox_messages(control_db_path)
            .expect("messages")
            .into_iter()
            .map(|message| {
                json!({
                    "recipient": message.recipient,
                    "sender": message.sender,
                    "kind": message.kind,
                    "body": message.body,
                })
            })
            .collect(),
    )
}

fn directory_scenarios() -> Vec<(&'static str, Vec<Value>, Value)> {
    vec![
        (
            "empty",
            vec![],
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [] }),
        ),
        (
            "live",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/session-a.jsonl",
                    "id": "session-a",
                    "cwd": "/repo/a",
                    "name": "Alpha",
                    "modifiedAt": "2026-01-01T00:10:00.000Z"
                }),
                json!({ "op": "goal", "sessionPath": "/sessions/session-a.jsonl", "goalJson": "{\"objective\":\"ship feature\"}" }),
                json!({ "op": "listener", "sessionId": "session-a", "pid": 4242, "updatedAt": "2026-01-01T00:10:00.000Z" }),
            ],
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [4242] }),
        ),
        (
            "sticky-dead",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/session-dead.jsonl",
                    "id": "session-dead",
                    "cwd": "/repo/dead"
                }),
                json!({
                    "op": "health",
                    "record": {
                        "sessionId": "session-dead",
                        "agentGeneration": 1,
                        "pid": 999999,
                        "lastActiveAt": null,
                        "lastCheckedAt": "2026-01-01T00:01:00.000Z",
                        "checkStatus": "dead",
                        "checkedGeneration": 1,
                        "checkLatencyMs": 0,
                        "updatedAt": "2026-01-01T00:01:00.000Z"
                    }
                }),
            ],
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [], "includeEnded": false }),
        ),
        (
            "duplicate-paths",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/a-historical.jsonl",
                    "id": "session-live",
                    "cwd": "/repo/historical",
                    "modifiedAt": "2026-01-01T00:11:00.000Z"
                }),
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/z-current.jsonl",
                    "id": "session-live",
                    "cwd": "/repo/current",
                    "modifiedAt": "2026-01-01T00:11:00.000Z"
                }),
                json!({ "op": "listener", "sessionId": "session-live", "pid": 4242, "updatedAt": "2026-01-01T00:11:00.000Z" }),
            ],
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [4242] }),
        ),
        (
            "unbound-historical",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/session-historical.jsonl",
                    "id": "session-historical",
                    "cwd": "/repo/historical"
                }),
                json!({
                    "op": "health",
                    "record": {
                        "sessionId": "session-historical",
                        "agentGeneration": 1,
                        "pid": 4242,
                        "lastActiveAt": "2026-01-01T00:00:00.000Z",
                        "lastCheckedAt": "2026-01-01T00:00:00.000Z",
                        "checkStatus": "ok",
                        "checkedGeneration": 1,
                        "checkLatencyMs": 0,
                        "updatedAt": "2026-01-01T00:00:00.000Z"
                    }
                }),
            ],
            json!({ "nowIso": "2026-01-01T00:11:00.000Z", "alivePids": [4242] }),
        ),
        (
            "subagents-and-archived",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/active-main.jsonl",
                    "id": "active-main",
                    "cwd": "/repo/active",
                    "modifiedAt": "2026-01-01T00:40:00.000Z"
                }),
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/archived-main.jsonl",
                    "id": "archived-main",
                    "cwd": "/repo/archived",
                    "modifiedAt": "2026-01-01T00:20:00.000Z"
                }),
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/historical-child.jsonl",
                    "id": "historical-child",
                    "cwd": "/repo/history",
                    "isSubagent": true,
                    "modifiedAt": "2026-01-01T00:00:00.000Z"
                }),
                json!({ "op": "archive", "sessionPath": "/sessions/archived-main.jsonl" }),
                json!({ "op": "listener", "sessionId": "active-main", "pid": 4242, "updatedAt": "2026-01-01T00:40:00.000Z" }),
            ],
            json!({ "nowIso": "2026-01-01T00:42:00.000Z", "alivePids": [4242] }),
        ),
    ]
}

#[test]
fn session_directory_list_differential() {
    let mut oracle = Oracle::new();
    for (name, setup, options) in directory_scenarios() {
        let rust_dir = temp_root("pi-session-directory-rs-");
        let rust_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((rust_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        apply_setup(&rust_db, &setup);
        let ts_dir = temp_root("pi-session-directory-ts-");
        let ts_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((ts_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");

        let mut ts_request = json!({
            "operation": "session-directory:list",
            "controlDbPath": ts_db,
            "setup": setup,
        });
        for (key, value) in options.as_object().expect("options") {
            ts_request.as_object_mut().unwrap().insert(key.clone(), value.clone());
        }
        let response = oracle.request(ts_request);
        assert_eq!(response["ok"], json!(true), "{name}: {:?}", response["error"]);

        let sessions = list_sessions(&JsString::from(&rust_db), &directory_options(&options)).expect("list sessions");
        let rust = json!({ "sessions": stable_sessions(&sessions) });
        assert_eq!(response["output"], rust, "{name}");
    }
}

fn broadcast_scenarios() -> Vec<(&'static str, Vec<Value>, Value, Value)> {
    let base = json!({
        "message": "please restart",
        "senderSessionId": "sender-session",
        "senderSessionPath": "/sessions/sender.jsonl",
        "senderAgentId": null
    });
    vec![
        (
            "empty",
            vec![],
            base.clone(),
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [] }),
        ),
        (
            "ended",
            vec![
                json!({ "op": "metadata", "sessionPath": "/sessions/ended.jsonl", "id": "ended", "cwd": "/repo/ended" }),
                json!({
                    "op": "health",
                    "record": {
                        "sessionId": "ended",
                        "agentGeneration": 1,
                        "pid": 999999,
                        "lastActiveAt": null,
                        "lastCheckedAt": "2026-01-01T00:01:00.000Z",
                        "checkStatus": "dead",
                        "checkedGeneration": 1,
                        "checkLatencyMs": 0,
                        "updatedAt": "2026-01-01T00:01:00.000Z"
                    }
                }),
            ],
            base.clone(),
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [] }),
        ),
        (
            "sent-and-filtered",
            vec![
                json!({ "op": "metadata", "sessionPath": "/sessions/live.jsonl", "id": "live", "cwd": "/repo/live", "name": "Live", "modifiedAt": "2026-01-01T00:10:00.000Z" }),
                json!({ "op": "metadata", "sessionPath": "/sessions/other.jsonl", "id": "other", "cwd": "/repo/other", "name": "Other", "modifiedAt": "2026-01-01T00:09:00.000Z" }),
                json!({ "op": "listener", "sessionId": "live", "pid": 4242, "updatedAt": "2026-01-01T00:10:00.000Z" }),
                json!({ "op": "listener", "sessionId": "other", "pid": 4243, "updatedAt": "2026-01-01T00:09:00.000Z" }),
            ],
            json!({
                "message": "  please restart  ",
                "filters": { "sessionIds": ["live"] },
                "senderSessionId": "sender-session",
                "senderSessionPath": "/sessions/sender.jsonl",
                "senderAgentId": null
            }),
            json!({ "nowIso": "2026-01-01T00:12:00.000Z", "alivePids": [4242, 4243] }),
        ),
    ]
}

#[test]
fn session_directory_broadcast_differential() {
    let mut oracle = Oracle::new();
    for (name, setup, broadcast, options) in broadcast_scenarios() {
        let rust_dir = temp_root("pi-session-broadcast-rs-");
        let rust_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((rust_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        apply_setup(&rust_db, &setup);
        let ts_dir = temp_root("pi-session-broadcast-ts-");
        let ts_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((ts_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");

        let mut ts_request = json!({
            "operation": "session-directory:broadcast",
            "controlDbPath": ts_db,
            "setup": setup,
            "broadcast": broadcast,
        });
        for (key, value) in options.as_object().expect("options") {
            ts_request.as_object_mut().unwrap().insert(key.clone(), value.clone());
        }
        let response = oracle.request(ts_request);
        assert_eq!(response["ok"], json!(true), "{name}: {:?}", response["error"]);

        let rust_results = broadcast_to_sessions(
            &JsString::from(&rust_db),
            &BroadcastSessionsInput {
                message: broadcast["message"].as_string().expect("message").to_owned().into(),
                filters: Some(BroadcastSessionFilters {
                    session_ids: broadcast["filters"]["sessionIds"]
                        .as_array()
                        .map(|ids| ids.iter().map(|id| id.as_string().expect("id").to_owned()).collect()),
                    cwd: broadcast["filters"]["cwd"].as_string().cloned(),
                    name: broadcast["filters"]["name"].as_string().cloned(),
                    status: None,
                }),
                sender_session_id: broadcast["senderSessionId"]
                    .as_string()
                    .expect("senderSessionId")
                    .to_owned()
                    .into(),
                sender_session_path: broadcast["senderSessionPath"]
                    .as_string()
                    .expect("senderSessionPath")
                    .to_owned()
                    .into(),
                sender_agent_id: broadcast["senderAgentId"].as_string().cloned(),
            },
            &directory_options(&options),
        )
        .expect("broadcast");

        let rust = json!({
            "results": rust_results,
            "listeners": listener_state(&rust_db),
            "health": health_state(&rust_db),
            "messages": mailbox_state(&rust_db),
        });
        assert_eq!(response["output"], rust, "{name}");
    }
}

fn tool_request(module: &str, factory: &str, params: Value, extra: Value) -> Value {
    let mut request = json!({
        "operation": "tool:execute",
        "module": module,
        "factory": factory,
        "params": params,
    });
    for (key, value) in extra.as_object().expect("extra") {
        request.as_object_mut().unwrap().insert(key.clone(), value.clone());
    }
    request
}

fn rust_tool_result(result: Result<pi_agent_core::AgentToolResult, pi_agent_core::ToolError>) -> Value {
    match result {
        Ok(result) => json!({
            "ok": true,
            "content": js_json::to_value(&result.content).expect("content"),
            "details": result.details,
            "isError": result.is_error,
            "terminate": result.terminate,
        }),
        Err(error) => json!({ "ok": false, "error": error.message }),
    }
}

#[tokio::test]
async fn end_turn_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::tools::end_turn::EndTurnTool;

    let context = tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .build()
        .await
        .1;

    let mut oracle = Oracle::new();
    for params in [
        json!({ "reason": "Finished requested work" }),
        json!({ "reason": "  " }),
    ] {
        let response = oracle.request(tool_request(
            "core/tools/end-turn.ts",
            "createEndTurnToolDefinition",
            params.clone(),
            json!({ "cwd": "/repo" }),
        ));
        let rust = rust_tool_result(
            EndTurnTool::new()
                .execute(
                    "tool-call".to_owned(),
                    json!(params),
                    None,
                    None,
                    Some(context.clone()),
                    None,
                )
                .await,
        );
        assert_eq!(response["output"], rust);
    }
}

#[tokio::test]
async fn channel_post_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::tools::channel_post::ChannelPostTool;

    let mut oracle = Oracle::new();
    for (name, params, session_id) in [
        ("posted", json!({ "message": "  hello  " }), "main-session"),
        ("empty", json!({ "message": "   " }), "main-session"),
    ] {
        let rust_dir = temp_root("pi-channel-post-rs-");
        let rust_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((rust_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let ts_dir = temp_root("pi-channel-post-ts-");
        let ts_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((ts_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let response = oracle.request(tool_request(
            "core/tools/channel-post.ts",
            "createChannelPostToolDefinition",
            params.clone(),
            json!({ "controlDbPath": ts_db, "cwd": "/repo", "sessionId": session_id }),
        ));
        let context =
            tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", session_id))
                .control_db_path(&rust_db)
                .build()
                .await
                .1;
        let rust = rust_tool_result(
            ChannelPostTool::new()
                .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
                .await,
        );
        assert_eq!(response["output"], rust, "{name}");

        let rust_state = json!({
            "messages": list_shared_channel_messages_after(&rust_db, 0, 100, i64::MAX)
                .expect("messages")
                .into_iter()
                .map(|message| json!({
                    "id": message.id,
                    "body": message.body,
                    "sender": message.sender,
                }))
                .collect::<Vec<_>>(),
            "cursor": read_shared_channel_cursor(
                &rust_db,
                &RuntimeMailboxAddress { agent_id: None, session_id: session_id.to_owned().into() },
            )
            .expect("cursor"),
        });
        let ts_state = oracle.request(json!({
            "operation": "tool:channel-post-state",
            "controlDbPath": ts_db,
            "sessionId": session_id,
        }));
        assert_eq!(ts_state["output"], rust_state, "{name} state");
    }
}

#[tokio::test]
async fn ask_architect_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::tools::ask_architect::AskArchitectTool;

    let root = temp_root("pi-ask-architect-");
    let project = root.path().join("project-worktree");
    std::fs::create_dir_all(&project).expect("project dir");
    let project_link = root.path().join("active-worktree");
    std::os::unix::fs::symlink(&project, &project_link).expect("symlink");

    let mut oracle = Oracle::new();
    for (name, params) in [
        (
            "canonical",
            json!({ "message": "inspect this", "project_path": project_link }),
        ),
        ("default", json!({ "message": "inspect this" })),
        (
            "relative",
            json!({ "message": "inspect this", "project_path": "../worktree" }),
        ),
        (
            "missing",
            json!({ "message": "inspect this", "project_path": root.path().join("missing") }),
        ),
    ] {
        let rust_dir = temp_root("pi-ask-architect-db-");
        let rust_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((rust_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let ts_dir = temp_root("pi-ask-architect-ts-db-");
        let ts_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((ts_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let response = oracle.request(tool_request(
            "core/tools/ask-architect.ts",
            "createAskArchitectToolDefinition",
            params.clone(),
            json!({ "controlDbPath": ts_db, "cwd": "/repo", "sessionId": "main-session" }),
        ));
        let context =
            tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "main-session"))
                .control_db_path(&rust_db)
                .build()
                .await
                .1;
        let rust = rust_tool_result(
            AskArchitectTool::new()
                .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
                .await,
        );
        assert_eq!(response["output"], rust, "{name}");

        let rust_state = list_pending_architect_requests(&rust_db, 10)
            .expect("requests")
            .into_iter()
            .map(|request| {
                json!({
                    "senderSessionId": request.sender_session_id,
                    "projectCwd": request.project_cwd,
                    "body": request.body,
                    "status": request.status,
                })
            })
            .collect::<Vec<_>>();
        let ts_state = oracle.request(json!({
            "operation": "tool:architect-state",
            "controlDbPath": ts_db,
        }));
        assert_eq!(ts_state["output"], Value::Array(rust_state), "{name} state");
    }
}

fn session_file(id: &str, cwd: &str, entries: &[(&str, &str)], name: Option<&str>) -> String {
    let mut lines = vec![json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": "2026-01-01T00:00:00.000Z",
        "cwd": cwd
    })];
    let mut parent: Option<String> = None;
    for (index, (kind, content)) in entries.iter().enumerate() {
        let entry_id = format!("{id}-entry-{index}");
        let line = match *kind {
            "user" => json!({
                "type": "message",
                "id": entry_id,
                "parentId": parent,
                "timestamp": format!("2026-01-01T00:00:0{}.000Z", index + 1),
                "message": { "role": "user", "content": content, "timestamp": index + 1 }
            }),
            "assistant" => json!({
                "type": "message",
                "id": entry_id,
                "parentId": parent,
                "timestamp": format!("2026-01-01T00:00:0{}.000Z", index + 1),
                "message": {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": content }],
                    "api": "openai-responses",
                    "provider": "openai",
                    "model": "gpt-5.5",
                    "usage": {
                        "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                        "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 }
                    },
                    "stopReason": "stop",
                    "timestamp": index + 1
                }
            }),
            "session_info" => json!({
                "type": "session_info",
                "id": entry_id,
                "parentId": parent,
                "timestamp": format!("2026-01-01T00:00:0{}.000Z", index + 1),
                "name": content
            }),
            "compaction" => json!({
                "type": "compaction",
                "id": entry_id,
                "parentId": parent,
                "timestamp": format!("2026-01-01T00:00:0{}.000Z", index + 1),
                "summary": content,
                "firstKeptEntryId": format!("{id}-entry-1"),
                "tokensBefore": 1000
            }),
            other => panic!("unknown entry kind {other}"),
        };
        parent = Some(entry_id);
        lines.push(line);
    }
    if let Some(name) = name {
        let entry_id = format!("{id}-entry-name");
        lines.push(json!({
            "type": "session_info",
            "id": entry_id,
            "parentId": parent,
            "timestamp": "2026-01-01T00:00:09.000Z",
            "name": name
        }));
    }
    format!(
        "{}\n",
        lines
            .iter()
            .map(js_json::stringify_lossless)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn write_file(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, content).expect("write fixture");
}

fn session_fixture_root() -> tempfile::TempDir {
    let root = temp_root("pi-session-tools-fixtures-");
    std::fs::create_dir_all(root.path().join("sessions")).expect("sessions dir");
    std::fs::create_dir_all(root.path().join("current")).expect("current dir");
    std::fs::create_dir_all(root.path().join("target")).expect("target dir");
    std::fs::create_dir_all(root.path().join("short")).expect("short dir");
    let current_cwd = root
        .path()
        .join("current")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let target_cwd = root.path().join("target").to_str().expect("utf-8 test path").to_owned();
    write_file(
        root.path(),
        "sessions/current.jsonl",
        &session_file(
            "current-session",
            &current_cwd,
            &[("user", "root work"), ("assistant", "root reply")],
            Some("current session"),
        ),
    );
    write_file(
        root.path(),
        "sessions/target.jsonl",
        &session_file(
            "019f7421-2000-7000-8000-000000000001",
            &target_cwd,
            &[("user", "target work"), ("assistant", "target reply")],
            Some("named target"),
        ),
    );
    write_file(
        root.path(),
        "sessions/short.jsonl",
        &session_file(
            "019f8def-1111-7000-8000-000000000001",
            &root.path().join("short").to_str().expect("utf-8 test path").to_owned(),
            &[("user", "short work"), ("assistant", "short reply")],
            None,
        ),
    );
    write_file(
        root.path(),
        "sessions/dup-a.jsonl",
        &session_file(
            "dup-a-session",
            &target_cwd,
            &[("user", "dup a"), ("assistant", "dup a reply")],
            Some("duplicate target"),
        ),
    );
    write_file(
        root.path(),
        "sessions/dup-b.jsonl",
        &session_file(
            "dup-b-session",
            &target_cwd,
            &[("user", "dup b"), ("assistant", "dup b reply")],
            Some("duplicate target"),
        ),
    );
    root
}

#[tokio::test]
async fn search_current_session_history_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::tools::search_current_session_history::SearchCurrentSessionHistoryTool;

    let root = temp_root("pi-search-history-");
    std::fs::create_dir_all(root.path().join("sessions")).expect("sessions dir");
    let session_path = root
        .path()
        .join("sessions/history.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let cwd = root.path().to_str().expect("utf-8 test path").to_owned();
    write_file(
        root.path(),
        "sessions/history.jsonl",
        &session_file(
            "history-session",
            &cwd,
            &[
                ("user", "Original deployment used cobalt"),
                ("user", "Neighbor before compaction"),
                ("compaction", "Earlier deployment discussion"),
                ("user", "Current follow-up deployment"),
                ("assistant", "assistant secret text"),
            ],
            None,
        ),
    );

    let mut rows: Vec<Value> = std::fs::read_to_string(&session_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows.last_mut().unwrap()["message"]["content"] = json!([
        {"type":"text","text":"assistant secret text"},
        {"type":"thinking","thinking":"hidden-thought","thinkingSignature":"hidden-signature"},
        {"type":"toolCall","id":"call","name":"inspectWidget","arguments":{"propertyNeedle":[true,17,null,{"nested":"argumentNeedle"}],"scalar":1.0,"small":0.000001}},
        {"type":"toolCall","id":"self","name":"search_current_session_history","arguments":{"query":"self-traffic"}}
    ]);
    let mut parent = "history-session-entry-4".to_owned();
    for (index, mut row) in [
        json!({"type":"message","message":{"role":"toolResult","toolCallId":"call","toolName":"inspectWidget","content":[{"type":"text","text":"resultNeedle"}],"isError":false,"timestamp":6}}),
        json!({"type":"message","message":{"role":"toolResult","toolCallId":"self","toolName":"search_current_session_history","content":[{"type":"text","text":"self-traffic"}],"isError":false,"timestamp":7}}),
        json!({"type":"custom_message","customType":"fixture","content":[{"type":"text","text":"customNeedle"}],"display":true}),
        json!({"type":"branch_summary","fromId":"history-session-entry-0","summary":"branchNeedle"}),
        json!({"type":"message","message":{"role":"bashExecution","command":"commandNeedle","output":"outputNeedle","exitCode":0,"cancelled":false,"truncated":false,"timestamp":10}}),
        json!({"type":"message","message":{"role":"compactionSummary","summary":"inlineSummaryNeedle","tokensBefore":100,"timestamp":11}}),
    ].into_iter().enumerate() {
        let id = format!("extra-{index}");
        row["id"] = json!(id); row["parentId"] = json!(parent); row["timestamp"] = json!("2026-01-01T00:01:00.000Z");
        parent = id; rows.push(row);
    }
    std::fs::write(
        &session_path,
        rows.iter()
            .map(js_json::stringify_lossless)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    let mut oracle = Oracle::new();
    let registration = oracle.request(json!({"operation":"history:definition"}));
    let definition = pi_coding_agent::core::tools::search_current_session_history::create_search_current_session_history_tool_definition();
    assert_eq!(
        registration["output"],
        json!({"name":definition.name,"label":definition.label,"description":definition.description,"promptSnippet":definition.prompt_snippet,"promptGuidelines":definition.prompt_guidelines,"parameters":definition.parameters,"executionMode":definition.execution_mode.map(|mode| mode.as_str())})
    );
    for (name, params) in [
        ("tool-name", json!({"query":"inspectWidget"})),
        ("argument-key", json!({"query":"propertyNeedle"})),
        ("argument-value", json!({"query":"argumentNeedle"})),
        ("argument-bool", json!({"query":"true"})),
        ("argument-number", json!({"query":"17"})),
        ("number-no-decimal-false-positive", json!({"query":"1.0"})),
        ("number-fixed-false-negative", json!({"query":"0.000001"})),
        ("number-no-exponent-false-positive", json!({"query":"1e-6"})),
        ("number-integer", json!({"query":"1"})),
        ("number-fixed-context", json!({"query":"0.000001","context_entries":1})),
        ("argument-null", json!({"query":"null"})),
        ("tool-result", json!({"query":"resultNeedle"})),
        ("custom", json!({"query":"customNeedle"})),
        ("branch-summary", json!({"query":"branchNeedle"})),
        ("bash-command", json!({"query":"commandNeedle"})),
        ("bash-output", json!({"query":"outputNeedle"})),
        ("inline-summary", json!({"query":"inlineSummaryNeedle"})),
        ("thinking-excluded", json!({"query":"hidden-thought"})),
        ("signature-excluded", json!({"query":"hidden-signature"})),
        ("self-excluded", json!({"query":"self-traffic"})),
        (
            "context-all-roles",
            json!({"query":"resultNeedle", "context_entries":2}),
        ),
        ("match", json!({ "query": "COBALT", "context_entries": 1 })),
        ("assistant", json!({ "query": "assistant secret" })),
        ("page", json!({ "query": "deployment", "limit": 1 })),
        ("page-2", json!({ "query": "deployment", "limit": 1, "cursor": "1" })),
        ("none", json!({ "query": "absent text" })),
        ("bad-cursor", json!({ "query": "deployment", "cursor": "-1" })),
        ("empty-query", json!({"query":"  "})),
        (
            "huge-cursor",
            json!({"query":"deployment","cursor":"999999999999999999999999999999999999"}),
        ),
        (
            "cursor-leading-zero",
            json!({"query":"deployment","limit":1,"cursor":"01"}),
        ),
    ] {
        let response = oracle.request(tool_request(
            "core/tools/search-current-session-history.ts",
            "createSearchCurrentSessionHistoryToolDefinition",
            params.clone(),
            json!({
                "cwd": cwd,
                "sessionFile": session_path,
                "sessionDir": root.path().join("sessions").to_str().expect("utf-8 test path").to_owned()
            }),
        ));
        let manager = SessionManager::open(
            &JsString::from(&session_path),
            Some(&JsString::from(
                &root
                    .path()
                    .join("sessions")
                    .to_str()
                    .expect("utf-8 test path")
                    .to_owned(),
            )),
            None,
        )
        .expect("open session");
        let context = tool_context::TestContextBuilder::new(&cwd, std::sync::Arc::new(std::sync::Mutex::new(manager)))
            .build()
            .await
            .1;
        let rust = rust_tool_result(
            SearchCurrentSessionHistoryTool::new()
                .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
                .await,
        );
        assert_eq!(response["output"], rust, "{name}");
    }
}

#[tokio::test]
async fn search_current_session_history_live_numbers_differential() {
    use pi_agent_core::AgentMessage;
    use pi_ai::types::{AssistantContent, AssistantMessage, ToolCall};
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::tools::search_current_session_history::SearchCurrentSessionHistoryTool;
    use std::sync::{Arc, Mutex};

    let root = temp_root("pi-history-live-numbers-");
    let cwd = root.path().to_str().expect("utf-8 test path").to_owned();
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&cwd)), None).unwrap();
    manager
        .append_message(
            js_json::from_value::<AgentMessage>(&json!({"role":"user","content":"fixture","timestamp":1})).unwrap(),
        )
        .unwrap();
    let mut assistant = AssistantMessage::empty("openai-responses", "openai", "fixture", 2.0);
    assistant.content.push(AssistantContent::ToolCall(ToolCall::new(
        "call",
        "inspect",
        serde_json::from_str(r#"{"scalar":1.0,"small":0.000001}"#).unwrap(),
    )));
    manager
        .append_message(AgentMessage::Assistant(Box::new(assistant)))
        .unwrap();
    let session_path = manager.get_session_file().unwrap().to_owned();
    let context = tool_context::TestContextBuilder::new(&cwd, Arc::new(Mutex::new(manager)))
        .build()
        .await
        .1;
    let mut oracle = Oracle::new();
    for (query, count) in [("1.0", 0), ("0.000001", 1), ("1e-6", 0)] {
        let params = json!({"query":query});
        let source = oracle.request(tool_request(
            "core/tools/search-current-session-history.ts",
            "createSearchCurrentSessionHistoryToolDefinition",
            params.clone(),
            json!({"cwd":cwd,"sessionFile":session_path,"sessionDir":cwd}),
        ));
        let native = rust_tool_result(
            SearchCurrentSessionHistoryTool::new()
                .execute("call".into(), json!(params), None, None, Some(context.clone()), None)
                .await,
        );
        assert_eq!(source["output"]["details"]["totalMatches"], json!(count), "{query}");
        assert_eq!(native["details"]["totalMatches"], json!(count), "{query}");
        assert_eq!(native["content"], source["output"]["content"], "{query}");
    }
}

#[tokio::test]
async fn resume_session_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::extensions::{ExtensionContext, SwitchSessionCallback};
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::tools::resume_session::ResumeSessionTool;
    use std::sync::{Arc, Mutex};

    struct Recorder {
        cancelled: bool,
        path: Arc<Mutex<Option<JsString>>>,
        starter: Arc<Mutex<Option<JsString>>>,
    }

    impl Recorder {
        fn new(cancelled: bool) -> Arc<Self> {
            Arc::new(Self {
                cancelled,
                path: Arc::new(Mutex::new(None)),
                starter: Arc::new(Mutex::new(None)),
            })
        }

        fn callback(&self, context: ExtensionContext) -> SwitchSessionCallback {
            tool_context::recording_switch_session(
                context,
                self.cancelled,
                Arc::clone(&self.path),
                Arc::clone(&self.starter),
            )
        }
    }

    let root = session_fixture_root();
    let root_path = root.path().to_str().expect("utf-8 test path").to_owned();
    let sessions_dir = root
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let current_path = root
        .path()
        .join("sessions/current.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let target_path = root
        .path()
        .join("sessions/target.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();

    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((root.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    for (file, name) in [
        ("current.jsonl", Some("current session")),
        ("target.jsonl", Some("named target")),
        ("short.jsonl", None),
        ("dup-a.jsonl", Some("duplicate target")),
        ("dup-b.jsonl", Some("duplicate target")),
    ] {
        let path = root
            .path()
            .join("sessions")
            .join(file)
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        let mut manager = SessionManager::open(&JsString::from(&path), Some(&JsString::from(&sessions_dir)), None)
            .expect("fixture manager");
        manager
            .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
            .expect("metadata");
        if let Some(name) = name {
            manager
                .set_session_name(Some(&JsString::from(name)))
                .expect("fixture name");
        }
    }
    let mut oracle = Oracle {
        inner: oracle::TsOracle::spawn(
            "coding-agent-session-tools-oracle.ts",
            "session-names:packages/coding-agent/src",
        ),
        next_id: 0,
    };
    for (name, params) in [
        ("by-id", json!({ "id": "019f7421-2000" })),
        ("by-name", json!({ "name": "named target" })),
        ("by-path", json!({ "path": target_path })),
        (
            "starter",
            json!({ "path": target_path, "starter_prompt": "continue in target" }),
        ),
        ("current", json!({ "path": current_path })),
        ("current-id", json!({ "id": "current-session" })),
        ("current-name", json!({ "name": "current session" })),
        ("ambiguous", json!({ "name": "duplicate target" })),
        (
            "missing",
            json!({ "path": root.path().join("missing.jsonl").to_str().expect("utf-8 test path").to_owned() }),
        ),
        ("both", json!({ "id": "target", "path": target_path })),
        ("neither", json!({})),
        ("blank-name", json!({ "name": "   " })),
        (
            "cancelled",
            json!({ "path": target_path, "starter_prompt": "continue" }),
        ),
    ] {
        let response = oracle.request(tool_request(
            "core/tools/resume-session.ts",
            "createResumeSessionToolDefinition",
            params.clone(),
            json!({
                "cwd": root_path,
                "controlDbPath": control_db_path,
                "sessionFile": current_path,
                "sessionDir": sessions_dir,
                "toolMode": "resume",
                "cancelled": name == "cancelled"
            }),
        ));
        let recorder = Recorder::new(name == "cancelled");
        let manager = SessionManager::open(
            &JsString::from(&current_path),
            Some(&JsString::from(&sessions_dir)),
            None,
        )
        .expect("open session");
        let (mut runner, context) = tool_context::TestContextBuilder::new(&root_path, Arc::new(Mutex::new(manager)))
            .control_db_path(&control_db_path)
            .build()
            .await;
        runner.bind_command_context(Some(tool_context::command_context_actions(
            recorder.callback(context.clone()),
        )));
        let result = ResumeSessionTool::new()
            .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
            .await;
        let mut rust = rust_tool_result(result);
        if rust["ok"] == json!(true) {
            let path = recorder.path.lock().expect("path").clone();
            let starter = recorder.starter.lock().expect("starter").clone();
            let mut switched = js_json::JsObject::new();
            if let Some(path) = path {
                switched.insert("path".into(), Value::String(path));
            }
            if let Some(starter) = starter {
                switched.insert("starter".into(), Value::String(starter));
            }
            rust["switched"] = Value::Object(switched);
        }
        assert_eq!(response["output"], rust, "{name}");
    }
}

#[tokio::test]
async fn change_working_directory_tool_differential() {
    use pi_coding_agent::core::extensions::RelocateCallback;
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::tools::change_working_directory::ChangeWorkingDirectoryTool;
    use std::sync::{Arc, Mutex};

    struct Recorder {
        calls: Mutex<Vec<Value>>,
    }

    fn relocate_callback(recorder: Arc<Recorder>) -> RelocateCallback {
        Arc::new(move |target_cwd| {
            let recorder = Arc::clone(&recorder);
            let target_cwd = target_cwd.to_owned();
            Box::pin(async move {
                recorder.calls.lock().expect("calls").push(json!({
                    "toolCallId": "tool-call",
                    "targetCwd": target_cwd,
                }));
                Ok(())
            })
        })
    }

    let root = session_fixture_root();
    let root_path = root.path().to_str().expect("utf-8 test path").to_owned();
    let sessions_dir = root
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let current_path = root
        .path()
        .join("sessions/current.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let target_cwd = root.path().join("target").to_str().expect("utf-8 test path").to_owned();
    let whitespace_dir = root.path().join(" target with whitespace ");
    std::fs::create_dir_all(&whitespace_dir).expect("whitespace dir");
    let symlink = root.path().join("target-link");
    std::os::unix::fs::symlink(&target_cwd, &symlink).unwrap();
    let raw = JsString::from_utf16(vec![0xd800]);
    let raw_existing = JsString::from(&root_path) + "/native-" + &raw;
    std::fs::create_dir(pi_js_compat::js_json::encode_node_utf8(&raw_existing)).unwrap();

    let mut oracle = Oracle::new();
    for (name, params) in [
        ("path", json!({ "path": target_cwd })),
        ("whitespace", json!({ "path": whitespace_dir })),
        ("id", json!({ "id": "019f8def-1111" })),
        ("current-id", json!({ "id": "current-session" })),
        ("missing", json!({ "path": root.path().join("missing") })),
        ("both", json!({ "path": target_cwd, "id": "019f8def-1111" })),
        ("neither", json!({})),
        ("null-path", json!({"path": null, "id": "019f8def-1111"})),
        ("null-id", json!({"id": null, "path": target_cwd})),
        ("numeric-path", json!({"path": 7})),
        ("blank-path", json!({"path": " \u{feff}\t"})),
        ("blank-id", json!({"id": " \u{a0}\t"})),
        ("trim-id", json!({"id": " \u{feff}019f8def-1111\t"})),
        ("unknown-id", json!({"id": "not-present"})),
        ("ambiguous-id", json!({"id": "dup-"})),
        ("relative-path", json!({"path": "target"})),
        ("dot-parent-path", json!({"path": "target/../short"})),
        ("file", json!({"path": current_path})),
        ("symlink", json!({"path": symlink})),
        ("nul-path", json!({"path": "target\0missing"})),
        ("raw-existing-path", json!({"path": raw_existing})),
        (
            "raw-missing-path",
            json!({"path": JsString::from(&root_path) + "/missing-" + &raw}),
        ),
        ("raw-missing-id", json!({"id": JsString::from("missing-") + &raw})),
    ] {
        let response = oracle.request(tool_request(
            "core/tools/change-working-directory.ts",
            "createChangeWorkingDirectoryToolDefinition",
            params.clone(),
            json!({
                "cwd": root_path,
                "sessionFile": current_path,
                "sessionDir": sessions_dir,
                "toolMode": "change-cwd"
            }),
        ));
        let recorder = Arc::new(Recorder {
            calls: Mutex::new(Vec::new()),
        });
        let manager = SessionManager::open(
            &JsString::from(&current_path),
            Some(&JsString::from(&sessions_dir)),
            None,
        )
        .expect("open session");
        let (mut runner, context) = tool_context::TestContextBuilder::new(&root_path, Arc::new(Mutex::new(manager)))
            .build()
            .await;
        runner.set_tool_result_relocation_handlers(
            relocate_callback(Arc::clone(&recorder)),
            Arc::new(|| Box::pin(async { Ok(()) })),
        );
        let result = ChangeWorkingDirectoryTool::new()
            .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
            .await;
        let mut rust = rust_tool_result(result);
        if rust["ok"] == json!(true) {
            runner
                .deliver_tool_result_relocation("tool-call")
                .await
                .expect("deliver relocation");
            rust["relocations"] = Value::Array(recorder.calls.lock().expect("calls").clone());
        }
        assert_eq!(response["output"], rust, "{name}");
    }
}

#[tokio::test]
async fn change_working_directory_child_guard_precedes_target_validation() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::tools::change_working_directory::ChangeWorkingDirectoryTool;

    let root = session_fixture_root();
    let cwd = pi_js_compat::node_fs::decode_node_path(root.path().as_os_str());
    let mut oracle = Oracle::new();
    for (agent_id, requires_agent_id) in [
        (Some("worker"), false),
        (None, true),
        (Some(""), true),
        (Some(""), false),
    ] {
        // Frozen change-working-directory.ts:105-110: child guard wins even over missing target/mode.
        let response = oracle.request(tool_request(
            "core/tools/change-working-directory.ts",
            "createChangeWorkingDirectoryToolDefinition",
            json!({}),
            json!({ "cwd": cwd, "toolMode": "change-cwd", "multiAgentAgentId": agent_id, "multiAgentRequiresAgentId": requires_agent_id }),
        ));
        let expected_error = if agent_id.is_some_and(|id| !id.is_empty()) || requires_agent_id {
            "change_working_directory is not available from child agent runtimes"
        } else {
            "change_working_directory requires exactly one of path or id"
        };
        assert_eq!(response["output"]["error"], json!(expected_error));
        let session = tool_context::in_memory_session(cwd.clone(), "child-guard-current");
        let mut builder =
            tool_context::TestContextBuilder::new(cwd.clone(), session).requires_agent_id(requires_agent_id);
        if let Some(agent_id) = agent_id {
            builder = builder.agent_id(agent_id);
        }
        let (_runner, context) = builder.build().await;
        let result = ChangeWorkingDirectoryTool::new()
            .execute("tool-call".to_owned(), json!({}), None, None, Some(context), None)
            .await;
        assert_eq!(
            rust_tool_result(result),
            response["output"],
            "agent_id={agent_id:?}, required={requires_agent_id}"
        );
    }
}

#[tokio::test]
async fn list_sessions_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::tools::list_sessions::ListSessionsTool;

    let mut oracle = Oracle::new();
    for (name, setup, params) in [
        ("empty", vec![], json!({})),
        (
            "ended",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/ended.jsonl",
                    "id": "ended",
                    "cwd": "/repo/ended",
                    "modifiedAt": "2026-01-01T00:10:00.000Z"
                }),
                json!({
                    "op": "health",
                    "record": {
                        "sessionId": "ended",
                        "agentGeneration": 1,
                        "pid": null,
                        "lastActiveAt": null,
                        "lastCheckedAt": "2026-01-01T00:01:00.000Z",
                        "checkStatus": "dead",
                        "checkedGeneration": 1,
                        "checkLatencyMs": 0,
                        "updatedAt": "2026-01-01T00:01:00.000Z"
                    }
                }),
            ],
            json!({ "include_ended": true }),
        ),
        (
            "ended-hidden",
            vec![
                json!({
                    "op": "metadata",
                    "sessionPath": "/sessions/ended.jsonl",
                    "id": "ended",
                    "cwd": "/repo/ended",
                    "modifiedAt": "2026-01-01T00:10:00.000Z"
                }),
                json!({
                    "op": "health",
                    "record": {
                        "sessionId": "ended",
                        "agentGeneration": 1,
                        "pid": null,
                        "lastActiveAt": null,
                        "lastCheckedAt": "2026-01-01T00:01:00.000Z",
                        "checkStatus": "dead",
                        "checkedGeneration": 1,
                        "checkLatencyMs": 0,
                        "updatedAt": "2026-01-01T00:01:00.000Z"
                    }
                }),
            ],
            json!({ "include_ended": false }),
        ),
    ] {
        let rust_dir = temp_root("pi-list-sessions-tool-rs-");
        let rust_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((rust_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        apply_setup(&rust_db, &setup);
        let ts_dir = temp_root("pi-list-sessions-tool-ts-");
        let ts_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((ts_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let response = oracle.request(tool_request(
            "core/tools/list-sessions.ts",
            "createListSessionsToolDefinition",
            params.clone(),
            json!({ "controlDbPath": ts_db, "cwd": "/repo", "sessionId": "current", "setup": setup.clone() }),
        ));
        let context =
            tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "current"))
                .control_db_path(&rust_db)
                .build()
                .await
                .1;
        let rust = rust_tool_result(
            ListSessionsTool::new()
                .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
                .await,
        );
        assert_eq!(response["output"], rust, "{name}");
    }
}

#[tokio::test]
async fn broadcast_tool_differential() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::tools::broadcast::BroadcastTool;
    use std::sync::{Arc, Mutex};

    let setup = vec![
        json!({
            "op": "metadata",
            "sessionPath": "/sessions/ended.jsonl",
            "id": "ended",
            "cwd": "/repo/ended",
            "modifiedAt": "2026-01-01T00:10:00.000Z"
        }),
        json!({
            "op": "health",
            "record": {
                "sessionId": "ended",
                "agentGeneration": 1,
                "pid": null,
                "lastActiveAt": null,
                "lastCheckedAt": "2026-01-01T00:01:00.000Z",
                "checkStatus": "dead",
                "checkedGeneration": 1,
                "checkLatencyMs": 0,
                "updatedAt": "2026-01-01T00:01:00.000Z"
            }
        }),
    ];
    let root = session_fixture_root();
    let sessions_dir = root
        .path()
        .join("sessions")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let sender_path = root
        .path()
        .join("sessions/sender.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    write_file(
        root.path(),
        "sessions/sender.jsonl",
        &session_file(
            "sender-session",
            &root.path().to_str().expect("utf-8 test path").to_owned(),
            &[],
            None,
        ),
    );

    let mut oracle = Oracle::new();
    for (name, params) in [
        ("skipped-dead", json!({ "message": "please restart" })),
        ("filtered", json!({ "message": "please restart", "name": "other" })),
    ] {
        let rust_dir = temp_root("pi-broadcast-tool-rs-");
        let rust_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((rust_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        apply_setup(&rust_db, &setup);
        let ts_dir = temp_root("pi-broadcast-tool-ts-");
        let ts_db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((ts_dir.path()).as_os_str()))
            .to_utf8()
            .expect("fixture control path is scalar");
        let response = oracle.request(tool_request(
            "core/tools/broadcast.ts",
            "createBroadcastToolDefinition",
            params.clone(),
            json!({
                "controlDbPath": ts_db,
                "cwd": root.path().to_str().expect("utf-8 test path").to_owned(),
                "sessionId": "sender-session",
                "sessionFile": sender_path,
                "sessionDir": sessions_dir,
                "setup": setup.clone()
            }),
        ));
        let sender = SessionManager::open(
            &JsString::from(&sender_path),
            Some(&JsString::from(&sessions_dir)),
            None,
        )
        .expect("open sender session");
        let context = tool_context::TestContextBuilder::new(
            &root.path().to_str().expect("utf-8 test path").to_owned(),
            Arc::new(Mutex::new(sender)),
        )
        .control_db_path(&rust_db)
        .build()
        .await
        .1;
        let rust = rust_tool_result(
            BroadcastTool::new()
                .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
                .await,
        );
        assert_eq!(response["ok"], json!(true), "{name}: {:?}", response["error"]);
        assert_eq!(response["output"], rust, "{name}");
    }
}

#[tokio::test]
async fn ask_questions_tool_differential() {
    use pi_ai::request::BoxFuture;
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::extensions::{ExtensionMode, ExtensionUiContext};
    use pi_coding_agent::core::tools::ask_questions::AskQuestionsTool;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    struct ScriptedUi {
        selects: Mutex<VecDeque<Option<JsString>>>,
        inputs: Mutex<VecDeque<Option<JsString>>>,
        panel_result: Value,
    }

    impl ExtensionUiContext for ScriptedUi {
        fn custom<'a>(
            &'a self,
            _: pi_coding_agent::core::extensions::types::ExtensionUiCustomFactory,
            _: Option<pi_coding_agent::core::extensions::types::ExtensionUiCustomOptions>,
        ) -> BoxFuture<'a, Result<Option<Value>, JsString>> {
            let value = self.panel_result.clone();
            Box::pin(async move { Ok(Some(value)) })
        }
        fn select<'a>(
            &'a self,
            _title: &JsString,
            _options: &[JsString],
            _opts: Option<&pi_coding_agent::core::extensions::ExtensionUiDialogOptions>,
        ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
            let choice = self.selects.lock().expect("selects").pop_front().flatten();
            Box::pin(async move { Ok(choice) })
        }

        fn input<'a>(
            &'a self,
            _title: &JsString,
            _placeholder: Option<&JsString>,
            _opts: Option<&pi_coding_agent::core::extensions::ExtensionUiDialogOptions>,
        ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
            let answer = self.inputs.lock().expect("inputs").pop_front().flatten();
            Box::pin(async move { Ok(answer) })
        }
    }

    let bin_dir = temp_root("pi-fake-notify-send-");
    let notify_send = bin_dir.path().join("notify-send");
    executable_script::write_executable_script(&notify_send, "#!/bin/sh\nexit 0\n");
    let path = format!(
        "{}:{}",
        bin_dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut oracle = Oracle {
        inner: oracle::TsOracle::spawn_with_environment(
            "coding-agent-session-tools-oracle.ts",
            "packages/coding-agent/src",
            &[("PATH", path.into())],
        ),
        next_id: 0,
    };
    for (name, params, selects, inputs) in [
        (
            "single",
            json!({ "questions": [{ "question": "Which approach?", "options": [{ "label": "Direct API", "description": "Keep code simple" }, { "label": "Adapter" }] }] }),
            vec![Some("1. Direct API — Keep code simple")],
            vec![],
        ),
        (
            "other",
            json!({ "questions": [{ "question": "What should we build?", "options": [{ "label": "Tool" }, { "label": "Extension" }] }] }),
            vec![Some("Other")],
            vec![Some("Use a plugin")],
        ),
        (
            "multi",
            json!({ "questions": [{ "question": "Which follow-ups?", "multiSelect": true, "options": [{ "label": "Tests" }, { "label": "Docs" }] }] }),
            vec![Some("[ ] 1. Tests"), Some("[ ] 2. Docs"), Some("Done")],
            vec![],
        ),
        (
            "cancel",
            json!({ "questions": [{ "question": "Proceed?", "options": [{ "label": "Yes" }, { "label": "No" }] }] }),
            vec![Some("Cancel")],
            vec![],
        ),
        (
            "duplicate-options",
            json!({ "questions": [{ "question": "Pick one?", "options": [{ "label": "A" }, { "label": "A" }] }] }),
            vec![],
            vec![],
        ),
        (
            "duplicate-questions",
            json!({ "questions": [
                { "question": "Duplicate?", "options": [{ "label": "A" }, { "label": "B" }] },
                { "question": "Duplicate?", "options": [{ "label": "C" }, { "label": "D" }] }
            ] }),
            vec![],
            vec![],
        ),
    ] {
        for explicit in [false, true] {
            let mut params = params.clone();
            if explicit {
                params["blocking"] = json!(true);
            }
            let panel_result = match name {
                "single" => json!({"answers":{"Which approach?":"Direct API"},"cancelled":false}),
                "other" => json!({"answers":{"What should we build?":"Use a plugin"},"cancelled":false}),
                "multi" => json!({"answers":{"Which follow-ups?":"Tests, Docs"},"cancelled":false}),
                _ => json!({"answers":{},"cancelled":true}),
            };
            let ui_script = json!({
                "panelResult": panel_result,
                "selects": selects.iter().map(|value| value.map(JsString::from)).collect::<Vec<_>>(),
                "inputs": inputs.iter().map(|value| value.map(JsString::from)).collect::<Vec<_>>()
            });
            let response = oracle.request(tool_request(
                "core/tools/ask-questions.ts",
                "createAskQuestionsToolDefinition",
                params.clone(),
                json!({ "cwd": "/repo", "uiScript": ui_script }),
            ));
            let ui = Arc::new(ScriptedUi {
                selects: Mutex::new(selects.iter().map(|value| value.map(JsString::from)).collect()),
                inputs: Mutex::new(inputs.iter().map(|value| value.map(JsString::from)).collect()),
                panel_result,
            });
            let context =
                tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
                    .mode(ExtensionMode::Tui)
                    .ui(ui)
                    .build()
                    .await
                    .1;
            let rust = rust_tool_result(
                AskQuestionsTool::new()
                    .execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
                    .await,
            );
            assert_eq!(response["output"], rust, "{name}, explicit={explicit}");
        }
    }
}
