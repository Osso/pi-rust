//! AskClaude's source renderCall/renderResult, preserving UTF-16 preview boundaries.
use super::stream::string;
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::js_json::{JsString, JsValue};

const PREVIEW_MAX_CHARS: usize = 1000;
const PREVIEW_MAX_LINES: usize = 6;

fn preview(text: &JsString) -> (JsString, bool) {
    let lines: Vec<_> = text
        .slice(0, PREVIEW_MAX_CHARS)
        .split('\n')
        .take(PREVIEW_MAX_LINES)
        .collect();
    let clipped = text.len() > PREVIEW_MAX_CHARS || text.split('\n').count() > PREVIEW_MAX_LINES;
    (JsString::join(lines, "\n"), clipped)
}

pub fn call_text(args: &JsValue, default_mode: &str, default_isolated: bool, theme: &Theme) -> JsString {
    let mut text = theme.fg(ThemeColor::MdLink, theme.bold("AskClaude "));
    let mut tags = Vec::new();
    let mode = args
        .get("mode")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| default_mode.into());
    if mode != "read" || (args.get("mode").is_some() && mode != default_mode) {
        tags.push(pi_js_compat::js_format!("mode={}", mode));
    }
    for key in ["model", "thinking"] {
        let value = string(args, key);
        if !value.is_empty() {
            tags.push(pi_js_compat::js_format!("{}={}", key, value));
        }
    }
    if args
        .get("isolated")
        .and_then(JsValue::as_bool)
        .unwrap_or(default_isolated)
    {
        tags.push("isolated".into());
    }
    if !tags.is_empty() {
        text.push_str(theme.fg(
            ThemeColor::Accent,
            pi_js_compat::js_format!("[{}]", JsString::join(tags, ", ")),
        ));
        text.push_str(" ");
    }
    let (body, clipped) = preview(&string(args, "prompt"));
    text.push_str(theme.fg(ThemeColor::Muted, pi_js_compat::js_format!("\"{}\"", body)));
    if clipped {
        text.push_str(theme.fg(ThemeColor::Dim, " …"));
    }
    text
}

fn result_body(result: &AgentToolResult) -> JsString {
    match result.content.first() {
        Some(TextOrImageContent::Text(text)) => text.text.clone(),
        _ => JsString::default(),
    }
}

pub fn result_text(result: &AgentToolResult, expanded: bool, partial: bool, theme: &Theme) -> JsString {
    let body = result_body(result);
    if partial {
        let status = if matches!(result.content.first(), Some(TextOrImageContent::Text(_))) {
            body
        } else {
            "working...".into()
        };
        return theme.fg(ThemeColor::MdLink, "◉ Claude Code ") + theme.fg(ThemeColor::Muted, status);
    }
    let details = result.details.as_ref().unwrap_or(&JsValue::Null);
    let mut text = if details.get("error").and_then(JsValue::as_bool) == Some(true) {
        theme.fg(ThemeColor::Error, "✗ Claude Code error")
    } else {
        theme.fg(ThemeColor::MdLink, "✓ Claude Code")
    };
    if let Some(elapsed) = details
        .get("executionTime")
        .and_then(JsValue::as_f64)
        .filter(|v| *v != 0.0)
    {
        text.push_str(" ");
        text.push_str(theme.fg(
            ThemeColor::Dim,
            format!("{}s", pi_js_compat::number::to_fixed(elapsed / 1000.0, 1)),
        ));
    }
    let actions = string(details, "actions");
    if !actions.is_empty() {
        text.push_str(" ");
        text.push_str(theme.fg(ThemeColor::Muted, actions));
    }
    text.push_str(result_preview(&body, details, expanded, theme));
    text
}

fn result_preview(body: &JsString, details: &JsValue, expanded: bool, theme: &Theme) -> JsString {
    let mut text = JsString::default();
    if expanded {
        let prompt = string(details, "prompt");
        if !prompt.is_empty() {
            text.push_str("\n");
            text.push_str(theme.fg(ThemeColor::Dim, pi_js_compat::js_format!("Prompt: {}", prompt)));
        }
        if !prompt.is_empty() && !body.is_empty() {
            text.push_str("\n");
            text.push_str(theme.fg(ThemeColor::Dim, "─".repeat(40)));
        }
        if !body.is_empty() {
            text.push_str("\n");
            text.push_str(theme.fg(ThemeColor::ToolOutput, body));
        }
    } else {
        let (preview, clipped) = preview(body);
        text.push_str("\n");
        text.push_str(theme.fg(ThemeColor::ToolOutput, preview));
        if clipped {
            text.push_str("\n");
            text.push_str(theme.fg(
                ThemeColor::Dim,
                pi_js_compat::js_format!("… ({})", key_hint("app.tools.expand", "to expand")),
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::interactive::theme::get_theme_by_name;
    use pi_js_compat::js_json;

    #[test]
    fn claude_bridge_askclaude_render_call_uses_source_tags_and_utf16_preview() {
        let theme = get_theme_by_name(&"dark".into()).unwrap();
        let mut prompt = JsString::from("a".repeat(999));
        prompt.push_str("😀continued");
        let args = JsValue::object([
            ("prompt", JsValue::String(prompt)),
            ("mode", JsValue::String("none".into())),
            ("isolated", JsValue::Bool(true)),
        ]);
        let text = call_text(&args, "read", false, &theme);
        assert!(text.contains("[mode=none, isolated]"));
        assert!(text.units().contains(&0xd83d));
        assert!(!text.units().contains(&0xde00));
        assert!(text.contains(" …"));
    }

    #[test]
    fn claude_bridge_askclaude_render_result_has_expanded_prompt_actions_and_error() {
        let theme = get_theme_by_name(&"dark".into()).unwrap();
        let mut result = AgentToolResult::text("result body");
        result.details = Some(
            js_json::parse_lossless(
                r#"{"prompt":"task","executionTime":1200,"actions":"Read(src/lib.rs)","error":true}"#,
            )
            .unwrap(),
        );
        let text = result_text(&result, true, false, &theme);
        assert!(text.contains("✗ Claude Code error"));
        assert!(text.contains("1.2s"));
        assert!(text.contains("Prompt: task"));
        assert!(text.contains("Read(src/lib.rs)"));
        assert!(text.contains("result body"));
    }
}
