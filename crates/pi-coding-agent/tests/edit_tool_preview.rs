//! Behavioral tests for the edit on-disk preview and the bash elapsed-time state,
//! both of which keep mutable state across `renderCall`/`renderResult` calls.

use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::bash::render_call as bash_render_call;
use pi_coding_agent::core::tools::edit::{render_call as edit_render_call, render_result as edit_render_result};
use pi_coding_agent::modes::interactive::theme::{ColorMode, Theme, ThemeBg, load_theme_from_path, set_theme_instance};
use pi_tui::strip_terminal_sequences;
use serde_json::{Value, json};

fn theme() -> Arc<Theme> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(load_theme_from_path(&path, Some(ColorMode::Truecolor)).expect("built-in dark theme"));
    set_theme_instance(Arc::clone(&theme));
    theme
}

fn context(args: Value, cwd: &str, state: ToolRenderState) -> ToolRenderContext {
    ToolRenderContext {
        args,
        tool_call_id: "preview-call".to_owned(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        state,
        cwd: cwd.to_owned(),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: true,
        is_error: false,
    }
}

fn plain(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn wait_for_preview(state: &ToolRenderState) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = state.get();
        if let Some(preview) = snapshot.get("preview") {
            return preview.clone();
        }
        assert!(Instant::now() < deadline, "preview never completed: {snapshot}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn result(content: &str, details: Option<Value>, is_error: bool) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(content.to_owned()))],
        details,
        usage: None,
        added_tool_names: None,
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
    let args = json!({"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]});
    let context = context(args.clone(), &temp.path().to_string_lossy(), state.clone());

    let pending = edit_render_call(&args, &theme, &context).expect("call renderer");
    let pending_lines = pending.borrow_mut().render(80);
    assert_eq!(plain(&pending_lines).trim(), "edit note.txt");
    assert!(
        pending_lines
            .iter()
            .any(|line| line.contains(theme.get_bg_ansi(ThemeBg::ToolPendingBg)))
    );

    let preview = wait_for_preview(&state);
    let preview_diff = preview["diff"].as_str().expect("diff").to_owned();

    let rendered = edit_render_call(&args, &theme, &context).expect("call renderer");
    let rendered_lines = rendered.borrow_mut().render(80);
    let text = plain(&rendered_lines);
    assert!(text.contains("edit note.txt"), "{text}");
    assert!(text.contains("-1 hello world"), "{text}");
    assert!(text.contains("+1 goodbye world"), "{text}");
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
    let result_component = edit_render_result(
        &result,
        &ToolRenderResultOptions {
            expanded: false,
            is_partial: false,
        },
        &theme,
        &context,
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
    let args = json!({"path": "note.txt", "edits": [{"oldText": "missing", "newText": "x"}]});
    let context = context(args.clone(), &temp.path().to_string_lossy(), state.clone());

    let _ = edit_render_call(&args, &theme, &context).expect("call renderer");
    let preview = wait_for_preview(&state);
    let preview_error = preview["error"].as_str().expect("error").to_owned();

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
    let cwd = temp.path().to_string_lossy().into_owned();
    let first_args = json!({"path": "note.txt", "edits": [{"oldText": "hello", "newText": "goodbye"}]});
    let first_context = context(first_args.clone(), &cwd, state.clone());
    let _ = edit_render_call(&first_args, &theme, &first_context).expect("call renderer");
    wait_for_preview(&state);

    let second_args = json!({"path": "note.txt", "edits": [{"oldText": "world", "newText": "earth"}]});
    let second_context = context(second_args.clone(), &cwd, state.clone());
    let reset = edit_render_call(&second_args, &theme, &second_context).expect("call renderer");
    let reset_text = plain(&reset.borrow_mut().render(80));
    assert_eq!(reset_text.trim(), "edit note.txt");

    let preview = wait_for_preview(&state);
    assert!(preview["diff"].as_str().expect("diff").contains("+1 hello earth"));
}

#[test]
fn bash_call_records_the_start_and_result_records_the_end() {
    let theme = theme();
    let state = ToolRenderState::new();
    let args = json!({"command": "echo hi"});
    let mut context = context(args.clone(), "/repo", state.clone());
    context.execution_started = true;

    let _ = bash_render_call(&args, &theme, &context);
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
    );
    let partial_text = plain(&partial.borrow_mut().render(80));
    assert!(partial_text.contains("Elapsed "), "{partial_text}");
    assert!(state.get().get("endedAt").is_none());

    let final_result = pi_coding_agent::core::tools::bash::render_result(
        &result("hi", None, false),
        &ToolRenderResultOptions {
            expanded: true,
            is_partial: false,
        },
        &theme,
        &context,
    );
    let ended_at = state.get()["endedAt"].as_f64().expect("endedAt recorded");
    assert!(ended_at >= started_at + 1000.0);
    let final_text = plain(&final_result.borrow_mut().render(80));
    assert!(final_text.contains("Took "), "{final_text}");
}
