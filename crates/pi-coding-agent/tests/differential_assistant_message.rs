use crate::shared_oracle as oracle;

use pi_ai::{
    providers::faux::{FauxAssistantMessageOptions, faux_assistant_message},
    types::{AssistantContent, AssistantMessage, StopReason, TextContent, ThinkingContent, ToolCall},
};
use pi_coding_agent::modes::interactive::{
    components::assistant_message::AssistantMessageComponent,
    theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
};
use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::{Component, MarkdownTheme};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

struct TestMarkdown {
    highlighted: Cell<usize>,
    stateful: bool,
}
macro_rules! identity_styles { ($($name:ident),*) => { $(fn $name(&self, text: &JsString) -> JsString { text.clone() })* }; }
impl MarkdownTheme for TestMarkdown {
    identity_styles!(
        heading,
        link,
        link_url,
        code,
        code_block,
        code_block_border,
        quote,
        quote_border,
        hr,
        list_bullet,
        bold,
        italic,
        strikethrough,
        underline
    );
    fn highlight_code(&self, code: &JsString, _: Option<&JsString>) -> Option<Vec<JsString>> {
        if !self.stateful {
            return None;
        }
        self.highlighted.set(self.highlighted.get() + 1);
        Some(vec![js_format!("[{}]{}", self.highlighted.get(), code)])
    }
}

fn initialize_theme() {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|name| ((*name).into(), ColorValue::Index(0.0)))
        .collect();
    set_theme_instance(std::sync::Arc::new(
        Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap(),
    ));
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn raw(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn block(kind: &str, text: &str) -> Value {
    json!({"kind":kind,"units":units(text)})
}
fn message(content: Vec<Value>, stop: &str) -> Value {
    json!({"content":content,"stop":stop})
}
fn request(initial: Value, hide: bool, steps: Vec<Value>) -> Value {
    json!({"initial":initial,"hide":hide,"label":units("PRIVATE"),"padding":1,"width":40,"steps":steps})
}
fn native_message(value: &Value) -> AssistantMessage {
    let mut result = faux_assistant_message("", FauxAssistantMessageOptions::default());
    result.content = value["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|content| match content["kind"].as_str().unwrap() {
            "text" => AssistantContent::Text(TextContent::new(raw(&content["units"]))),
            "thinking" => {
                let mut thinking = ThinkingContent::new(raw(&content["units"]));
                thinking.thinking_signature = content["signature"].as_str().map(str::to_owned);
                AssistantContent::Thinking(thinking)
            }
            "toolCall" => AssistantContent::ToolCall(ToolCall::new("tool-1", "read", Default::default())),
            kind => panic!("unknown content fixture {kind}"),
        })
        .collect();
    result.stop_reason = match value["stop"].as_str().unwrap() {
        "stop" => StopReason::Stop,
        "toolUse" => StopReason::ToolUse,
        "length" => StopReason::Length,
        "aborted" => StopReason::Aborted,
        "error" => StopReason::Error,
        value => panic!("unknown stop {value}"),
    };
    result.error_message = value.get("errorUnits").map(raw);
    result
}
fn native_snapshots(request: &Value) -> Value {
    let initial = (!request["initial"].is_null()).then(|| native_message(&request["initial"]));
    let mut component = AssistantMessageComponent::new(
        initial,
        request["hide"].as_bool().unwrap(),
        Some(Rc::new(TestMarkdown {
            highlighted: Cell::new(0),
            stateful: request["stateful"].as_bool().unwrap_or(false),
        })),
        Some(raw(&request["label"])),
        request["padding"].as_u64().unwrap() as usize,
    );
    let mut width = request["width"].as_u64().unwrap() as usize;
    let snapshot = |component: &mut AssistantMessageComponent, width| {
        component
            .render(width)
            .iter()
            .map(|line| line.units().to_vec())
            .collect::<Vec<_>>()
    };
    let mut snapshots = vec![snapshot(&mut component, width)];
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "update" => component.update_content(native_message(&step["message"])),
            "hide" => component.set_hide_thinking_block(step["value"].as_bool().unwrap()),
            "label" => component.set_hidden_thinking_label(raw(&step["units"])),
            "pad" => component.set_output_pad(step["value"].as_u64().unwrap() as usize),
            "invalidate" => component.invalidate(),
            "render" => {}
            value => panic!("unknown step {value}"),
        }
        if let Some(value) = step["width"].as_u64() {
            width = value as usize;
        }
        snapshots.push(snapshot(&mut component, width));
    }
    json!(snapshots)
}
fn compare(cases: Vec<Value>) {
    initialize_theme();
    let mut oracle = oracle::TsOracle::spawn("assistant-message-oracle.ts", "packages/coding-agent/src");
    for (index, case) in cases.into_iter().enumerate() {
        let expected = oracle.request(&case);
        assert_eq!(expected["ok"], true, "oracle {index}: {expected}");
        assert_eq!(native_snapshots(&case), expected["output"], "case {index}: {case}");
    }
}

