//! Port of `packages/coding-agent/src/core/export-html/index.ts`.

use std::path::Path;

use indexmap::IndexMap;
use pi_agent_core::agent::AgentState;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::base64::btoa;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_js_compat::js_regex::JsRegex;
use pi_js_compat::number::{js_number_to_string, parse_int};
use pi_js_compat::string::replace;
use serde::Serialize;

use crate::config::package_config;
use crate::core::session_manager::{SessionEntry, SessionHeader, SessionManager, SessionManagerError};
use crate::modes::interactive::theme::{ThemeError, get_resolved_theme_colors, get_theme_export_colors};
use crate::utils::paths::{PathError, PathInputOptions, node_basename, normalize_path, process_cwd, resolve_path};

const TEMPLATE_HTML: &str = include_str!("templates/template.html");
const TEMPLATE_CSS: &str = include_str!("templates/template.css");
const TEMPLATE_JS: &str = include_str!("templates/template.js");
const MARKED_JS: &str = include_str!("templates/vendor/marked.min.js");
const HIGHLIGHT_JS: &str = include_str!("templates/vendor/highlight.min.js");

/// Tools rendered directly by the HTML template (not pre-rendered via TUI→ANSI→HTML).
const TEMPLATE_RENDERED_TOOLS: [&str; 5] = ["bash", "read", "write", "edit", "ls"];

/// Failure raised while exporting a session to HTML.
#[derive(Debug)]
pub enum ExportError {
    Message(JsString),
    Theme(ThemeError),
    Path(PathError),
    Session(SessionManagerError),
    Io(std::io::Error),
}

impl ExportError {
    pub fn message(&self) -> JsString {
        match self {
            Self::Message(message) => message.clone(),
            Self::Path(error) => error.message().clone(),
            Self::Theme(error) => error.message(),
            Self::Session(error) => error.message().clone(),
            Self::Io(error) => error.to_string().into(),
        }
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(&encode_node_utf8(message)),
            Self::Theme(error) => error.fmt(formatter),
            Self::Path(error) => error.fmt(formatter),
            Self::Session(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<ThemeError> for ExportError {
    fn from(error: ThemeError) -> Self {
        Self::Theme(error)
    }
}

impl From<PathError> for ExportError {
    fn from(error: PathError) -> Self {
        Self::Path(error)
    }
}

impl From<SessionManagerError> for ExportError {
    fn from(error: SessionManagerError) -> Self {
        Self::Session(error)
    }
}

impl From<std::io::Error> for ExportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Pre-rendered HTML for a custom tool call and result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedToolHtml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_html: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_html_collapsed: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_html_expanded: Option<JsString>,
}

/// Collapsed/expanded HTML produced for one tool result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedToolResult {
    pub collapsed: Option<JsString>,
    pub expanded: Option<JsString>,
}

/// Renders custom tools to HTML using their TUI renderers.
///
/// `ComponentRef` is `Rc`-based, so the renderer is single-threaded like the
/// TypeScript object it replaces.
pub trait ToolHtmlRenderer {
    fn render_call(&self, tool_call_id: &str, tool_name: &str, args: &JsValue) -> Option<JsString>;
    fn render_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        result: &[TextOrImageContent],
        details: Option<&JsValue>,
        is_error: bool,
    ) -> Option<RenderedToolResult>;
}

/// Export options.
#[derive(Clone, Default)]
pub struct ExportOptions {
    pub output_path: Option<JsString>,
    pub theme_name: Option<JsString>,
    pub tool_renderer: Option<std::rc::Rc<dyn ToolHtmlRenderer>>,
}

