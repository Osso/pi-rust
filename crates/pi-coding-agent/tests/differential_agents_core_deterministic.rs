//! Differential proof for the deterministic agents-core modules.

use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;

use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
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
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

fn new_store() -> MultiAgentStore {
    MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-21T00:00:00.000Z".into()))),
    })
}

fn parse_agent(value: &Value) -> AgentSnapshot {
    pi_js_compat::js_json::from_value(&value.clone()).expect("agent snapshot")
}

fn seed(store: &MultiAgentStore, agents: &[Value]) {
    for agent in agents {
        store
            .publish_lifecycle_coordinator_snapshot(&parse_agent(agent))
            .unwrap();
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

fn lifecycle(value: &JsString) -> AgentLifecycle {
    match value {
        value if value == "running" => AgentLifecycle::Running,
        value if value == "waiting_for_input" => AgentLifecycle::WaitingForInput,
        value if value == "steering_pending" => AgentLifecycle::SteeringPending,
        value if value == "cancelling" => AgentLifecycle::Cancelling,
        value if value == "completed" => AgentLifecycle::Completed,
        value if value == "failed" => AgentLifecycle::Failed,
        value if value == "aborted" => AgentLifecycle::Aborted,
        other => panic!("unknown lifecycle {other:?}"),
    }
}

fn execute_case(case: &Value) -> Result<Value, JsString> {
    let op = case["op"].as_string().ok_or("op")?;
    match op {
        op if op == "emptyAgent" => {
            pi_js_compat::js_json::to_value(&empty_agent(case["agentId"].as_string().ok_or("agentId")?))
                .map_err(|error| JsString::from(error.to_string()))
        }
        op if op == "isInFlightLifecycle" => Ok(json!(is_in_flight_lifecycle(lifecycle(
            case["lifecycle"].as_string().ok_or("lifecycle")?
        )))),
        op if op == "validateGoalObjective" => {
            let validation = validate_goal_objective(case["prompt"].as_string().ok_or("prompt")?);
            Ok(json!({
                "validation": validation,
                "spawn": spawn_prompt_validation_message(&validation),
                "background": background_prompt_validation_message(&validation),
            }))
        }
        op if op == "toolResults" => Ok(json!({
            "success": normalize_result(&tool_result("ok", json!({"value": 1}))),
            "failure": normalize_result(&error_tool_result("bad", json!({"value": 2}))),
            "empty": normalize_result(&empty_tool_result()),
        })),
        op if op == "listAgents" => {
            let store = new_store();
            seed(&store, case["agents"].as_array().map(Vec::as_slice).unwrap_or_default());
            let params = ListAgentsParams {
                parent_id: case["parentId"].as_string().cloned(),
            };
            list_agents(&store, &params)
                .map(|result| normalize_result(&result))
                .map_err(|error| error.message)
        }
        op if op == "agentViewer" => {
            let store = new_store();
            seed(&store, case["agents"].as_array().map(Vec::as_slice).unwrap_or_default());
            let params = AgentViewerParams {
                agent_id: case["agentId"].as_string().ok_or("agentId")?.to_owned(),
                session_id: case["sessionId"].as_string().cloned(),
                store_session_id: None,
                trace: None,
            };
            agent_viewer(&store, &params, None)
                .map(|result| normalize_result(&result))
                .map_err(|error| error.message)
        }
        op if op == "assertNoWaitParams" => {
            let params = case.get("params").cloned().unwrap_or(Value::Null);
            let name = case["message"]
                .as_string()
                .map(|name| name.to_utf8().expect("fixture API name"))
                .unwrap_or_else(|| "wait_agent".to_owned());
            match pi_coding_agent::bundled_extensions::agents_core::runtime::assert_no_wait_agents_params(
                &params, &name,
            ) {
                Ok(()) => Ok(json!({"ok": true})),
                Err(error) => Ok(json!({"ok": false, "error": error.message})),
            }
        }
        other => Err(JsString::from(format!("unsupported agents-core case {other:?}"))),
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
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response:?}");
    let expected = response["output"]["results"].as_array().expect("results").clone();
    assert_eq!(expected.len(), cases.len());

    for (index, case) in cases.iter().enumerate() {
        let actual = execute_case(case);
        assert_eq!(
            expected[index]["value"],
            actual.as_ref().map_or(Value::Null, Clone::clone),
            "case {index} {:?} mismatch",
            case["name"]
        );
        if let Err(error) = actual {
            panic!("case {index} failed: {error:?}");
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
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response:?}");
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
        &JsString::from("/repo"),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<first-party:agents-core>"),
    )
    .await
    .expect("extension");

    let tools: Vec<Value> = extension
        .read()
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
        .read()
        .commands
        .values()
        .map(|command| json!({"name": command.name, "description": command.description}))
        .collect();
    let actual = json!({"commands": commands, "tools": tools});
    let expected_tools = expected["tools"].as_array().expect("tools");
    let actual_tools = actual["tools"].as_array().expect("tools");
    assert_eq!(
        expected_tools.len(),
        actual_tools.len(),
        "tool count\nexpected={expected:?}\nactual={actual:?}"
    );
    for (index, expected_tool) in expected_tools.iter().enumerate() {
        assert_eq!(
            expected_tool, &actual_tools[index],
            "tool {index} {:?}",
            expected_tool["name"]
        );
    }
    assert_eq!(expected["commands"], actual["commands"]);
}
