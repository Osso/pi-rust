//! Behavioral tests for the deterministic agents-core modules.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_coding_agent::bundled_extensions::agents_core::runtime_utils::create_failed_mailbox_message;
use pi_coding_agent::bundled_extensions::agents_core::session_factories::{
    MAX_GOAL_OBJECTIVE_CHARS, background_prompt_validation_message, spawn_prompt_validation_message,
    validate_goal_objective,
};
use pi_coding_agent::bundled_extensions::agents_core::shared::{
    CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, empty_agent, is_in_flight_lifecycle,
};
use pi_coding_agent::bundled_extensions::agents_core::tool_results::{
    empty_tool_result, error_tool_result, tool_result,
};
use pi_coding_agent::bundled_extensions::agents_core::viewer::{AgentViewerParams, agent_viewer, list_agents};
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentMailboxMessage, AgentSnapshot, MailboxMessageKindValue, MultiAgentStore,
    MultiAgentStoreOptions,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsObject as Map;
use pi_js_compat::js_json::JsValue as Value;

fn new_store() -> MultiAgentStore {
    MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-21T00:00:00.000Z".into()))),
    })
}

fn agent(id: &str, parent: Option<&str>, display: &str, kind: &str, lifecycle: &str, revision: i64) -> Value {
    json!({
        "id": id,
        "parentId": parent,
        "displayName": display,
        "agentType": kind,
        "lifecycle": lifecycle,
        "revision": revision,
        "createdAt": "2026-06-21T00:00:00.000Z",
        "updatedAt": "2026-06-21T00:00:00.000Z",
        "cwd": "/repo",
        "permission": { "policy": "on-request", "narrowed": true },
    })
}

fn parse(value: Value) -> AgentSnapshot {
    pi_js_compat::js_json::from_value(&value).expect("agent snapshot")
}

fn seed(store: &MultiAgentStore, values: &[Value]) {
    for value in values {
        store
            .publish_lifecycle_coordinator_snapshot(&parse(value.clone()))
            .unwrap();
    }
}

fn details(result: &AgentToolResult) -> Value {
    result.details.clone().expect("details")
}

fn text(result: &AgentToolResult) -> JsString {
    match &result.content[0] {
        pi_ai::types::TextOrImageContent::Text(text) => text.text.clone(),
        pi_ai::types::TextOrImageContent::Image(_) => panic!("expected text content"),
    }
}

#[test]
fn empty_agent_matches_the_source_literal() {
    let agent = empty_agent("agent-1");
    assert_eq!(agent.id, "agent-1");
    assert_eq!(agent.display_name, "agent-1");
    assert_eq!(agent.agent_type, "unknown");
    assert_eq!(agent.cwd, "");
    assert_eq!(agent.lifecycle, AgentLifecycle::Failed);
    assert_eq!(agent.created_at, "1970-01-01T00:00:00.000Z");
    assert_eq!(agent.updated_at, "1970-01-01T00:00:00.000Z");
    assert_eq!(agent.parent_id, None);
    assert_eq!(agent.revision, 0);
    assert_eq!(agent.permission.policy, "on-request");
    assert!(agent.permission.narrowed);
}

#[test]
fn lifecycle_classification_matches_the_source() {
    assert!(is_in_flight_lifecycle(AgentLifecycle::Running));
    assert!(is_in_flight_lifecycle(AgentLifecycle::Cancelling));
    assert!(is_in_flight_lifecycle(AgentLifecycle::SteeringPending));
    assert!(!is_in_flight_lifecycle(AgentLifecycle::WaitingForInput));
    assert!(!is_in_flight_lifecycle(AgentLifecycle::Completed));
    assert!(!is_in_flight_lifecycle(AgentLifecycle::Failed));
    assert!(!is_in_flight_lifecycle(AgentLifecycle::Aborted));
    assert_eq!(
        CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE,
        "Agent orchestration is unavailable from child agent runtimes."
    );
}

