//! Claude stream-json -> Pi assistant events (source index.ts streaming paths).
use indexmap::IndexMap;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, DoneReason, ErrorReason, Model, StopReason, TextContent,
    ThinkingContent, ToolCall,
};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};

pub const MCP_PREFIX: &str = "mcp__custom-tools__";

pub fn string(value: &JsValue, key: &str) -> JsString {
    value.get(key).and_then(JsValue::as_string).cloned().unwrap_or_default()
}

fn scalar(value: &JsValue, key: &str) -> Result<String, CallbackError> {
    string(value, key)
        .to_utf8()
        .map_err(|_| CallbackError::new(format!("Claude {key} contains unpaired UTF-16")))
}

pub fn result_error(frame: &JsValue) -> Option<JsString> {
    if string(frame, "subtype") == "success" {
        return (frame.get("is_error").and_then(JsValue::as_bool) == Some(true)).then(|| {
            let text = string(frame, "result");
            if text.is_empty() {
                "Claude Code reported an error".into()
            } else {
                text
            }
        });
    }
    if let Some(errors) = frame
        .get("errors")
        .and_then(JsValue::as_array)
        .filter(|v| !v.is_empty())
    {
        return Some(JsString::join(
            errors
                .iter()
                .map(|v| js_json::to_js_string(v).unwrap_or_else(|_| "[object Object]".into())),
            "\n",
        ));
    }
    if let Some(error) = frame.get("error").and_then(JsValue::as_string) {
        return Some(error.clone());
    }
    let subtype = frame
        .get("subtype")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| "unknown result".into());
    Some(pi_js_compat::js_format!("Claude Code failed: {}", subtype))
}

pub fn map_tool_args(name: &str, args: &JsObject) -> JsObject {
    let lower = name.to_lowercase();
    let mut result = JsObject::new();
    for (key, value) in args {
        let key = match (lower.as_str(), key.to_utf8().ok().as_deref()) {
            ("read" | "write" | "edit", Some("file_path")) => "path".into(),
            ("edit", Some("old_string" | "old_text")) => "oldText".into(),
            ("edit", Some("new_string" | "new_text")) => "newText".into(),
            _ => key.clone(),
        };
        result.entry(key).or_insert_with(|| value.clone());
    }
    if lower == "bash"
        && result
            .get(&JsString::from("timeout"))
            .is_none_or(|v| matches!(v, JsValue::Null))
    {
        result.insert("timeout".into(), JsValue::Number(120.0));
    }
    result
}

pub struct StreamDecoder {
    pub output: AssistantMessage,
    model: Model,
    pub(super) names: IndexMap<String, String>,
    started: bool,
    pub(super) streamed: bool,
    pub(super) message_id: JsString,
    pub(super) stream_open: bool,
    pub(super) block_start: usize,
    pub parked: bool,
}

impl StreamDecoder {
    pub fn new(model: &Model) -> Self {
        Self::with_tools(model, std::iter::empty::<String>())
    }

    pub fn with_tools(model: &Model, names: impl IntoIterator<Item = String>) -> Self {
        let names = names
            .into_iter()
            .flat_map(|name| {
                let sdk = format!("{MCP_PREFIX}{name}");
                [(sdk.clone(), name.clone()), (sdk.to_lowercase(), name)]
            })
            .collect();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        Self {
            output: AssistantMessage::empty(&model.api, &model.provider, &model.id, timestamp),
            model: model.clone(),
            names,
            started: false,
            streamed: false,
            message_id: JsString::default(),
            stream_open: false,
            block_start: 0,
            parked: false,
        }
    }

    fn start(&mut self, events: &mut Vec<AssistantMessageEvent>) {
        if !self.started {
            self.started = true;
            events.push(AssistantMessageEvent::Start {
                partial: self.output.clone(),
            });
        }
    }

    pub fn finish_events(&mut self) -> Vec<AssistantMessageEvent> {
        let mut events = Vec::new();
        self.start(&mut events);
        events.push(self.finish());
        events
    }

