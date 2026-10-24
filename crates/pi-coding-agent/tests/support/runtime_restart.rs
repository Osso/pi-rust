use super::*;
mod oracle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
}

struct DetachOrderHandle {
    index: usize,
    order: Arc<Mutex<Vec<usize>>>,
}
impl pi_coding_agent::core::tool_detach_registry::ToolDetachHandle for DetachOrderHandle {
    fn detach(&self) -> bool {
        self.order.lock().unwrap().push(self.index);
        self.index != 0
    }
}

// Frozen runtime:438 and tool-detach-registry:35-37: every registered handle
// is tried in insertion order, even when one rejects or accepts detachment.
#[tokio::test]
async fn restart_detaches_all_running_handles_like_frozen_before_teardown() {
    let harness = build_harness(false, false).await;
    let registry = harness
        .runtime
        .session()
        .extension_runner()
        .create_context()
        .tool_detach_registry()
        .unwrap()
        .unwrap();
    let order = Arc::new(Mutex::new(Vec::new()));
    let subscriptions: Vec<_> = (0..3)
        .map(|index| {
            registry.register(Arc::new(DetachOrderHandle {
                index,
                order: order.clone(),
            }))
        })
        .collect();
    let source_root = harness._temp.path().join("source-detach");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"detach","root":source_root}));
    assert_eq!(expected["detachOrder"], json!([0, 1, 2]));
    harness.runtime.restart(None, false).await.unwrap();
    assert_eq!(
        json!({"detachOrder":*order.lock().unwrap(),"results":[],"timestampsValid":true}),
        expected
    );
    drop(subscriptions);
}

fn seed_restart_jobs(db: &JsString, path: &JsString, jobs: &serde_json::Value) {
    pi_coding_agent::core::session_control_db::read_multi_agent_state(db, path).unwrap();
    let connection = rusqlite::Connection::open(encode_node_utf8(db)).unwrap();
    for (index, job) in jobs.as_array().unwrap().iter().enumerate() {
        connection
            .execute(
                "INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,?,?,'fixture')",
                rusqlite::params![path, format!("job-{index}"), job.to_string()],
            )
            .unwrap();
    }
}

fn observe_restart_results(messages: &[AgentMessage], before: f64, after: f64) -> serde_json::Value {
    let results: Vec<_> = messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::ToolResult(result) => Some(serde_json::to_value(result).unwrap()),
            _ => None,
        })
        .collect();
    let timestamps_valid = results.iter().all(|message| {
        message["timestamp"]
            .as_f64()
            .is_some_and(|time| time >= before && time <= after)
    });
    let results: Vec<_> = results
        .into_iter()
        .map(|mut message| {
            message.as_object_mut().unwrap().remove("timestamp");
            message
        })
        .collect();
    json!({"results":results,"timestampsValid":timestamps_valid})
}

// Frozen runtime:438-466 + self-restart:166-203. Compare like boundaries:
// direct resolver retains DB; persisted runtime restart reopens without DB.
#[tokio::test]
async fn restart_resolves_pending_calls_like_the_actual_frozen_session_manager() {
    use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, ToolCall};
    let harness = build_harness(false, false).await;
    let mut pending = AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
    pending.stop_reason = StopReason::ToolUse;
    pending.content = [("r", "restart_self"), ("b", "bash"), ("x", "read")]
        .into_iter()
        .map(|(id, name)| AssistantContent::ToolCall(ToolCall::new(id, name, Default::default())))
        .collect();
    let jobs = json!([
        {"id":"not-detached","detached":false,"worker":{"toolCallId":"b"}},
        {"id":"job-first","detached":true,"worker":{"toolCallId":"b"}},
        {"id":"job-later","detached":true,"worker":{"toolCallId":"b"}},
        {"id":"unrelated","detached":true}
    ]);
    let source_root = harness._temp.path().join("source");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");

    let direct_dir = harness._temp.path().join("native-direct");
    let cwd = harness.runtime.cwd();
    let mut direct = SessionManager::create(&cwd, Some(&JsString::from(direct_dir.to_str().unwrap())), None).unwrap();
    let db = harness.runtime.session().control_db_path().unwrap();
    direct
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    seed_restart_jobs(&db, direct.get_session_file().unwrap(), &jobs);
    direct
        .append_message(AgentMessage::Assistant(Box::new(pending.clone())))
        .unwrap();
    let expected = source.request(&json!({"op":"resolve","root":source_root,"messages":[pending],"jobs":jobs}));
    assert_eq!(expected["results"][1]["details"]["backgroundJobId"], "job-first");
    let before = pi_js_compat::date::js_now_millis();
    pi_coding_agent::core::self_restart::resolve_restart_interrupted_tool_calls(&mut direct).unwrap();
    let after = pi_js_compat::date::js_now_millis();
    assert_eq!(
        observe_restart_results(&direct.build_session_context().messages, before, after),
        expected
    );

    let session = harness.runtime.session();
    seed_restart_jobs(&db, &session.session_file().unwrap(), &jobs);
    session
        .session_manager()
        .append_message(AgentMessage::Assistant(Box::new(pending.clone())))
        .unwrap();
    let expected = source.request(&json!({"op":"restart","root":source_root,"messages":[pending],"jobs":jobs}));
    assert_eq!(expected["results"].as_array().unwrap().len(), 3, "{expected}");
    assert_eq!(expected["results"][1]["isError"], true, "source loses DB across reopen");
    let before = pi_js_compat::date::js_now_millis();
    harness.runtime.restart(None, false).await.unwrap();
    let after = pi_js_compat::date::js_now_millis();
    let messages = harness
        .runtime
        .session()
        .session_manager()
        .build_session_context()
        .messages;
    assert_eq!(observe_restart_results(&messages, before, after), expected);
}
