#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::{
    core::extensions::types::{HandlerError, RenderShell, ToolDefinition, ToolRenderContext},
    modes::interactive::{
        components::tool_execution::{
            ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult, ToolExecutionStartOptions,
        },
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless};
use pi_tui::{Component, Text};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn colors() -> Value {
    let mut colors = serde_json::Map::new();
    for (index, name) in REQUIRED_COLORS.iter().enumerate() {
        colors.insert(
            (*name).into(),
            json!(format!(
                "#{:02x}{:02x}{:02x}",
                (16 + 7 * index) % 256,
                (32 + 11 * index) % 256,
                (48 + 13 * index) % 256
            )),
        );
    }
    Value::Object(colors)
}
fn record(name: &str, context: &ToolRenderContext, events: &Mutex<Vec<Value>>) -> usize {
    let mut count = 0;
    context.state.update(|state| {
        let state = state.as_object_mut().unwrap();
        count = state.get(name).and_then(JsValue::as_f64).unwrap_or(0.0) as usize + 1;
        state.insert(name.into(), JsValue::Number(count as f64));
    });
    events.lock().unwrap().push(json!([
        name,
        stringify_lossless(&context.args),
        context.tool_call_id,
        context.cwd.to_utf8().unwrap(),
        context.execution_started,
        context.args_complete,
        context.is_partial,
        context.expanded,
        context.show_images,
        context.is_error,
        context.last_component.is_some(),
        stringify_lossless(&context.state.get())
    ]));
    count
}
fn returned(context: &ToolRenderContext, text: JsString) -> pi_tui::tui::ComponentRef {
    if let Some(component) = &context.last_component {
        component
            .borrow_mut()
            .as_any_mut()
            .unwrap()
            .downcast_mut::<Text>()
            .unwrap()
            .set_text(text);
        return component.clone();
    }
    Rc::new(RefCell::new(Text::new(text, 0, 0)))
}
fn definition(request: &Value, events: Arc<Mutex<Vec<Value>>>) -> Option<ToolDefinition> {
    if request["definition"] == "none" {
        return None;
    }
    let call_events = events.clone();
    let throw_call = request["throwCall"].as_bool().unwrap();
    let empty_call = request["emptyCall"].as_bool().unwrap();
    let throw_result = request["throwResult"].as_bool().unwrap();
    Some(ToolDefinition {
        name: request["name"].as_str().unwrap().into(),
        label: "tool".into(),
        description: "tool".into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type":"object"}),
        constrained_sampling: None,
        render_shell: Some(if request["definition"] == "self" {
            RenderShell::SelfRender
        } else {
            RenderShell::Default
        }),
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_, _, _, _, _| {
            Box::pin(async {
                Ok(AgentToolResult {
                    content: vec![],
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: request["call"].as_bool().unwrap().then(|| {
            Arc::new(move |_: &JsValue, _: &Theme, context: &ToolRenderContext| {
                let count = record("call", context, &call_events);
                if throw_call {
                    return Err(HandlerError::new("call"));
                }
                Ok(returned(
                    context,
                    if empty_call {
                        JsString::default()
                    } else {
                        format!("call {count}").into()
                    },
                ))
            }) as pi_coding_agent::core::extensions::types::ToolRenderer
        }),
        render_result: request["result"].as_bool().unwrap().then(|| {
            Arc::new(
                move |result: &AgentToolResult,
                      _: &pi_coding_agent::core::extensions::types::ToolRenderResultOptions,
                      _: &Theme,
                      context: &ToolRenderContext| {
                    record("result", context, &events);
                    if throw_result {
                        return Err(HandlerError::new("result"));
                    }
                    let mut value = pi_js_compat::js_json::JsObject::new();
                    value.insert(
                        "content".into(),
                        JsValue::Array(
                            result
                                .content
                                .iter()
                                .map(|content| parse_lossless(&serde_json::to_string(content).unwrap()).unwrap())
                                .collect(),
                        ),
                    );
                    if let Some(details) = &result.details {
                        value.insert("details".into(), details.clone());
                    }
                    Ok(returned(context, stringify_lossless(&JsValue::Object(value)).into()))
                },
            ) as pi_coding_agent::core::extensions::types::ToolResultRenderer
        }),
    })
}
fn result(value: &str) -> ToolExecutionResult {
    let parsed = parse_lossless(value).unwrap();
    ToolExecutionResult {
        content: parsed
            .get("content")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| serde_json::from_str::<TextOrImageContent>(&stringify_lossless(value)).unwrap())
            .collect(),
        details: parsed.get("details").cloned(),
        is_error: parsed.get("isError").and_then(JsValue::as_bool).unwrap(),
    }
}
fn native(request: &Value) -> Value {
    pi_tui::set_capabilities(pi_tui::TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: false,
    });
    let colors: Vec<_> = request["colors"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| (name.as_str().into(), ColorValue::Text(value.as_str().unwrap().into())))
        .collect();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap(),
    ));
    let events = Arc::new(Mutex::new(Vec::new()));
    let renders = Arc::new(AtomicUsize::new(0));
    let render_count = renders.clone();
    let mut component = ToolExecutionComponent::new(
        request["name"].as_str().unwrap(),
        "call-id",
        parse_lossless(request["argsJson"].as_str().unwrap()).unwrap(),
        ToolExecutionOptions::default(),
        definition(request, events.clone()),
        Arc::new(move || {
            render_count.fetch_add(1, Ordering::SeqCst);
        }),
        "/repo",
    );
    let snapshot = |component: &mut ToolExecutionComponent| json!({"lines":component.render(request["width"].as_u64().unwrap() as usize).iter().map(|line|line.units().to_vec()).collect::<Vec<_>>(),"events":events.lock().unwrap().clone(),"renders":renders.load(Ordering::SeqCst)});
    let mut output = vec![snapshot(&mut component)];
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "args" => component.update_args(parse_lossless(step["json"].as_str().unwrap()).unwrap()),
            "complete" => component.set_args_complete(),
            "start" => component.mark_execution_started(
                step["at"].as_f64().unwrap(),
                ToolExecutionStartOptions {
                    show_live_elapsed: step["live"].as_bool().unwrap(),
                },
            ),
            "result" => component.update_result(
                result(step["json"].as_str().unwrap()),
                step["partial"].as_bool().unwrap(),
                step["at"].as_f64(),
            ),
            "expanded" => component.set_expanded(step["value"].as_bool().unwrap()),
            "images" => component.set_show_images(step["value"].as_bool().unwrap()),
            "hide" => component.set_hide_output(step["value"].as_bool().unwrap()),
            "width" => component.set_image_width_cells(step["value"].as_f64().unwrap()),
            "invalidate" => component.invalidate(),
            op => panic!("unknown operation {op}"),
        }
        output.push(snapshot(&mut component));
    }
    json!(output)
}
fn request(steps: Vec<Value>) -> Value {
    json!({"name":"unknown_tool","definition":"none","call":false,"result":false,"throwCall":false,"throwResult":false,"emptyCall":false,"colors":colors(),"argsJson":r#"{"9":9,"text":"\ud800"}"#,"options":{},"width":100,"steps":steps})
}
fn steps() -> Vec<Value> {
    vec![
        json!({"op":"complete"}),
        json!({"op":"args","json":"{\"new\":true}"}),
        json!({"op":"result","json":r#"{"content":[{"type":"text","text":"result\ud800"},{"type":"image","data":"","mimeType":"image/png"}],"details":{"x":"\udc00"},"isError":false}"#,"partial":true}),
        json!({"op":"hide","value":true}),
        json!({"op":"expanded","value":true}),
        json!({"op":"images","value":false}),
        json!({"op":"hide","value":false}),
        json!({"op":"result","json":r#"{"content":[{"type":"text","text":"failed"}],"isError":true}"#,"partial":false,"at":1700000012345.5}),
        json!({"op":"invalidate"}),
    ]
}
#[test]
fn whole_tool_render_sequences_match_frozen() {
    let mut cases = Vec::new();
    for shell in ["none", "default", "self"] {
        for call in [false, true] {
            for result in [false, true] {
                let mut case = request(steps());
                case["definition"] = json!(shell);
                case["call"] = json!(call);
                case["result"] = json!(result);
                cases.push(case);
            }
        }
    }
    for (throw_call, throw_result) in [(true, false), (false, true), (true, true)] {
        let mut case = request(steps());
        case["definition"] = json!("self");
        case["call"] = json!(true);
        case["result"] = json!(true);
        case["throwCall"] = json!(throw_call);
        case["throwResult"] = json!(throw_result);
        cases.push(case);
    }
    let long = (0..150)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let content = json!({"content":[{"type":"text","text":long},{"type":"image","data":"","mimeType":"image/png"},{"type":"text","text":"tail"}],"isError":false});
    for shell in ["none", "default", "self"] {
        let mut case = request(vec![
            json!({"op":"result","json":content.to_string(),"partial":false,"at":1700000012345.0}),
            json!({"op":"hide","value":true}),
            json!({"op":"hide","value":false}),
        ]);
        case["definition"] = json!(shell);
        case["call"] = json!(true);
        case["result"] = json!(true);
        cases.push(case);
        let mut case = request(vec![
            json!({"op":"start","at":1700000000000.0,"live":false}),
            json!({"op":"result","json":r#"{"content":[{"type":"text","text":"done"}],"isError":false}"#,"partial":false,"at":1700000012345.0}),
        ]);
        case["definition"] = json!(shell);
        cases.push(case);
    }
    let mut oracle = oracle::TsOracle::spawn("tool-execution-oracle.mts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let expected = oracle.request(case);
        assert_eq!(expected["ok"], true, "oracle: {expected}");
        let actual = native(case);
        if actual != expected["output"] {
            let step = actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["output"].as_array().unwrap())
                .position(|(actual, expected)| actual != expected)
                .unwrap();
            eprintln!(
                "case {index} shell {} step {step}: native {} / source {}",
                case["definition"], actual[step], expected["output"][step]
            );
            failures.push(index);
        }
    }
    assert!(failures.is_empty(), "tool cases mismatched: {failures:?}");
}
