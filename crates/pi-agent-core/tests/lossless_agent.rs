use crate::shared_oracle as oracle;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{
    AgentContext, AgentLoopConfig, AgentMessage, AgentTool, AgentToolResult, AgentToolUpdateCallback, StreamFn,
    ToolError, agent_loop,
};
use pi_ai::request::{BoxFuture, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, Message, Model, StopReason,
    TextContent, ToolCall, UserContent, UserMessage, UserRole,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsValue, parse_lossless};
use serde_json::Value;

struct LosslessTool {
    schema: Value,
}
impl AgentTool for LosslessTool {
    fn name(&self) -> &str {
        "probe"
    }
    fn description(&self) -> pi_js_compat::js_json::JsString {
        pi_js_compat::js_json::JsString::from_utf16(vec![0xd800, 0x61])
    }
    fn label(&self) -> pi_js_compat::js_json::JsString {
        pi_js_compat::js_json::JsString::from_utf16(vec![0xdc00, 0x62])
    }
    fn parameters(&self) -> &Value {
        &self.schema
    }
    fn execute(
        &self,
        _: String,
        params: JsValue,
        _: Option<AbortSignal>,
        _: AgentToolUpdateCallback,
        _: Option<pi_agent_core::harness::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        assert_eq!(params.get("name").unwrap().as_string().unwrap().units(), &[0xd800]);
        Box::pin(async {
            Ok(
                serde_json::from_str(r#"{"content":[{"type":"text","text":"\udfff"}],"details":{"\ud800":"\udfff"}}"#)
                    .unwrap(),
            )
        })
    }
}

#[tokio::test]
async fn agent_loop_keeps_tool_text_details_and_arguments_through_message_wire_reload() {
    let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":"openai-responses","provider":"openai","baseUrl":"https://provider.invalid","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
    let call: ToolCall =
        serde_json::from_str(r#"{"type":"toolCall","id":"call","name":"probe","arguments":{"name":"\ud800"}}"#)
            .unwrap();
    let mut tool_use = AssistantMessage::empty("openai-responses", "openai", "probe", 1.0);
    tool_use.content.push(AssistantContent::ToolCall(call));
    tool_use.stop_reason = StopReason::ToolUse;
    let mut done = AssistantMessage::empty("openai-responses", "openai", "probe", 2.0);
    done.content.push(AssistantContent::Text(TextContent::new("done")));
    done.stop_reason = StopReason::Stop;
    let scripted = Arc::new(Mutex::new(VecDeque::from([tool_use, done])));
    let stream_fn: StreamFn = Arc::new(move |_: Model, context: Context, _: SimpleStreamOptions| {
        assert_eq!(context.system_prompt.as_ref().unwrap().units(), &[0xd800]);
        let tool = &context.tools.as_ref().unwrap()[0];
        assert_eq!(tool.description.units(), &[0xd800, 0x61]);
        assert_eq!(tool.label.as_ref().unwrap().units(), &[0xdc00, 0x62]);
        let message = scripted.lock().unwrap().pop_front().expect("two model turns");
        Box::pin(async move {
            let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
            let reason = if message.stop_reason == StopReason::ToolUse {
                DoneReason::ToolUse
            } else {
                DoneReason::Stop
            };
            sender.push(AssistantMessageEvent::Done {
                reason,
                message: message.clone(),
            });
            sender.end(Some(message));
            Ok(stream)
        })
    });
    let config = AgentLoopConfig::new(
        model,
        Arc::new(|messages| {
            Box::pin(async move {
                Ok(messages
                    .into_iter()
                    .filter_map(|message| match message {
                        AgentMessage::User(value) => Some(Message::User(value)),
                        AgentMessage::Assistant(value) => Some(Message::Assistant(value)),
                        AgentMessage::ToolResult(value) => Some(Message::ToolResult(value)),
                        AgentMessage::Custom(_) => None,
                    })
                    .collect())
            })
        }),
    );
    let tool = LosslessTool {
        schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
    };
    let user = AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text("start".into()),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    });
    let mut stream = agent_loop(
        vec![user],
        AgentContext {
            system_prompt: pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]),
            messages: vec![],
            tools: Some(vec![Arc::new(tool)]),
        },
        config,
        None,
        Some(stream_fn),
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while stream.next().await.is_some() {}
    })
    .await
    .unwrap();
    let messages = stream.result().await.unwrap();
    let result = messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::ToolResult(result) => Some(result),
            _ => None,
        })
        .unwrap();
    let details = parse_lossless(r#"{"\ud800":"\udfff"}"#).unwrap();
    assert_eq!(result.details.as_ref(), Some(&details));
    let wire = serde_json::to_string(&AgentMessage::ToolResult(result.clone())).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    let reference = oracle.request(&serde_json::json!({"wire":wire}));
    assert_eq!(reference["units"], serde_json::json!([0xdfff]));
    let reloaded: AgentMessage = serde_json::from_str(reference["wire"].as_str().unwrap()).unwrap();
    assert_eq!(reloaded, AgentMessage::ToolResult(result.clone()));
}

#[test]
fn tool_errors_keep_raw_units_before_becoming_result_text() {
    use pi_js_compat::js_json::JsString;
    let message = JsString::from_utf16(vec![0xd800]);
    let error = ToolError::new(message.clone());
    assert_eq!(error.message, message);
    let agent_error = pi_agent_core::agent::AgentError::from(error);
    assert_eq!(agent_error.message(), &message);
}

#[test]
fn result_details_distinguish_missing_from_explicit_null() {
    for source in [r#"{"content":[]}"#, r#"{"content":[],"details":null}"#] {
        let result: AgentToolResult = serde_json::from_str(source).unwrap();
        assert_eq!(
            parse_lossless(serde_json::to_string(&result).unwrap()).unwrap(),
            parse_lossless(source).unwrap()
        );
    }
}

#[tokio::test]
async fn agent_text_prompt_preserves_unpaired_units_through_public_input_and_transcript() {
    use pi_agent_core::{Agent, AgentOptions};
    use pi_js_compat::js_json::JsString;
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let captured = Arc::new(Mutex::new(Vec::new()));
    let observed = captured.clone();
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(Arc::new(move |_, context, _| {
            *observed.lock().unwrap() = context.messages.clone();
            Box::pin(async move {
                let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                let message = AssistantMessage::empty("fixture", "fixture", "fixture", 0.0);
                sender.push(AssistantMessageEvent::Done {
                    reason: DoneReason::Stop,
                    message,
                });
                Ok(stream)
            })
        })),
        ..Default::default()
    })
    .unwrap();
    agent.prompt(raw.clone()).await.unwrap();
    agent.prompt(&raw).await.unwrap();
    let requests = captured.lock().unwrap();
    for message in requests.iter().filter_map(|message| match message {
        Message::User(user) => Some(user),
        _ => None,
    }) {
        let UserContent::Blocks(blocks) = &message.content else {
            panic!("text block")
        };
        let pi_ai::types::TextOrImageContent::Text(text) = &blocks[0] else {
            panic!("text")
        };
        assert_eq!(text.text, raw);
    }
    let wire = pi_js_compat::js_json::to_value(&agent.messages()).unwrap();
    assert_eq!(wire[0]["content"][0]["text"], JsValue::String(raw));
}
