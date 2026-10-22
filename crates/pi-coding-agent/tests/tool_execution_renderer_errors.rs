//! `tool-execution.ts` wraps `renderCall`/`renderResult` in `try/catch` and falls
//! back when a renderer throws. Rust renderers return `Result`; an `Err` must take
//! the same fallback path.

use std::sync::Arc;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::types::{
    HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
};
use pi_coding_agent::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::Component;
use pi_tui::utils::strip_terminal_sequences;
use serde_json::json;

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    index % 256,
                    (index * 3) % 256,
                    (index * 7) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

fn erring_definition() -> ToolDefinition {
    ToolDefinition {
        name: "erring_tool".to_owned(),
        label: "Erring tool".to_owned(),
        description: "Renderer failures".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object"}),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_tool_call_id, _params, _signal, _on_update, _context| {
            Box::pin(async {
                Ok(AgentToolResult {
                    content: vec![],
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: Some(Arc::new(|_args, _theme, _context| {
            Err(HandlerError::new("call renderer failed"))
        })),
        render_result: Some(Arc::new(
            |_result: &AgentToolResult,
             _options: &ToolRenderResultOptions,
             _theme: &Theme,
             _context: &ToolRenderContext| { Err(HandlerError::new("result renderer failed")) },
        )),
    }
}

fn plain(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn failing_call_and_result_renderers_use_the_typescript_fallbacks() {
    let _theme = install_theme();
    let mut component = ToolExecutionComponent::new(
        "erring_tool",
        "call-1",
        json!({"value": 1}),
        ToolExecutionOptions::default(),
        Some(erring_definition()),
        Arc::new(|| {}),
        "/repo",
    );

    let call_text = plain(&component.render(80));
    assert_eq!(call_text.trim(), "erring_tool");
    assert!(!call_text.contains("call renderer failed"), "{call_text}");

    component.update_result(
        ToolExecutionResult {
            content: vec![TextOrImageContent::Text(TextContent::new("result output".to_owned()))],
            details: None,
            is_error: false,
        },
        false,
        None,
    );
    let result_text = plain(&component.render(80));
    assert!(result_text.contains("erring_tool"), "{result_text}");
    assert!(result_text.contains("result output"), "{result_text}");
    assert!(!result_text.contains("result renderer failed"), "{result_text}");
}
