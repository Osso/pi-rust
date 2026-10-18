pub mod support;

use std::sync::Arc;

use pi_agent_core::agent::{Agent, AgentOptions, AgentToolResult};
use pi_ai::types::{AssistantContent, ToolCall};
use serde_json::json;
use support::*;

#[tokio::test]
async fn settles_when_active_tool_ignores_its_abort_signal() {
    let started = Arc::new(tokio::sync::Notify::new());
    let started_hook = Arc::clone(&started);
    let mut tool = TestTool::no_args("stuck_tool", || text_result("unused"));
    tool.execute = Arc::new(move |_id, _params, _signal, _on_update| {
        let started_hook = Arc::clone(&started_hook);
        Box::pin(async move {
            started_hook.notify_waiters();
            std::future::pending::<()>().await;
            Ok(AgentToolResult::text("unreachable"))
        })
    });
    let tool_use_message = tool_use(vec![AssistantContent::ToolCall(ToolCall::new(
        "call-1",
        "stuck_tool",
        json!({}).as_object().cloned().unwrap_or_default(),
    ))]);
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(vec![Arc::new(tool)]),
            ..Default::default()
        }),
        stream_fn: Some(scripted_stream_fn(vec![Script::message(tool_use_message)])),
        ..Default::default()
    })
    .unwrap();
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move { agent_clone.prompt("run stuck tool").await });
    started.notified().await;
    agent.abort();
    tokio::time::timeout(std::time::Duration::from_secs(1), prompt)
        .await
        .expect("prompt must settle")
        .unwrap()
        .unwrap();
    assert!(!agent.is_streaming());
    let messages = agent.messages();
    let last = messages.last().unwrap().as_assistant().expect("assistant failure");
    assert_eq!(last.stop_reason, pi_ai::types::StopReason::Aborted);
    assert_eq!(last.error_message.as_deref(), Some("Agent run aborted"));
}
