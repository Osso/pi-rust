//! Source convert.ts and the used cc-session-io tool-pair repair behavior.
use super::stream::{MCP_PREFIX, string};
use pi_ai::types::Message;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};

fn text(value: impl Into<JsString>) -> JsValue {
    JsValue::String(value.into())
}
fn message(role: &str, content: JsValue) -> JsValue {
    JsValue::object([("role", text(role)), ("content", content)])
}

pub fn sanitize_tool_id(id: &JsString) -> JsString {
    JsString::from_utf16(
        id.units()
            .iter()
            .map(|unit| {
                if (u16::from(b'a')..=u16::from(b'z')).contains(unit)
                    || (u16::from(b'A')..=u16::from(b'Z')).contains(unit)
                    || (u16::from(b'0')..=u16::from(b'9')).contains(unit)
                    || *unit == u16::from(b'_')
                    || *unit == u16::from(b'-')
                {
                    *unit
                } else {
                    u16::from(b'_')
                }
            })
            .collect(),
    )
}

pub fn content_text(content: &JsValue) -> JsString {
    if let Some(value) = content.as_string() {
        return value.clone();
    }
    let Some(blocks) = content.as_array() else {
        return JsString::default();
    };
    let has_text = blocks
        .iter()
        .any(|block| string(block, "type") == "text" && !string(block, "text").is_empty());
    if !has_text {
        return JsString::default();
    }
    JsString::join(
        blocks.iter().filter_map(|block| {
            let kind = string(block, "type");
            if kind == "text" {
                let text = string(block, "text");
                return (!text.is_empty()).then_some(text);
            }
            if kind == "image" {
                return None;
            }
            Some(pi_js_compat::js_format!("[{}]", kind))
        }),
        "\n",
    )
}

fn image_block(block: &JsValue) -> JsValue {
    JsValue::object([
        ("type", text("image")),
        (
            "source",
            JsValue::object([
                ("type", text("base64")),
                ("media_type", text(string(block, "mimeType"))),
                ("data", text(string(block, "data"))),
            ]),
        ),
    ])
}

fn user_content(content: &JsValue) -> JsValue {
    if let Some(value) = content.as_string() {
        return text(if value.is_empty() {
            "[empty]".into()
        } else {
            value.clone()
        });
    }
    let Some(blocks) = content.as_array() else {
        return text("[empty]");
    };
    let parts: Vec<_> = blocks
        .iter()
        .filter_map(|block| {
            if string(block, "type") == "text" && !string(block, "text").is_empty() {
                return Some(JsValue::object([
                    ("type", text("text")),
                    ("text", text(string(block, "text"))),
                ]));
            }
            if string(block, "type") == "image"
                && !string(block, "data").is_empty()
                && !string(block, "mimeType").is_empty()
            {
                return Some(image_block(block));
            }
            None
        })
        .collect();
    if parts.is_empty() {
        text("[image]")
    } else {
        JsValue::Array(parts)
    }
}

/// Live tool results use extract-tool-results.ts, not transcript flattening.
/// An empty or unsupported result must still contain one blank MCP text block.
pub fn tool_result_to_mcp_content(content: &JsValue) -> JsValue {
    let blank = || JsValue::object([("type", text("text")), ("text", text(""))]);
    if let Some(value) = content.as_string() {
        return JsValue::Array(vec![JsValue::object([
            ("type", text("text")),
            ("text", text(value.clone())),
        ])]);
    }
    let blocks: Vec<_> = content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| {
            let kind = string(block, "type");
            if kind == "text" && !string(block, "text").is_empty() {
                return Some(JsValue::object([
                    ("type", text("text")),
                    ("text", text(string(block, "text"))),
                ]));
            }
            if kind == "image" && !string(block, "data").is_empty() && !string(block, "mimeType").is_empty() {
                return Some(JsValue::object([
                    ("type", text("image")),
                    ("data", text(string(block, "data"))),
                    ("mimeType", text(string(block, "mimeType"))),
                ]));
            }
            None
        })
        .collect();
    JsValue::Array(if blocks.is_empty() { vec![blank()] } else { blocks })
}