    pub fn finish(&self) -> AssistantMessageEvent {
        match self.output.stop_reason {
            StopReason::Error => AssistantMessageEvent::Error {
                reason: ErrorReason::Error,
                error: self.output.clone(),
            },
            StopReason::Aborted => AssistantMessageEvent::Error {
                reason: ErrorReason::Aborted,
                error: self.output.clone(),
            },
            reason => AssistantMessageEvent::Done {
                reason: match reason {
                    StopReason::ToolUse => DoneReason::ToolUse,
                    StopReason::Length => DoneReason::Length,
                    _ => DoneReason::Stop,
                },
                message: self.output.clone(),
            },
        }
    }

    pub fn decode(&mut self, frame: &JsValue) -> Result<Vec<AssistantMessageEvent>, CallbackError> {
        let mut events = Vec::new();
        match scalar(frame, "type")?.as_str() {
            "stream_event" => {
                if let Some(event) = frame.get("event") {
                    self.decode_event(event, &mut events)?;
                }
            }
            "assistant" => {
                if let Some(message) = frame.get("message") {
                    self.decode_assistant(message, &mut events)?;
                }
            }
            "result" => {
                if let Some(error) = result_error(frame) {
                    self.output.stop_reason = StopReason::Error;
                    self.output.error_message = Some(error);
                    events.push(self.finish());
                } else if !self.streamed {
                    self.start(&mut events);
                    self.append_complete_block(
                        &JsValue::object([
                            ("type", JsValue::String("text".into())),
                            ("text", JsValue::String(string(frame, "result"))),
                        ]),
                        &mut events,
                    )?;
                }
            }
            _ => {}
        }
        Ok(events)
    }

    fn decode_event(&mut self, event: &JsValue, events: &mut Vec<AssistantMessageEvent>) -> Result<(), CallbackError> {
        self.streamed = true;
        match scalar(event, "type")?.as_str() {
            "message_start" => {
                if self.stream_open {
                    self.output.content.truncate(self.block_start);
                }
                let message = event.get("message").unwrap_or(&JsValue::Null);
                self.message_id = string(message, "id");
                self.stream_open = true;
                self.block_start = self.output.content.len();
                self.update_usage(message.get("usage"));
            }
            "content_block_start" => {
                if let Some(block) = event.get("content_block") {
                    self.start_block(event, block, events)?;
                }
            }
            "content_block_delta" => self.delta(event, events)?,
            "content_block_stop" => self.end_block(event, events),
            "message_delta" => {
                self.output.stop_reason =
                    stop_reason(&string(event.get("delta").unwrap_or(&JsValue::Null), "stop_reason"));
                self.update_usage(event.get("usage"));
            }
            "message_stop" => {
                self.stream_open = false;
                self.park_tools(events);
            }
            _ => {}
        }
        Ok(())
    }

    fn start_block(
        &mut self,
        event: &JsValue,
        block: &JsValue,
        events: &mut Vec<AssistantMessageEvent>,
    ) -> Result<(), CallbackError> {
        let index = event.get("index").and_then(JsValue::as_f64);
        let content = match scalar(block, "type")?.as_str() {
            "text" => {
                let mut text = TextContent::new("");
                text.index = index;
                AssistantContent::Text(text)
            }
            "thinking" => {
                let mut thinking = ThinkingContent::new("");
                thinking.index = index;
                thinking.thinking_signature = Some(String::new());
                AssistantContent::Thinking(thinking)
            }
            "tool_use" => {
                let name = scalar(block, "name")?;
                let Some(name) = self.names.get(&name).or_else(|| self.names.get(&name.to_lowercase())) else {
                    return Ok(());
                };
                let mut call = ToolCall::new(
                    scalar(block, "id")?,
                    name,
                    block
                        .get("input")
                        .and_then(JsValue::as_object)
                        .cloned()
                        .unwrap_or_default(),
                );
                call.index = index;
                call.partial_json = Some(JsString::default());
                AssistantContent::ToolCall(call)
            }
            _ => return Ok(()),
        };
        self.start(events);
        let content_index = self.output.content.len();
        self.output.content.push(content);
        let partial = self.output.clone();
        events.push(match &self.output.content[content_index] {
            AssistantContent::Text(_) => AssistantMessageEvent::TextStart { content_index, partial },
            AssistantContent::Thinking(_) => AssistantMessageEvent::ThinkingStart { content_index, partial },
            _ => AssistantMessageEvent::ToolCallStart { content_index, partial },
        });
        Ok(())
    }