#[test]
fn separate_thinking_slots_status_truthiness_and_raw_values_match_frozen() {
    let mut cases = vec![request(
        message(
            vec![
                block("thinking", "one"),
                block("thinking", ""),
                block("thinking", "two"),
                block("text", "answer"),
            ],
            "stop",
        ),
        true,
        vec![],
    )];
    for stop in ["length", "aborted", "error"] {
        for error in [
            None,
            Some(vec![]),
            Some(units("Request was aborted")),
            Some(vec![0xd800]),
            Some(units("details")),
        ] {
            let mut msg = message(vec![block("text", "partial")], stop);
            if let Some(error) = error {
                msg["errorUnits"] = json!(error);
            }
            cases.push(request(msg, false, vec![]));
        }
        cases.push(request(message(vec![block("toolCall", "")], stop), false, vec![]));
    }
    cases.push(request(
        Value::Null,
        true,
        vec![
            json!({"op":"hide","value":false}),
            json!({"op":"label","units":[0xdc00]}),
            json!({"op":"pad","value":0}),
            json!({"op":"invalidate"}),
        ],
    ));
    compare(cases);
}

#[test]
fn content_slot_transitions_setters_and_invalidation_match_frozen() {
    let messages = vec![
        message(
            vec![
                block("text", " **alpha** "),
                block("thinking", "private"),
                block("thinking", "second"),
            ],
            "stop",
        ),
        message(
            vec![
                block("thinking", "private"),
                block("text", " **next** "),
                block("toolCall", ""),
            ],
            "toolUse",
        ),
        message(
            vec![block("text", ""), block("thinking", ""), block("text", "last")],
            "stop",
        ),
        message(vec![block("thinking", "reused")], "aborted"),
        message(vec![], "error"),
        message(vec![], "stop"),
    ];
    let mut steps = Vec::new();
    for msg in &messages {
        steps.push(json!({"op":"update","message":msg}));
        steps.push(json!({"op":"hide","value":true}));
        steps.push(json!({"op":"label","units":[0xd800,88]}));
        steps.push(json!({"op":"pad","value":0}));
        steps.push(json!({"op":"hide","value":false}));
        steps.push(json!({"op":"invalidate","width":23}));
    }
    compare(vec![request(messages[0].clone(), false, steps)]);
}

#[test]
fn unchanged_markdown_keeps_observable_highlighter_cache_until_invalidation() {
    let code = message(vec![block("text", "```probe\ncache\n```")], "stop");
    let appended = message(
        vec![block("text", "```probe\ncache\n```"), block("thinking", "private")],
        "stop",
    );
    let mut case = request(
        code.clone(),
        false,
        vec![
            json!({"op":"update","message":code}),
            json!({"op":"pad","value":1}),
            json!({"op":"label","units":units("hidden")}),
            json!({"op":"update","message":appended}),
            json!({"op":"hide","value":true}),
            json!({"op":"render"}),
            json!({"op":"invalidate"}),
            json!({"op":"update","message":message(vec![block("text","```probe\nchanged\n```")],"stop")}),
        ],
    );
    case["stateful"] = json!(true);
    compare(vec![case]);
}

fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        bytes.push(byte | if value == 0 { 0 } else { 128 });
        if value == 0 {
            return bytes;
        }
    }
}
fn field(key: u64, bytes: &[u8]) -> Vec<u8> {
    [varint(key), varint(bytes.len() as u64), bytes.to_vec()].concat()
}
fn narration(kind: &[u8]) -> Vec<u8> {
    field(18, &field(10, &field(66, kind)))
}
fn signature_corpus() -> Vec<Option<String>> {
    use pi_js_compat::base64::buffer_to_base64;
    let valid = narration(b"narration");
    let mut packets = vec![
        valid.clone(),
        narration(b"private"),
        narration(b"narration\0"),
        narration(b"\xef\xbb\xbfnarration"),
        vec![],
        vec![128],
        vec![18, 127],
        vec![19],
        vec![18, 1, 128],
        vec![18, 0],
    ];
    for prefix in [
        vec![8, 7],
        vec![9, 0, 0, 0, 0, 0, 0, 0, 0],
        vec![13, 0, 0, 0, 0],
        vec![16, 7],
        vec![0, 0],
        vec![19],
        vec![18, 127],
    ] {
        packets.push([prefix, valid.clone()].concat());
    }
    packets.push([narration(b"private"), valid.clone()].concat());
    packets.push(field(
        18 + (1 << 32),
        &field(10 + (1 << 33), &field(66 + (1 << 34), b"narration")),
    ));
    packets.push(field(18 + (1 << 35), &field(10, &field(66, b"narration"))));
    packets.push([field(58, &[251, 255]), valid.clone()].concat());
    for position in 0..valid.len() {
        for byte in [0, 127, 128, 255] {
            let mut packet = valid.clone();
            packet[position] = byte;
            packets.push(packet);
        }
    }
    let mut signatures: Vec<_> = packets.iter().map(|packet| buffer_to_base64(packet)).collect();
    let encoded = buffer_to_base64(&valid);
    signatures.extend([
        format!(" !\n{encoded}\t"),
        format!("={encoded}"),
        format!("{encoded}=ignored"),
        format!("😀{encoded}"),
        format!("{encoded}😀"),
        format!("\u{0145}{}", &encoded[1..]),
    ]);
    signatures.extend(
        signatures
            .clone()
            .into_iter()
            .map(|value| value.replace('+', "-").replace('/', "_")),
    );
    signatures.sort();
    signatures.dedup();
    std::iter::once(None).chain(signatures.into_iter().map(Some)).collect()
}

#[test]
fn narration_protobuf_base64_signature_corpus_matches_frozen_public_rendering() {
    let cases = signature_corpus()
        .into_iter()
        .map(|signature| {
            let mut content = block("thinking", "PUBLIC");
            content["signature"] = json!(signature);
            request(message(vec![content], "stop"), true, vec![])
        })
        .collect();
    compare(cases);
}
