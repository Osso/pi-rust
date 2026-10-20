//! Opt-in minimal-mode example. Embed and register explicitly; never loaded by default.
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::{
    extensions::*,
    tools::{self, ToolsOptions},
};
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor};
use pi_js_compat::{number::js_number_to_string, string::js_trim};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub fn minimal_mode_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    for (name, description) in [
        (
            "read",
            "Read the contents of a file. Supports text files and images (jpg, png, gif, webp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 10KB (whichever is hit first). Use offset/limit for large files.",
        ),
        (
            "bash",
            "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last 2000 lines or 10KB (whichever is hit first).",
        ),
        (
            "write",
            "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.",
        ),
        (
            "edit",
            "Edit a file by replacing exact text. The oldText must match exactly (including whitespace). Use this for precise, surgical edits.",
        ),
        (
            "find",
            "Find files by name pattern (glob). Searches recursively from the specified path. Output limited to 200 results.",
        ),
        (
            "grep",
            "Search file contents by regex pattern. Uses ripgrep for fast searching. Output limited to 200 matches.",
        ),
        (
            "ls",
            "List directory contents with file sizes. Shows files and directories with their sizes. Output limited to 500 entries.",
        ),
    ] {
        let kind = tools::ToolName::from_name(name).expect("built-in name");
        let mut definition = tools::create_tool_definition(kind, ".", ToolsOptions::default());
        definition.label = name.into();
        definition.description = description.into();
        definition.prompt_snippet = None;
        definition.prompt_guidelines = None;
        definition.prepare_arguments = None;
        definition.prepare_execution = None;
        definition.execution_mode = None;
        definition.execute = Arc::new(move |id, params, signal, update, ctx| {
            Box::pin(async move {
                let tool = tools::create_tool_definition(kind, &ctx.cwd()?, ToolsOptions::default());
                (tool.execute)(id, params, signal, update, ctx).await
            })
        });
        definition.render_call = Some(Arc::new(move |args, theme, _| {
            Ok(text_component(render_call(name, args, theme)))
        }));
        definition.render_result = Some(Arc::new(move |result, options, theme, _| {
            let text = result.content.iter().find_map(|c| match c {
                TextOrImageContent::Text(text) => Some(text.text.as_str()),
                _ => None,
            });
            Ok(text_component(render_result(name, text, options.expanded, theme)))
        }));
        pi.register_tool(definition)?;
    }
    Ok(())
}
fn text_component(text: String) -> pi_tui::tui::ComponentRef {
    Rc::new(RefCell::new(pi_tui::Text::new(text, 0, 0)))
}
fn string_arg<'a>(args: &'a Value, key: &str, default: &'a str) -> &'a str {
    args[key].as_str().filter(|s| !s.is_empty()).unwrap_or(default)
}
fn shorten_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|error| panic!("HOME unavailable: {error}"));
    path.strip_prefix(&home)
        .map_or_else(|| path.into(), |tail| format!("~{tail}"))
}
fn number_text(value: &Value) -> String {
    js_number_to_string(value.as_f64().expect("numeric tool parameter"))
}
fn render_call(name: &str, args: &Value, theme: &Theme) -> String {
    use ThemeColor::{Accent, Muted, ToolOutput, ToolTitle, Warning};
    if name == "bash" {
        let command = string_arg(args, "command", "...");
        let suffix = if args["timeout"].as_f64().is_some_and(|n| n != 0.0) {
            theme.fg(Muted, &format!(" (timeout {}s)", number_text(&args["timeout"])))
        } else {
            String::new()
        };
        return theme.fg(ToolTitle, &theme.bold(&format!("$ {command}"))) + &suffix;
    }
    let title = theme.fg(ToolTitle, &theme.bold(name));
    let default = if ["find", "grep", "ls"].contains(&name) {
        "."
    } else {
        ""
    };
    let path = shorten_path(string_arg(args, "path", default));
    let path_display = if path.is_empty() {
        theme.fg(ToolOutput, "...")
    } else {
        theme.fg(Accent, &path)
    };
    let mut output = format!("{title} {path_display}");
    match name {
        "read" if args.get("offset").is_some() || args.get("limit").is_some() => {
            let start = args["offset"].as_f64().unwrap_or(1.0);
            let end = args["limit"]
                .as_f64()
                .map(|limit| start + limit - 1.0)
                .filter(|n| *n != 0.0)
                .map(|n| format!("-{}", js_number_to_string(n)))
                .unwrap_or_default();
            output += &theme.fg(Warning, &format!(":{}{end}", js_number_to_string(start)));
        }
        "write" => {
            let count = args["content"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map_or(0, |s| s.split('\n').count());
            if count > 0 {
                output += &theme.fg(Muted, &format!(" ({count} lines)"));
            }
        }
        "find" | "grep" => {
            let pattern = string_arg(args, "pattern", "");
            let pattern = if name == "grep" {
                format!("/{pattern}/")
            } else {
                pattern.into()
            };
            output = format!(
                "{title} {}{}",
                theme.fg(Accent, &pattern),
                theme.fg(ToolOutput, &format!(" in {path}"))
            );
            if name == "grep"
                && let Some(glob) = args["glob"].as_str().filter(|s| !s.is_empty())
            {
                output += &theme.fg(ToolOutput, &format!(" ({glob})"));
            }
            if let Some(limit) = args.get("limit") {
                let limit = number_text(limit);
                let suffix = if name == "find" {
                    format!(" (limit {limit})")
                } else {
                    format!(" limit {limit}")
                };
                output += &theme.fg(ToolOutput, &suffix);
            }
        }
        "ls" => {
            if let Some(limit) = args.get("limit") {
                output += &theme.fg(ToolOutput, &format!(" (limit {})", number_text(limit)));
            }
        }
        _ => {}
    }
    output
}
fn render_result(name: &str, text: Option<&str>, expanded: bool, theme: &Theme) -> String {
    let Some(text) = text else {
        return String::new();
    };
    if !expanded {
        if let Some(unit) = match name {
            "find" => Some("files"),
            "grep" => Some("matches"),
            "ls" => Some("entries"),
            _ => None,
        } {
            let count = js_trim(text).split('\n').filter(|line| !line.is_empty()).count();
            if count > 0 {
                return theme.fg(ThemeColor::Muted, &format!(" → {count} {unit}"));
            }
        }
        return String::new();
    }
    if name == "write" {
        return if text.is_empty() {
            String::new()
        } else {
            format!("\n{}", theme.fg(ThemeColor::Error, text))
        };
    }
    if name == "edit" {
        return format!(
            "\n{}",
            theme.fg(
                if text.contains("Error") || text.contains("error") {
                    ThemeColor::Error
                } else {
                    ThemeColor::ToolOutput
                },
                text
            )
        );
    }
    let text = if name == "read" { text } else { js_trim(text) };
    format!(
        "\n{}",
        text.split('\n')
            .map(|line| theme.fg(ThemeColor::ToolOutput, line))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