fn result_content(content: &JsValue) -> JsValue {
    let blocks = content.as_array();
    let images = blocks.is_some_and(|blocks| {
        blocks
            .iter()
            .any(|b| string(b, "type") == "image" && !string(b, "data").is_empty() && !string(b, "mimeType").is_empty())
    });
    if !images {
        return text(content_text(content));
    }
    JsValue::Array(
        blocks
            .into_iter()
            .flatten()
            .filter_map(|block| {
                let kind = string(block, "type");
                if kind == "text" && !string(block, "text").is_empty() {
                    return Some(JsValue::object([
                        ("type", text("text")),
                        ("text", text(string(block, "text"))),
                    ]));
                }
                if kind == "image" && !string(block, "data").is_empty() && !string(block, "mimeType").is_empty() {
                    return Some(image_block(block));
                }
                if kind != "text" && kind != "image" {
                    return Some(JsValue::object([
                        ("type", text("text")),
                        ("text", text(pi_js_compat::js_format!("[{}]", kind))),
                    ]));
                }
                None
            })
            .collect(),
    )
}

fn sdk_name(name: &JsString, provider: bool) -> Result<JsString, CallbackError> {
    if name.is_empty() {
        return Ok(name.clone());
    }
    let normalized = name.to_lowercase();
    if normalized.starts_with(MCP_PREFIX) {
        return Err(CallbackError::new(pi_js_compat::js_format!(
            "mapPiToolNameToSdk: \"{}\" is already an SDK tool name — pi history holds pi tool names",
            name
        )));
    }
    if provider {
        return Ok(pi_js_compat::js_format!("{}{}", MCP_PREFIX, name));
    }
    for (pi, sdk) in [("read", "Read"), ("write", "Write"), ("edit", "Edit"), ("bash", "Bash")] {
        if normalized == pi {
            return Ok(sdk.into());
        }
    }
    pascal_case(name)
}

fn pascal_case(name: &JsString) -> Result<JsString, CallbackError> {
    let name = name.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?;
    let lower_upper = regex::Regex::new(r"([\p{Ll}0-9])(\p{Lu})").map_err(|e| CallbackError::new(e.to_string()))?;
    let upper_upper =
        regex::Regex::new(r"(\p{Lu})([\p{Lu}][\p{Ll}])").map_err(|e| CallbackError::new(e.to_string()))?;
    let strip = regex::Regex::new(r"[^\p{L}0-9]+").map_err(|e| CallbackError::new(e.to_string()))?;
    let words = lower_upper.replace_all(name.trim(), "$1\0$2");
    let words = upper_upper.replace_all(&words, "$1\0$2");
    let words = strip.replace_all(&words, "\0");
    Ok(JsString::join(
        words
            .trim_matches('\0')
            .split('\0')
            .filter(|v| !v.is_empty())
            .enumerate()
            .map(|(index, word)| {
                let word = JsString::from(word);
                let first = word.slice(0, 1);
                let prefix = if index > 0
                    && first
                        .units()
                        .first()
                        .is_some_and(|v| (u16::from(b'0')..=u16::from(b'9')).contains(v))
                {
                    JsString::from("_") + first
                } else {
                    first.to_uppercase()
                };
                prefix + word.slice(1, word.len()).to_lowercase()
            }),
        "",
    ))
}

