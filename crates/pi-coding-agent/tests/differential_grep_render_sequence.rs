#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::grep::create_grep_tool_definition;
use pi_coding_agent::core::tools::truncate::{TruncationOptions, truncate_head};
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::Arc;

#[test]
fn grep_renderer_reuses_text_and_preserves_numeric_warning_truthiness() {
    let theme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(theme_path.to_str().unwrap()),
            Some(ColorMode::Truecolor),
        )
        .unwrap(),
    );
    set_theme_instance(theme.clone());
    let truncation = truncate_head(JsString::from("x".repeat(12000)), TruncationOptions::default());
    let steps = vec![
        json!({"kind":"call","args":{}}),
        json!({"kind":"call","args":{"pattern":"","path":"","limit":0}}),
        json!({"kind":"call","args":{"pattern":"*.rs","path":JsString::from("raw-")+JsString::from_utf16(vec![0xd800]),"limit":1.5}}),
        json!({"kind":"call","args":{"pattern":"*.rs","path":"file"},"number":"NaN"}),
        json!({"kind":"call","args":{"pattern":"*.rs","path":"file"},"number":"Infinity"}),
        json!({"kind":"result","result":{"content":[]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"line\n".repeat(22)}]}}),
        json!({"kind":"result","expanded":true,"result":{"content":[{"type":"text","text":"line\n".repeat(22)}]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"\u{feff} body \u{feff}"}],"details":{"matchLimitReached":0}}}),
        json!({"kind":"result","result":{"content":[],"details":{"matchLimitReached":1.5}}}),
        json!({"kind":"result","result":{"content":[],"details":{"matchLimitReached":-1}}}),
        json!({"kind":"result","number":"Infinity","result":{"content":[]}}),
        json!({"kind":"result","number":"NaN","result":{"content":[]}}),
        json!({"kind":"result","result":{"content":[],"details":{"matchLimitReached":1.5,"truncation":js_json::to_value(&truncation).unwrap()}}}),
        json!({"kind":"result","result":{"content":[{"type":"image","data":"AA==","mimeType":"image/png"},{"type":"text","text":"after image"}]}}),
        json!({"kind":"result","showImages":true,"result":{"content":[{"type":"image","data":"AA==","mimeType":"image/png"},{"type":"text","text":"after image"}]}}),
        json!({"kind":"result","result":{"content":[],"details":{"linesTruncated":true,"truncation":js_json::to_value(&truncation).unwrap()}}}),
        json!({"kind":"result","isError":false,"result":{"content":[{"type":"text","text":"hidden success"}],"details":{"linesTruncated":true}}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"visible again"}]}}),
        json!({"kind":"call","args":{"pattern":"raw","path":"file","glob":JsString::from_utf16(vec![0xdfff])}}),
        json!({"kind":"call","wrongComponent":true,"args":{"pattern":"*","path":""}}),
        json!({"kind":"result","wrongComponent":true,"result":{"content":[]}}),
        json!({"kind":"result","result":{"content":[{"type":"text","text":"after failure"}]}}),
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-render-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cases":[steps]}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let definition = create_grep_tool_definition("/repo", Default::default());
    let call_renderer = definition.render_call.unwrap();
    let result_renderer = definition.render_result.unwrap();
    let state = ToolRenderState::new();
    let mut call = None;
    let mut result = None;
    let mut failures = Vec::new();
    for (index, mut step) in steps.into_iter().enumerate() {
        let is_call = step["kind"].as_string().unwrap() == "call";
        if let Some(number) = step.get("number").and_then(JsValue::as_string) {
            let value = match number.to_utf8().unwrap().as_str() {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                _ => unreachable!(),
            };
            if is_call {
                step["args"]
                    .as_object_mut()
                    .unwrap()
                    .insert("limit".into(), JsValue::Number(value));
            } else {
                step["result"].as_object_mut().unwrap().insert(
                    "details".into(),
                    JsValue::Object(pi_js_compat::js_json::JsObject::from([(
                        "matchLimitReached".into(),
                        JsValue::Number(value),
                    )])),
                );
            }
        }
        let previous = if is_call { call.clone() } else { result.clone() };
        let expanded = step["expanded"] == JsValue::Bool(true);
        let context = ToolRenderContext {
            args: step["args"].clone(),
            tool_call_id: "grep-sequence".into(),
            invalidate: Arc::new(|| {}),
            last_component: if step["wrongComponent"] == JsValue::Bool(true) {
                Some(std::rc::Rc::new(std::cell::RefCell::new(pi_tui::Container::new())))
            } else {
                previous.clone()
            },
            call_component: None,
            state: state.clone(),
            cwd: "/repo".into(),
            execution_started: true,
            args_complete: true,
            is_partial: false,
            expanded,
            show_images: step["showImages"] == JsValue::Bool(true),
            is_error: step["isError"] != JsValue::Bool(false),
        };
        let rendered = if is_call {
            call_renderer(&step["args"], &theme, &context)
        } else {
            let mut value: AgentToolResult = js_json::from_value(&step["result"]).unwrap();
            // These are direct callback inputs, not JSON-wire projections of nonfinite numbers.
            value.details = step["result"].get("details").cloned();
            result_renderer(
                &value,
                &ToolRenderResultOptions {
                    expanded,
                    is_partial: false,
                },
                &theme,
                &context,
            )
        };
        if let Err(error) = &rendered {
            let native = std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
                .expect("native error identity");
            let actual = json!({"error":{"name":native.name.clone(),"message":error.message.clone()}});
            if actual != expected["output"][0][index] {
                failures.push(format!("frame {index}: unexpected native error {actual:?}"));
            }
            continue;
        }
        let component = rendered.unwrap();
        let reused = previous
            .as_ref()
            .is_some_and(|old| std::rc::Rc::ptr_eq(old, &component));
        let lines = component.borrow_mut().render(100);
        if is_call {
            call = Some(component);
        } else {
            result = Some(component);
        }
        let actual = json!({"lines":lines,"reused":reused});
        if actual != expected["output"][0][index] {
            failures.push(format!(
                "step {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][0][index])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
