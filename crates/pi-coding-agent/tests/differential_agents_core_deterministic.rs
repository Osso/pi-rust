//! Differential proof for the deterministic agents-core modules.

#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::bundled_extensions::agents_core::parent_agent_journal::{
    ParentAgentJournalWriter, append_parent_agent_start,
};
use pi_coding_agent::bundled_extensions::agents_core::runtime_utils::create_failed_mailbox_message;
use pi_coding_agent::bundled_extensions::agents_core::session_factories::{
    background_prompt_validation_message, spawn_prompt_validation_message, validate_goal_objective,
};
use pi_coding_agent::bundled_extensions::agents_core::shared::{empty_agent, is_in_flight_lifecycle};
use pi_coding_agent::bundled_extensions::agents_core::tool_results::{
    empty_tool_result, error_tool_result, tool_result,
};
use pi_coding_agent::bundled_extensions::agents_core::viewer::{
    AgentViewerParams, ListAgentsParams, agent_viewer, list_agents,
};
use pi_coding_agent::core::extensions::HandlerError;
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentSnapshot, MailboxMessageKindValue, MultiAgentStore, MultiAgentStoreOptions,
};
use serde_json::{Value, json};

fn new_store() -> MultiAgentStore {
    MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| "2026-06-21T00:00:00.000Z".to_owned())),
    })
}

fn parse_agent(value: &Value) -> AgentSnapshot {
    serde_json::from_value(value.clone()).expect("agent snapshot")
}

fn seed(store: &MultiAgentStore, agents: &[Value]) {
    for agent in agents {
        store.publish_lifecycle_coordinator_snapshot(&parse_agent(agent));
    }
}

fn normalize_result(result: &AgentToolResult) -> Value {
    let content: Vec<Value> = result
        .content
        .iter()
        .map(|content| match content {
            TextOrImageContent::Text(text) => json!({"type": "text", "text": text.text}),
            TextOrImageContent::Image(_) => json!({"type": "image"}),
        })
        .collect();
    json!({
        "content": content,
        "details": result.details.clone().unwrap_or(Value::Null),
        "isError": result.is_error,
    })
}

fn lifecycle(value: &str) -> AgentLifecycle {
    match value {
        "running" => AgentLifecycle::Running,
        "waiting_for_input" => AgentLifecycle::WaitingForInput,
        "steering_pending" => AgentLifecycle::SteeringPending,
        "cancelling" => AgentLifecycle::Cancelling,
        "completed" => AgentLifecycle::Completed,
        "failed" => AgentLifecycle::Failed,
        "aborted" => AgentLifecycle::Aborted,
        other => panic!("unknown lifecycle {other}"),
    }
}

struct RecordingJournal {
    entries: Mutex<Vec<(String, Option<Value>)>>,
}

impl ParentAgentJournalWriter for RecordingJournal {
    fn append_entry(&self, custom_type: &str, data: Option<Value>) -> Result<(), HandlerError> {
        self.entries
            .lock()
            .expect("journal")
            .push((custom_type.to_owned(), data));
        Ok(())
    }
}

