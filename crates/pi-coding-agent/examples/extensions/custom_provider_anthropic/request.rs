use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Model, ThinkingLevel};
use pi_js_compat::number::js_number_value;
use serde_json::{Value, json};

const TOOLS: [&str; 10] = [
    "Read",
    "Write",
    "Edit",
    "Bash",
    "Grep",
    "Glob",
    "AskUserQuestion",
    "TodoWrite",
    "WebFetch",
    "WebSearch",
];

pub(super) fn oauth_key(key: &str) -> bool {
    key.contains("sk-ant-oat")
}

fn tool_name(name: &str, oauth: bool) -> String {
    if oauth {
        TOOLS
            .iter()
            .find(|tool| tool.to_lowercase() == name.to_lowercase())
            .copied()
            .unwrap_or(name)
            .to_owned()
    } else {
        name.to_owned()
    }
}

pub(super) fn original_tool_name(name: &str, context: &Context, oauth: bool) -> String {
    if oauth {
        context
            .tools
            .as_ref()
            .and_then(|tools| {
                tools
                    .iter()
                    .find(|tool| tool.name.to_lowercase() == name.to_lowercase())
            })
            .map(|tool| tool.name.clone())
            .unwrap_or_else(|| name.to_owned())
    } else {
        name.to_owned()
    }
}

// The example's regex replaces BOTH UTF-16 halves, including valid pairs.
// Do not substitute the core backend's pair-preserving sanitizer.
fn sanitize(text: &str) -> String {
    text.encode_utf16()
        .map(|unit| {
            if (0xD800..=0xDFFF).contains(&unit) {
                '\u{FFFD}'
            } else {
                char::from_u32(u32::from(unit)).expect("non-surrogate UTF-16")
            }
        })
        .collect()
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn image(block: &Value) -> Value {
    json!({"type":"image", "source":{"type":"base64", "media_type":block["mimeType"], "data":block["data"]}})
}
fn content_block(block: &Value) -> Value {
    if block["type"] == "text" {
        json!({"type":"text","text":sanitize(text(&block["text"]))})
    } else {
        image(block)
    }
}
fn tool_result_content(blocks: &[Value]) -> Value {
    if blocks.iter().all(|b| b["type"] != "image") {
        return json!(sanitize(
            &blocks.iter().map(|b| text(&b["text"])).collect::<Vec<_>>().join("\n")
        ));
    }
    let mut blocks = blocks.iter().map(content_block).collect::<Vec<_>>();
    if blocks.iter().all(|b| b["type"] != "text") {
        blocks.insert(0, json!({"type":"text","text":"(see attached image)"}));
    }
    json!(blocks)
}
fn assistant_blocks(message: &Value, oauth: bool) -> Vec<Value> {
    message["content"].as_array().expect("typed assistant content").iter().filter_map(|block| {
        match text(&block["type"]) {
            "text" if !pi_js_compat::string::js_trim(text(&block["text"])).is_empty()=>Some(json!({"type":"text","text":sanitize(text(&block["text"]))})),
            "thinking" if !pi_js_compat::string::js_trim(text(&block["thinking"])).is_empty()=> {
                if truthy(&block["thinkingSignature"]) { Some(json!({"type":"thinking","thinking":sanitize(text(&block["thinking"])),"signature":block["thinkingSignature"]})) }
                else { Some(json!({"type":"text","text":sanitize(text(&block["thinking"]))})) }
            }
            "toolCall"=>Some(json!({"type":"tool_use","id":block["id"],"name":tool_name(text(&block["name"]),oauth),"input":block["arguments"]})),
            _=>None,
        }
    }).collect()
}
fn messages(context: &Context, oauth: bool) -> Vec<Value> {
    let messages = serde_json::to_value(&context.messages).expect("typed messages");
    let messages = messages.as_array().expect("message array");
    let mut params = vec![];
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        match text(&message["role"]) {
            "user" => {
                if let Some(content) = message["content"].as_str() {
                    if !pi_js_compat::string::js_trim(content).is_empty() {
                        params.push(json!({"role":"user","content":sanitize(content)}));
                    }
                } else if let Some(blocks) = message["content"].as_array().filter(|b| !b.is_empty()) {
                    params.push(json!({"role":"user","content":blocks.iter().map(content_block).collect::<Vec<_>>()}));
                }
            }
            "assistant" => {
                let blocks = assistant_blocks(message, oauth);
                if !blocks.is_empty() {
                    params.push(json!({"role":"assistant","content":blocks}));
                }
            }
            "toolResult" => {
                let mut results = vec![];
                while index < messages.len() && messages[index]["role"] == "toolResult" {
                    let m = &messages[index];
                    results.push(json!({"type":"tool_result","tool_use_id":m["toolCallId"],"content":tool_result_content(m["content"].as_array().expect("typed result")),"is_error":m["isError"]}));
                    index += 1;
                }
                params.push(json!({"role":"user","content":results}));
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    if let Some(last) = params.last_mut() {
        if last["role"] == "user" {
            if let Some(block) = last["content"].as_array_mut().and_then(|blocks| blocks.last_mut()) {
                block["cache_control"] = json!({"type":"ephemeral"});
            }
        }
    }
    params
}

pub(super) fn build_params(model: &Model, context: &Context, options: &SimpleStreamOptions, oauth: bool) -> Value {
    let max_tokens = options
        .stream
        .max_tokens
        .filter(|n| *n != 0.0 && !n.is_nan())
        .unwrap_or_else(|| (model.max_tokens / 3.0).floor());
    let mut params = json!({"model":model.id,"messages":messages(context,oauth),"max_tokens":js_number_value(max_tokens),"stream":true});
    let mut system = vec![];
    if oauth {
        system.push(json!({"type":"text","text":"You are Claude Code, Anthropic's official CLI for Claude.","cache_control":{"type":"ephemeral"}}));
    }
    if let Some(prompt) = context.system_prompt.as_ref().filter(|p| !p.is_empty()) {
        system.push(json!({"type":"text","text":sanitize(prompt),"cache_control":{"type":"ephemeral"}}));
    }
    if !system.is_empty() {
        params["system"] = json!(system);
    }
    if let Some(tools) = &context.tools {
        params["tools"]=json!(tools.iter().map(|tool| {
        let properties=tool.parameters.get("properties").filter(|p|truthy(p)).cloned().unwrap_or(json!({}));
        let required=tool.parameters.get("required").filter(|p|truthy(p)).cloned().unwrap_or(json!([]));
        json!({"name":tool_name(&tool.name,oauth),"description":tool.description,"input_schema":{"type":"object","properties":properties,"required":required}})
    }).collect::<Vec<_>>());
    }
    if let Some(reasoning) = options.reasoning.filter(|_| model.reasoning) {
        let custom = options.thinking_budgets.as_ref().and_then(|budgets| match reasoning {
            ThinkingLevel::Minimal => budgets.minimal,
            ThinkingLevel::Low => budgets.low,
            ThinkingLevel::Medium => budgets.medium,
            ThinkingLevel::High => budgets.high,
            _ => None,
        });
        let budget = custom.unwrap_or(match reasoning {
            ThinkingLevel::Minimal => 1024.0,
            ThinkingLevel::Low => 4096.0,
            ThinkingLevel::High => 20480.0,
            _ => 10240.0,
        });
        params["thinking"] = json!({"type":"enabled","budget_tokens":js_number_value(budget)});
    }
    params
}