#[test]
fn goal_objective_validation_matches_js_trim_and_utf16_length() {
    let empty = validate_goal_objective("   \n\t ");
    assert!(!empty.ok);
    assert_eq!(empty.reason_str(), Some("empty"));
    assert_eq!(empty.length, Some(0));
    assert_eq!(
        spawn_prompt_validation_message(&empty),
        "spawn_agent requires a non-empty prompt"
    );
    assert_eq!(background_prompt_validation_message(&empty), "Usage: /bg <prompt>");

    let trimmed = validate_goal_objective("  do the thing \n");
    assert!(trimmed.ok);
    assert_eq!(trimmed.objective.clone(), Some(JsString::from("do the thing")));

    let exact = "a".repeat(MAX_GOAL_OBJECTIVE_CHARS);
    assert!(validate_goal_objective(&exact).ok);

    let oversized = "a".repeat(MAX_GOAL_OBJECTIVE_CHARS + 1);
    let too_long = validate_goal_objective(&oversized);
    assert!(!too_long.ok);
    assert_eq!(too_long.reason_str(), Some("too_long"));
    assert_eq!(
        spawn_prompt_validation_message(&too_long),
        "spawn_agent prompt too long (10001 > 10000 chars)"
    );
    assert_eq!(
        background_prompt_validation_message(&too_long),
        "Objective too long (10001 > 10000 chars)"
    );

    // JS `String.length` counts UTF-16 code units.
    let emoji = "😀".repeat(MAX_GOAL_OBJECTIVE_CHARS / 2 + 1);
    let emoji_validation = validate_goal_objective(&emoji);
    assert_eq!(
        emoji_validation.length,
        Some(MAX_GOAL_OBJECTIVE_CHARS + 2),
        "emoji length uses UTF-16 units"
    );
}

#[test]
fn failed_mailbox_message_uses_epoch_timestamps_and_empty_id() {
    let message = create_failed_mailbox_message(
        &JsString::from("body"),
        "child",
        MailboxMessageKindValue::Steer,
        "target",
    );
    assert_eq!(message.id, "");
    assert_eq!(message.body.clone(), Some(JsString::from("body")));
    assert_eq!(message.from_agent_id, "child");
    assert_eq!(message.to_agent_id, "target");
    assert_eq!(message.kind, MailboxMessageKindValue::Steer);
    assert_eq!(
        message.status,
        pi_coding_agent::core::multi_agent_store::MailboxMessageStatusValue::Failed
    );
    assert_eq!(message.created_at, "1970-01-01T00:00:00.000Z");
    assert_eq!(message.updated_at, Some(JsString::from("1970-01-01T00:00:00.000Z")));
}

#[test]
fn tool_results_match_source_shapes() {
    let success = tool_result("ok", json!({"value": 1}));
    assert_eq!(text(&success), "ok");
    assert_eq!(details(&success), json!({"value": 1}));
    assert_eq!(success.is_error, None);

    let failure = error_tool_result("bad", json!({"value": 2}));
    assert_eq!(text(&failure), "bad");
    // Frozen runtime.ts:402-412 errorResult omits isError.
    assert_eq!(failure.is_error, None);

    let empty = empty_tool_result();
    assert!(empty.content.is_empty());
    assert_eq!(details(&empty), json!({}));
}

#[test]
fn list_agents_formats_active_identities_in_store_order() {
    let store = new_store();
    seed(
        &store,
        &[
            agent("a", None, "Alpha", "explore", "running", 1),
            agent("b", Some("a"), "Beta", "implement", "waiting_for_input", 1),
            agent("c", None, "Done", "implement", "completed", 2),
        ],
    );

    let result = list_agents(&store, &Default::default()).expect("list agents");
    assert_eq!(
        text(&result),
        "Found 2 agents.\n\
         id=a name=\"Alpha\" type=explore status=active lifecycle=running\n\
         id=b name=\"Beta\" type=implement status=active lifecycle=waiting_for_input"
    );
    assert_eq!(details(&result)["activeCount"], json!(2));
    assert_eq!(details(&result)["agents"].as_array().expect("agents").len(), 2);

    let single = new_store();
    seed(&single, &[agent("only", None, "Solo", "explore", "running", 1)]);
    assert!(text(&list_agents(&single, &Default::default()).expect("list")).starts_with("Found 1 agent."));
}

#[test]
fn list_agents_filters_descendants_and_excludes_terminal_rows() {
    let store = new_store();
    seed(
        &store,
        &[
            agent("root", None, "Root", "explore", "running", 1),
            agent("child", Some("root"), "Child", "implement", "running", 1),
            agent("done", Some("root"), "Done", "implement", "failed", 2),
        ],
    );
    let params = pi_coding_agent::bundled_extensions::agents_core::viewer::ListAgentsParams {
        parent_id: Some("root".to_owned().into()),
    };
    let result = list_agents(&store, &params).expect("list");
    assert_eq!(
        text(&result),
        "Found 1 agent.\nid=child name=\"Child\" type=implement status=active lifecycle=running"
    );
}

