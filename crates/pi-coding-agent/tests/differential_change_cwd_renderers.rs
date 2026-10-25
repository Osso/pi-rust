use crate::shared_oracle as oracle;

use std::{cell::RefCell, error::Error, rc::Rc, sync::Arc};

use pi_coding_agent::core::extensions::types::ToolDefinition;
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::change_working_directory::create_change_working_directory_tool_definition;
use pi_coding_agent::core::tools::channel_post::create_channel_post_tool_definition;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue};
use pi_tui::{Spacer, tui::ComponentRef};

#[test]
fn complete_definition_matches_frozen_schema_metadata_and_execution_mode() {
    check_definition(
        "change-working-directory.ts",
        "createChangeWorkingDirectoryToolDefinition",
        create_change_working_directory_tool_definition(),
    );
}

#[test]
fn end_turn_complete_definition_matches_frozen() {
    check_definition(
        "end-turn.ts",
        "createEndTurnToolDefinition",
        pi_coding_agent::core::tools::end_turn::create_end_turn_tool_definition(),
    );
}

#[test]
fn resume_session_complete_definition_matches_frozen() {
    check_definition(
        "resume-session.ts",
        "createResumeSessionToolDefinition",
        pi_coding_agent::core::tools::resume_session::create_resume_session_tool_definition(),
    );
}

#[test]
fn resume_session_renderer_history_preserves_frozen_reuse_and_errors() {
    check_renderer_history(
        "resume-session.ts",
        "createResumeSessionToolDefinition",
        pi_coding_agent::core::tools::resume_session::create_resume_session_tool_definition(),
    );
}

#[test]
fn ask_questions_complete_definition_matches_frozen() {
    check_definition(
        "ask-questions.ts",
        "createAskQuestionsToolDefinition",
        pi_coding_agent::core::tools::ask_questions::create_ask_questions_tool_definition(),
    );
}

#[test]
fn ask_supervisor_complete_definition_matches_frozen() {
    check_definition(
        "ask-supervisor.ts",
        "createAskSupervisorToolDefinition",
        pi_coding_agent::core::tools::ask_supervisor::create_ask_supervisor_tool_definition(),
    );
}

#[test]
fn ask_supervisor_renderer_history_preserves_frozen_reuse_and_errors() {
    check_renderer_history(
        "ask-supervisor.ts",
        "createAskSupervisorToolDefinition",
        pi_coding_agent::core::tools::ask_supervisor::create_ask_supervisor_tool_definition(),
    );
}

#[test]
fn channel_post_complete_definition_matches_frozen() {
    check_definition(
        "channel-post.ts",
        "createChannelPostToolDefinition",
        create_channel_post_tool_definition(),
    );
}

fn check_definition(module: &str, factory: &str, definition: ToolDefinition) {
    let mut oracle = oracle::TsOracle::spawn(
        "coordination-render-sequence-oracle.ts",
        &format!("packages/coding-agent/src/core/tools/{module}"),
    );
    let expected = oracle.request(&json!({"id": 2, "operation": "definition", "factory": factory}));
    assert_eq!(expected["ok"], JsValue::Bool(true));
    let mode = definition.execution_mode.map(|mode| match mode {
        pi_agent_core::ToolExecutionMode::Sequential => "sequential",
        pi_agent_core::ToolExecutionMode::Parallel => "parallel",
    });
    let mut actual = json!({
        "name": definition.name, "label": definition.label, "description": definition.description,
        "promptSnippet": definition.prompt_snippet, "promptGuidelines": definition.prompt_guidelines,
        "parameters": definition.parameters, "executionMode": mode,
    });
    if let Some(required) = definition.approval_required {
        actual["approvalRequired"] = json!(required);
    }
    assert_eq!(actual, expected["output"]);
}

#[test]
fn renderer_reuse_keeps_frozen_frames_and_incompatible_component_errors() {
    check_renderer_history(
        "change-working-directory.ts",
        "createChangeWorkingDirectoryToolDefinition",
        create_change_working_directory_tool_definition(),
    );
}

#[test]
fn channel_post_renderer_history_preserves_frozen_reuse_and_errors() {
    check_renderer_history(
        "channel-post.ts",
        "createChannelPostToolDefinition",
        create_channel_post_tool_definition(),
    );
}