#[derive(Default)]
pub(super) struct ConversionStats {
    pub thinking: usize,
    pub aborted_turns: usize,
    pub providers: indexmap::IndexSet<JsString>,
    pub other: indexmap::IndexMap<JsString, usize>,
    pub sanitized_ids: indexmap::IndexMap<JsString, JsString>,
}
impl ConversionStats {
    fn sanitize(&mut self, id: JsString) -> JsString {
        self.sanitized_ids
            .entry(id.clone())
            .or_insert_with(|| sanitize_tool_id(&id))
            .clone()
    }
}
pub(super) struct ConversionReport {
    pub messages: Vec<JsValue>,
    pub stats: ConversionStats,
}
fn assistant_content(
    raw: &JsValue,
    provider: bool,
    stats: &mut ConversionStats,
) -> Result<Vec<JsValue>, CallbackError> {
    let mut output = Vec::new();
    for block in raw.get("content").and_then(JsValue::as_array).into_iter().flatten() {
        let kind = string(block, "type");
        if kind == "text" && !string(block, "text").is_empty() {
            output.push(JsValue::object([
                ("type", text("text")),
                ("text", text(string(block, "text"))),
            ]));
        } else if kind == "thinking"
            && string(raw, "provider") == "claude-bridge"
            && !string(block, "thinkingSignature").is_empty()
        {
            output.push(JsValue::object([
                ("type", text("thinking")),
                ("thinking", text(string(block, "thinking"))),
                ("signature", text(string(block, "thinkingSignature"))),
            ]));
        } else if kind == "toolCall" {
            output.push(JsValue::object([
                ("type", text("tool_use")),
                ("id", text(stats.sanitize(string(block, "id")))),
                ("name", text(sdk_name(&string(block, "name"), provider)?)),
                (
                    "input",
                    block
                        .get("arguments")
                        .cloned()
                        .unwrap_or(JsValue::Object(Default::default())),
                ),
            ]));
        } else if kind == "thinking" {
            stats.thinking += 1;
            stats.providers.insert(
                raw.get("provider")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
            );
        } else {
            *stats.other.entry(kind).or_default() += 1;
        }
    }
    if output.is_empty() {
        output.push(JsValue::object([
            ("type", text("text")),
            ("text", text("[incompatible content omitted]")),
        ]));
    }
    Ok(output)
}

