use crate::shared_oracle as oracle;

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
static TEST_STATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
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
                                .map(|content| parse_lossless(serde_json::to_string(content).unwrap()).unwrap())
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
    let component = ToolExecutionComponent::new(
        request["name"].as_str().unwrap(),
        "call-id",
        parse_lossless(request["argsJson"].as_str().unwrap()).unwrap(),
        ToolExecutionOptions {
            show_images: request["options"]["showImages"].as_bool(),
            image_width_cells: request["options"]["imageWidthCells"].as_f64(),
            hide_output: request["options"]["hideOutput"].as_bool(),
        },
        definition(request, events.clone()),
        Arc::new(move || {
            render_count.fetch_add(1, Ordering::SeqCst);
        }),
        "/repo",
    );
    let mut component = component.borrow_mut();
    let snapshot = |component: &mut ToolExecutionComponent| json!({"lines":component.render(request["width"].as_u64().unwrap() as usize).iter().map(|line|line.units().to_vec()).collect::<Vec<_>>(),"events":events.lock().unwrap().clone(),"renders":renders.load(Ordering::SeqCst)});
    let mut output = vec![snapshot(&mut component)];
    let mut added: Vec<pi_tui::tui::ComponentRef> = Vec::new();
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
            "add" => {
                let text = match step["units"].as_array() {
                    Some(units) => {
                        JsString::from_utf16(units.iter().map(|unit| unit.as_u64().unwrap() as u16).collect())
                    }
                    None => JsString::from(step["value"].as_str().unwrap()),
                };
                let child = pi_tui::tui::component(Text::new(text, 0, 0));
                component.add_child(child.clone());
                added.push(child);
            }
            "remove" => component.remove_child(&added[step["index"].as_u64().unwrap() as usize]),
            "clear" => component.clear(),
            op => panic!("unknown operation {op}"),
        }
        output.push(snapshot(&mut component));
    }
    json!(output)
}
fn base_request(steps: Vec<Value>) -> Value {
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
    let _state = TEST_STATE.blocking_lock();
    let mut cases = Vec::new();
    for shell in ["none", "default", "self"] {
        for call in [false, true] {
            for result in [false, true] {
                let mut case = base_request(steps());
                case["definition"] = json!(shell);
                case["call"] = json!(call);
                case["result"] = json!(result);
                cases.push(case);
            }
        }
    }
    for (throw_call, throw_result) in [(true, false), (false, true), (true, true)] {
        let mut case = base_request(steps());
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
        let mut case = base_request(vec![
            json!({"op":"result","json":content.to_string(),"partial":false,"at":1700000012345.0}),
            json!({"op":"hide","value":true}),
            json!({"op":"hide","value":false}),
        ]);
        case["definition"] = json!(shell);
        case["call"] = json!(true);
        case["result"] = json!(true);
        cases.push(case);
        let mut case = base_request(vec![
            json!({"op":"start","at":1700000000000.0,"live":false}),
            json!({"op":"result","json":r#"{"content":[{"type":"text","text":"done"}],"isError":false}"#,"partial":false,"at":1700000012345.0}),
        ]);
        case["definition"] = json!(shell);
        cases.push(case);
    }
    for shell in ["none", "default", "self"] {
        for at in [
            -8_640_000_000_000_001.0,
            -8_640_000_000_000_000.0,
            -377_736_739_200_000.0,
            -0.5,
            1.5,
            253_402_300_800_000.0,
            8_640_000_000_000_000.0,
            8_640_000_000_000_001.0,
        ] {
            let mut case = base_request(vec![
                json!({"op":"result","json":r#"{"content":[{"type":"text","text":"done"}],"isError":false}"#,"partial":false,"at":at}),
            ]);
            case["definition"] = json!(shell);
            cases.push(case);
        }
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

#[test]
fn inherited_mutations_and_constructor_options_match_frozen() {
    let _state = TEST_STATE.blocking_lock();
    let mut oracle = oracle::TsOracle::spawn("tool-execution-oracle.mts", "packages/coding-agent/src");
    let mutations = vec![
        json!({"op":"add","value":"inherited child"}),
        json!({"op":"remove","index":0}),
        json!({"op":"add","units":[0xd800]}),
        json!({"op":"complete"}),
        json!({"op":"clear"}),
        json!({"op":"args","json":"{\"updated\":true}"}),
        json!({"op":"add","value":"after clear"}),
        json!({"op":"hide","value":true}),
        json!({"op":"hide","value":false}),
        json!({"op":"invalidate"}),
    ];
    let mut count = 0;
    for shell in ["none", "default", "self"] {
        for show_images in [Value::Null, json!(false), json!(true)] {
            for hide_output in [Value::Null, json!(false), json!(true)] {
                for image_width in [
                    Value::Null,
                    json!(0.0),
                    json!(-0.5),
                    json!(1.5),
                    json!(60.0),
                    json!(120.0),
                ] {
                    let mut request = base_request(mutations.clone());
                    request["definition"] = json!(shell);
                    request["call"] = json!(true);
                    request["result"] = json!(true);
                    request["options"] =
                        json!({"showImages":show_images,"hideOutput":hide_output,"imageWidthCells":image_width});
                    let expected = oracle.request(&request);
                    assert_eq!(expected["ok"], true, "{expected}");
                    assert_eq!(native(&request), expected["output"], "{request}");
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 162);
}

fn normalize_image_ids(line: &JsString) -> Vec<u16> {
    let mut text = line.to_utf8().unwrap();
    if let Some(start) = text.find(",i=") {
        let digits = start + 3;
        let end = text[digits..]
            .find(|character: char| !character.is_ascii_digit())
            .map_or(text.len(), |offset| digits + offset);
        text.replace_range(digits..end, "ID");
    }
    text.encode_utf16().collect()
}

async fn image_native(request: &Value) -> Value {
    pi_tui::set_capabilities(pi_tui::TerminalCapabilities {
        images: if request["kind"] == "no-caps" {
            None
        } else {
            Some(pi_tui::ImageProtocol::Kitty)
        },
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
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let requests = Arc::new(Mutex::new(Vec::<usize>::new()));
    let mut config = base_request(Vec::new());
    config["definition"] = request["shell"].clone();
    let unused = Arc::new(Mutex::new(Vec::new()));
    let mut definition = definition(&config, unused).unwrap();
    let calls = events.clone();
    definition.render_call = Some(Arc::new(move |_, _, _| {
        calls.lock().unwrap().push("call".into());
        Ok(Rc::new(RefCell::new(Text::new("call", 0, 0))))
    }));
    let results = events.clone();
    definition.render_result = Some(Arc::new(move |_, _, _, _| {
        results.lock().unwrap().push("result".into());
        Ok(Rc::new(RefCell::new(Text::new("result", 0, 0))))
    }));
    let requested = requests.clone();
    let observed = events.clone();
    let mut component = Some(ToolExecutionComponent::new(
        "unknown_tool",
        "call-id",
        parse_lossless("{}").unwrap(),
        ToolExecutionOptions {
            hide_output: request["hide"].as_bool(),
            show_images: request["options"]["showImages"].as_bool(),
            image_width_cells: request["options"]["imageWidthCells"].as_f64(),
        },
        Some(definition),
        Arc::new(move || {
            requested.lock().unwrap().push(observed.lock().unwrap().len());
        }),
        "/repo",
    ));
    let gif = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aTl0AAAAASUVORK5CYII=";
    let image_result = |data: &str, mime: &str| {
        result(&json!({"content":[{"type":"image","data":data,"mimeType":mime}],"isError":false}).to_string())
    };
    let snapshot = |component: &mut Option<Rc<RefCell<ToolExecutionComponent>>>, label: &str| json!({"label":label,"lines":component.as_ref().map(|component|component.borrow_mut().render(30).iter().map(normalize_image_ids).collect::<Vec<_>>()).unwrap_or_default(),"events":events.lock().unwrap().clone(),"requests":requests.lock().unwrap().clone()});
    let kind = request["kind"].as_str().unwrap_or("gif");
    let data = match kind {
        "png" => png,
        "invalid" => "AAAA",
        "empty" => "",
        _ => gif,
    };
    let mime = if kind == "png" { "image/png" } else { "image/gif" };
    let weak = Rc::downgrade(component.as_ref().unwrap());
    component
        .as_ref()
        .unwrap()
        .borrow_mut()
        .update_result(image_result(data, mime), true, None);
    let mut output = vec![snapshot(&mut component, "first-sync")];
    let replace = request["replace"].as_bool().unwrap();
    component.as_ref().unwrap().borrow_mut().update_result(
        if replace {
            image_result(png, "image/png")
        } else {
            image_result(data, mime)
        },
        true,
        None,
    );
    output.push(snapshot(&mut component, "second-sync"));
    if request["drop"].as_bool().unwrap() {
        component.take();
    }
    let expected = if kind != "gif" {
        0
    } else if replace {
        1
    } else {
        2
    };
    if expected == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while requests.lock().unwrap().len() < expected && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(requests.lock().unwrap().len(), expected);
    if request["drop"].as_bool().unwrap() {
        assert!(weak.upgrade().is_none(), "completed conversion retained a removed row");
    }
    output.push(snapshot(&mut component, "settled"));
    if let Some(current) = component.as_mut() {
        current.borrow_mut().set_hide_output(false);
    }
    if component.is_some() {
        output.push(snapshot(&mut component, "unhidden"));
        component.as_ref().unwrap().borrow_mut().invalidate();
        output.push(snapshot(&mut component, "invalidate"));
        component
            .as_ref()
            .unwrap()
            .borrow_mut()
            .update_result(image_result(data, mime), true, None);
        output.push(snapshot(&mut component, "cached"));
    }
    json!(output)
}

#[tokio::test(flavor = "current_thread")]
async fn kitty_conversion_updates_later_before_request_render_and_retains_removed_rows() {
    let _state = TEST_STATE.lock().await;
    tokio::task::LocalSet::new().run_until(async {
        let mut oracle = oracle::TsOracle::spawn("tool-image-lifecycle-oracle.mts", "packages/coding-agent/src");
        let mut cases = Vec::new();
        for mask in 0..16 {
            cases.push(json!({"colors":colors(),"shell":if mask & 1 == 0 {"default"} else {"self"},"hide":mask & 2 != 0,"replace":mask & 4 != 0,"drop":mask & 8 != 0}));
        }
        for kind in ["png", "invalid", "empty", "no-caps"] {
            cases.push(json!({"colors":colors(),"shell":"self","hide":false,"replace":false,"drop":false,"kind":kind}));
        }
        for shell in ["default", "self"] {
            for hide in [false, true] {
                for show_images in [Value::Null, json!(false), json!(true)] {
                    for image_width in [Value::Null, json!(-0.5), json!(0.0), json!(0.5), json!(1.5), json!(30.0), json!(60.0), json!(120.0)] {
                        cases.push(json!({"colors":colors(),"shell":shell,"hide":hide,"replace":false,"drop":false,"kind":"png","options":{"showImages":show_images,"imageWidthCells":image_width}}));
                    }
                }
            }
        }
        let mut failures = Vec::new();
        for request in cases {
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"], true, "source: {expected}");
            let actual = image_native(&request).await;
            if actual != expected["output"] {
                let step = actual.as_array().unwrap().iter().zip(expected["output"].as_array().unwrap()).position(|(actual, expected)|actual!=expected).unwrap();
                eprintln!("{request} first mismatch step {step}: native events {} requests {} / source events {} requests {}",actual[step]["events"],actual[step]["requests"],expected["output"][step]["events"],expected["output"][step]["requests"]);
                failures.push(request);
            }
        }
        assert!(failures.is_empty(), "{} async image cases differ", failures.len());
    }).await;
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
