//! Opt-in native `built-in-tool-renderer.ts`; register its factory explicitly.
//! Execution delegates to the original built-ins; only rendering is replaced.
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::is_truthy as truthy;
use pi_js_compat::js_regex::JsRegex;
use std::{cell::RefCell, rc::Rc};

use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::extensions::{
    ExtensionApi, HandlerError, RenderShell, ToolRenderResultOptions, infallible_render_call, infallible_render_result,
};
use pi_coding_agent::core::tools::{
    bash::{BashToolOptions, create_bash_tool},
    edit::{EditToolOptions, create_edit_tool},
    read::{ReadToolOptions, create_read_tool},
    tool_definition_wrapper::create_tool_definition_from_agent_tool,
    write::{WriteToolOptions, create_write_tool},
};
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor};
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::{Text, tui::ComponentRef};

pub fn built_in_tool_renderer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let cwd = pi_js_compat::process_cwd::cwd().map_err(HandlerError::from)?;
    let tools = [
        create_read_tool(&cwd, ReadToolOptions::default()),
        create_bash_tool(&cwd, BashToolOptions::default()),
        create_edit_tool(&cwd, EditToolOptions::default()),
        create_write_tool(&cwd, WriteToolOptions::default()),
    ];
    for tool in tools {
        let mut definition = create_tool_definition_from_agent_tool(tool);
        let name = definition.name.clone();
        definition.label = name.clone().into();
        if name == "edit" {
            definition.render_shell = Some(RenderShell::SelfRender);
        }
        let call_name = name.clone();
        definition.render_call = Some(infallible_render_call(move |args, theme, _| {
            text_component(render_call(&call_name, args, theme))
        }));
        definition.render_result = Some(infallible_render_result(move |result, options, theme, _| {
            text_component(render_result(&name, result, options, theme))
        }));
        pi.register_tool(definition)?;
    }
    Ok(())
}

fn text_component(text: JsString) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(text, 0, 0)))
}

fn number_text(value: &Value) -> String {
    pi_js_compat::number::js_number_to_string(value.as_f64().expect("numeric tool parameter"))
}

fn render_call(name: &str, args: &Value, theme: &Theme) -> JsString {
    let title = if name == "bash" { "$ " } else { name };
    let title = if name == "bash" {
        JsString::from(title)
    } else {
        js_format!("{} ", title)
    };
    let mut text = theme.fg(ThemeColor::ToolTitle, theme.bold(&title));
    if name == "bash" {
        let command = args["command"].as_string().cloned().unwrap_or_else(JsString::default);
        let command = if command.encode_utf16().count() > 80 {
            js_format!("{}...", command.slice(0, 77))
        } else {
            command.to_owned()
        };
        text.push_str(theme.fg(ThemeColor::Accent, &command));
        if truthy(&args["timeout"]) {
            text.push_str(theme.fg(
                ThemeColor::Dim,
                &js_format!(" (timeout: {}s)", number_text(&args["timeout"])),
            ));
        }
        return text;
    }
    text.push_str(theme.fg(
        ThemeColor::Accent,
        args["path"].as_string().cloned().unwrap_or_else(JsString::default),
    ));
    if name == "read" && (truthy(&args["offset"]) || truthy(&args["limit"])) {
        let parts = ["offset", "limit"]
            .into_iter()
            .filter(|key| truthy(&args[*key]))
            .map(|key| js_format!("{}={}", key, number_text(&args[key])))
            .collect::<Vec<_>>();
        text.push_str(theme.fg(ThemeColor::Dim, &js_format!(" ({})", JsString::join(parts, ", "))));
    }
    if name == "write" {
        let count = args["content"]
            .as_string()
            .cloned()
            .unwrap_or_else(JsString::default)
            .split('\n')
            .count();
        text.push_str(theme.fg(ThemeColor::Dim, &js_format!(" ({} lines)", count)));
    }
    text
}

fn first_text(result: &AgentToolResult) -> Option<&JsString> {
    match result.content.first() {
        Some(TextOrImageContent::Text(content)) => Some(&content.text),
        _ => None,
    }
}

fn render_result(name: &str, result: &AgentToolResult, options: &ToolRenderResultOptions, theme: &Theme) -> JsString {
    if options.is_partial {
        let message = match name {
            "read" => "Reading...",
            "bash" => "Running...",
            "edit" => "Editing...",
            _ => "Writing...",
        };
        return theme.fg(ThemeColor::Warning, message);
    }
    match name {
        "read" => render_read(result, options.expanded, theme),
        "bash" => render_bash(result, options.expanded, theme),
        "edit" => render_edit(result, options.expanded, theme),
        _ => first_error(result).map_or_else(
            || theme.fg(ThemeColor::Success, "Written"),
            |error| theme.fg(ThemeColor::Error, error),
        ),
    }
}