impl ExportOptions {
    /// `options: string` shorthand from the source.
    pub fn from_output_path(output_path: impl Into<JsString>) -> Self {
        Self {
            output_path: Some(output_path.into()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgb {
    r: f64,
    g: f64,
    b: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DerivedExportColors {
    page_bg: JsString,
    card_bg: JsString,
    info_bg: JsString,
}

fn hex_color_pattern() -> &'static JsRegex {
    static PATTERN: std::sync::OnceLock<JsRegex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| JsRegex::new(r"^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$").expect("valid"))
}

fn rgb_color_pattern() -> &'static JsRegex {
    static PATTERN: std::sync::OnceLock<JsRegex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| JsRegex::new(r"^rgb\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$").expect("valid"))
}

fn parse_color(color: &JsString) -> Option<Rgb> {
    if let Some(captures) = hex_color_pattern().captures(color) {
        return Some(Rgb {
            r: parse_int(captures.get(1).expect("red capture").text().units(), 16),
            g: parse_int(captures.get(2).expect("green capture").text().units(), 16),
            b: parse_int(captures.get(3).expect("blue capture").text().units(), 16),
        });
    }
    let captures = rgb_color_pattern().captures(color)?;
    Some(Rgb {
        r: parse_int(captures.get(1).expect("red capture").text().units(), 10),
        g: parse_int(captures.get(2).expect("green capture").text().units(), 10),
        b: parse_int(captures.get(3).expect("blue capture").text().units(), 10),
    })
}

fn get_luminance(rgb: Rgb) -> f64 {
    let to_linear = |channel: f64| {
        let s = channel / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * to_linear(rgb.r) + 0.7152 * to_linear(rgb.g) + 0.0722 * to_linear(rgb.b)
}

fn adjust_brightness(color: &JsString, factor: f64) -> JsString {
    let Some(parsed) = parse_color(color) else {
        return color.to_owned();
    };
    let adjust = |channel: f64| (channel * factor).round().clamp(0.0, 255.0);
    js_format!(
        "rgb({}, {}, {})",
        js_number_to_string(adjust(parsed.r)),
        js_number_to_string(adjust(parsed.g)),
        js_number_to_string(adjust(parsed.b))
    )
}

fn derive_export_colors(base_color: &JsString) -> DerivedExportColors {
    let Some(parsed) = parse_color(base_color) else {
        return DerivedExportColors {
            page_bg: "rgb(24, 24, 30)".into(),
            card_bg: "rgb(30, 30, 36)".into(),
            info_bg: "rgb(60, 55, 40)".into(),
        };
    };

    let luminance = get_luminance(parsed);
    let is_light = luminance > 0.5;

    if is_light {
        return DerivedExportColors {
            page_bg: adjust_brightness(base_color, 0.96),
            card_bg: base_color.to_owned(),
            info_bg: js_format!(
                "rgb({}, {}, {})",
                js_number_to_string((parsed.r + 10.0).min(255.0)),
                js_number_to_string((parsed.g + 5.0).min(255.0)),
                js_number_to_string((parsed.b - 20.0).max(0.0))
            ),
        };
    }
    DerivedExportColors {
        page_bg: adjust_brightness(base_color, 0.7),
        card_bg: adjust_brightness(base_color, 0.85),
        info_bg: js_format!(
            "rgb({}, {}, {})",
            js_number_to_string((parsed.r + 20.0).min(255.0)),
            js_number_to_string((parsed.g + 15.0).min(255.0)),
            js_number_to_string(parsed.b)
        ),
    }
}

fn user_message_bg(colors: &[(JsString, JsString)]) -> JsString {
    colors
        .iter()
        .find(|(key, _)| key == "userMessageBg")
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "#343541".into())
}

fn generate_theme_vars(theme_name: Option<&JsString>) -> Result<JsString, ThemeError> {
    let colors = get_resolved_theme_colors(theme_name)?;
    let mut lines: Vec<JsString> = colors
        .iter()
        .map(|(key, value)| js_format!("--{}: {};", key, value))
        .collect();

    let theme_export = get_theme_export_colors(theme_name);
    let derived_colors = derive_export_colors(&user_message_bg(&colors));

    lines.push(js_format!(
        "--exportPageBg: {};",
        theme_export.page_bg.unwrap_or(derived_colors.page_bg)
    ));
    lines.push(js_format!(
        "--exportCardBg: {};",
        theme_export.card_bg.unwrap_or(derived_colors.card_bg)
    ));
    lines.push(js_format!(
        "--exportInfoBg: {};",
        theme_export.info_bg.unwrap_or(derived_colors.info_bg)
    ));

    Ok(JsString::join(lines, "\n      "))
}

/// Tool metadata embedded in the export.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: JsString,
    pub parameters: JsValue,
}

