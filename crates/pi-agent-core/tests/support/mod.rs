use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{
    AgentEvent, AgentTool, AgentToolResult, AgentToolUpdateCallback, ConvertToLlm, StreamFn, ToolError,
};
use pi_ai::request::{BoxFuture, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, Message, Modality, Model,
    ModelCost, ModelCostRates, StopReason, TextContent, ToolCall, ToolCallExecution, UserMessage, UserRole,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, EventStream, create_assistant_message_event_stream};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use serde_json::Value as SchemaValue;

pub fn model() -> Model {
    Model {
        properties: Default::default(),
        id: "mock".to_owned(),
        name: "mock".into(),
        api: "openai-responses".to_owned(),
        provider: "openai".to_owned(),
        base_url: JsString::from("https://example.invalid"),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
        },
        context_window: 8192.0,
        auto_compaction_threshold: None,
        max_tokens: 2048.0,
        headers: None,
        compat: None,
    }
}

pub fn assistant(content: Vec<AssistantContent>, stop_reason: StopReason) -> AssistantMessage {
    let mut message = AssistantMessage::empty("openai-responses", "openai", "mock", 1000.0);
    message.content = content;
    message.stop_reason = stop_reason;
    message
}

pub fn text_assistant(text: &str) -> AssistantMessage {
    assistant(vec![AssistantContent::Text(TextContent::new(text))], StopReason::Stop)
}

pub fn tool_call(id: &str, name: &str, arguments: impl Into<Value>) -> ToolCall {
    let arguments = arguments.into();
    ToolCall::new(
        id,
        name,
        arguments
            .as_object()
            .expect("tool fixture arguments must be an object")
            .clone(),
    )
}

pub fn tool_use(content: Vec<AssistantContent>) -> AssistantMessage {
    assistant(content, StopReason::ToolUse)
}

pub fn user(text: &str) -> UserMessage {
    UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Text(text.to_owned().into()),
        input_source: None,
        provider_native: None,
        timestamp: 1000.0,
    }
}

pub fn user_block(text: &str) -> UserMessage {
    UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Text(TextContent::new(
            text,
        ))]),
        input_source: None,
        provider_native: None,
        timestamp: 1000.0,
    }
}

pub fn identity_convert() -> ConvertToLlm {
    Arc::new(|messages| {
        Box::pin(async move {
            Ok(messages
                .into_iter()
                .filter_map(|message| match message {
                    pi_agent_core::agent::AgentMessage::User(message) => Some(Message::User(message)),
                    pi_agent_core::agent::AgentMessage::Assistant(message) => Some(Message::Assistant(message)),
                    pi_agent_core::agent::AgentMessage::ToolResult(message) => Some(Message::ToolResult(message)),
                    pi_agent_core::agent::AgentMessage::Custom(_) => None,
                })
                .collect())
        })
    })
}

pub type PrepareArgumentsFn = Arc<dyn Fn(Value) -> Result<Value, pi_agent_core::agent::ToolError> + Send + Sync>;
pub type PrepareExecutionFn = Arc<
    dyn Fn(
            String,
            Option<AbortSignal>,
        ) -> BoxFuture<'static, Result<Option<ToolCallExecution>, pi_agent_core::agent::ToolError>>
        + Send
        + Sync,
