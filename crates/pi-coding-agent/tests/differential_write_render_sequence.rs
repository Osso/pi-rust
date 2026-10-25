use crate::shared_oracle as oracle;

use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderState};
use pi_coding_agent::core::tools::write::render_call;
use pi_coding_agent::modes::interactive::theme::{ColorMode, Theme, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::Arc;

fn dark_theme() -> Arc<Theme> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme =
        Arc::new(load_theme_from_path(&JsString::from(path.to_str().unwrap()), Some(ColorMode::Truecolor)).unwrap());
    set_theme_instance(theme.clone());
    theme
}

#[test]
fn write_streaming_preview_matches_frozen_component_history() {
    let theme = dark_theme();
    let comment = JsString::from("/*\n") + "comment\n".repeat(51) + "const pending";
    let appended = comment.clone() + " = 1;";
    let closed = appended.clone() + "\n*/\nconst after = 2;";
    let sequences = vec![
        json!({"steps":[
            {"args":{"path":"example.ts","content":comment}},
            {"args":{"path":"example.ts","content":appended}},
            {"args":{"path":"example.ts","content":appended},"complete":true},
            {"args":{"path":"example.ts","content":closed}},
            {"args":{"path":"other.ts","content":closed}},
            {"args":{"path":"other.js","content":"const reset = 1;\n\n"}},
        ]}),
        json!({"steps":[
            {"args":{"path":"example.ts","content":"const prefix = 1;\n"}},
            {"args":{"path":"example.ts","content":"const prefix = 1;\nconst next = 2;"}},
            {"args":{"path":"example.ts","content":"replacement()"}},
            {"args":{"path":"example.ts","content":false}},
            {"args":{"path":"example.ts","content":""}},
            {"args":{"path":"example.ts","content":"const restarted = 3;"}},
            {"args":{"path":"example.unknown","content":"\tplain\n\n"}},
        ]}),
        json!({"steps":[
            {"args":{"path":"long.txt","content":"line\n".repeat(12)},"expanded":false},
            {"args":{"file_path":"long.ts","path":"ignored.txt","content":"const value = 1;\n".repeat(12)},"expanded":false},
            {"args":{"file_path":"long.ts","content":"const value = 1;\n".repeat(12)},"expanded":true},
            {"args":{"file_path":null,"path":"alias.rs","content":"\tfn main() {}\r\n"}},
            {"args":{"file_path":false,"path":"ignored.ts","content":"plain\ttext\n\n"}},
            {"args":null},
            {"args":{"path":"empty.txt","content":"\n\n"}},
            {"args":{"path":"line.ts","content":"const value = 1;\r"}},
            {"args":{"path":"line.ts","content":"const value = 1;\r\nnext();"}},
            {"args":{"path":"line.ts","content":"const value = 1;\r\nnext();"}},
        ]}),
    ];
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "write-render-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let expected: JsValue = oracle.request(&json!({"id":1,"cases":sequences}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    for (sequence_index, sequence) in sequences.iter().enumerate() {
        let mut previous = None;
        let state = ToolRenderState::new();
        for (step_index, step) in sequence["steps"].as_array().unwrap().iter().enumerate() {
            let context = ToolRenderContext {
                args: step["args"].clone(),
                tool_call_id: "write-sequence".into(),
                invalidate: Arc::new(|| {}),
                last_component: previous.clone(),
                call_component: None,
                state: state.clone(),
                cwd: "/repo".into(),
                execution_started: false,
                args_complete: step["complete"] == JsValue::Bool(true),
                is_partial: true,
                expanded: step["expanded"].as_bool().unwrap_or(true),
                show_images: false,
                is_error: false,
            };
            let actual = match render_call(&step["args"], &theme, &context) {
                Ok(component) => {
                    let lines = component.borrow_mut().render(120);
                    previous = Some(component);
                    json!({"ok":true,"lines":lines})
                }
                Err(error) => {
                    use std::error::Error;
                    let native = error
                        .source()
                        .and_then(|source| source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>())
                        .expect("original native renderer error");
                    json!({"ok":false,"error":error.message,"constructor":native.constructor_class().name(),"text":native.string_value()})
                }
            };
            let expected_step = &expected["output"][sequence_index][step_index];
            assert!(
                actual == *expected_step,
                "sequence {sequence_index}, step {step_index}: expected {}",
                pi_js_compat::js_json::stringify_lossless(expected_step)
            );
        }
    }
}

