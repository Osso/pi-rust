pub mod support;

use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{Agent, AgentMessage, AgentOptions, ContinueOptions};
use pi_ai::types::{AssistantContent, TextContent, TextOrImageContent, ToolResultMessage, ToolResultRole, UserContent};
use pi_ai::utils::event_stream::create_assistant_message_event_stream;
use serde_json::json;
use support::*;

#[tokio::test]
async fn processes_agent_end_follow_up_before_continuing_terminal_tool_result_tail() {
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let requests_hook = Arc::clone(&requests);
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(Arc::new(move |_model, context, _options| {
            requests_hook
                .lock()
                .unwrap()
                .push(serde_json::to_string(&context.messages).unwrap());
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(pi_ai::types::AssistantMessageEvent::Done {
                    reason: pi_ai::types::DoneReason::Stop,
                    message: text_assistant("Processed"),
                });
                Ok(stream)
            })
        })),
        ..Default::default()
    })
    .unwrap();

    let mut end_turn = text_assistant("");
    end_turn.content = vec![AssistantContent::ToolCall(tool_call(
        "end-1",
        "end_turn",
        json!({"reason": "Finished work"}),
    ))];
    end_turn.stop_reason = pi_ai::types::StopReason::ToolUse;

    agent.set_messages(vec![
        AgentMessage::User(user_block("Initial")),
        AgentMessage::Assistant(Box::new(end_turn)),
        AgentMessage::ToolResult(ToolResultMessage {
            role: ToolResultRole::Tag,
            tool_call_id: "end-1".to_owned(),
            tool_name: "end_turn".to_owned(),
            content: vec![TextOrImageContent::Text(TextContent::new("Turn ended: Finished work"))],
            details: Some(JsValue::from(json!({"reason": "Finished work"}))),
            is_error: false,
            timestamp: 1000.0,
        }),
    ]);
    agent.follow_up(AgentMessage::User(user_block("Queued follow-up")));

    agent
        .continue_run(ContinueOptions {
            process_queued_messages_first: true,
        })
        .await
        .unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("Queued follow-up"));
    let last = agent.messages().last().cloned().unwrap();
    assert_eq!(last.role(), "assistant");
    let queued = agent.messages().iter().any(|message| match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text == "Queued follow-up",
            UserContent::Blocks(blocks) => blocks.iter().any(|block| {
                matches!(
                    block,
                    TextOrImageContent::Text(text) if text.text == "Queued follow-up"
                )
            }),
        },
        _ => false,
    });
    assert!(queued);
}