#[test]
fn agent_viewer_reports_missing_and_mismatched_sessions() {
    let store = new_store();
    seed(&store, &[agent("a", None, "Alpha", "explore", "running", 1)]);

    let missing = agent_viewer(
        &store,
        &AgentViewerParams {
            agent_id: "nope".to_owned().into(),
            session_id: None,
            store_session_id: None,
            trace: None,
        },
        None,
    )
    .expect("viewer");
    assert_eq!(text(&missing), "Agent not found: nope.");
    assert_eq!(details(&missing), json!({"agentId": "nope", "error": "not_found"}));

    let mismatch = agent_viewer(
        &store,
        &AgentViewerParams {
            agent_id: "a".to_owned().into(),
            session_id: Some("other".to_owned().into()),
            store_session_id: None,
            trace: None,
        },
        None,
    )
    .expect("viewer");
    assert_eq!(text(&mismatch), "Agent a is not attached to session other.");
    assert_eq!(
        details(&mismatch),
        json!({"agentId": "a", "error": "session_mismatch", "sessionId": "other"})
    );
}

#[test]
fn agent_viewer_exposes_status_transcript_children_and_commands() {
    let store = new_store();
    let mut parent = agent("a", None, "Alpha", "explore", "running", 3);
    parent["transcript"] = json!({"path": "/tmp/a.jsonl", "sessionId": "session-a"});
    seed(
        &store,
        &[parent, agent("b", Some("a"), "Beta", "implement", "running", 1)],
    );

    let result = agent_viewer(
        &store,
        &AgentViewerParams {
            agent_id: "a".to_owned().into(),
            session_id: Some("session-a".to_owned().into()),
            store_session_id: None,
            trace: None,
        },
        None,
    )
    .expect("viewer");
    assert_eq!(
        text(&result),
        "Viewing agent a: name=\"Alpha\" type=explore status=active lifecycle=running"
    );
    let details = details(&result);
    assert_eq!(details["children"], json!(["b"]));
    assert_eq!(
        details["commands"],
        json!([
            {"agentId": "a", "command": "stop", "tool": "close_agent"},
            {"agentId": "a", "command": "steer", "tool": "steer_agent"}
        ])
    );
    assert_eq!(details["parentId"], Value::Null);
    assert_eq!(
        details["status"],
        json!({"agentId": "a", "lifecycle": "running", "revision": 3, "terminal": false})
    );
    assert_eq!(
        details["transcript"],
        json!({"agentId": "a", "path": "/tmp/a.jsonl", "sessionId": "session-a"})
    );
}

#[test]
fn agent_viewer_renders_terminal_summary_and_error() {
    let store = new_store();
    let mut completed = agent("a", None, "Alpha", "reviewer", "completed", 4);
    completed["result"] = json!({"summary": "all good"});
    seed(&store, &[completed]);

    let result = agent_viewer(
        &store,
        &AgentViewerParams {
            agent_id: "a".to_owned().into(),
            session_id: None,
            store_session_id: None,
            trace: None,
        },
        None,
    )
    .expect("viewer");
    assert_eq!(
        text(&result),
        "Viewing agent a: name=\"Alpha\" type=reviewer status=terminal lifecycle=completed\nSummary: all good"
    );
    assert_eq!(details(&result)["status"]["terminal"], json!(true));

    let store = new_store();
    let mut failed = agent("b", None, "Beta", "reviewer", "failed", 5);
    failed["error"] = json!({"code": "E_BOOM", "message": "boom"});
    seed(&store, &[failed]);
    let result = agent_viewer(
        &store,
        &AgentViewerParams {
            agent_id: "b".to_owned().into(),
            session_id: None,
            store_session_id: None,
            trace: None,
        },
        None,
    )
    .expect("viewer");
    assert_eq!(
        text(&result),
        "Viewing agent b: name=\"Beta\" type=reviewer status=terminal lifecycle=failed\nError: boom (E_BOOM)"
    );
}

#[test]
fn persisted_agent_viewer_requires_a_control_db_and_known_session() {
    let store = new_store();
    let result = agent_viewer(
        &store,
        &AgentViewerParams {
            agent_id: "a".to_owned().into(),
            session_id: None,
            store_session_id: Some("store-session".to_owned().into()),
            trace: None,
        },
        None,
    )
    .expect("viewer");
    assert_eq!(text(&result), "Cannot view a persisted agent without a control DB.");
    assert_eq!(
        details(&result),
        json!({
            "agentId": "a",
            "error": "missing_control_db",
            "storeSessionId": "store-session"
        })
    );
}

#[test]
fn mailbox_message_round_trips_through_serde() {
    let message: AgentMailboxMessage = pi_js_compat::js_json::from_value(&json!({
        "id": "m1",
        "fromAgentId": "a",
        "toAgentId": "b",
        "kind": "message",
        "status": "pending",
        "createdAt": "2026-06-21T00:00:00.000Z",
        "updatedAt": "2026-06-21T00:00:00.000Z",
        "body": "hi"
    }))
    .expect("message");
    let value = pi_js_compat::js_json::to_value(&message).expect("serialize");
    let map: &Map = value.as_object().expect("object");
    assert!(!map.contains_key("threadId"));
}
