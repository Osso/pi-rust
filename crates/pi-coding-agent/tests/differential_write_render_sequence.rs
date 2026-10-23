#[path = "support/oracle.rs"]
mod oracle;

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
            {"args":{"path":"example.constructor","content":"plain"}},
            {"args":{"path":"example.__proto__","content":"plain"}},
            {"args":{"path":"example.ts","content":"const recovered = 1;"}},
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
                expanded: true,
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
