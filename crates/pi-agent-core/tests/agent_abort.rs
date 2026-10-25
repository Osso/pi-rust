pub use crate::shared_agent_support as support;

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{Agent, AgentEvent, AgentMessage, AgentOptions, AgentToolResult};
use pi_ai::types::{AssistantContent, AssistantMessageEvent, StopReason, TextContent, ThinkingContent, ToolCall};
use pi_ai::utils::event_stream::{StreamFailure, create_assistant_message_event_stream};
use serde_json::json;
use support::*;

#[tokio::test]
async fn streamed_thinking_text_tool_and_usage_survive_each_interruption() {
    for interruption in ["steer", "abort", "failure"] {
        let (sender, stream) = create_assistant_message_event_stream();
        let stream = Arc::new(Mutex::new(Some(stream)));
        let tool = TestTool::no_args("lookup", || panic!("interrupted tool must not execute"));
        let agent = Agent::new(AgentOptions {
            initial_state: Some(pi_agent_core::AgentInitialState {
                tools: Some(vec![Arc::new(tool)]),
                ..Default::default()
            }),
            stream_fn: Some(Arc::new(move |_, _, _| {
                let stream = stream.lock().unwrap().take().unwrap();
                Box::pin(async move { Ok(stream) })
            })),
            ..Default::default()
        })
        .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed = events.clone();
        let control = agent.clone();
        let failure_sender = sender.clone();
        agent.subscribe(Arc::new(move |event, _| {
            observed.lock().unwrap().push(event.clone());
            if matches!(event, AgentEvent::MessageUpdate { assistant_message_event, .. } if matches!(*assistant_message_event, AssistantMessageEvent::ToolCallDelta { .. })) {
                match interruption {
                    "steer" => control.steer(user("queued steering").into()),
                    "abort" => control.abort(),
                    _ => failure_sender.fail(StreamFailure::new("stream connection lost")),
                }
            }
            Box::pin(async { Ok(()) })
        }));
        let mut initial = text_assistant("");
        initial.timestamp = 1234.0;
        initial.usage.output = 7.0;
        initial.usage.total_tokens = 7.0;
        initial.content.clear();
        sender.push(AssistantMessageEvent::Start {
            partial: initial.clone(),
        });
        let mut partial = initial;
        partial
            .content
            .push(AssistantContent::Thinking(ThinkingContent::new("concrete reasoning")));
        sender.push(AssistantMessageEvent::ThinkingDelta {
            content_index: 0,
            delta: "concrete reasoning".into(),
            partial: partial.clone(),
        });
        partial
            .content
            .push(AssistantContent::Text(TextContent::new("concrete partial reply")));
        sender.push(AssistantMessageEvent::TextDelta {
            content_index: 1,
            delta: "concrete partial reply".into(),
            partial: partial.clone(),
        });
        partial
            .content
            .push(AssistantContent::ToolCall(tool_call("pending", "lookup", json!({}))));
        sender.push(AssistantMessageEvent::ToolCallDelta {
            content_index: 2,
            delta: "{}".into(),
            partial: partial.clone(),
        });
        agent.prompt("start").await.unwrap();
        let messages = agent.messages();
        let failed = messages.last().unwrap().as_assistant().unwrap();
        partial.stop_reason = if interruption == "failure" {
            StopReason::Error
        } else {
            StopReason::Aborted
        };
        partial.error_message = Some(
            if interruption == "failure" {
                "stream connection lost"
            } else {
                "Agent run aborted"
            }
            .into(),
        );
        assert_eq!(failed, &partial);
        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    AgentEvent::MessageStart {
                        message: AgentMessage::Assistant(_)
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    AgentEvent::MessageEnd {
                        message: AgentMessage::Assistant(_)
                    }
                ))
                .count(),
            1
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolExecutionStart { .. }))
        );
        assert!(!agent.is_streaming());
        assert!(agent.streaming_message().is_none());
    }
}

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
        pi_js_compat::js_json::JsObject::new(),
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
    assert_eq!(last.error_message.as_ref(), Some(&JsString::from("Agent run aborted")));
}
