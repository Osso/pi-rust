//! Behavioral tests for the edit on-disk preview and the bash elapsed-time state,
//! both of which keep mutable state across `renderCall`/`renderResult` calls.

#[path = "support/render_owner.rs"]
pub mod render_owner;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::bash::render_call as bash_render_call;
use pi_coding_agent::core::tools::edit::{
    create_edit_tool_definition, render_call as edit_render_call, render_result as edit_render_result,
};
use pi_coding_agent::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, Theme, ThemeBg, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_tui::Component;
use pi_tui::utils::strip_terminal_sequences;

fn theme() -> Arc<Theme> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("utf-8 theme path")),
            Some(ColorMode::Truecolor),
        )
        .expect("built-in dark theme"),
    );
    set_theme_instance(Arc::clone(&theme));
    theme
}

fn context(args: Value, cwd: &str, state: ToolRenderState) -> ToolRenderContext {
    ToolRenderContext {
        args,
        tool_call_id: "preview-call".into(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state,
        cwd: cwd.into(),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: true,
        is_error: false,
    }
}

fn plain(lines: &[JsString]) -> JsString {
    JsString::join(lines.iter().map(strip_terminal_sequences), "\n")
}

fn wait_for_preview(state: &ToolRenderState, owner: &render_owner::RenderOwner) -> Value {
    owner.wait_for(|| state.get().get("preview").is_some());
    state.get()["preview"].clone()
}

fn result(content: impl Into<JsString>, details: Option<Value>, is_error: bool) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(content))],
        details,
        is_error: Some(is_error),
        terminate: None,
    }
}

#[test]
fn edit_call_renders_the_pending_header_then_the_on_disk_preview() {
    let theme = theme();
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("note.txt"), "hello world\n").expect("fixture");
    let state = ToolRenderState::new();
    let owner = render_owner::RenderOwner::new();
    owner.bind(&state);
    let args = json!({"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]});
    let context = context(
        args.clone(),
        temp.path().to_str().expect("utf-8 test path"),
        state.clone(),
    );

    let pending = edit_render_call(&args, &theme, &context).expect("call renderer");
    let pending_lines = pending.borrow_mut().render(80);
    assert_eq!(plain(&pending_lines).trim(), "edit note.txt");
    assert!(
        pending_lines
            .iter()
            .any(|line| line.contains(theme.get_bg_ansi(ThemeBg::ToolPendingBg)))
    );

    let preview = wait_for_preview(&state, &owner);
    let preview_diff = preview["diff"].as_string().expect("diff").to_owned();

    let rendered = edit_render_call(&args, &theme, &context).expect("call renderer");
    let rendered_lines = rendered.borrow_mut().render(80);
    let text = plain(&rendered_lines);
    assert!(text.contains("edit note.txt"), "{text:?}");
    assert!(text.contains("-1 hello world"), "{text:?}");
    assert!(text.contains("+1 goodbye world"), "{text:?}");
    assert!(
        rendered_lines
            .iter()
            .any(|line| line.contains(theme.get_bg_ansi(ThemeBg::ToolSuccessBg)))
    );

    let result = result(
        "Successfully replaced 1 block(s) in note.txt.",
        Some(json!({"diff": preview_diff, "patch": "@@", "firstChangedLine": 1})),
        false,
    );
    let mut result_context = context.clone();
    result_context.call_component = Some(Rc::clone(&rendered));
    let result_component = edit_render_result(
        &result,
        &ToolRenderResultOptions {
            expanded: false,
            is_partial: false,
        },
        &theme,
        &result_context,
    )
    .expect("result renderer");
    assert!(result_component.borrow_mut().render(80).is_empty());
}

#[test]
fn edit_preview_errors_render_in_the_call_header_and_suppress_duplicate_result_text() {
    let theme = theme();
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("note.txt"), "hello world\n").expect("fixture");
    let state = ToolRenderState::new();
    let owner = render_owner::RenderOwner::new();
    owner.bind(&state);
    let args = json!({"path": "note.txt", "edits": [{"oldText": "missing", "newText": "x"}]});
    let context = context(
        args.clone(),
        temp.path().to_str().expect("utf-8 test path"),
        state.clone(),
    );

    let _ = edit_render_call(&args, &theme, &context).expect("call renderer");
    let preview = wait_for_preview(&state, &owner);
    let preview_error = preview["error"].as_string().expect("error").to_owned();

    let rendered = edit_render_call(&args, &theme, &context).expect("call renderer");
    let rendered_lines = rendered.borrow_mut().render(80);
    let rendered_text = plain(&rendered_lines);
    assert!(
        rendered_text.contains("Could not find the exact text in note.txt."),
        "{rendered_lines:?}"
    );
    assert!(
        rendered_text.contains("including all whitespace and newlines."),
        "{rendered_lines:?}"
    );
    assert!(
        rendered_lines
            .iter()
            .any(|line| line.contains(theme.get_bg_ansi(ThemeBg::ToolErrorBg)))
    );

    let mut error_context = context.clone();
    error_context.is_error = true;
    error_context.call_component = Some(Rc::clone(&rendered));
    let result = result(&preview_error, None, true);
    let result_component = edit_render_result(
        &result,
        &ToolRenderResultOptions {
            expanded: false,
            is_partial: false,
        },
        &theme,
        &error_context,
    )
    .expect("result renderer");
    assert!(result_component.borrow_mut().render(80).is_empty());
}

