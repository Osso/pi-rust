//! Bounded model output, retained render details and TUI renderers.

use std::cell::RefCell;
use std::rc::Rc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::{
    js_format, js_json,
    js_json::{JsString, JsValue, is_truthy, stringify_lossless, to_js_string},
};
use pi_tui::{components::text::Text, tui::ComponentRef};

use crate::core::extensions::{HandlerError, ToolRenderContext, ToolRenderResultOptions};
use crate::core::tools::truncate::{TruncationOptions, format_size, truncate_head};
use crate::modes::interactive::theme::{Theme, ThemeColor};

const OUTPUT_MAX_BYTES: usize = 8 * 1024;
const OUTPUT_MAX_LINES: usize = 1000;
const DETAILS_MAX_BYTES: usize = 8 * 1024;
const DETAILS_NOTICE: &str = "[Diff truncated in tool details.]";

pub(super) fn tool_result(result: &JsValue) -> Result<AgentToolResult, HandlerError> {
    let changes = result["changes"]
        .as_array()
        .ok_or_else(|| HandlerError::new("Invalid replacement changes"))?;
    let diffs = changes
        .iter()
        .map(|change| {
            change
                .get("diff")
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let diff = JsString::join(diffs, "\n");
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(format_model_output(
            result, &diff,
        )?))],
        details: Some(build_render_details(result, &diff)),
        is_error: None,
        terminate: None,
    })
}

fn plural_suffix(count: &JsValue) -> &'static str {
    if count.as_f64() == Some(1.0) { "" } else { "s" }
}

fn summary(result: &JsValue) -> Result<JsString, HandlerError> {
    let mode = if is_truthy(&result["dryRun"]) {
        "Previewed"
    } else {
        "Applied"
    };
    let replacements = to_js_string(&result["totalReplacements"]).map_err(HandlerError::new)?;
    let files = to_js_string(&result["filesModified"]).map_err(HandlerError::new)?;
    Ok(js_format!(
        "{} {} replacement{} in {} file{}",
        mode,
        replacements,
        plural_suffix(&result["totalReplacements"]),
        files,
        plural_suffix(&result["filesModified"])
    ))
}

fn format_model_output(result: &JsValue, diff: &JsString) -> Result<JsString, HandlerError> {
    let hash = to_js_string(&result["planHash"]).map_err(HandlerError::new)?;
    let summary = js_format!("{}. Plan: {}", summary(result)?, hash);
    let output = if diff.is_empty() {
        summary
    } else {
        js_format!("{}\n\n{}", summary, diff)
    };
    let truncation = truncate_head(
        output,
        TruncationOptions {
            max_bytes: OUTPUT_MAX_BYTES as f64,
            max_lines: OUTPUT_MAX_LINES as f64,
        },
    );
    if !truncation.truncated {
        return Ok(truncation.content);
    }
    Ok(js_format!(
        "{}\n\n[Diff truncated: {}/{} lines, {}/{}.]",
        truncation.content,
        truncation.output_lines,
        truncation.total_lines,
        format_size(truncation.output_bytes as f64),
        format_size(truncation.total_bytes as f64)
    ))
}

fn details(result: &JsValue, diff: JsString) -> JsValue {
    js_json!({ "dryRun": result["dryRun"].clone(), "totalReplacements": result["totalReplacements"].clone(), "filesModified": result["filesModified"].clone(), "diff": diff })
}

fn build_render_details(result: &JsValue, diff: &JsString) -> JsValue {
    let full = details(result, diff.clone());
    if stringify_lossless(&full).len() <= DETAILS_MAX_BYTES {
        return full;
    }
    let mut bounded = JsString::default();
    for line in diff.split('\n') {
        let candidate = if bounded.is_empty() {
            line
        } else {
            js_format!("{}\n{}", bounded, line)
        };
        let truncated = js_format!("{}\n{}", candidate, DETAILS_NOTICE);
        if stringify_lossless(&details(result, truncated)).len() > DETAILS_MAX_BYTES {
            break;
        }
        bounded = candidate;
    }
    let truncated = if bounded.is_empty() {
        DETAILS_NOTICE.into()
    } else {
        js_format!("{}\n{}", bounded, DETAILS_NOTICE)
    };
    details(result, truncated)
}

fn text_component(text: JsString) -> ComponentRef {
    Rc::new(RefCell::new(Text::new(text, 0, 0))) as ComponentRef
}

pub(super) fn render_call(
    args: &JsValue,
    theme: &Theme,
    _ctx: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    let mode = if is_truthy(&args["dryRun"]) { "preview" } else { "apply" };
    let files = to_js_string(&args["files"]).map_err(HandlerError::new)?;
    let pattern = to_js_string(&args["pattern"]).map_err(HandlerError::new)?;
    let expected = to_js_string(&args["expectedMatches"]).map_err(HandlerError::new)?;
    let title = theme.fg(ThemeColor::ToolTitle, theme.bold("regex_replace "));
    let files = theme.fg(ThemeColor::Accent, files);
    let suffix = theme.fg(
        ThemeColor::Dim,
        js_format!("/{}/ expected={} {}", pattern, expected, mode),
    );
    Ok(text_component(js_format!("{}{} {}", title, files, suffix)))
}

pub(super) fn render_result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    theme: &Theme,
    _ctx: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    if options.is_partial {
        return Ok(text_component(
            theme.fg(ThemeColor::Warning, "Planning regex replacement..."),
        ));
    }
    let Some(details) = result.details.as_ref().filter(|details| is_render_details(details)) else {
        return Ok(text_component(
            theme.fg(ThemeColor::Error, "Invalid replacement details"),
        ));
    };
    let mut text = theme.fg(ThemeColor::Success, summary(details)?);
    if options.expanded
        && let Some(diff) = details["diff"].as_string().filter(|diff| !diff.is_empty())
    {
        text = js_format!("{}\n{}", text, color_diff(diff, theme));
    }
    Ok(text_component(text))
}

fn is_render_details(details: &JsValue) -> bool {
    let has_boolean = details.get("dryRun").and_then(JsValue::as_bool).is_some();
    let has_numbers = ["totalReplacements", "filesModified"]
        .iter()
        .all(|key| details.get(key).and_then(JsValue::as_f64).is_some());
    let has_diff = details.get("diff").and_then(JsValue::as_string).is_some();
    has_boolean && has_numbers && has_diff
}

fn color_diff(diff: &JsString, theme: &Theme) -> JsString {
    let lines = diff
        .split('\n')
        .map(|line| {
            let color = if line.starts_with('+') && !line.starts_with("+++") {
                ThemeColor::Success
            } else if line.starts_with('-') && !line.starts_with("---") {
                ThemeColor::Error
            } else {
                ThemeColor::Dim
            };
            theme.fg(color, line)
        })
        .collect::<Vec<_>>();
    JsString::join(lines, "\n")
}
