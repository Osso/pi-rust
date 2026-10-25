use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Model, ThinkingLevel};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::is_truthy as truthy;
use pi_js_compat::number::js_number_value;

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

pub(super) fn oauth_key(key: &JsString) -> bool {
    key.contains("sk-ant-oat")
}

fn tool_name(name: &JsString, oauth: bool) -> JsString {
    if !oauth {
        return name.clone();
    }
    TOOLS
        .iter()
        .find(|tool| JsString::from(**tool).to_lowercase() == name.to_lowercase())
        .map_or_else(|| name.clone(), |tool| JsString::from(*tool))
}

pub(super) fn original_tool_name(name: &JsString, context: &Context, oauth: bool) -> JsString {
    if !oauth {
        return name.clone();
    }
    context
        .tools
        .as_ref()
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| JsString::from(&tool.name).to_lowercase() == name.to_lowercase())
        })
        .map_or_else(|| name.clone(), |tool| JsString::from(&tool.name))
}

// The example's regex replaces BOTH UTF-16 halves, including valid pairs.
// Do not substitute the core backend's pair-preserving sanitizer.
fn sanitize(text: &JsString) -> JsString {
    JsString::from_utf16(
        text.units()
            .iter()
            .map(|unit| {
                if (0xD800..=0xDFFF).contains(unit) {
                    0xFFFD
                } else {
                    *unit
                }
            })
            .collect(),
    )
}
fn text(value: &Value) -> JsString {
    value.as_string().cloned().unwrap_or_else(JsString::default)
}

fn image(block: &Value) -> Value {
    json!({"type":"image", "source":{"type":"base64", "media_type":block["mimeType"], "data":block["data"]}})
}
fn content_block(block: &Value) -> Value {
    if block["type"] == json!("text") {
        json!({"type":"text","text":sanitize(&text(&block["text"]))})
    } else {
        image(block)
    }
}
fn tool_result_content(blocks: &[Value]) -> Value {
    if blocks.iter().all(|b| b["type"] != json!("image")) {
        return json!(sanitize(&JsString::from(&JsString::join(
            blocks.iter().map(|b| text(&b["text"])),
            "\n"
        ))));
    }
    let mut blocks = blocks.iter().map(content_block).collect::<Vec<_>>();
    if blocks.iter().all(|b| b["type"] != json!("text")) {
        blocks.insert(0, json!({"type":"text","text":"(see attached image)"}));
    }
    json!(blocks)
}
fn assistant_blocks(message: &Value, oauth: bool) -> Vec<Value> {
    message["content"].as_array().expect("typed assistant content").iter().filter_map(|block| {
        match text(&block["type"]) {
            kind if kind == "text" && !text(&block["text"]).trim().is_empty()=>Some(json!({"type":"text","text":sanitize(&text(&block["text"]))})),
            kind if kind == "thinking" && !text(&block["thinking"]).trim().is_empty()=> {
                if truthy(&block["thinkingSignature"]) { Some(json!({"type":"thinking","thinking":sanitize(&text(&block["thinking"])),"signature":block["thinkingSignature"]})) }
                else { Some(json!({"type":"text","text":sanitize(&text(&block["thinking"]))})) }
            }
            kind if kind == "toolCall" =>Some(json!({"type":"tool_use","id":block["id"],"name":tool_name(&text(&block["name"]),oauth),"input":block["arguments"]})),
            _=>None,
        }
    }).collect()
}
fn messages(context: &Context, oauth: bool) -> Vec<Value> {
    let messages = pi_js_compat::js_json::to_value(&&context.messages).expect("typed messages");
    let messages = messages.as_array().expect("message array");
    let mut params = vec![];
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        match text(&message["role"]) {
            role if role == "user" => {
                if let Some(content) = message["content"].as_string() {
                    if !content.trim().is_empty() {
                        params.push(json!({"role":"user","content":sanitize(&JsString::from(content))}));
                    }
                } else if let Some(blocks) = message["content"].as_array().filter(|b| !b.is_empty()) {
                    params.push(json!({"role":"user","content":blocks.iter().map(content_block).collect::<Vec<_>>()}));
                }
            }
            role if role == "assistant" => {
                let blocks = assistant_blocks(message, oauth);
                if !blocks.is_empty() {
                    params.push(json!({"role":"assistant","content":blocks}));
                }
            }
            role if role == "toolResult" => {
                let mut results = vec![];
                while index < messages.len() && messages[index]["role"] == json!("toolResult") {
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
    if let Some(last) = params.last_mut()
        && last["role"] == json!("user")
        && let Some(block) = last["content"].as_array_mut().and_then(|blocks| blocks.last_mut())
    {
        block["cache_control"] = json!({"type":"ephemeral"});
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
        let parameters = Value::from(&tool.parameters);
        let properties=parameters.get("properties").filter(|p|truthy(p)).cloned().unwrap_or(json!({}));
        let required=parameters.get("required").filter(|p|truthy(p)).cloned().unwrap_or(json!([]));
        json!({"name":tool_name(&JsString::from(&tool.name),oauth),"description":tool.description,"input_schema":{"type":"object","properties":properties,"required":required}})
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