#[test]
fn edit_args_change_discards_the_previous_preview() {
    let theme = theme();
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("note.txt"), "hello world\n").expect("fixture");
    let state = ToolRenderState::new();
    let owner = render_owner::RenderOwner::new();
    owner.bind(&state);
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    let first_args = json!({"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]});
    let first_context = context(first_args.clone(), &cwd, state.clone());
    let _ = edit_render_call(&first_args, &theme, &first_context).expect("call renderer");
    wait_for_preview(&state, &owner);

    let second_args = json!({"path": "note.txt", "edits": [{"oldText": "world", "newText": "earth"}]});
    let second_context = context(second_args.clone(), &cwd, state.clone());
    let reset = edit_render_call(&second_args, &theme, &second_context).expect("call renderer");
    let reset_text = plain(&reset.borrow_mut().render(80));
    assert_eq!(reset_text.trim(), "edit note.txt");

    let preview = wait_for_preview(&state, &owner);
    assert!(preview["diff"].as_string().expect("diff").contains("+1 hello earth"));
}

#[test]
fn edit_settled_result_reconstructs_the_preview_without_complete_args() {
    let theme = theme();
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("note.txt"), "hello world\n").expect("fixture");
    let state = ToolRenderState::new();
    let args = json!({"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]});
    let mut context = context(
        args.clone(),
        temp.path().to_str().expect("utf-8 test path"),
        state.clone(),
    );
    context.args_complete = false;

    let pending = edit_render_call(&args, &theme, &context).expect("call renderer");
    assert_eq!(plain(&pending.borrow_mut().render(80)).trim(), "edit note.txt");
    let mut result_context = context.clone();
    result_context.call_component = Some(Rc::clone(&pending));

    let diff = "@@ -1,1 +1,1 @@\n-1 hello world\n+1 goodbye world";
    let result = result(
        "Successfully replaced 1 block(s) in note.txt.",
        Some(json!({"diff": diff, "firstChangedLine": 1})),
        false,
    );
    let result_component = edit_render_result(
        &result,
        &ToolRenderResultOptions {
            expanded: false,
            is_partial: false,
        },
        &theme,
        &result_context,
    )
    .expect("result renderer");
    assert!(result_component.borrow_mut().render(80).is_empty());

    let settled = edit_render_call(&args, &theme, &context).expect("call renderer");
    let text = plain(&settled.borrow_mut().render(80));
    assert!(text.contains("-1 hello world"), "{text:?}");
    assert!(text.contains("+1 goodbye world"), "{text:?}");
}

#[test]
fn edit_call_falls_back_to_path_for_a_null_file_path() {
    let theme = theme();
    let state = ToolRenderState::new();
    let mut context = context(json!({}), "/repo", state);
    context.args_complete = false;

    let args = json!({"file_path": "notes.txt", "edits": [{"oldText": "a", "newText": "b"}]});
    let rendered = edit_render_call(&args, &theme, &context).expect("call renderer");
    assert_eq!(plain(&rendered.borrow_mut().render(80)).trim(), "edit notes.txt");

    let args = json!({"file_path": null, "path": "notes.txt", "edits": [{"oldText": "a", "newText": "b"}]});
    let rendered = edit_render_call(&args, &theme, &context).expect("call renderer");
    assert_eq!(plain(&rendered.borrow_mut().render(80)).trim(), "edit notes.txt");
}

#[test]
fn bash_call_records_the_start_and_result_records_the_end() {
    let theme = theme();
    let state = ToolRenderState::new();
    let args = json!({"command": "echo hi"});
    let mut context = context(args.clone(), "/repo", state.clone());
    context.execution_started = true;

    bash_render_call(&args, &theme, &context).expect("bash renderer");
    let started_at = state.get()["startedAt"].as_f64().expect("startedAt recorded");

    std::thread::sleep(Duration::from_millis(1100));
    let partial = pi_coding_agent::core::tools::bash::render_result(
        &result("hi", None, false),
        &ToolRenderResultOptions {
            expanded: true,
            is_partial: true,
        },
        &theme,
        &context,
    )
    .expect("bash renderer");
    let partial_text = plain(&partial.borrow_mut().render(80));
    assert!(partial_text.contains("Elapsed "), "{partial_text:?}");
    assert!(state.get().get("endedAt").is_none());

    let final_result = pi_coding_agent::core::tools::bash::render_result(
        &result("hi", None, false),
        &ToolRenderResultOptions {
            expanded: true,
            is_partial: false,
        },
        &theme,
        &context,
    )
    .expect("bash renderer");
    let ended_at = state.get()["endedAt"].as_f64().expect("endedAt recorded");
    assert!(ended_at >= started_at + 1000.0);
    let final_text = plain(&final_result.borrow_mut().render(80));
    assert!(final_text.contains("Took "), "{final_text:?}");
}

#[test]
fn edit_settled_result_rebuilds_the_rendered_call_component() {
    let _theme = theme();
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    std::fs::write(temp.path().join("note.txt"), "hello world\n").expect("fixture");
    let args = json!({"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]});
    let definition = create_edit_tool_definition(JsString::from(&cwd), Default::default());
    let component = ToolExecutionComponent::new(
        "edit",
        "edit-call",
        args,
        ToolExecutionOptions::default(),
        Some(definition),
        Arc::new(|| {}),
        JsString::from(&cwd),
    );
    let mut component = component.borrow_mut();

    let pending = plain(&component.render(80));
    assert_eq!(pending.trim(), "edit note.txt");

    let diff = "@@ -1,1 +1,1 @@\n-1 hello world\n+1 goodbye world";
    component.update_result(
        ToolExecutionResult {
            content: vec![TextOrImageContent::Text(TextContent::new(
                "Successfully replaced 1 block(s) in note.txt.".to_owned(),
            ))],
            details: Some(json!({"diff": diff, "firstChangedLine": 1})),
            is_error: false,
        },
        false,
        None,
    );

    let settled = plain(&component.render(80));
    assert!(settled.contains("-1 hello world"), "{settled:?}");
    assert!(settled.contains("+1 goodbye world"), "{settled:?}");
    assert!(!settled.contains("Successfully replaced"), "{settled:?}");
}
