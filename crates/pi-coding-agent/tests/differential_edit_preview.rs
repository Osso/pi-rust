use crate::shared_oracle as oracle;
pub use crate::shared_render_owner as render_owner;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::edit::{render_call, render_result};
use pi_coding_agent::modes::interactive::theme::{ColorMode, Theme, load_theme_from_path, set_theme_instance};
use serde_json::{Value, json};

fn dark_theme() -> Arc<Theme> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            Some(ColorMode::Truecolor),
        )
        .expect("built-in dark theme"),
    );
    set_theme_instance(Arc::clone(&theme));
    theme
}

fn context(args: Value, cwd: &str, state: ToolRenderState) -> ToolRenderContext {
    ToolRenderContext {
        args: JsValue::from(args),
        tool_call_id: "edit-preview-differential".to_owned(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state,
        cwd: JsString::from(cwd.to_owned()),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: true,
        is_error: false,
    }
}

fn wait_for_preview(state: &ToolRenderState, owner: &render_owner::RenderOwner) {
    owner.wait_for(|| state.get().get("preview").is_some());
}

fn agent_result(value: &Value) -> AgentToolResult {
    AgentToolResult {
        content: value["content"]
            .as_array()
            .expect("content")
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .map(|text| TextOrImageContent::Text(TextContent::new(text.to_owned())))
            .collect(),
        details: value.get("details").cloned().map(JsValue::from),
        is_error: None,
        terminate: None,
    }
}

#[test]
fn edit_preview_renderers_match_the_pinned_typescript_source() {
    let theme = dark_theme();
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("note.txt"), "hello world\n").expect("fixture");
    std::fs::write(temp.path().join("crlf.txt"), "alpha\r\nbeta\r\ngamma\r\n").expect("fixture");
    let cwd = temp.path().to_string_lossy().into_owned();

    let cases = vec![
        json!({
            "cwd": cwd,
            "args": {"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]},
        }),
        json!({
            "cwd": cwd,
            "args": {"path": "crlf.txt", "edits": [{"oldText": "beta", "newText": "delta"}]},
        }),
        json!({
            "cwd": cwd,
            "args": {"path": "missing.txt", "edits": [{"oldText": "a", "newText": "b"}]},
        }),
        json!({
            "cwd": cwd,
            "args": {"path": "note.txt", "edits": [{"oldText": "absent", "newText": "b"}]},
        }),
        json!({
            "cwd": cwd,
            "args": {"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]},
            "result": {
                "content": [{"type": "text", "text": "Successfully replaced 1 block(s) in note.txt."}],
                "details": {"diff": "@@ -1,1 +1,1 @@\n-hello world\n+goodbye world", "firstChangedLine": 1},
            },
        }),
        json!({
            "cwd": cwd,
            "args": {"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]},
            "result": {"content": [{"type": "text", "text": "a different failure"}]},
            "isError": true,
        }),
    ];

    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "coding-agent-edit-preview-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
        &[("COLORTERM", std::ffi::OsString::from("truecolor"))],
    );
    let expected = oracle.request(&json!({
        "id": 27_950,
        "operation": "coding_agent_edit_preview",
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());

    for (index, (case, expected)) in cases.iter().zip(expected_results).enumerate() {
        let state = ToolRenderState::new();
        let owner = render_owner::RenderOwner::new();
        owner.bind(&state);
        let args = case["args"].clone();
        let mut context = context(args.clone(), &cwd, state.clone());

        let pending = render_call(&JsValue::from(args.clone()), &theme, &context).expect("call renderer");
        let pending_lines = pending.borrow_mut().render(80);
        assert_eq!(json!(pending_lines), expected["pending"], "pending case {index}");

        wait_for_preview(&state, &owner);
        let call = render_call(&JsValue::from(args.clone()), &theme, &context).expect("call renderer");
        let call_lines = call.borrow_mut().render(80);
        assert_eq!(json!(call_lines), expected["call"], "call case {index}");

        if let Some(result_value) = case.get("result") {
            context.is_error = case.get("isError").and_then(Value::as_bool).unwrap_or(false);
            context.call_component = Some(Rc::clone(&call));
            let component = render_result(
                &agent_result(result_value),
                &ToolRenderResultOptions {
                    expanded: false,
                    is_partial: false,
                },
                &theme,
                &context,
            )
            .expect("result renderer");
            let result_lines = component.borrow_mut().render(80);
            assert_eq!(json!(result_lines), expected["result"], "result case {index}");
        }
    }
}
