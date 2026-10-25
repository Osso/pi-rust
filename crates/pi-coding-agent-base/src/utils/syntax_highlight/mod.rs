//! Syntax highlighting for tool output and markdown code blocks.

mod hljs;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::JsRegex;

use crate::utils::html::decode_html_entity_at;

/// Callback that styles one UTF-16 text run.
pub type HighlightFormatter = Arc<dyn Fn(&JsString) -> JsString + Send + Sync>;

/// Scope name to formatter mapping.
pub type HighlightTheme = HashMap<String, HighlightFormatter>;

#[derive(Clone, Default)]
pub struct HighlightOptions {
    pub language: Option<JsString>,
    pub ignore_illegals: Option<bool>,
    pub language_subset: Option<Vec<JsString>>,
    pub theme: HighlightTheme,
}

/// Exact message of the frozen highlighter's thrown Error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightError {
    pub message: JsString,
}

// ASCII tag delimiters, compared directly against borrowed UTF-16 slices.
const SPAN_OPEN: &[u16] = &[60, 115, 112, 97, 110]; // <span
const SPAN_CLOSE: &[u16] = &[60, 47, 115, 112, 97, 110, 62]; // </span>
const HIGHLIGHT_CLASS_PREFIX: &str = "hljs-";

/// Applies theme formatters to highlight.js HTML without encoding its text.
pub fn render_highlighted_html(html: &JsString, theme: &HighlightTheme) -> JsString {
    let mut output = JsString::default();
    let mut text_buffer = JsString::default();
    let mut scopes = Vec::new();
    let mut index = 0;
    while index < html.len() {
        if is_span_open_tag_start(html, index)
            && let Some(tag_end) = html.find_from(">", index + 5)
        {
            flush_text(&mut output, &mut text_buffer, &scopes, theme);
            scopes.push(scope_from_span_tag(&html.slice(index, tag_end + 1)));
            index = tag_end + 1;
            continue;
        }
        if html.units()[index..].starts_with(SPAN_CLOSE) {
            flush_text(&mut output, &mut text_buffer, &scopes, theme);
            scopes.pop();
            index += SPAN_CLOSE.len();
            continue;
        }
        if html.units()[index] == u16::from(b'&')
            && let Some(decoded) = decode_html_entity_at(html, index)
        {
            text_buffer.push_str(decoded.text);
            index += decoded.length;
            continue;
        }
        text_buffer.push_unit(html.units()[index]);
        index += 1;
    }
    flush_text(&mut output, &mut text_buffer, &scopes, theme);
    output
}

fn flush_text(output: &mut JsString, buffer: &mut JsString, scopes: &[Option<JsString>], theme: &HighlightTheme) {
    if buffer.is_empty() {
        return;
    }
    let text = std::mem::take(buffer);
    output.push_str(match active_formatter(scopes, theme) {
        Some(formatter) => formatter(&text),
        None => text,
    });
}

/// Returns raw highlight.js HTML, preserving the frozen unknown-language error.
pub fn highlight_html(code: &JsString, options: &HighlightOptions) -> Result<JsString, HighlightError> {
    match options.language.as_ref().filter(|language| !language.is_empty()) {
        Some(language) => hljs::highlight(language, code, options.ignore_illegals.unwrap_or(false))
            .map(|result| result.value)
            .ok_or_else(|| HighlightError {
                message: JsString::from("Unknown language: \"") + language + "\"",
            }),
        None => Ok(hljs::highlight_auto(code, options.language_subset.as_deref()).value),
    }
}

pub fn highlight(code: &JsString, options: &HighlightOptions) -> Result<JsString, HighlightError> {
    Ok(render_highlighted_html(&highlight_html(code, options)?, &options.theme))
}

pub fn supports_language(name: &JsString) -> bool {
    hljs::supports_language(name)
}

fn active_formatter(scopes: &[Option<JsString>], theme: &HighlightTheme) -> Option<HighlightFormatter> {
    for scope in scopes.iter().rev().flatten().filter(|scope| !scope.is_empty()) {
        if let Some(formatter) = scope_formatter(scope, theme) {
            return Some(formatter);
        }
    }
    theme.get("default").cloned()
}

fn scope_formatter(scope: &JsString, theme: &HighlightTheme) -> Option<HighlightFormatter> {
    if let Some(formatter) = theme_formatter(scope, theme) {
        return Some(formatter);
    }
    for separator in [".", "-"] {
        if let Some(index) = scope.find(separator)
            && let Some(formatter) = theme_formatter(&scope.slice(0, index), theme)
        {
            return Some(formatter);
        }
    }
    None
}

fn theme_formatter(scope: &JsString, theme: &HighlightTheme) -> Option<HighlightFormatter> {
    if let Some((_, formatter)) = theme.iter().find(|(key, _)| scope == *key) {
        return Some(formatter.clone());
    }
    // The frozen plain object inherits Object.prototype.constructor.
    (scope == "constructor").then(|| Arc::new(JsString::clone) as HighlightFormatter)
}

fn is_span_open_tag_start(html: &JsString, index: usize) -> bool {
    html.units()[index..].starts_with(SPAN_OPEN)
        && matches!(html.units().get(index + SPAN_OPEN.len()), Some(62 | 32 | 9 | 10 | 13))
}

fn scope_from_span_tag(tag: &JsString) -> Option<JsString> {
    static CLASS: LazyLock<JsRegex> =
        LazyLock::new(|| JsRegex::new(r#"\sclass\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("class attribute regex"));
    static WORD: LazyLock<JsRegex> = LazyLock::new(|| JsRegex::new(r"\S+").expect("class word regex"));
    let captures = CLASS.captures(tag)?;
    let class_value = captures.get(1).or_else(|| captures.get(2))?.text();
    WORD.find_iter(class_value)
        .find_map(|word| word.text().strip_prefix(HIGHLIGHT_CLASS_PREFIX))
}

fn escape_html(text: &JsString) -> JsString {
    let mut output = JsString::default();
    for unit in text.units() {
        match unit {
            38 => output.push_str("&amp;"),
            60 => output.push_str("&lt;"),
            62 => output.push_str("&gt;"),
            34 => output.push_str("&quot;"),
            39 => output.push_str("&#x27;"),
            other => output.push_unit(*other),
        }
    }
    output
}