pub fn convert_messages(messages: &[Message], provider: bool) -> Result<Vec<JsValue>, CallbackError> {
    Ok(convert_messages_report(messages, provider)?.messages)
}
pub(super) fn convert_messages_report(messages: &[Message], provider: bool) -> Result<ConversionReport, CallbackError> {
    let mut stats = ConversionStats::default();
    let mut output: Vec<JsValue> = Vec::new();
    let mut assistant_index: Option<usize> = None;
    let mut results_index: Option<usize> = None;
    for item in messages {
        let raw = js_json::to_value(item).map_err(|e| CallbackError::new(e.to_string()))?;
        let content = raw.get("content").unwrap_or(&JsValue::Null);
        match item {
            Message::User(_) => output.push(message("user", user_content(content))),
            Message::Assistant(_) => {
                if content.as_array().is_none_or(|blocks| blocks.is_empty()) {
                    stats.aborted_turns += 1;
                    continue;
                }
                let blocks = assistant_content(&raw, provider, &mut stats)?;
                results_index = None;
                assistant_index = Some(output.len());
                output.push(message("assistant", JsValue::Array(blocks)));
            }
            Message::ToolResult(_) => {
                let block = JsValue::object([
                    ("type", text("tool_result")),
                    ("tool_use_id", text(stats.sanitize(string(&raw, "toolCallId")))),
                    ("content", result_content(content)),
                    ("is_error", raw.get("isError").cloned().unwrap_or(JsValue::Bool(false))),
                ]);
                if let Some(index) = results_index {
                    if let Some(blocks) = output[index].get_mut("content").and_then(JsValue::as_array_mut) {
                        blocks.push(block);
                    }
                } else {
                    let index = assistant_index.map_or(output.len(), |index| index + 1);
                    output.insert(index, message("user", JsValue::Array(vec![block])));
                    results_index = Some(index);
                }
            }
        }
    }
    Ok(ConversionReport {
        messages: output,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;

    #[test]
    fn claude_bridge_empty_tool_results_match_ts_conversion_and_extraction() {
        let fixture = js_json::parse_lossless(include_str!("fixtures/stream-semantics-oracle.json")).unwrap();
        for case in fixture.get("mcp").unwrap().as_array().unwrap() {
            assert_eq!(
                tool_result_to_mcp_content(case.get("input").unwrap()),
                *case.get("output").unwrap()
            );
        }
        let messages: Vec<Message> = js_json::from_value(fixture.get("messages").unwrap()).unwrap();
        assert_eq!(
            JsValue::Array(convert_messages(&messages, true).unwrap()),
            *fixture.get("converted").unwrap()
        );
    }

    #[test]
    fn claude_bridge_conversion_hoists_parallel_results_before_steer_and_preserves_arguments() {
        let raw = js_json::parse_lossless(r#"[
            {"role":"assistant","content":[{"type":"toolCall","id":"a|1","name":"read","arguments":{"path":"\ud800"}},{"type":"toolCall","id":"b|2","name":"bash","arguments":{"command":"pwd"}}],"api":"faux","provider":"other","model":"m","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"toolUse","timestamp":1},
            {"role":"user","content":"steer","timestamp":2},
            {"role":"toolResult","toolCallId":"a|1","toolName":"read","content":[{"type":"text","text":"A"}],"isError":false,"timestamp":3},
            {"role":"assistant","content":[],"api":"faux","provider":"other","model":"m","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"aborted","timestamp":4},
            {"role":"toolResult","toolCallId":"b|2","toolName":"bash","content":[{"type":"text","text":"B"}],"isError":true,"timestamp":5}
        ]"#).unwrap();
        let messages: Vec<Message> = js_json::from_value(&raw).unwrap();
        let output = convert_messages(&messages, true).unwrap();
        let expected = js_json::parse_lossless(r#"[
            {"role":"assistant","content":[{"type":"tool_use","id":"a_1","name":"mcp__custom-tools__read","input":{"path":"\ud800"}},{"type":"tool_use","id":"b_2","name":"mcp__custom-tools__bash","input":{"command":"pwd"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"a_1","content":"A","is_error":false},{"type":"tool_result","tool_use_id":"b_2","content":"B","is_error":true}]},
            {"role":"user","content":"steer"}
        ]"#).unwrap();
        assert_eq!(JsValue::Array(output), expected);
    }

    #[test]
    fn claude_bridge_conversion_matches_frozen_provider_and_ask_fixtures() {
        let fixture = js_json::parse_lossless(include_str!("fixtures/session-oracle.json")).unwrap();
        let messages: Vec<Message> = js_json::from_value(fixture.get("input").unwrap()).unwrap();
        assert_eq!(
            JsValue::Array(convert_messages(&messages, true).unwrap()),
            fixture.get("provider").unwrap().clone()
        );
        assert_eq!(
            JsValue::Array(convert_messages(&messages, false).unwrap()),
            fixture.get("ask").unwrap().clone()
        );
    }

    #[test]
    fn claude_bridge_conversion_keeps_images_in_user_and_tool_result_blocks() {
        let raw = js_json::parse_lossless(r#"[
            {"role":"user","content":[{"type":"image","data":"AQ==","mimeType":"image/png"}],"timestamp":1},
            {"role":"toolResult","toolCallId":"i","toolName":"read","content":[{"type":"text","text":"look"},{"type":"image","data":"AQ==","mimeType":"image/png"}],"isError":false,"timestamp":2}
        ]"#).unwrap();
        let messages: Vec<Message> = js_json::from_value(&raw).unwrap();
        let output = convert_messages(&messages, true).unwrap();
        assert_eq!(
            output[0].get("content").and_then(JsValue::as_array).unwrap()[0]
                .get("source")
                .unwrap()
                .get("media_type")
                .unwrap(),
            &JsValue::String("image/png".into())
        );
        assert!(
            output[1].get("content").and_then(JsValue::as_array).unwrap()[0]
                .get("content")
                .and_then(JsValue::as_array)
                .is_some()
        );
    }
}
