//! Syntax highlighting for tool output and markdown code blocks.

mod hljs;
mod js_regex;

use std::collections::HashMap;
use std::sync::Arc;

use crate::utils::html::decode_html_entity_at;

/// Callback that styles one text run.
pub type HighlightFormatter = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Scope name to formatter mapping.
pub type HighlightTheme = HashMap<String, HighlightFormatter>;

/// Highlight options mirroring the source `HighlightOptions`.
#[derive(Clone, Default)]
pub struct HighlightOptions {
    pub language: Option<String>,
    pub ignore_illegals: Option<bool>,
    pub language_subset: Option<Vec<String>>,
    pub theme: HighlightTheme,
}

const SPAN_CLOSE: &str = "</span>";
const HIGHLIGHT_CLASS_PREFIX: &str = "hljs-";

/// Applies theme formatters to the HTML produced by highlight.js.
pub fn render_highlighted_html(html: &str, theme: &HighlightTheme) -> String {
    let mut output = String::new();
    let mut text_buffer = String::new();
    let mut scopes: Vec<Option<String>> = Vec::new();

    let flush_text = |output: &mut String, text_buffer: &mut String, scopes: &[Option<String>]| {
        if text_buffer.is_empty() {
            return;
        }
        match active_formatter(scopes, theme) {
            Some(formatter) => output.push_str(&formatter(text_buffer)),
            None => output.push_str(text_buffer),
        }
        text_buffer.clear();
    };

    let mut index = 0;
    while index < html.len() {
        if is_span_open_tag_start(html, index)
            && let Some(relative_end) = html[index + 5..].find('>')
        {
            let tag_end = index + 5 + relative_end;
            flush_text(&mut output, &mut text_buffer, &scopes);
            let scope = scope_from_span_tag(&html[index..=tag_end]);
            scopes.push(scope);
            index = tag_end + 1;
            continue;
        }

        if html[index..].starts_with(SPAN_CLOSE) {
            flush_text(&mut output, &mut text_buffer, &scopes);
            scopes.pop();
            index += SPAN_CLOSE.len();
            continue;
        }

        if html[index..].starts_with('&')
            && let Some(decoded) = decode_html_entity_at(html, index)
        {
            text_buffer.push_str(&decoded.text);
            index += decoded.length;
            continue;
        }

        let character = html[index..].chars().next().expect("index is on a character boundary");
        text_buffer.push(character);
        index += character.len_utf8();
    }

    flush_text(&mut output, &mut text_buffer, &scopes);
    output
}

/// Highlights `code` and renders it through the theme.
///
/// When `options.language` is absent, auto-detection runs over the registered
/// languages (or `options.language_subset`).
pub fn highlight(code: &str, options: &HighlightOptions) -> String {
    let html = match &options.language {
        Some(language) => hljs::highlight(language, code, options.ignore_illegals.unwrap_or(false))
            .map_or_else(|| escape_html(code), |result| result.value),
        None => hljs::highlight_auto(code, options.language_subset.as_deref()).value,
    };
    render_highlighted_html(&html, &options.theme)
}

/// True when highlight.js registers the language name or alias.
pub fn supports_language(name: &str) -> bool {
    hljs::supports_language(name)
}

fn active_formatter(scopes: &[Option<String>], theme: &HighlightTheme) -> Option<HighlightFormatter> {
    for scope in scopes.iter().rev() {
        let Some(scope) = scope else {
            continue;
        };
        if scope.is_empty() {
            continue;
        }
        if let Some(formatter) = scope_formatter(scope, theme) {
            return Some(formatter);
        }
    }
    theme.get("default").cloned()
}

fn scope_formatter(scope: &str, theme: &HighlightTheme) -> Option<HighlightFormatter> {
    if let Some(formatter) = theme.get(scope) {
        return Some(formatter.clone());
    }
    if let Some(dot) = scope.find('.')
        && let Some(formatter) = theme.get(&scope[..dot])
    {
        return Some(formatter.clone());
    }
    if let Some(dash) = scope.find('-')
        && let Some(formatter) = theme.get(&scope[..dash])
    {
        return Some(formatter.clone());
    }
    None
}

fn is_span_open_tag_start(html: &str, index: usize) -> bool {
    if !html[index..].starts_with("<span") {
        return false;
    }
    matches!(
        html[index + "<span".len()..].chars().next(),
        Some('>' | ' ' | '\t' | '\n' | '\r')
    )
}

/// Extracts the first `hljs-` scope from a span tag's `class` attribute.
fn scope_from_span_tag(tag: &str) -> Option<String> {
    let class_value = class_attribute(tag)?;
    class_value
        .split_whitespace()
        .find_map(|class_name| class_name.strip_prefix(HIGHLIGHT_CLASS_PREFIX))
        .map(str::to_owned)
}

/// Mirrors `/\sclass\s*=\s*(?:"([^"]*)"|'([^']*)')/` on one tag.
fn class_attribute(tag: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut index = 0;
    while index + "class".len() <= bytes.len() {
        if &bytes[index..index + "class".len()] != b"class" {
            index += 1;
            continue;
        }
        let preceded_by_space = index > 0 && tag[..index].chars().next_back().is_some_and(char::is_whitespace);
        if !preceded_by_space {
            index += "class".len();
            continue;
        }
        let mut cursor = index + "class".len();
        while tag[cursor..].chars().next().is_some_and(char::is_whitespace) {
            cursor += tag[cursor..].chars().next().expect("checked").len_utf8();
        }
        if !tag[cursor..].starts_with('=') {
            index += "class".len();
            continue;
        }
        cursor += 1;
        while tag[cursor..].chars().next().is_some_and(char::is_whitespace) {
            cursor += tag[cursor..].chars().next().expect("checked").len_utf8();
        }
        let quote = tag[cursor..].chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        cursor += quote.len_utf8();
        let end = tag[cursor..].find(quote)?;
        return Some(tag[cursor..cursor + end].to_owned());
    }
    None
}

fn escape_html(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            other => output.push(other),
        }
    }
    output
}
