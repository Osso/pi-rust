use crate::shared_oracle as oracle;
pub use crate::shared_render_owner as render_owner;

use pi_agent_core::agent::AgentToolResult;
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::edit::create_edit_tool_definition;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_tui::tui::ComponentRef;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

fn compare_edit_histories(selection: &[usize]) {
    let theme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(theme_path.to_str().unwrap()),
            Some(ColorMode::Truecolor),
        )
        .unwrap(),
    );
    set_theme_instance(theme.clone());
    let cases = [
        json!({"steps":[
            {"kind":"result","result":{"content":[],"details":{"diff":"-1 before\n+1 after","patch":"@@"}}},
            {"kind":"result","result":{"content":[]}},
            {"kind":"result","isError":true,"result":{"content":[{"type":"text","text":"error body"}]}},
            {"kind":"result","result":{"content":[]}},
        ]}),
        json!({"steps":[
            {"kind":"call","complete":false,"args":{"path":"file.txt","edits":[{"oldText":"alpha","newText":"ALPHA"}]}},
            {"kind":"result","result":{"content":[],"details":{"diff":"-1 forced\n+1 result","patch":"@@","firstChangedLine":1}}},
            {"kind":"result","result":{"content":[],"details":{"diff":"","patch":"@@"}}},
            {"kind":"result","isError":true,"result":{"content":[{"type":"text","text":"settled error"}]}},
        ]}),
        json!({"steps":[
            {"kind":"call","args":{"path":"file.txt","edits":[{"oldText":"alpha","newText":"ALPHA"}]}},
            {"kind":"settle"},
            {"kind":"call","args":{"path":"file.txt","file_path":"display-only.txt","edits":[{"oldText":"alpha","newText":"ALPHA"}]}},
            {"kind":"call","args":{"path":"missing.txt","oldText":"alpha","newText":"ALPHA"}},
            {"kind":"settle"},
            {"kind":"result","isError":true,"result":{"content":[{"type":"text","text":"Could not edit file: missing.txt. Error code: ENOENT."}]}},
        ]}),
        json!({"steps":[
            {"kind":"call","args":{"path":"","file_path":"file.txt","edits":[{"oldText":"alpha","newText":"ALPHA"}]}},
            {"kind":"quiet"},
        ]}),
        json!({"steps":[
            {"kind":"call","args":{"path":"file.txt","edits":[{"oldText":"alpha","newText":"obsolete"}]}},
            {"kind":"call","args":{"path":"file.txt","edits":[{"oldText":"alpha","newText":"current"}]}},
            {"kind":"settle"},
            {"kind":"quiet"},
        ]}),
        json!({"steps":[
            {"kind":"call","args":{"path":"file.txt","edits":[{"oldText":"alpha","newText":"ALPHA"}]}},
            {"kind":"result","result":{"content":[],"details":{"diff":"-1 temporary\n+1 settled","patch":"@@","firstChangedLine":1}}},
            {"kind":"settle"},
        ]}),
        json!({"steps":[
            {"kind":"call","args":{"path":"file.txt","edits":[{"oldText":"alpha","newText":"ALPHA","extra":"one"}]}},
            {"kind":"settle"},
            {"kind":"call","args":{"path":"file.txt","edits":[{"extra":"one","oldText":"alpha","newText":"ALPHA"}]}},
            {"kind":"settle"},
        ]}),
        json!({"steps":[
            {"kind":"call","args":{"path":"file.txt","edits":[{"oldText":42,"newText":"ALPHA"}]}},
            {"kind":"quiet"},
            {"kind":"call","args":{"path":"file.txt","edits":"not an array","oldText":"alpha","newText":"ALPHA"}},
            {"kind":"settle"},
        ]}),
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "edit-render-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
        &[("COLORTERM", "truecolor".into())],
    );
    let mut failures = Vec::new();
    for (case_index, input) in cases.iter().enumerate() {
        if !selection.contains(&case_index) {
            continue;
        }
        let source_root = tempfile::tempdir().unwrap();
        let native_root = tempfile::tempdir().unwrap();
        for root in [&source_root, &native_root] {
            std::fs::write(root.path().join("file.txt"), "alpha\nbeta\n").unwrap();
        }
        let mut request = input.clone();
        request
            .as_object_mut()
            .unwrap()
            .insert("root".into(), json!(source_root.path()));
        let expected: JsValue = source.request(&json!({"id":case_index,"cases":[request]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let definition = create_edit_tool_definition(native_root.path().to_str().unwrap(), Default::default());
        let render_call = definition.render_call.unwrap();
        let render_result = definition.render_result.unwrap();
        let state = ToolRenderState::new();
        let owner = render_owner::RenderOwner::new();
        owner.bind(&state);
        let invalidations = Arc::new(AtomicUsize::new(0));
        let counter = invalidations.clone();
        let invalidate: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let mut call: Option<ComponentRef> = None;
        let mut result: Option<ComponentRef> = None;
        let mut args = JsValue::Null;
        let mut complete = true;
        let mut consumed = 0;
        for (step_index, step) in input["steps"].as_array().unwrap().iter().enumerate() {
            let kind = step["kind"].as_string().unwrap().to_utf8().unwrap();
            if kind == "quiet" {
                owner.yield_for(Duration::from_millis(250));
            }
            if kind == "settle" {
                owner.wait_for(|| invalidations.load(Ordering::SeqCst) > consumed);
                consumed = invalidations.load(Ordering::SeqCst);
            }
            if kind == "call" {
                args = step["args"].clone();
                complete = step["complete"].as_bool().unwrap_or(true);
            }
            let context = ToolRenderContext {
                args: args.clone(),
                tool_call_id: "edit-sequence".into(),
                invalidate: invalidate.clone(),
                last_component: if kind == "result" { result.clone() } else { call.clone() },
                call_component: call.clone(),
                state: state.clone(),
                cwd: native_root.path().to_str().unwrap().into(),
                execution_started: true,
                args_complete: complete,
                is_partial: false,
                expanded: false,
                show_images: false,
                is_error: step["isError"] == JsValue::Bool(true),
            };
            let reused = if kind == "result" {
                let value: AgentToolResult = js_json::from_value(&step["result"]).unwrap();
                let next = render_result(
                    &value,
                    &ToolRenderResultOptions {
                        expanded: false,
                        is_partial: false,
                    },
                    &theme,
                    &context,
                )
                .unwrap();
                let reused = result.as_ref().is_some_and(|old| std::rc::Rc::ptr_eq(old, &next));
                result = Some(next);
                json!(reused)
            } else {
                call = Some(render_call(&args, &theme, &context).unwrap());
                JsValue::Null
            };
            let call_lines = call.as_ref().map(|c| c.borrow_mut().render(80));
            let result_lines = result.as_ref().map(|c| c.borrow_mut().render(80));
            let actual = json!({"call":call_lines,"result":result_lines,"reused":reused,"invalidations":invalidations.load(Ordering::SeqCst)});
            if actual != expected["output"][0][step_index] {
                failures.push(format!(
                    "case {case_index} step {step_index}: {} != {}",
                    js_json::stringify_lossless(&actual),
                    js_json::stringify_lossless(&expected["output"][0][step_index])
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn edit_result_history_matches_frozen_container_and_call_mutation() {
    compare_edit_histories(&[0, 1]);
}

#[test]
fn empty_edit_preview_path_does_not_start_io_or_invalidate() {
    compare_edit_histories(&[3]);
}

#[test]
fn edit_async_preview_history_matches_frozen_owner_loop_ordering() {
    compare_edit_histories(&[2, 4, 5, 6, 7]);
}
