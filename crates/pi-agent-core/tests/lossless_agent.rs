#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;

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
use pi_js_compat::js_json::{JsValue, parse_lossless};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

struct LosslessTool {
    schema: Value,
}
impl AgentTool for LosslessTool {
    fn name(&self) -> &str {
        "probe"
    }
    fn description(&self) -> &str {
        "probe"
    }
    fn label(&self) -> &str {
        "probe"
    }
    fn parameters(&self) -> &Value {
        &self.schema
    }
    fn execute(
        &self,
        _: String,
        params: JsValue,
        _: Option<CancellationToken>,
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
    let mut tool_use = AssistantMessage::empty("openai-responses", "openai", "probe", 1);
    tool_use.content.push(AssistantContent::ToolCall(call));
    tool_use.stop_reason = StopReason::ToolUse;
    let mut done = AssistantMessage::empty("openai-responses", "openai", "probe", 2);
    done.content.push(AssistantContent::Text(TextContent::new("done")));
    done.stop_reason = StopReason::Stop;
    let scripted = Arc::new(Mutex::new(VecDeque::from([tool_use, done])));
    let stream_fn: StreamFn = Arc::new(move |_: Model, _: Context, _: SimpleStreamOptions| {
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
            stream
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
        timestamp: 0,
    });
    let mut stream = agent_loop(
        vec![user],
        AgentContext {
            system_prompt: String::new(),
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
fn result_details_distinguish_missing_from_explicit_null() {
    for source in [r#"{"content":[]}"#, r#"{"content":[],"details":null}"#] {
        let result: AgentToolResult = serde_json::from_str(source).unwrap();
        assert_eq!(
            parse_lossless(&serde_json::to_string(&result).unwrap()).unwrap(),
            parse_lossless(source).unwrap()
        );
    }
}