/// Session payload embedded as base64 in the HTML.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub header: Option<SessionHeader>,
    pub entries: Vec<SessionEntry>,
    pub leaf_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_tools: Option<IndexMap<String, RenderedToolHtml>>,
}

fn read_export_templates() -> Result<[JsString; 5], ExportError> {
    let override_dir =
        crate::config::package_dir_override().map_err(|error| ExportError::Message(error.message().clone()))?;
    let template_dir = override_dir
        .map(|_| crate::config::get_export_template_dir())
        .transpose()
        .map_err(|error| ExportError::Message(error.message().clone()))?;
    let read_asset = |name: &str, embedded: &str| -> Result<JsString, ExportError> {
        let Some(directory) = &template_dir else {
            return Ok(embedded.into());
        };
        let path = crate::utils::paths::node_join(directory, name);
        let bytes = std::fs::read(encode_node_utf8(&path))
            .map_err(|error| ExportError::Message(pi_js_compat::node_fs::node_read_error(&error, &path)))?;
        Ok(String::from_utf8_lossy(&bytes).as_ref().into())
    };
    Ok([
        read_asset("template.html", TEMPLATE_HTML)?,
        read_asset("template.css", TEMPLATE_CSS)?,
        read_asset("template.js", TEMPLATE_JS)?,
        read_asset("vendor/marked.min.js", MARKED_JS)?,
        read_asset("vendor/highlight.min.js", HIGHLIGHT_JS)?,
    ])
}

fn generate_html(session_data: &SessionData, theme_name: Option<&JsString>) -> Result<JsString, ExportError> {
    let [template, template_css, template_js, marked_js, highlight_js] = read_export_templates()?;
    let theme_vars = generate_theme_vars(theme_name)?;
    let colors = get_resolved_theme_colors(theme_name)?;
    let theme_export = get_theme_export_colors(theme_name);
    let derived_export_colors = derive_export_colors(&user_message_bg(&colors));
    let body_bg = theme_export
        .page_bg
        .unwrap_or_else(|| derived_export_colors.page_bg.clone());
    let container_bg = theme_export
        .card_bg
        .unwrap_or_else(|| derived_export_colors.card_bg.clone());
    let info_bg = theme_export
        .info_bg
        .unwrap_or_else(|| derived_export_colors.info_bg.clone());

    let session_value = pi_js_compat::js_json::to_value(session_data)
        .map_err(|error| ExportError::Message(error.to_string().into()))?;
    let session_json = pi_js_compat::js_json::stringify_lossless(&session_value);
    let session_data_base64 = btoa(session_json.as_bytes());

    let css = replace(&template_css, "{{THEME_VARS}}", &theme_vars);
    let css = replace(&css, "{{BODY_BG}}", &body_bg);
    let css = replace(&css, "{{CONTAINER_BG}}", &container_bg);
    let css = replace(&css, "{{INFO_BG}}", &info_bg);

    let html = replace(&template, "{{CSS}}", &css);
    let html = replace(&html, "{{JS}}", &template_js);
    let html = replace(&html, "{{SESSION_DATA}}", &session_data_base64);
    let html = replace(&html, "{{MARKED_JS}}", &marked_js);
    Ok(replace(&html, "{{HIGHLIGHT_JS}}", &highlight_js))
}

fn is_template_rendered_tool(tool_name: &str) -> bool {
    TEMPLATE_RENDERED_TOOLS.contains(&tool_name)
}