    fn block_index(&self, event: &JsValue) -> Option<usize> {
        let index = event.get("index").and_then(JsValue::as_f64)?;
        self.output.content.iter().position(|block| match block {
            AssistantContent::Text(v) => v.index == Some(index),
            AssistantContent::Thinking(v) => v.index == Some(index),
            AssistantContent::ToolCall(v) => v.index == Some(index),
        })
    }

    fn delta(&mut self, event: &JsValue, events: &mut Vec<AssistantMessageEvent>) -> Result<(), CallbackError> {
        let Some(content_index) = self.block_index(event) else {
            return Ok(());
        };
        let delta = event.get("delta").unwrap_or(&JsValue::Null);
        let tag = scalar(delta, "type")?;
        let text = match tag.as_str() {
            "thinking_delta" => string(delta, "thinking"),
            "input_json_delta" => string(delta, "partial_json"),
            "signature_delta" => string(delta, "signature"),
            _ => string(delta, "text"),
        };
        match (&mut self.output.content[content_index], tag.as_str()) {
            (AssistantContent::Text(block), "text_delta") => block.text.push_str(text.clone()),
            (AssistantContent::Thinking(block), "thinking_delta") => block.thinking.push_str(text.clone()),
            (AssistantContent::Thinking(block), "signature_delta") => {
                block.thinking_signature.get_or_insert_default().push_str(
                    &text
                        .to_utf8()
                        .map_err(|_| CallbackError::new("Claude signature contains unpaired UTF-16"))?,
                );
                return Ok(());
            }
            (AssistantContent::ToolCall(block), "input_json_delta") => {
                block.partial_json.get_or_insert_default().push_str(text.clone());
                // Source parsePartialJson is strict JSON.parse: incomplete or invalid
                // deltas retain the last successful arguments, including at block stop.
                if let Ok(JsValue::Object(args)) =
                    js_json::parse_lossless(block.partial_json.clone().unwrap_or_default())
                {
                    block.arguments = args;
                }
            }
            _ => return Ok(()),
        }
        let partial = self.output.clone();
        events.push(match tag.as_str() {
            "thinking_delta" => AssistantMessageEvent::ThinkingDelta {
                content_index,
                delta: text,
                partial,
            },
            "input_json_delta" => AssistantMessageEvent::ToolCallDelta {
                content_index,
                delta: text,
                partial,
            },
            _ => AssistantMessageEvent::TextDelta {
                content_index,
                delta: text,
                partial,
            },
        });
        Ok(())
    }

    fn end_block(&mut self, event: &JsValue, events: &mut Vec<AssistantMessageEvent>) {
        let Some(content_index) = self.block_index(event) else {
            return;
        };
        match &mut self.output.content[content_index] {
            AssistantContent::Text(v) => v.index = None,
            AssistantContent::Thinking(v) => v.index = None,
            AssistantContent::ToolCall(v) => {
                v.index = None;
                v.arguments = map_tool_args(&v.name, &v.arguments);
                v.partial_json = None;
            }
        }
        let partial = self.output.clone();
        events.push(match &self.output.content[content_index] {
            AssistantContent::Text(v) => AssistantMessageEvent::TextEnd {
                content_index,
                content: v.text.clone(),
                partial,
            },
            AssistantContent::Thinking(v) => AssistantMessageEvent::ThinkingEnd {
                content_index,
                content: v.thinking.clone(),
                partial,
            },
            AssistantContent::ToolCall(v) => AssistantMessageEvent::ToolCallEnd {
                content_index,
                tool_call: Box::new(v.clone()),
                partial,
            },
        });
    }

