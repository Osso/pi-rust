//! Port of `packages/coding-agent/src/core/export-html/index.ts`.

use std::path::Path;

use pi_agent_core::agent::AgentState;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::base64::btoa;
use pi_js_compat::number::{js_number_to_string, parse_int};
use pi_js_compat::string::replace;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::config::package_config;
use crate::core::session_manager::{SessionEntry, SessionHeader, SessionManager, SessionManagerError};
use crate::modes::interactive::theme::{ThemeError, get_resolved_theme_colors, get_theme_export_colors};
use crate::utils::paths::{PathError, PathInputOptions, normalize_path, resolve_path};

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
    Message(String),
    Theme(ThemeError),
    Path(PathError),
    Session(SessionManagerError),
    Io(std::io::Error),
}

impl ExportError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
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
    pub call_html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_html_collapsed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_html_expanded: Option<String>,
}

/// Collapsed/expanded HTML produced for one tool result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedToolResult {
    pub collapsed: Option<String>,
    pub expanded: Option<String>,
}

/// Renders custom tools to HTML using their TUI renderers.
///
/// `ComponentRef` is `Rc`-based, so the renderer is single-threaded like the
/// TypeScript object it replaces.
pub trait ToolHtmlRenderer {
    fn render_call(&self, tool_call_id: &str, tool_name: &str, args: &Value) -> Option<String>;
    fn render_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        result: &[TextOrImageContent],
        details: Option<&Value>,
        is_error: bool,
    ) -> Option<RenderedToolResult>;
}

/// Export options.
#[derive(Clone, Default)]
pub struct ExportOptions {
    pub output_path: Option<String>,
    pub theme_name: Option<String>,
    pub tool_renderer: Option<std::rc::Rc<dyn ToolHtmlRenderer>>,
}

impl ExportOptions {
    /// `options: string` shorthand from the source.
    pub fn from_output_path(output_path: impl Into<String>) -> Self {
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
    page_bg: String,
    card_bg: String,
    info_bg: String,
}

fn hex_color_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$").expect("valid"))
}

fn rgb_color_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^rgb\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$").expect("valid"))
}

fn parse_color(color: &str) -> Option<Rgb> {
    if let Some(captures) = hex_color_pattern().captures(color) {
        return Some(Rgb {
            r: parse_int(&captures[1], 16),
            g: parse_int(&captures[2], 16),
            b: parse_int(&captures[3], 16),
        });
    }
    let captures = rgb_color_pattern().captures(color)?;
    Some(Rgb {
        r: parse_int(&captures[1], 10),
        g: parse_int(&captures[2], 10),
        b: parse_int(&captures[3], 10),
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

fn adjust_brightness(color: &str, factor: f64) -> String {
    let Some(parsed) = parse_color(color) else {
        return color.to_owned();
    };
    let adjust = |channel: f64| (channel * factor).round().clamp(0.0, 255.0);
    format!(
        "rgb({}, {}, {})",
        js_number_to_string(adjust(parsed.r)),
        js_number_to_string(adjust(parsed.g)),
        js_number_to_string(adjust(parsed.b))
    )
}

fn derive_export_colors(base_color: &str) -> DerivedExportColors {
    let Some(parsed) = parse_color(base_color) else {
        return DerivedExportColors {
            page_bg: "rgb(24, 24, 30)".to_owned(),
            card_bg: "rgb(30, 30, 36)".to_owned(),
            info_bg: "rgb(60, 55, 40)".to_owned(),
        };
    };

    let luminance = get_luminance(parsed);
    let is_light = luminance > 0.5;

    if is_light {
        return DerivedExportColors {
            page_bg: adjust_brightness(base_color, 0.96),
            card_bg: base_color.to_owned(),
            info_bg: format!(
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
        info_bg: format!(
            "rgb({}, {}, {})",
            js_number_to_string((parsed.r + 20.0).min(255.0)),
            js_number_to_string((parsed.g + 15.0).min(255.0)),
            js_number_to_string(parsed.b)
        ),
    }
}

fn user_message_bg(colors: &[(String, String)]) -> String {
    colors
        .iter()
        .find(|(key, _)| key == "userMessageBg")
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "#343541".to_owned())
}

fn generate_theme_vars(theme_name: Option<&str>) -> Result<String, ThemeError> {
    let colors = get_resolved_theme_colors(theme_name)?;
    let mut lines: Vec<String> = colors.iter().map(|(key, value)| format!("--{key}: {value};")).collect();

    let theme_export = get_theme_export_colors(theme_name);
    let derived_colors = derive_export_colors(&user_message_bg(&colors));

    lines.push(format!(
        "--exportPageBg: {};",
        theme_export.page_bg.unwrap_or(derived_colors.page_bg)
    ));
    lines.push(format!(
        "--exportCardBg: {};",
        theme_export.card_bg.unwrap_or(derived_colors.card_bg)
    ));
    lines.push(format!(
        "--exportInfoBg: {};",
        theme_export.info_bg.unwrap_or(derived_colors.info_bg)
    ));

    Ok(lines.join("\n      "))
}

/// Tool metadata embedded in the export.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Session payload embedded as base64 in the HTML.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub header: Option<SessionHeader>,
    pub entries: Vec<SessionEntry>,
    pub leaf_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_tools: Option<Map<String, Value>>,
}

fn generate_html(session_data: &SessionData, theme_name: Option<&str>) -> Result<String, ExportError> {
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

    let session_json = serde_json::to_string(session_data).map_err(|error| ExportError::Message(error.to_string()))?;
    let session_data_base64 = btoa(session_json.as_bytes());

    let css = replace(TEMPLATE_CSS, "{{THEME_VARS}}", &theme_vars);
    let css = replace(&css, "{{BODY_BG}}", &body_bg);
    let css = replace(&css, "{{CONTAINER_BG}}", &container_bg);
    let css = replace(&css, "{{INFO_BG}}", &info_bg);

    let html = replace(TEMPLATE_HTML, "{{CSS}}", &css);
    let html = replace(&html, "{{JS}}", TEMPLATE_JS);
    let html = replace(&html, "{{SESSION_DATA}}", &session_data_base64);
    let html = replace(&html, "{{MARKED_JS}}", MARKED_JS);
    Ok(replace(&html, "{{HIGHLIGHT_JS}}", HIGHLIGHT_JS))
}

fn is_template_rendered_tool(tool_name: &str) -> bool {
    TEMPLATE_RENDERED_TOOLS.contains(&tool_name)
}

fn pre_render_custom_tools(entries: &[SessionEntry], tool_renderer: &dyn ToolHtmlRenderer) -> Map<String, Value> {
    let mut rendered_tools = Map::new();

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
                        tool_renderer.render_call(&call.id, &call.name, &Value::Object(call.arguments.clone()));
                    if let Some(call_html) = call_html {
                        rendered_tools.insert(
                            call.id.clone(),
                            serde_json::to_value(RenderedToolHtml {
                                call_html: Some(call_html),
                                ..RenderedToolHtml::default()
                            })
                            .unwrap_or(Value::Null),
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
                        let mut entry = existing.and_then(Value::as_object).cloned().unwrap_or_default();
                        if let Some(collapsed) = rendered.collapsed {
                            entry.insert("resultHtmlCollapsed".to_owned(), Value::String(collapsed));
                        }
                        if let Some(expanded) = rendered.expanded {
                            entry.insert("resultHtmlExpanded".to_owned(), Value::String(expanded));
                        }
                        rendered_tools.insert(result.tool_call_id.clone(), Value::Object(entry));
                    }
                }
            }
            _ => {}
        }
    }

    rendered_tools
}