fn execute_case(case: &Value) -> Result<Value, String> {
    let op = case["op"].as_str().ok_or("op")?;
    match op {
        "emptyAgent" => serde_json::to_value(empty_agent(case["agentId"].as_str().ok_or("agentId")?))
            .map_err(|error| error.to_string()),
        "isInFlightLifecycle" => Ok(json!(is_in_flight_lifecycle(lifecycle(
            case["lifecycle"].as_str().ok_or("lifecycle")?
        )))),
        "validateGoalObjective" => {
            let validation = validate_goal_objective(case["prompt"].as_str().ok_or("prompt")?);
            Ok(json!({
                "validation": validation,
                "spawn": spawn_prompt_validation_message(&validation),
                "background": background_prompt_validation_message(&validation),
            }))
        }
        "createFailedMailboxMessage" => {
            let kind = match case["kind"].as_str().ok_or("kind")? {
                "message" => MailboxMessageKindValue::Message,
                "ask" => MailboxMessageKindValue::Ask,
                "reply" => MailboxMessageKindValue::Reply,
                "steer" => MailboxMessageKindValue::Steer,
                "parent_request" => MailboxMessageKindValue::ParentRequest,
                "system" => MailboxMessageKindValue::System,
                other => return Err(format!("unknown kind {other}")),
            };
            serde_json::to_value(create_failed_mailbox_message(
                case["body"].as_str().ok_or("body")?,
                case["fromAgentId"].as_str().ok_or("fromAgentId")?,
                kind,
                case["toAgentId"].as_str().ok_or("toAgentId")?,
            ))
            .map_err(|error| error.to_string())
        }
        "toolResults" => Ok(json!({
            "success": normalize_result(&tool_result("ok", json!({"value": 1}))),
            "failure": normalize_result(&error_tool_result("bad", json!({"value": 2}))),
            "empty": normalize_result(&empty_tool_result()),
        })),
        "listAgents" => {
            let store = new_store();
            seed(&store, case["agents"].as_array().map(Vec::as_slice).unwrap_or_default());
            let params = ListAgentsParams {
                parent_id: case["parentId"].as_str().map(str::to_owned),
            };
            list_agents(&store, &params)
                .map(|result| normalize_result(&result))
                .map_err(|error| error.message)
        }
        "agentViewer" => {
            let store = new_store();
            seed(&store, case["agents"].as_array().map(Vec::as_slice).unwrap_or_default());
            let params = AgentViewerParams {
                agent_id: case["agentId"].as_str().ok_or("agentId")?.to_owned(),
                session_id: case["sessionId"].as_str().map(str::to_owned),
                store_session_id: None,
            };
            agent_viewer(&store, &params, None)
                .map(|result| normalize_result(&result))
                .map_err(|error| error.message)
        }
        "assertNoWaitParams" => {
            let params = case.get("params").cloned().unwrap_or(Value::Null);
            let name = case["message"].as_str().unwrap_or("wait_agent");
            match pi_coding_agent::bundled_extensions::agents_core::runtime::assert_no_wait_agents_params(&params, name)
            {
                Ok(()) => Ok(json!({"ok": true})),
                Err(error) => Ok(json!({"ok": false, "error": error.message})),
            }
        }
        "appendParentAgentStart" => {
            let journal = RecordingJournal {
                entries: Mutex::new(Vec::new()),
            };
            append_parent_agent_start(&journal, &parse_agent(&case["agent"])).map_err(|error| error.message)?;
            Ok(json!(
                journal
                    .entries
                    .lock()
                    .expect("journal")
                    .iter()
                    .map(|(kind, data)| json!({"type": kind, "data": data}))
                    .collect::<Vec<_>>()
            ))
        }
        other => Err(format!("unsupported agents-core case {other}")),
    }
}

fn agent(id: &str, parent: Option<&str>, display: &str, kind: &str, lifecycle: &str, revision: i64) -> Value {
    let mut agent = json!({
        "id": id,
        "displayName": display,
        "agentType": kind,
        "lifecycle": lifecycle,
        "revision": revision,
        "createdAt": "2026-06-21T00:00:00.000Z",
        "updatedAt": "2026-06-21T00:00:00.000Z",
        "cwd": "/repo",
        "permission": { "policy": "on-request", "narrowed": true },
    });
    if let Some(parent) = parent {
        agent["parentId"] = json!(parent);
    }
    agent
}