    fn park_tools(&mut self, events: &mut Vec<AssistantMessageEvent>) {
        if self
            .output
            .content
            .iter()
            .any(|c| matches!(c, AssistantContent::ToolCall(_)))
        {
            self.output.stop_reason = StopReason::ToolUse;
            self.parked = true;
            events.push(self.finish());
        }
    }

    fn decode_assistant(
        &mut self,
        message: &JsValue,
        events: &mut Vec<AssistantMessageEvent>,
    ) -> Result<(), CallbackError> {
        if self.parked || message.get("content").is_none() {
            return Ok(());
        }
        let synthetic = string(message, "model") == "<synthetic>";
        if self.streamed
            && !synthetic
            && (string(message, "id").is_empty()
                || self.message_id.is_empty()
                || string(message, "id") == self.message_id)
        {
            return Ok(());
        }
        if self.stream_open {
            self.output.content.truncate(self.block_start);
            self.stream_open = false;
        }
        self.output.stop_reason = stop_reason(&string(message, "stop_reason"));
        if let Some(blocks) = message.get("content").and_then(JsValue::as_array) {
            for block in blocks {
                if synthetic {
                    if string(block, "type") == "text" && !string(block, "text").is_empty() {
                        self.output
                            .content
                            .push(AssistantContent::Text(TextContent::new(string(block, "text"))));
                    }
                } else if string(block, "type") != "text" || !string(block, "text").is_empty() {
                    self.append_complete_block(block, events)?;
                }
            }
        }
        self.update_usage(message.get("usage"));
        if !synthetic {
            self.park_tools(events);
        }
        Ok(())
    }

    fn append_complete_block(
        &mut self,
        block: &JsValue,
        events: &mut Vec<AssistantMessageEvent>,
    ) -> Result<(), CallbackError> {
        let fake_event = JsValue::object([("index", JsValue::Number(-1.0))]);
        let count = self.output.content.len();
        self.start_block(&fake_event, block, events)?;
        if count == self.output.content.len() {
            return Ok(());
        }
        let kind = scalar(block, "type")?;
        let (delta_type, key) = match kind.as_str() {
            "thinking" => ("thinking_delta", "thinking"),
            "text" => ("text_delta", "text"),
            _ => ("input_json_delta", "partial_json"),
        };
        let text = if kind == "tool_use" {
            js_json::stringify_lossless(block.get("input").unwrap_or(&JsValue::Object(JsObject::new()))).into()
        } else {
            string(block, key)
        };
        let delta = JsValue::object([
            ("index", JsValue::Number(-1.0)),
            (
                "delta",
                JsValue::object([
                    ("type", JsValue::String(delta_type.into())),
                    (key, JsValue::String(text)),
                ]),
            ),
        ]);
        self.delta(&delta, events)?;
        if kind == "thinking"
            && let Some(AssistantContent::Thinking(v)) = self.output.content.last_mut()
        {
            v.thinking_signature = Some(scalar(block, "signature")?);
        }
        self.end_block(&fake_event, events);
        Ok(())
    }

    fn update_usage(&mut self, value: Option<&JsValue>) {
        let Some(usage) = value else {
            return;
        };
        for (key, target) in [
            ("input_tokens", &mut self.output.usage.input),
            ("output_tokens", &mut self.output.usage.output),
            ("cache_read_input_tokens", &mut self.output.usage.cache_read),
            ("cache_creation_input_tokens", &mut self.output.usage.cache_write),
        ] {
            if let Some(number) = usage.get(key).and_then(JsValue::as_f64) {
                *target = number;
            }
        }
        if let Some(tokens) = usage
            .get("output_tokens_details")
            .and_then(|v| v.get("thinking_tokens"))
            .and_then(JsValue::as_f64)
        {
            self.output.usage.reasoning = Some(tokens);
        }
        let usage = &mut self.output.usage;
        usage.total_tokens = usage.input + usage.output + usage.cache_read + usage.cache_write;
        usage.cost = pi_ai::models::calculate_cost(&self.model, usage);
    }
}