>;
pub type ExecuteFn = Arc<
    dyn Fn(
            String,
            Value,
            Option<AbortSignal>,
            AgentToolUpdateCallback,
        ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>>
        + Send
        + Sync,
>;

pub type StreamBehavior =
    Arc<dyn Fn(Model, Context, SimpleStreamOptions) -> BoxFuture<'static, AssistantMessageEventStream> + Send + Sync>;

#[derive(Clone)]
pub enum Script {
    Message(Box<AssistantMessage>),
    Events(Vec<AssistantMessageEvent>),
    Behavior(StreamBehavior),
}

impl Script {
    pub fn message(message: AssistantMessage) -> Self {
        Self::Message(Box::new(message))
    }

    fn into_behavior(self) -> StreamBehavior {
        match self {
            Self::Message(message) => Arc::new(move |_model, _context, _options| {
                let message = message.as_ref().clone();
                Box::pin(async move {
                    let (sender, stream) = create_assistant_message_event_stream();
                    let reason = done_reason(message.stop_reason);
                    sender.push(AssistantMessageEvent::Done { reason, message });
                    stream
                })
            }),
            Self::Events(events) => Arc::new(move |_model, _context, _options| {
                let events = events.clone();
                Box::pin(async move {
                    let (sender, stream) = create_assistant_message_event_stream();
                    for event in events {
                        sender.push(event);
                    }
                    stream
                })
            }),
            Self::Behavior(behavior) => behavior,
        }
    }
}

fn done_reason(stop_reason: StopReason) -> DoneReason {
    match stop_reason {
        StopReason::Length => DoneReason::Length,
        StopReason::ToolUse => DoneReason::ToolUse,
        _ => DoneReason::Stop,
    }
}

/// Stream function that pops one scripted response per call.
pub fn scripted_stream_fn(scripts: Vec<Script>) -> StreamFn {
    let scripts = Arc::new(Mutex::new(VecDeque::from(
        scripts.into_iter().map(Script::into_behavior).collect::<Vec<_>>(),
    )));
    Arc::new(move |model, context, options| {
        let next = scripts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front();
        match next {
            Some(behavior) => Box::pin(async move { Ok(behavior(model, context, options).await) }),
            None => Box::pin(async move { Ok(pi_ai::request::error_stream(&model, "Unexpected stream call")) }),
        }
    })
}

pub struct TestTool {
    pub name: String,
    pub label: String,
    pub description: String,
    pub parameters: SchemaValue,
    pub execution_mode: Option<pi_agent_core::harness::ToolExecutionMode>,
    pub prepare_arguments: Option<PrepareArgumentsFn>,
    pub prepare_execution: Option<PrepareExecutionFn>,
    pub execute: ExecuteFn,
}

impl TestTool {
    pub fn new(name: &str, execute: impl Fn(String, Value) -> AgentToolResult + Send + Sync + 'static) -> Self {
        let execute = Arc::new(execute);
        Self {
            name: name.to_owned(),
            label: name.to_owned(),
            description: format!("{name} tool"),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"value": {"type": "string"}},
                "required": ["value"]
            }),
            execution_mode: None,
            prepare_arguments: None,
            prepare_execution: None,
            execute: Arc::new(move |id, params, _signal, _on_update| {
                let execute = Arc::clone(&execute);
                Box::pin(async move { Ok(execute(id, params)) })
            }),
        }
    }

    pub fn no_args(name: &str, execute: impl Fn() -> AgentToolResult + Send + Sync + 'static) -> Self {
        let execute = Arc::new(execute);
        Self {
            name: name.to_owned(),
            label: name.to_owned(),
            description: format!("{name} tool"),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
            execution_mode: None,
            prepare_arguments: None,
            prepare_execution: None,
            execute: Arc::new(move |_id, _params, _signal, _on_update| {
                let execute = Arc::clone(&execute);
                Box::pin(async move { Ok(execute()) })
            }),
        }
    }
}

impl AgentTool for TestTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        self.description.clone().into()
    }

    fn parameters(&self) -> &SchemaValue {
        &self.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        self.label.clone().into()
    }

    fn execution_mode(&self) -> Option<pi_agent_core::harness::ToolExecutionMode> {
        self.execution_mode
    }

    fn prepare_arguments(&self, args: Value) -> Result<Value, pi_agent_core::agent::ToolError> {
        match &self.prepare_arguments {
            Some(prepare) => prepare(args),
            None => Ok(args),
        }
    }

    fn prepare_execution(&self) -> Option<pi_agent_core::agent::types::AgentToolPrepareExecution> {
        self.prepare_execution.clone()
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<AbortSignal>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        (self.execute)(tool_call_id, params, signal, on_update)
    }
}

pub async fn collect_events(
    mut stream: EventStream<AgentEvent, Vec<pi_agent_core::agent::AgentMessage>>,
) -> (
    Vec<AgentEvent>,
    Result<Vec<pi_agent_core::agent::AgentMessage>, JsString>,
) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("agent event stream must not fail"));
    }
    let result = stream.result().await.map_err(|failure| failure.message().to_owned());
    (events, result)
}

pub fn event_types(events: &[AgentEvent]) -> Vec<&'static str> {
    events
        .iter()
        .map(|event| match event {
            AgentEvent::AgentStart => "agent_start",
            AgentEvent::AgentEnd { .. } => "agent_end",
            AgentEvent::TurnStart => "turn_start",
            AgentEvent::TurnEnd { .. } => "turn_end",
            AgentEvent::ModelRequestStart => "model_request_start",
            AgentEvent::ModelRequestEnd => "model_request_end",
            AgentEvent::MessageStart { .. } => "message_start",
            AgentEvent::MessageUpdate { .. } => "message_update",
            AgentEvent::MessageEnd { .. } => "message_end",
            AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
            AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
            AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
        })
        .collect()
}

pub fn text_result(text: impl Into<JsString>) -> AgentToolResult {
    let text = text.into();
    AgentToolResult {
        content: vec![pi_ai::types::TextOrImageContent::Text(TextContent::new(text.clone()))],
        details: Some(pi_js_compat::js_json!({"value": text})),
        is_error: None,
        terminate: None,
    }
}
