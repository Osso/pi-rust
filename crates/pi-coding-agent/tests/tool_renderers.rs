use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{ExtensionTheme, ToolRenderContext};
use pi_coding_agent::core::tools::bash::{
    BashToolOptions, create_bash_tool_definition, render_call as bash_render_call,
};
use pi_coding_agent::core::tools::code_index::{
    CodeIndexToolOptions, create_outline_tool_definition, create_references_tool_definition,
    create_symbol_tool_definition,
};
use pi_coding_agent::core::tools::find::{FindToolOptions, create_find_tool_definition};
use pi_coding_agent::core::tools::grep::{GrepToolOptions, create_grep_tool_definition};
use pi_coding_agent::core::tools::ls::{LsToolOptions, create_ls_tool_definition};
use pi_coding_agent::core::tools::read::{ReadToolOptions, create_read_tool_definition};
use pi_coding_agent::core::tools::write::{WriteToolOptions, create_write_tool_definition};
use serde_json::json;

struct FakeTheme;

impl ExtensionTheme for FakeTheme {
    fn fg(&self, color: &str, text: &str) -> String {
        format!("[{color}]{text}[/]")
    }

    fn bold(&self, text: &str) -> String {
        format!("*{text}*")
    }
}

fn context() -> ToolRenderContext {
    ToolRenderContext {
        args: json!({}),
        tool_call_id: "call".to_owned(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        state: json!({}),
        cwd: "/repo".to_owned(),
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
    let component = bash_render_call(&json!({"command": "echo hi", "timeout": 5}), &FakeTheme, &context());
    let lines = component.borrow_mut().render(80);
    assert_eq!(lines[0].trim_end(), "[toolTitle]*$ echo hi*[/][muted] (timeout 5s)[/]");

    let component = bash_render_call(&json!({"command": ""}), &FakeTheme, &context());
    let lines = component.borrow_mut().render(80);
    assert_eq!(lines[0].trim_end(), "[toolTitle]*$ [toolOutput]...[/]*[/]");

    let component = bash_render_call(&json!({"command": null}), &FakeTheme, &context());
    let lines = component.borrow_mut().render(80);
    assert_eq!(lines[0].trim_end(), "[toolTitle]*$ [toolOutput]...[/]*[/]");

    let component = bash_render_call(&json!({"command": 42}), &FakeTheme, &context());
    let lines = component.borrow_mut().render(80);
    assert_eq!(lines[0].trim_end(), "[toolTitle]*$ [error][invalid arg][/]*[/]");
}

#[test]
fn tool_definitions_expose_renderers() {
    let definitions = vec![
        create_read_tool_definition("/repo", ReadToolOptions::default()),
        create_write_tool_definition("/repo", WriteToolOptions::default()),
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
        if definition.name != "bash" {
            assert!(definition.render_result.is_some(), "{}", definition.name);
        }
    }
}