fn check_renderer_history(module: &str, factory: &str, definition: ToolDefinition) {
    let raw = pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]);
    let cases = json!([
        {"args": {"path": "/repo/first"}},
        {"args": {"id": pi_js_compat::js_json::JsString::from("session-") + &raw}},
        {"args": {"path": "", "id": "second"}},
        {"args": {}},
        {"result": {"content": [{"type": "text", "text": pi_js_compat::js_json::JsString::from("first") + &raw}, {"type": "image", "data": "AA==", "mimeType": "image/png"}, {"type": "text", "text": "last"}]}},
        {"result": {"content": [{"type": "text", "text": "failed"}], "isError": true}},
        {"result": {"content": []}},
        {"args": {"path": "/repo/narrow"}, "width": 12},
        {"args": {"path": "/repo/incompatible"}, "incompatible": true},
        {"result": {"content": []}, "incompatible": true},
        {"args": {"id": "reset"}, "reset": true},
        {"args": {"name": "named", "starter_prompt": pi_js_compat::js_json::JsString::from("instructions-") + &raw}},
        {"args": {"path": "", "id": "", "name": "named", "starter_prompt": " "}},
        {"args": null}
    ]);
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "coordination-render-sequence-oracle.ts",
        &format!("packages/coding-agent/src/core/tools/{module}"),
        &[("COLORTERM", "truecolor".into())],
    );
    let response = oracle.request(&json!({"id": 1, "cases": cases, "factory": factory}));
    assert_eq!(response["ok"], json!(true), "{response:?}");
    // Frozen change-working-directory.ts:141-150 and channel-post.ts:58-73: reject incompatible prior Text.
    assert_eq!(response["output"][8]["name"], json!("TypeError"));
    assert_eq!(
        response["output"][8]["message"],
        json!("text.setText is not a function")
    );
    assert_eq!(response["output"][9]["name"], json!("TypeError"));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = load_theme_from_path(
        &pi_js_compat::node_fs::decode_node_path(path.as_os_str()),
        Some(ColorMode::Truecolor),
    )
    .unwrap();
    let mut previous: Option<ComponentRef> = None;
    let actual: Vec<JsValue> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|step| {
            if step["reset"].as_bool() == Some(true) {
                previous = None;
            }
            let last_component = if step["incompatible"].as_bool() == Some(true) {
                Some(Rc::new(RefCell::new(Spacer::new(1))) as ComponentRef)
            } else {
                previous.clone()
            };
            let context = ToolRenderContext {
                args: step["args"].clone(),
                tool_call_id: "cwd-render".into(),
                invalidate: Arc::new(|| {}),
                last_component,
                call_component: None,
                state: ToolRenderState::new(),
                cwd: "/repo".into(),
                execution_started: true,
                args_complete: true,
                is_partial: false,
                expanded: true,
                show_images: false,
                is_error: false,
            };
            let result = if let Some(result) = step.get("result") {
                let result = js_json::from_value(result).unwrap();
                definition.render_result.as_ref().unwrap()(
                    &result,
                    &ToolRenderResultOptions {
                        expanded: true,
                        is_partial: false,
                    },
                    &theme,
                    &context,
                )
            } else {
                definition.render_call.as_ref().unwrap()(&step["args"], &theme, &context)
            };
            match result {
                Ok(component) => {
                    let reused = previous.as_ref().is_some_and(|last| Rc::ptr_eq(last, &component));
                    let width = step["width"].as_u64().unwrap_or(100) as usize;
                    let lines = component.borrow_mut().render(width);
                    previous = Some(component);
                    json!({"ok": true, "reused": reused, "lines": lines})
                }
                Err(error) => {
                    let native = error
                        .source()
                        .and_then(|source| source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>())
                        .expect("renderer TypeError retains native identity");
                    json!({"ok": false, "name": native.name, "message": native.message})
                }
            }
        })
        .collect();
    for (index, frame) in actual.iter().enumerate() {
        assert!(
            frame == &response["output"][index],
            "frame {index}: {} != {}",
            js_json::stringify_lossless(frame),
            js_json::stringify_lossless(&response["output"][index])
        );
    }
}