fn basename(path: &str, suffix: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.strip_suffix(suffix).unwrap_or(&name).to_owned()
}

fn default_output_path(input_path: &str) -> String {
    format!(
        "{}-session-{}.html",
        package_config().app_name,
        basename(input_path, ".jsonl")
    )
}

/// Export a session to HTML using its [`SessionManager`].
pub async fn export_session_to_html(
    sm: &SessionManager,
    state: Option<&AgentState>,
    options: Option<ExportOptions>,
) -> Result<String, ExportError> {
    let options = options.unwrap_or_default();
    let session_file = sm
        .get_session_file()
        .ok_or_else(|| ExportError::Message("Cannot export in-memory session to HTML".to_owned()))?
        .to_owned();
    if !Path::new(&session_file).exists() {
        return Err(ExportError::Message(
            "Nothing to export yet - start a conversation first".to_owned(),
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
        leaf_id: sm.get_leaf_id().map(str::to_owned),
        system_prompt: state.map(|state| state.system_prompt.clone()),
        tools: state.map(|state| {
            state
                .tools
                .iter()
                .map(|tool| ToolSummary {
                    name: tool.name().to_owned(),
                    description: tool.description().to_owned(),
                    parameters: tool.parameters().clone(),
                })
                .collect()
        }),
        rendered_tools,
    };

    let html = generate_html(&session_data, options.theme_name.as_deref())?;

    let output_path = match options.output_path.filter(|path| !path.is_empty()) {
        Some(path) => normalize_path(&path, &PathInputOptions::default())?,
        None => default_output_path(&session_file),
    };

    std::fs::write(&output_path, html)?;
    Ok(output_path)
}

/// Export a session file to HTML without an agent state.
pub async fn export_from_file(input_path: &str, options: Option<ExportOptions>) -> Result<String, ExportError> {
    let options = options.unwrap_or_default();
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let resolved_input_path = resolve_path(input_path, &cwd, &PathInputOptions::default())?;

    if !Path::new(&resolved_input_path).exists() {
        return Err(ExportError::Message(format!("File not found: {resolved_input_path}")));
    }

    let sm = SessionManager::open(&resolved_input_path, None, None)?;

    let session_data = SessionData {
        header: sm.get_header().cloned(),
        entries: sm.get_entries(),
        leaf_id: sm.get_leaf_id().map(str::to_owned),
        system_prompt: None,
        tools: None,
        rendered_tools: None,
    };

    let html = generate_html(&session_data, options.theme_name.as_deref())?;

    let output_path = match options.output_path.filter(|path| !path.is_empty()) {
        Some(path) => normalize_path(&path, &PathInputOptions::default())?,
        None => default_output_path(&resolved_input_path),
    };

    std::fs::write(&output_path, html)?;
    Ok(output_path)
}