fn cases() -> Vec<Value> {
    let agents = vec![
        agent("a", None, "Alpha", "explore", "running", 1),
        agent("b", Some("a"), "Beta", "implement", "waiting_for_input", 2),
        agent("c", Some("a"), "Gamma", "implement", "completed", 3),
        agent("d", None, "Delta", "reviewer", "failed", 4),
    ];
    let mut transcript_agent = agent("a", None, "Alpha", "explore", "running", 1);
    transcript_agent["transcript"] = json!({"path": "/tmp/a.jsonl", "sessionId": "session-a"});
    let mut failed_agent = agent("d", None, "Delta", "reviewer", "failed", 4);
    failed_agent["error"] = json!({"code": "E_BOOM", "message": "boom"});
    let mut completed_agent = agent("c", None, "Gamma", "reviewer", "completed", 3);
    completed_agent["result"] = json!({"summary": "all good"});
    vec![
        json!({"name": "empty", "op": "emptyAgent", "agentId": "agent-1"}),
        json!({"name": "running", "op": "isInFlightLifecycle", "lifecycle": "running"}),
        json!({"name": "waiting", "op": "isInFlightLifecycle", "lifecycle": "waiting_for_input"}),
        json!({"name": "cancelling", "op": "isInFlightLifecycle", "lifecycle": "cancelling"}),
        json!({"name": "completed", "op": "isInFlightLifecycle", "lifecycle": "completed"}),
        json!({"name": "blank", "op": "validateGoalObjective", "prompt": " \n\t "}),
        json!({"name": "trimmed", "op": "validateGoalObjective", "prompt": "  do it \n"}),
        json!({"name": "long", "op": "validateGoalObjective", "prompt": "a".repeat(10_001)}),
        json!({"name": "exact", "op": "validateGoalObjective", "prompt": "a".repeat(10_000)}),
        json!({"name": "emoji", "op": "validateGoalObjective", "prompt": "😀".repeat(5_001)}),
        json!({"name": "failed-message", "op": "createFailedMailboxMessage", "body": "body", "fromAgentId": "child", "kind": "steer", "toAgentId": "target"}),
        json!({"name": "tool-results", "op": "toolResults"}),
        json!({"name": "list-all", "op": "listAgents", "agents": agents, "activeCount": 2}),
        json!({"name": "list-descendants", "op": "listAgents", "agents": agents, "parentId": "a"}),
        json!({"name": "view-running", "op": "agentViewer", "agents": [transcript_agent.clone()], "agentId": "a", "sessionId": "session-a"}),
        json!({"name": "view-missing", "op": "agentViewer", "agents": agents.clone(), "agentId": "nope"}),
        json!({"name": "view-mismatch", "op": "agentViewer", "agents": [transcript_agent.clone()], "agentId": "a", "sessionId": "other"}),
        json!({"name": "view-terminal-error", "op": "agentViewer", "agents": [failed_agent], "agentId": "d"}),
        json!({"name": "view-terminal-summary", "op": "agentViewer", "agents": [completed_agent], "agentId": "c"}),
        json!({"name": "wait-null", "op": "assertNoWaitParams", "params": null}),
        json!({"name": "wait-empty", "op": "assertNoWaitParams", "params": {}}),
        json!({"name": "wait-invalid", "op": "assertNoWaitParams", "params": {"parentId": "main"}}),
        json!({"name": "wait-array", "op": "assertNoWaitParams", "params": []}),
        json!({"name": "wait-invalid-name", "op": "assertNoWaitParams", "params": {"a": 1}, "message": "pi.agents.wait"}),
        json!({"name": "journal", "op": "appendParentAgentStart", "agent": transcript_agent}),
    ]
}

#[test]
fn deterministic_agents_core_cases_match_the_pinned_typescript_modules() {
    let mut oracle = oracle::TsOracle::spawn(
        "agents-core-deterministic-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    let cases = cases();
    let response = oracle.request(&json!({"id": 9301, "cases": cases}));
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().expect("results").clone();
    assert_eq!(expected.len(), cases.len());

    for (index, case) in cases.iter().enumerate() {
        let actual = execute_case(case);
        assert_eq!(
            expected[index]["value"],
            actual.as_ref().map_or(Value::Null, Clone::clone),
            "case {index} {} mismatch",
            case["name"]
        );
        if let Err(error) = actual {
            panic!("case {index} failed: {error}");
        }
    }
}

#[tokio::test]
async fn registration_metadata_matches_the_pinned_typescript_registration() {
    use std::sync::Arc;

    use pi_coding_agent::bundled_extensions::agents_core::runtime::{
        MultiAgentExtensionOptions, create_multi_agent_runtime_handles, multi_agent_extension,
    };
    use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};

    let mut oracle = oracle::TsOracle::spawn(
        "agents-core-deterministic-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    let response = oracle.request(&json!({
        "id": 9302,
        "cases": [{"name": "tools", "op": "allToolMetadata"}]
    }));
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
    let expected = response["output"]["results"][0]["value"].clone();

    let options = MultiAgentExtensionOptions {
        runtime_handles: Some(create_multi_agent_runtime_handles()),
        ..Default::default()
    };
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let options = options.clone();
            let result = multi_agent_extension(pi, &options);
            Box::pin(async move { result })
        }),
        "/repo",
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        "<first-party:agents-core>",
    )
    .await
    .expect("extension");

    let tools: Vec<Value> = extension
        .tools
        .values()
        .map(|registered| {
            let tool = &registered.definition;
            json!({
                "name": tool.name,
                "label": tool.label,
                "description": tool.description,
                "promptGuidelines": tool.prompt_guidelines,
                "parameters": tool.parameters,
                "approvalRequired": tool.approval_required,
            })
        })
        .collect();
    let commands: Vec<Value> = extension
        .commands
        .values()
        .map(|command| json!({"name": command.name, "description": command.description}))
        .collect();
    let actual = json!({"commands": commands, "tools": tools});
    let expected_tools = expected["tools"].as_array().expect("tools");
    let actual_tools = actual["tools"].as_array().expect("tools");
    assert_eq!(expected_tools.len(), actual_tools.len(), "tool count");
    for (index, expected_tool) in expected_tools.iter().enumerate() {
        assert_eq!(
            expected_tool, &actual_tools[index],
            "tool {index} {}",
            expected_tool["name"]
        );
    }
    assert_eq!(expected["commands"], actual["commands"]);
}
