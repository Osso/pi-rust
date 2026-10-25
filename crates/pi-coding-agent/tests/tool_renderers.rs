use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::Arc;

use pi_coding_agent::core::extensions::types::ToolRenderContext;
use pi_coding_agent::core::tools::bash::{
    BashToolOptions, create_bash_tool_definition, render_call as bash_render_call,
};
use pi_coding_agent::core::tools::code_index::{
    CodeIndexToolOptions, create_outline_tool_definition, create_references_tool_definition,
    create_symbol_tool_definition,
};
use pi_coding_agent::core::tools::edit::{EditToolOptions, create_edit_tool_definition};
use pi_coding_agent::core::tools::find::{FindToolOptions, create_find_tool_definition};
use pi_coding_agent::core::tools::grep::{GrepToolOptions, create_grep_tool_definition};
use pi_coding_agent::core::tools::ls::{LsToolOptions, create_ls_tool_definition};
use pi_coding_agent::core::tools::read::{ReadToolOptions, create_read_tool_definition};
use pi_coding_agent::core::tools::write::{WriteToolOptions, create_write_tool_definition};
use serde_json::json;

fn dark_theme() -> std::sync::Arc<pi_coding_agent::modes::interactive::theme::Theme> {
    use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = std::sync::Arc::new(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            Some(ColorMode::Truecolor),
        )
        .expect("built-in dark theme"),
    );
    set_theme_instance(std::sync::Arc::clone(&theme));
    theme
}

fn tool_title(text: &str) -> String {
    format!("\x1b[38;2;212;212;212m{text}\x1b[39m")
}

fn tool_output(text: &str) -> String {
    format!("\x1b[38;2;128;128;128m{text}\x1b[39m")
}

fn muted(text: &str) -> String {
    format!("\x1b[38;2;128;128;128m{text}\x1b[39m")
}

fn error(text: &str) -> String {
    format!("\x1b[38;2;204;102;102m{text}\x1b[39m")
}

/// Chalk level is plain when stdout is not a TTY, matching the oracle subprocess.
fn bold(text: &str) -> String {
    text.to_owned()
}

fn context() -> ToolRenderContext {
    ToolRenderContext {
        args: pi_js_compat::js_json!({}),
        tool_call_id: "call".to_owned(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: pi_coding_agent::core::extensions::types::ToolRenderState::new(),
        cwd: JsString::from("/repo"),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: true,
        show_images: true,
        is_error: false,
    }
}

#[test]
fn bash_render_call_formats_command_and_timeout() {
    let theme = dark_theme();
    let component = bash_render_call(
        &JsValue::from(json!({"command": "echo hi", "timeout": 5}).clone()),
        &theme,
        &context(),
    )
    .expect("bash renderer");
    let lines = component.borrow_mut().render(80);
    assert_eq!(
        lines[0].trim_end(),
        tool_title(&bold("$ echo hi")) + &muted(" (timeout 5s)")
    );

    let component =
        bash_render_call(&JsValue::from(json!({"command": ""}).clone()), &theme, &context()).expect("bash renderer");
    let lines = component.borrow_mut().render(80);
    assert_eq!(
        lines[0].trim_end(),
        tool_title(&bold(&format!("$ {}", tool_output("..."))))
    );

    let component =
        bash_render_call(&JsValue::from(json!({"command": null}).clone()), &theme, &context()).expect("bash renderer");
    let lines = component.borrow_mut().render(80);
    assert_eq!(
        lines[0].trim_end(),
        tool_title(&bold(&format!("$ {}", tool_output("..."))))
    );

    let component =
        bash_render_call(&JsValue::from(json!({"command": 42}).clone()), &theme, &context()).expect("bash renderer");
    let lines = component.borrow_mut().render(80);
    assert_eq!(
        lines[0].trim_end(),
        tool_title(&bold(&format!("$ {}", error("[invalid arg]"))))
    );
}

#[test]
fn tool_definitions_expose_renderers() {
    let definitions = vec![
        create_read_tool_definition("/repo", ReadToolOptions::default()),
        create_write_tool_definition("/repo", WriteToolOptions::default()),
        create_edit_tool_definition("/repo", EditToolOptions::default()),
        create_ls_tool_definition("/repo", LsToolOptions::default()),
        create_grep_tool_definition("/repo", GrepToolOptions::default()),
        create_find_tool_definition("/repo", FindToolOptions::default()),
        create_outline_tool_definition("/repo", CodeIndexToolOptions::default()),
        create_symbol_tool_definition("/repo", CodeIndexToolOptions::default()),
        create_references_tool_definition("/repo", CodeIndexToolOptions::default()),
        create_bash_tool_definition("/repo", BashToolOptions::default()),
    ];
    for definition in definitions {
        assert!(definition.render_call.is_some(), "{}", definition.name);
        assert!(definition.render_result.is_some(), "{}", definition.name);
    }
}