fn pre_render_custom_tools(
    entries: &[SessionEntry],
    tool_renderer: &dyn ToolHtmlRenderer,
) -> IndexMap<String, RenderedToolHtml> {
    let mut rendered_tools = IndexMap::new();

    for entry in entries {
        let Some(message) = entry.as_message() else {
            continue;
        };
        match message.message.as_ref() {
            pi_agent_core::agent::AgentMessage::Assistant(assistant) => {
                for block in &assistant.content {
                    let pi_ai::types::AssistantContent::ToolCall(call) = block else {
                        continue;
                    };
                    if is_template_rendered_tool(&call.name) {
                        continue;
                    }
                    let call_html =
                        tool_renderer.render_call(&call.id, &call.name, &JsValue::Object(call.arguments.clone()));
                    if let Some(call_html) = call_html.filter(|html| !html.is_empty()) {
                        rendered_tools.insert(
                            call.id.clone(),
                            RenderedToolHtml {
                                call_html: Some(call_html),
                                ..RenderedToolHtml::default()
                            },
                        );
                    }
                }
            }
            pi_agent_core::agent::AgentMessage::ToolResult(result) => {
                if result.tool_call_id.is_empty() {
                    continue;
                }
                let tool_name = result.tool_name.as_str();
                let existing = rendered_tools.get(&result.tool_call_id);
                if existing.is_some() || !is_template_rendered_tool(tool_name) {
                    let rendered = tool_renderer.render_result(
                        &result.tool_call_id,
                        tool_name,
                        &result.content,
                        result.details.as_ref(),
                        result.is_error,
                    );
                    if let Some(rendered) = rendered {
                        let entry = RenderedToolHtml {
                            call_html: existing.and_then(|existing| existing.call_html.clone()),
                            result_html_collapsed: rendered.collapsed,
                            result_html_expanded: rendered.expanded,
                        };
                        rendered_tools.insert(result.tool_call_id.clone(), entry);
                    }
                }
            }
            _ => {}
        }
    }

    rendered_tools
}

/// `basename(path, suffix)` from `node:path`.
fn basename(path: &JsString, suffix: &str) -> JsString {
    let name = node_basename(path);
    name.strip_suffix(suffix).unwrap_or(name)
}

fn output_path_or_default(output_path: Option<JsString>, input_path: &JsString) -> Result<JsString, ExportError> {
    let normalized = match output_path {
        Some(path) if !path.is_empty() => normalize_path(path, &PathInputOptions::default())?,
        _ => JsString::default(),
    };
    if !normalized.is_empty() {
        return Ok(normalized);
    }
    Ok(js_format!(
        "{}-session-{}.html",
        package_config().app_name,
        basename(input_path, ".jsonl")
    ))
}

fn path_exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

fn write_html(output_path: &JsString, html: JsString) -> Result<(), ExportError> {
    std::fs::write(encode_node_utf8(output_path), encode_node_utf8(&html))
        .map_err(|error| ExportError::Message(pi_js_compat::node_fs::node_fs_error(&error, "open", output_path)))?;
    Ok(())
}

/// Export a session to HTML using its [`SessionManager`].
pub async fn export_session_to_html(
    sm: &SessionManager,
    state: Option<&AgentState>,
    options: Option<ExportOptions>,
) -> Result<JsString, ExportError> {
    let options = options.unwrap_or_default();
    let session_file = sm
        .get_session_file()
        .ok_or_else(|| ExportError::Message("Cannot export in-memory session to HTML".into()))?
        .clone();
    if !path_exists(&session_file) {
        return Err(ExportError::Message(
            "Nothing to export yet - start a conversation first".into(),
        ));
    }

    let entries = sm.get_entries();
    let rendered_tools = options
        .tool_renderer
        .as_ref()
        .map(|tool_renderer| pre_render_custom_tools(&entries, tool_renderer.as_ref()))
        .filter(|rendered_tools| !rendered_tools.is_empty());

    let session_data = SessionData {
        header: sm.get_header().cloned(),
        entries,
        leaf_id: sm.get_leaf_id().cloned(),
        system_prompt: state.map(|state| state.system_prompt.clone()),
        tools: state.map(|state| {
            state
                .tools
                .iter()
                .map(|tool| ToolSummary {
                    name: tool.name().to_owned(),
                    description: tool.description().to_owned(),
                    parameters: tool.parameters().clone().into(),
                })
                .collect()
        }),
        rendered_tools,
    };

    let html = generate_html(&session_data, options.theme_name.as_ref())?;

    let output_path = output_path_or_default(options.output_path, &session_file)?;
    write_html(&output_path, html)?;
    Ok(output_path)
}