fn truncation(result: &AgentToolResult) -> Option<&Value> {
    result
        .details
        .as_ref()?
        .get("truncation")
        .filter(|value| value["truncated"] == pi_js_compat::js_json!(true))
}

fn render_read(result: &AgentToolResult, expanded: bool, theme: &Theme) -> JsString {
    if matches!(result.content.first(), Some(TextOrImageContent::Image(_))) {
        return theme.fg(ThemeColor::Success, "Image loaded");
    }
    let Some(content) = first_text(result) else {
        return theme.fg(ThemeColor::Error, "No content");
    };
    let lines = content.split('\n').collect::<Vec<_>>();
    let mut text = theme.fg(ThemeColor::Success, &js_format!("{} lines", lines.len()));
    if let Some(truncation) = truncation(result) {
        text.push_str(theme.fg(
            ThemeColor::Warning,
            &js_format!(" (truncated from {})", number_text(&truncation["totalLines"])),
        ));
    }
    if expanded {
        append_lines(&mut text, &lines, 15, theme);
        if lines.len() > 15 {
            text.push_str(&js_format!(
                "\n{}",
                theme.fg(ThemeColor::Muted, &js_format!("... {} more lines", lines.len() - 15))
            ));
        }
    }
    text
}

fn render_bash(result: &AgentToolResult, expanded: bool, theme: &Theme) -> JsString {
    let output = first_text(result).cloned().unwrap_or_else(JsString::default);
    let exit_code = JsRegex::new(r"exit code: ([0-9]+)")
        .expect("valid exit-code regex")
        .captures(&output)
        .map(|captures| {
            let digits = captures.get(1).expect("exit-code digits group").text();
            let digits = digits.to_utf8().expect("regex captures only ASCII digits");
            digits.parse::<f64>().unwrap_or(f64::INFINITY)
        });
    let count = output.split('\n').filter(|line| !(line).trim().is_empty()).count();
    let mut text = match exit_code {
        Some(code) if code != 0.0 => theme.fg(
            ThemeColor::Error,
            &js_format!("exit {}", pi_js_compat::number::js_number_to_string(code)),
        ),
        _ => theme.fg(ThemeColor::Success, "done"),
    };
    text.push_str(theme.fg(ThemeColor::Dim, &js_format!(" ({} lines)", count)));
    if truncation(result).is_some() {
        text.push_str(theme.fg(ThemeColor::Warning, " [truncated]"));
    }
    if expanded {
        let lines = output.split('\n').collect::<Vec<_>>();
        append_lines(&mut text, &lines, 20, theme);
        if lines.len() > 20 {
            text.push_str(&js_format!("\n{}", theme.fg(ThemeColor::Muted, "... more output")));
        }
    }
    text
}

fn first_error(result: &AgentToolResult) -> Option<JsString> {
    first_text(result)
        .filter(|text| text.starts_with("Error"))
        .and_then(|text| text.split('\n').next())
}

fn diff_color(line: &JsString) -> ThemeColor {
    if line.starts_with('+') && !line.starts_with("+++") {
        ThemeColor::Success
    } else if line.starts_with('-') && !line.starts_with("---") {
        ThemeColor::Error
    } else {
        ThemeColor::Dim
    }
}

fn render_edit(result: &AgentToolResult, expanded: bool, theme: &Theme) -> JsString {
    if let Some(error) = first_error(result) {
        return theme.fg(ThemeColor::Error, error);
    }
    let diff = result
        .details
        .as_ref()
        .and_then(|details| details["diff"].as_string())
        .cloned()
        .unwrap_or_else(JsString::default);
    if diff.is_empty() {
        return theme.fg(ThemeColor::Success, "Applied");
    }
    let lines = diff.split('\n').collect::<Vec<_>>();
    let additions = lines
        .iter()
        .filter(|line| diff_color(line) == ThemeColor::Success)
        .count();
    let removals = lines
        .iter()
        .filter(|line| diff_color(line) == ThemeColor::Error)
        .count();
    let mut text = js_format!(
        "{}{}{}",
        theme.fg(ThemeColor::Success, &js_format!("+{}", additions)),
        theme.fg(ThemeColor::Dim, " / "),
        theme.fg(ThemeColor::Error, &js_format!("-{}", removals))
    );
    if expanded {
        for line in lines.iter().take(30) {
            text.push_str(&js_format!("\n{}", theme.fg(diff_color(line), line)));
        }
        if lines.len() > 30 {
            text.push_str(&js_format!(
                "\n{}",
                theme.fg(
                    ThemeColor::Muted,
                    &js_format!("... {} more diff lines", lines.len() - 30)
                )
            ));
        }
    }
    text
}

fn append_lines(text: &mut JsString, lines: &[JsString], limit: usize, theme: &Theme) {
    for line in lines.iter().take(limit) {
        text.push_str(&js_format!("\n{}", theme.fg(ThemeColor::Dim, line)));
    }
}
