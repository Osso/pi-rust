//! Source index.ts stream diagnostics; these observers do not alter decoded output.
use super::{
    logging::Logs,
    stream::{StreamDecoder, string},
};
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{JsString, JsValue},
};

impl Logs {
    pub(super) fn before_frame(&self, frame: &JsValue, decoder: &StreamDecoder) -> Result<(), CallbackError> {
        match string(frame, "type")
            .to_utf8()
            .map_err(|e| CallbackError::new(e.to_string()))?
            .as_str()
        {
            "stream_event" => self.stream_event(frame.get("event").unwrap_or(&JsValue::Null), decoder),
            "assistant" => self.assistant_frame(frame.get("message").unwrap_or(&JsValue::Null), decoder),
            "system" | "user" | "result" | "rate_limit_event" => Ok(()),
            _ => self.debug(pi_js_compat::js_format!(
                "consumeQuery: unhandled SDK message type {}",
                string(frame, "type")
            )),
        }
    }
    fn stream_event(&self, event: &JsValue, decoder: &StreamDecoder) -> Result<(), CallbackError> {
        let block = event.get("content_block").unwrap_or(&JsValue::Null);
        let kind = string(event, "type");
        if kind == "message_start" && decoder.stream_open {
            self.discarded(
                decoder,
                pi_js_compat::js_format!(
                    "restreamed as {}",
                    string(event.get("message").unwrap_or(&JsValue::Null), "id")
                ),
            )?;
        }
        if kind == "content_block_start" {
            let tag = string(block, "type");
            if tag == "tool_use" {
                self.skipped_tool("processStreamEvent", block, decoder)?;
            } else if tag != "text" && tag != "thinking" {
                self.debug(pi_js_compat::js_format!(
                    "processStreamEvent: unhandled content_block_start type {}",
                    tag
                ))?;
            }
        } else if kind == "content_block_delta" {
            let tag = string(event.get("delta").unwrap_or(&JsValue::Null), "type");
            if tag != "text_delta" && tag != "thinking_delta" && tag != "input_json_delta" && tag != "signature_delta" {
                self.debug(pi_js_compat::js_format!(
                    "processStreamEvent: unhandled content_block_delta type {}",
                    tag
                ))?;
            }
        } else if !["message_start", "content_block_stop", "message_delta", "message_stop"]
            .iter()
            .any(|s| kind == *s)
        {
            self.debug(pi_js_compat::js_format!(
                "processStreamEvent: unhandled event type {}",
                kind
            ))?;
        }
        Ok(())
    }
    fn assistant_frame(&self, message: &JsValue, decoder: &StreamDecoder) -> Result<(), CallbackError> {
        let Some(blocks) = message.get("content").and_then(JsValue::as_array) else {
            return Ok(());
        };
        if string(message, "model") == "<synthetic>" {
            if decoder.streamed && decoder.stream_open {
                self.discarded(decoder, "synthetic failure report".into())?;
            }
            self.debug(format!(
                "processAssistantMessage: <synthetic> message, keeping {} block(s) off the stream",
                blocks.len()
            ))?;
            for block in blocks {
                if string(block, "type") != "text" || string(block, "text").is_empty() {
                    self.debug(pi_js_compat::js_format!(
                        "processAssistantMessage: unhandled <synthetic> block type {}",
                        string(block, "type")
                    ))?;
                }
            }
            return Ok(());
        }
        let id = string(message, "id");
        if decoder.streamed {
            if id.is_empty() || decoder.message_id.is_empty() || id == decoder.message_id {
                return Ok(());
            }
            if decoder.stream_open {
                self.discarded(decoder, pi_js_compat::js_format!("non-streaming fallback {}", id))?;
            }
        }
        self.debug(pi_js_compat::js_format!(
            "processAssistantMessage fallback: {} blocks, types={}",
            blocks.len(),
            JsString::join(blocks.iter().map(|b| string(b, "type")), ",")
        ))?;
        for block in blocks {
            let kind = string(block, "type");
            if kind == "tool_use" {
                self.skipped_tool("processAssistantMessage", block, decoder)?;
            } else if kind != "thinking" && (kind != "text" || string(block, "text").is_empty()) {
                self.debug(pi_js_compat::js_format!(
                    "processAssistantMessage: unhandled block type {}",
                    kind
                ))?;
            }
        }
        Ok(())
    }
    fn skipped_tool(&self, label: &str, block: &JsValue, decoder: &StreamDecoder) -> Result<(), CallbackError> {
        let name = string(block, "name")
            .to_utf8()
            .map_err(|e| CallbackError::new(e.to_string()))?;
        if !decoder.names.contains_key(&name) && !decoder.names.contains_key(&name.to_lowercase()) {
            self.debug(pi_js_compat::js_format!(
                "{}: skipping tool_use outside pi's MCP tools {} [{}] — handled internally by CC",
                label,
                name,
                string(block, "id")
            ))?;
        }
        Ok(())
    }
    fn discarded(&self, decoder: &StreamDecoder, why: JsString) -> Result<(), CallbackError> {
        let blocks = &decoder.output.content[decoder.block_start..];
        let types = blocks.iter().map(|block| match block {
            pi_ai::types::AssistantContent::Text(_) => "text",
            pi_ai::types::AssistantContent::Thinking(_) => "thinking",
            pi_ai::types::AssistantContent::ToolCall(_) => "toolCall",
        });
        self.debug(pi_js_compat::js_format!(
            "dropAbandonedStreamBlocks: {}; dropped {} blocks from {} types={}",
            why,
            blocks.len(),
            decoder.message_id,
            types.collect::<Vec<_>>().join(",")
        ))
    }
    pub(super) fn after_frame(&self, frame: &JsValue, decoder: &StreamDecoder) -> Result<(), CallbackError> {
        let kind = string(frame, "type");
        let usage = if kind == "stream_event" {
            let event = frame.get("event").unwrap_or(&JsValue::Null);
            if string(event, "type") == "message_start" {
                event.get("message").and_then(|m| m.get("usage"))
            } else if string(event, "type") == "message_delta" {
                event.get("usage")
            } else {
                None
            }
        } else if kind == "assistant" {
            frame
                .get("message")
                .filter(|message| {
                    string(message, "model") == "<synthetic>"
                        || !decoder.streamed
                        || (!string(message, "id").is_empty()
                            && !decoder.message_id.is_empty()
                            && string(message, "id") != decoder.message_id)
                })
                .and_then(|m| m.get("usage"))
        } else {
            None
        };
        let Some(usage) = usage else {
            return Ok(());
        };
        let u = &decoder.output.usage;
        let prompt = u.input + u.cache_read + u.cache_write;
        let pct = if prompt > 0.0 {
            (u.cache_read / prompt * 100.0).round()
        } else {
            0.0
        };
        let reasoning = usage
            .get("output_tokens_details")
            .and_then(|d| d.get("thinking_tokens"))
            .and_then(JsValue::as_f64);
        self.debug(format!(
            "usage: in={} out={} cacheRead={} cacheWrite={} total={}{} cachePct={}% model={}",
            u.input,
            u.output,
            u.cache_read,
            u.cache_write,
            u.total_tokens,
            reasoning.map(|r| format!(" reasoning={r}")).unwrap_or_default(),
            pct,
            decoder.output.model
        ))
    }
}