/// Export a session file to HTML without an agent state.
pub async fn export_from_file(input_path: &JsString, options: Option<ExportOptions>) -> Result<JsString, ExportError> {
    let options = options.unwrap_or_default();
    let resolved_input_path = crate::core::session_transcript::read_session_transcript_path(resolve_path(
        input_path,
        process_cwd()?,
        &PathInputOptions::default(),
    )?);

    if !path_exists(&resolved_input_path) {
        return Err(ExportError::Message(js_format!(
            "File not found: {}",
            resolved_input_path
        )));
    }

    let sm = SessionManager::open_read_only(&resolved_input_path, None)?;

    let session_data = SessionData {
        header: sm.get_header().cloned(),
        entries: sm.get_entries(),
        leaf_id: sm.get_leaf_id().cloned(),
        system_prompt: None,
        tools: None,
        rendered_tools: None,
    };

    let html = generate_html(&session_data, options.theme_name.as_ref())?;

    let output_path = output_path_or_default(options.output_path, &resolved_input_path)?;
    write_html(&output_path, html)?;
    Ok(output_path)
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsString;

    #[test]
    fn html_write_error_keeps_original_raw_output_path() {
        let temporary = tempfile::tempdir().unwrap();
        let path = JsString::from(temporary.path().to_str().expect("utf-8 test path"))
            + "/"
            + JsString::from_utf16(vec![0xd800]);
        std::fs::create_dir_all(encode_node_utf8(&path)).unwrap();
        let error = write_html(&path, "content".into()).unwrap_err();
        // Node fs errors report the UTF-8-repaired path (pi_js_compat::node_fs format_fs_error_record).
        assert!(error.message().contains(&JsString::from(encode_node_utf8(&path))));
    }

    #[test]
    fn tool_summary_payload_keeps_raw_description_and_schema_text() {
        let raw = JsString::from_utf16(vec![0xd800]);
        let data = SessionData {
            header: None,
            entries: vec![],
            leaf_id: None,
            system_prompt: None,
            tools: Some(vec![ToolSummary {
                name: "fixture".into(),
                description: raw.clone(),
                parameters: pi_js_compat::js_json!({"description": raw}),
            }]),
            rendered_tools: None,
        };
        let html = generate_html(&data, None).unwrap();
        let expected = btoa(br#"{"header":null,"entries":[],"leafId":null,"tools":[{"name":"fixture","description":"\ud800","parameters":{"description":"\ud800"}}]}"#);
        assert!(html.contains(expected));
    }

    #[test]
    fn html_payload_stringifies_surrogates_without_replacing_or_html_escaping() {
        let data = SessionData {
            header: None,
            entries: vec![],
            leaf_id: None,
            system_prompt: Some(JsString::from_utf16(vec![0xd800, 0x3c, 0x26, 0xd83d, 0xde00])),
            tools: None,
            rendered_tools: None,
        };
        let html = generate_html(&data, None).expect("HTML export");
        let expected = pi_js_compat::base64::btoa(
            br#"{"header":null,"entries":[],"leafId":null,"systemPrompt":"\ud800<&"#
                .iter()
                .copied()
                .chain("😀\"}".bytes())
                .collect::<Vec<_>>()
                .as_slice(),
        );
        assert!(html.contains(&expected));
    }
}