#[test]
fn write_result_components_reuse_and_reject_cross_kind_history_like_frozen() {
    use pi_agent_core::agent::AgentToolResult;
    use pi_coding_agent::core::tools::write::{WriteToolOptions, create_write_tool_definition};
    use pi_js_compat::js_json;
    use std::error::Error;
    let theme = dark_theme();
    let sequences = [json!({"mode":"result","steps":[
        {"result":{"content":[]}},
        {"result":{"content":[{"type":"text","text":"hidden success"}]}},
        {"isError":true,"result":{"content":[{"type":"text","text":"first failure"}]}},
        {"reset":true,"isError":true,"result":{"content":[{"type":"text","text":"first failure"}]}},
        {"isError":true,"result":{"content":[{"type":"text","text":JsString::from_utf16(vec![0xd800])+" second failure"}]}},
        {"isError":true,"result":{"content":[]}},
        {"result":{"content":[]}},
        {"reset":true,"isError":true,"result":{"content":[{"type":"image","data":"AA==","mimeType":"image/png"}]}},
        {"isError":true,"result":{"content":[{"type":"text","text":""}]}},
        {"reset":true,"isError":true,"result":{"content":[{"type":"text","text":""},{"type":"text","text":""}]}},
        {"isError":true,"result":{"content":[{"type":"text","text":"a"},{"type":"image","data":"AA==","mimeType":"image/png"},{"type":"text","text":"b"}]}},
    ]})];
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "write-render-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let expected: JsValue = oracle.request(&json!({"id":2,"cases":sequences}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let definition = create_write_tool_definition("/repo", WriteToolOptions::default());
    let renderer = definition.render_result.unwrap();
    let mut previous = None;
    let state = ToolRenderState::new();
    let mut failures = Vec::new();
    for (index, step) in sequences[0]["steps"].as_array().unwrap().iter().enumerate() {
        if step["reset"] == JsValue::Bool(true) {
            previous = None;
        }
        let context = ToolRenderContext {
            args: json!({"path":"example.txt","content":"body"}),
            tool_call_id: "write-result".into(),
            invalidate: Arc::new(|| {}),
            last_component: previous.clone(),
            call_component: None,
            state: state.clone(),
            cwd: "/repo".into(),
            execution_started: true,
            args_complete: true,
            is_partial: true,
            expanded: true,
            show_images: false,
            is_error: step["isError"] == JsValue::Bool(true),
        };
        let result: AgentToolResult = js_json::from_value(&step["result"]).unwrap();
        let options = pi_coding_agent::core::extensions::types::ToolRenderResultOptions {
            expanded: true,
            is_partial: true,
        };
        let actual = match renderer(&result, &options, &theme, &context) {
            Ok(component) => {
                let reused = previous
                    .as_ref()
                    .is_some_and(|last| std::rc::Rc::ptr_eq(last, &component));
                let lines = component.borrow_mut().render(120);
                previous = Some(component);
                json!({"ok":true,"lines":lines,"reused":reused})
            }
            Err(error) => {
                let native = error
                    .source()
                    .and_then(|source| source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>())
                    .expect("original native result error");
                json!({"ok":false,"error":error.message,"constructor":native.constructor_class().name(),"text":native.string_value()})
            }
        };
        if actual != expected["output"][0][index] {
            failures.push(format!(
                "result step {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][0][index])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
