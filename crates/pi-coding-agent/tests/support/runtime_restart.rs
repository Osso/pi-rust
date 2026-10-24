use super::*;
mod oracle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
}

// Frozen runtime:438-466 + self-restart:166-203: actual persisted replacement
// supplies each interrupted result before any restart notice.
#[tokio::test]
async fn restart_resolves_pending_calls_like_the_actual_frozen_session_manager() {
    use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, ToolCall};
    use pi_coding_agent::core::session_control_db::read_multi_agent_state;
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
    let session = harness.runtime.session();
    let db = session.control_db_path().unwrap();
    let path = session.session_file().unwrap();
    read_multi_agent_state(&db, &path).unwrap();
    let connection = rusqlite::Connection::open(encode_node_utf8(&db)).unwrap();
    for (index, job) in jobs.as_array().unwrap().iter().enumerate() {
        connection
            .execute(
                "INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,?,?,'fixture')",
                rusqlite::params![path, format!("job-{index}"), job.to_string()],
            )
            .unwrap();
    }
    drop(connection);
    session
        .session_manager()
        .append_message(AgentMessage::Assistant(Box::new(pending.clone())))
        .unwrap();
    let source_root = harness._temp.path().join("source");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"resolve","root":source_root,"messages":[pending],"jobs":jobs}));
    assert_eq!(expected["results"].as_array().unwrap().len(), 3, "{expected}");
    let before = pi_js_compat::date::js_now_millis();
    harness.runtime.restart(None, false).await.unwrap();
    let after = pi_js_compat::date::js_now_millis();
    let messages = harness
        .runtime
        .session()
        .session_manager()
        .build_session_context()
        .messages;
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
    assert_eq!(json!({"results":results,"timestampsValid":timestamps_valid}), expected);
}