fn stop_reason(reason: &JsString) -> StopReason {
    if reason == "max_tokens" {
        StopReason::Length
    } else if reason == "tool_use" {
        StopReason::ToolUse
    } else {
        StopReason::Stop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_ai::types::{AssistantContent, StopReason};

    fn decoder() -> StreamDecoder {
        let mut model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
        model.api = "claude-bridge".into();
        model.provider = "claude-bridge".into();
        StreamDecoder::with_tools(&model, ["bash".into()])
    }

    fn frames(decoder: &mut StreamDecoder, lines: &[&str]) -> Vec<AssistantMessageEvent> {
        lines
            .iter()
            .flat_map(|line| decoder.decode(&js_json::parse_lossless(*line).unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn claude_bridge_usage_recalculates_cost_with_model_rates() {
        let mut model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
        model.cost.rates = pi_ai::types::ModelCostRates {
            input: 10.0,
            output: 20.0,
            cache_read: 1.0,
            cache_write: 12.5,
        };
        let mut decoder = StreamDecoder::new(&model);
        // Exact costs from TS usage.ts updateUsage with these rates and successive frames.
        let cases = [
            (
                r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg-cost","usage":{"input_tokens":100}}}}"#,
                pi_ai::types::UsageCost {
                    input: 0.001,
                    total: 0.001,
                    ..Default::default()
                },
                100.0,
            ),
            (
                r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":50,"cache_read_input_tokens":200,"cache_creation_input_tokens":80,"output_tokens_details":{"thinking_tokens":30}}}}"#,
                pi_ai::types::UsageCost {
                    input: 0.001,
                    output: 0.001,
                    cache_read: 0.00019999999999999998,
                    cache_write: 0.001,
                    total: 0.0032,
                },
                430.0,
            ),
            (
                r#"{"type":"assistant","message":{"id":"msg-next","content":[],"usage":{"input_tokens":120,"output_tokens":null}}}"#,
                pi_ai::types::UsageCost {
                    input: 0.0012000000000000001,
                    output: 0.001,
                    cache_read: 0.00019999999999999998,
                    cache_write: 0.001,
                    total: 0.0034000000000000002,
                },
                450.0,
            ),
        ];
        for (frame, cost, total_tokens) in cases {
            frames(&mut decoder, &[frame]);
            assert_eq!(decoder.output.usage.cost, cost);
            assert_eq!(decoder.output.usage.total_tokens, total_tokens);
        }
        assert_eq!(decoder.output.usage.reasoning, Some(30.0));
    }

    #[test]
    fn claude_bridge_partial_json_retains_initial_arguments_until_complete() {
        let mut decoder = decoder();
        frames(
            &mut decoder,
            &[
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"mcp__custom-tools__bash","input":{"command":"seed"}}}}"#,
            ],
        );
        let fixture = js_json::parse_lossless(include_str!("fixtures/stream-semantics-oracle.json")).unwrap();
        for step in fixture.get("partial").unwrap().as_array().unwrap() {
            let delta = string(step, "delta");
            let frame = js_json::parse_lossless(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta"}}}"#).unwrap();
            let mut frame = frame;
            frame
                .get_mut("event")
                .unwrap()
                .get_mut("delta")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("partial_json".into(), JsValue::String(delta.clone()));
            let events = decoder.decode(&frame).unwrap();
            let [
                AssistantMessageEvent::ToolCallDelta {
                    content_index,
                    delta: actual,
                    partial,
                },
            ] = events.as_slice()
            else {
                panic!("toolcall_delta");
            };
            assert_eq!(*content_index, 0);
            assert_eq!(actual, &delta);
            let AssistantContent::ToolCall(call) = &partial.content[0] else {
                panic!("tool call");
            };
            assert_eq!(JsValue::Object(call.arguments.clone()), *step.get("arguments").unwrap());
            assert_eq!(call.partial_json.as_ref(), Some(&string(step, "accumulated")));
        }
        let events = frames(
            &mut decoder,
            &[
                r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
                r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
            ],
        );
        let AssistantMessageEvent::ToolCallEnd { tool_call, .. } = &events[0] else {
            panic!("toolcall_end");
        };
        assert_eq!(
            JsValue::Object(tool_call.arguments.clone()),
            js_json::parse_lossless(r#"{"command":"new \ud800","nested":{"key":1},"timeout":120}"#).unwrap()
        );
        assert_eq!(tool_call.partial_json, None);
        assert_eq!(tool_call.index, None);
        assert!(matches!(
            events[1],
            AssistantMessageEvent::Done {
                reason: DoneReason::ToolUse,
                ..
            }
        ));
        assert_eq!(decoder.output.usage.total_tokens, 0.0);
    }

    #[test]
    fn claude_bridge_incomplete_json_at_stop_preserves_initial_arguments() {
        let mut decoder = decoder();
        frames(
            &mut decoder,
            &[
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"mcp__custom-tools__bash","input":{"command":"seed"}}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"unfinished"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            ],
        );
        let AssistantContent::ToolCall(call) = &decoder.output.content[0] else {
            panic!("tool call");
        };
        assert_eq!(
            JsValue::Object(call.arguments.clone()),
            js_json::parse_lossless(r#"{"command":"seed","timeout":120}"#).unwrap()
        );
    }

    #[test]
    fn claude_bridge_stream_preserves_utf16_and_partial_usage_frames() {
        let mut decoder = decoder();
        let events = frames(
            &mut decoder,
            &[
                r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","usage":{"input_tokens":12,"cache_read_input_tokens":90}}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello \ud800"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
                r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}}"#,
                r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
                r#"{"type":"result","subtype":"success","result":"Hello duplicate"}"#,
            ],
        );
        assert!(matches!(events.first(), Some(AssistantMessageEvent::Start { .. })));
        let AssistantContent::Text(text) = &decoder.output.content[0] else {
            panic!("text");
        };
        assert_eq!(text.text.units().last(), Some(&0xd800));
        assert_eq!(decoder.output.usage.input, 12.0);
        assert_eq!(decoder.output.usage.output, 7.0);
        assert_eq!(decoder.output.usage.cache_read, 90.0);
        assert_eq!(decoder.output.usage.total_tokens, 109.0);
    }

    #[test]
    fn claude_bridge_tool_boundary_maps_bash_timeout_and_ignores_native_search() {
        let mut decoder = decoder();
        let events = frames(
            &mut decoder,
            &[
                r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg2"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"search","name":"WebSearch","input":{}}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call1","name":"mcp__custom-tools__bash","input":{}}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"pwd\"}"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
                r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
            ],
        );
        assert_eq!(decoder.output.stop_reason, StopReason::ToolUse);
        assert!(matches!(events.last(), Some(AssistantMessageEvent::Done { .. })));
        assert_eq!(decoder.output.content.len(), 1);
        let AssistantContent::ToolCall(call) = &decoder.output.content[0] else {
            panic!("tool call");
        };
        assert_eq!(call.name, "bash");
        assert_eq!(call.arguments.get("timeout").and_then(JsValue::as_f64), Some(120.0));
    }

    #[test]
    fn claude_bridge_result_failure_is_error_not_success_text() {
        let mut decoder = decoder();
        let events = frames(
            &mut decoder,
            &[r#"{"type":"result","subtype":"success","is_error":true,"result":"Quota \udfff"}"#],
        );
        assert!(matches!(events.last(), Some(AssistantMessageEvent::Error { .. })));
        assert_eq!(decoder.output.stop_reason, StopReason::Error);
        assert_eq!(decoder.output.error_message.unwrap().units().last(), Some(&0xdfff));
    }
}
