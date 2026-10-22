//! Ported from `packages/coding-agent/src/core/tools/render-utils.ts`.
//!
//! Pure display helpers plus the text/image result projection shared by tool renderers.

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_tui::{get_capabilities, get_image_dimensions, hyperlink, image_fallback};

use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::ansi::strip_ansi;
use crate::utils::paths::{PathInputOptions, resolve_path};
use crate::utils::shell::sanitize_binary_output;

/// `shortenPath`: replace the home-directory prefix with `~`.
pub fn shorten_path(path: &JsString) -> JsString {
    let home = home_directory();
    if path.starts_with(&home) {
        return JsString::from("~") + path.slice(home.len(), path.len());
    }
    path.to_owned()
}

/// `str`: strings pass through, `null`/`undefined` become `""`, other types become `null`.
pub fn str_value(value: Option<&JsValue>) -> Option<JsString> {
    match value {
        Some(JsValue::String(text)) => Some(text.clone()),
        None | Some(JsValue::Null) => Some(JsString::default()),
        Some(_) => None,
    }
}

/// `replaceTabs`: replace every tab with three spaces.
pub fn replace_tabs(text: &JsString) -> JsString {
    text.replace("\t", "   ")
}

/// `normalizeDisplayText`: drop every carriage return.
pub fn normalize_display_text(text: &JsString) -> JsString {
    text.replace("\r", "")
}

/// `linkPath`: wrap styled text in an OSC 8 link when the terminal supports hyperlinks.
pub fn link_path(styled_text: &JsString, raw_path: &JsString, cwd: &JsString) -> JsString {
    if !get_capabilities().hyperlinks {
        return styled_text.to_owned();
    }
    let absolute_path =
        resolve_path(raw_path, cwd, &PathInputOptions::default()).expect("tool render cwd must resolve");
    hyperlink(styled_text, &path_to_file_url(&absolute_path))
}

/// Node `pathToFileURL` file URL for an absolute path (or `cwd`-joined relative path).
fn path_to_file_url(filename: &JsString) -> JsString {
    let path = encode_node_utf8(&crate::utils::paths::node_resolve("", filename).expect("file URL path must resolve"));

    let mut encoded = String::from("file://");
    for byte in path.bytes() {
        let allowed = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
                    | b'/'
            );
        if allowed {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded.into()
}

/// `getTextOutput`: join text blocks and append image fallbacks when images cannot be shown.
pub fn get_text_output(result: Option<&AgentToolResult>, show_images: bool) -> JsString {
    let Some(result) = result else {
        return JsString::default();
    };
    let content = &result.content;
    let text_blocks = content
        .iter()
        .filter_map(|block| match block {
            TextOrImageContent::Text(text) => Some(text),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>();
    let image_blocks = content
        .iter()
        .filter_map(|block| match block {
            TextOrImageContent::Image(image) => Some(image),
            TextOrImageContent::Text(_) => None,
        })
        .collect::<Vec<_>>();

    let mut output = JsString::join(
        text_blocks
            .iter()
            .map(|text| normalize_display_text(&sanitize_binary_output(&strip_ansi(&text.text)))),
        "\n",
    );

    let capabilities = get_capabilities();
    if !image_blocks.is_empty() && (capabilities.images.is_none() || !show_images) {
        let image_indicators = JsString::join(
            image_blocks.iter().map(|image| {
                let dimensions = get_image_dimensions(&image.data, &image.mime_type);
                image_fallback(&image.mime_type, dimensions, None)
            }),
            "\n",
        );
        output = if output.is_empty() {
            image_indicators
        } else {
            js_format!("{}\n{}", output, image_indicators)
        };
    }

    output
}

fn home_directory() -> JsString {
    #[cfg(windows)]
    {
        pi_js_compat::process_env::get("USERPROFILE").unwrap_or_default().into()
    }
    #[cfg(not(windows))]
    {
        pi_js_compat::process_env::get("HOME").unwrap_or_default().into()
    }
}

/// `invalidArgText`: the error-styled invalid-argument marker.
pub fn invalid_arg_text(theme: &Theme) -> JsString {
    theme.fg(ThemeColor::Error, "[invalid arg]")
}

/// `renderToolPath`: accent-styled shortened path wrapped in a file link.
pub fn render_tool_path(
    raw_path: Option<&JsString>,
    theme: &Theme,
    cwd: &JsString,
    empty_fallback: Option<&str>,
) -> JsString {
    let Some(raw_path) = raw_path else {
        return invalid_arg_text(theme);
    };
    let value = if raw_path.is_empty() {
        JsString::from(empty_fallback.unwrap_or(""))
    } else {
        raw_path.clone()
    };
    if value.is_empty() {
        return theme.fg(ThemeColor::ToolOutput, "...");
    }
    link_path(&theme.fg(ThemeColor::Accent, &shorten_path(&value)), &value, cwd)
}
