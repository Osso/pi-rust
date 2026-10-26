//! Port of `packages/tui/src/components/markdown.ts`.
//!
//! Rendering walks the ported marked token stream and applies a caller-provided
//! [`MarkdownTheme`]; the token stream itself lives in [`lexer`].

pub mod lexer;
pub mod tokens;

use std::cell::RefCell;
use std::rc::Rc;

use pi_js_compat::js_json::JsString;

use crate::terminal_image::{get_capabilities, hyperlink, is_image_line};
use crate::tui::Component;
use crate::utils::{apply_background_to_line, visible_width, wrap_text_with_ansi};

use self::tokens::{ListItem, ListStart, TableCell, Token};

/// Caller-supplied text styling function.
pub type StyleFn = Box<dyn Fn(&JsString) -> JsString>;

/// Default text styling for markdown content.
#[derive(Default)]
pub struct DefaultTextStyle {
    pub color: Option<StyleFn>,
    pub bg_color: Option<StyleFn>,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
}

/// Theme functions for markdown elements. Each returns styled text with ANSI codes.
pub trait MarkdownTheme {
    fn heading(&self, text: &JsString) -> JsString;
    fn link(&self, text: &JsString) -> JsString;
    fn link_url(&self, text: &JsString) -> JsString;
    fn code(&self, text: &JsString) -> JsString;
    fn code_block(&self, text: &JsString) -> JsString;
    fn code_block_border(&self, text: &JsString) -> JsString;
    fn quote(&self, text: &JsString) -> JsString;
    fn quote_border(&self, text: &JsString) -> JsString;
    fn hr(&self, text: &JsString) -> JsString;
    fn list_bullet(&self, text: &JsString) -> JsString;
    fn bold(&self, text: &JsString) -> JsString;
    fn italic(&self, text: &JsString) -> JsString;
    fn strikethrough(&self, text: &JsString) -> JsString;
    fn underline(&self, text: &JsString) -> JsString;

    fn mermaid_border(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn mermaid_text(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn mermaid_edge(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn mermaid_edge_label(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn mermaid_warning(&self, text: &JsString) -> JsString {
        text.clone()
    }

    fn highlight_code(&self, _code: &JsString, _lang: Option<&JsString>) -> Option<Vec<JsString>> {
        None
    }

    fn code_block_indent(&self) -> JsString {
        "  ".into()
    }
}

/// Options mirroring `MarkdownOptions`.
#[derive(Default)]
pub struct MarkdownOptions {
    pub preserve_ordered_list_markers: bool,
    pub preserve_backslash_escapes: bool,
}

struct InlineStyleContext<'a> {
    apply_text: &'a dyn Fn(&JsString) -> JsString,
    style_prefix: JsString,
}

struct TokenRenderCache {
    next_token_type: Option<String>,
    width: usize,
    fingerprint: String,
    wrapped_lines: Vec<JsString>,
}

pub struct Markdown {
    text: JsString,
    padding_x: usize,
    padding_y: usize,
    theme: Rc<dyn MarkdownTheme>,
    default_text_style: Option<DefaultTextStyle>,
    options: MarkdownOptions,
    cached_text: Option<JsString>,
    cached_width: Option<usize>,
    cached_lines: Option<Vec<JsString>>,
    previous_token_caches: Vec<TokenRenderCache>,
    mermaid_enabled: bool,
    mermaid_streaming: bool,
    default_style_prefix: RefCell<Option<JsString>>,
}

impl Markdown {
    pub fn new(
        text: impl Into<JsString>,
        padding_x: usize,
        padding_y: usize,
        theme: Box<dyn MarkdownTheme>,
        default_text_style: Option<DefaultTextStyle>,
        options: MarkdownOptions,
    ) -> Self {
        Self::with_shared_theme(text, padding_x, padding_y, Rc::from(theme), default_text_style, options)
    }

    /// Same as [`Markdown::new`] for a theme shared with the caller.
    pub fn with_shared_theme(
        text: impl Into<JsString>,
        padding_x: usize,
        padding_y: usize,
        theme: Rc<dyn MarkdownTheme>,
        default_text_style: Option<DefaultTextStyle>,
        options: MarkdownOptions,
    ) -> Self {
        Self {
            text: text.into(),
            padding_x,
            padding_y,
            theme,
            default_text_style,
            options,
            cached_text: None,
            cached_width: None,
            cached_lines: None,
            previous_token_caches: Vec::new(),
            mermaid_enabled: false,
            mermaid_streaming: false,
            default_style_prefix: RefCell::new(None),
        }
    }

    pub fn set_mermaid_rendering(&mut self, enabled: bool, is_streaming: bool) {
        if (self.mermaid_enabled, self.mermaid_streaming) != (enabled, is_streaming) {
            self.mermaid_enabled = enabled;
            self.mermaid_streaming = is_streaming;
            self.invalidate();
        }
    }

    pub fn set_text(&mut self, text: impl Into<JsString>) {
        let text = text.into();
        if self.text == text {
            return;
        }
        self.text = text;
        self.clear_render_cache();
    }

    pub fn set_padding(&mut self, padding_x: usize, padding_y: usize) {
        if self.padding_x == padding_x && self.padding_y == padding_y {
            return;
        }
        self.padding_x = padding_x;
        self.padding_y = padding_y;
        self.clear_render_cache();
    }

    pub fn set_theme(&mut self, theme: Rc<dyn MarkdownTheme>) {
        if Rc::ptr_eq(&self.theme, &theme) {
            return;
        }
        self.theme = theme;
        *self.default_style_prefix.borrow_mut() = None;
        self.invalidate();
    }

    fn clear_render_cache(&mut self) {
        self.cached_text = None;
        self.cached_width = None;
        self.cached_lines = None;
    }

    fn render_cached_tokens(&mut self, tokens: &[Token], width: usize) -> Vec<JsString> {
        let mut caches = Vec::with_capacity(tokens.len());
        let mut lines = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let next_type = tokens.get(index + 1).map(lexer::type_name);
            let fingerprint = serde_json::to_string(token).expect("Markdown token serialization");
            let previous = self.previous_token_caches.get(index).filter(|cache| {
                cache.width == width
                    && cache.next_token_type.as_deref() == next_type
                    && cache.fingerprint == fingerprint
            });
            let wrapped_lines = match previous {
                Some(cache) => cache.wrapped_lines.clone(),
                None => self.render_and_wrap_token(token, width, next_type),
            };
            lines.extend(wrapped_lines.iter().cloned());
            caches.push(TokenRenderCache {
                next_token_type: next_type.map(str::to_string),
                width,
                fingerprint,
                wrapped_lines,
            });
        }
        self.previous_token_caches = caches;
        lines
    }

    fn render_and_wrap_token(&self, token: &Token, width: usize, next_type: Option<&str>) -> Vec<JsString> {
        self.render_mermaid(token, width)
            .unwrap_or_else(|| self.render_token(token, width, next_type, None))
            .into_iter()
            .flat_map(|line| {
                if is_image_line(&line) {
                    vec![line]
                } else {
                    wrap_text_with_ansi(&line, width)
                }
            })
            .collect()
    }

    fn render_mermaid(&self, token: &Token, width: usize) -> Option<Vec<JsString>> {
        if !self.mermaid_enabled {
            return None;
        }
        let Token::Code {
            lang: Some(lang), text, ..
        } = token
        else {
            return None;
        };
        let language = lang.to_utf8().ok()?;
        if !language.split_whitespace().next()?.eq_ignore_ascii_case("mermaid") {
            return None;
        }
        let art = crate::mermaid::render(&text.to_utf8().ok()?)?;
        if art.width > width {
            return None;
        }
        if !self.mermaid_streaming {
            if let Some(warnings) = art.warnings.as_ref().filter(|warnings| !warnings.is_empty()) {
                let suffix = if warnings.len() > 1 {
                    format!(" (+{} more)", warnings.len() - 1)
                } else {
                    String::new()
                };
                let warning = JsString::from(format!("Mermaid diagram not rendered: {}{}", warnings[0], suffix));
                let mut lines = self.render_token(token, width, None, None);
                lines.push(JsString::default());
                lines.push(self.theme.code(&self.theme.mermaid_warning(&warning)));
                return Some(lines);
            }
        }
        let lines: Vec<JsString> = art
            .styled_lines
            .into_iter()
            .map(|line| {
                let mut result = JsString::default();
                for span in line.spans {
                    let text = JsString::from(span.text);
                    let styled = match span.style {
                        crate::mermaid::Style::Empty => text,
                        crate::mermaid::Style::Border => self.theme.mermaid_border(&text),
                        crate::mermaid::Style::Text => self.theme.mermaid_text(&text),
                        crate::mermaid::Style::Edge => self.theme.mermaid_edge(&text),
                        crate::mermaid::Style::EdgeLabel => self.theme.mermaid_edge_label(&text),
                    };
                    result.push_str(styled);
                }
                if result.is_empty() {
                    result = JsString::from("\u{00a0}");
                }
                self.theme.code(&result)
            })
            .collect();
        Some(lines)
    }

    pub fn text(&self) -> &JsString {
        &self.text
    }

    fn render_content(&mut self, width: usize) -> Vec<JsString> {
        let content_width = width.saturating_sub(self.padding_x * 2).max(1);
        let text = self.text.clone();

        if text.trim().is_empty() {
            return Vec::new();
        }

        let normalized = text.replace('\t', "   ");
        let mut tokens = lexer::lex(&normalized);
        trim_partial_closing_fences(&mut tokens);

        let wrapped_lines = self.render_cached_tokens(&tokens, content_width);

        let left_margin = " ".repeat(self.padding_x);
        let right_margin = left_margin.clone();
        let background = self
            .default_text_style
            .as_ref()
            .and_then(|style| style.bg_color.as_ref());
        let mut content_lines: Vec<JsString> = Vec::new();

        for line in wrapped_lines {
            if is_image_line(&line) {
                content_lines.push(line);
                continue;
            }
            let line_with_margins = JsString::from(left_margin.clone()) + line + JsString::from(right_margin.clone());
            if let Some(background) = background {
                content_lines.push(apply_background_to_line(&line_with_margins, width, |text| {
                    background(text)
                }));
            } else {
                let visible_len = visible_width(&line_with_margins);
                let padding = width.saturating_sub(visible_len);
                content_lines.push(line_with_margins + JsString::from(" ".repeat(padding)));
            }
        }

        let empty_line = JsString::from(" ".repeat(width));
        let mut empty_lines: Vec<JsString> = Vec::new();
        for _ in 0..self.padding_y {
            empty_lines.push(match background {
                Some(background) => apply_background_to_line(&empty_line, width, |text| background(text)),
                None => empty_line.clone(),
            });
        }

        let mut result = empty_lines.clone();
        result.extend(content_lines);
        result.extend(empty_lines);
        if result.is_empty() {
            result.push(JsString::default());
        }
        result
    }

    fn apply_default_style(&self, text: &JsString) -> JsString {
        let Some(style) = &self.default_text_style else {
            return text.clone();
        };
        let mut styled = text.clone();
        if let Some(color) = &style.color {
            styled = color(&styled);
        }
        if style.bold {
            styled = self.theme.bold(&styled);
        }
        if style.italic {
            styled = self.theme.italic(&styled);
        }
        if style.strikethrough {
            styled = self.theme.strikethrough(&styled);
        }
        if style.underline {
            styled = self.theme.underline(&styled);
        }
        styled
    }

    fn default_style_prefix(&self) -> JsString {
        let Some(style) = &self.default_text_style else {
            return JsString::default();
        };
        if let Some(prefix) = self.default_style_prefix.borrow().as_ref() {
            return prefix.clone();
        }
        let sentinel = "\u{0}";
        let mut styled = JsString::from(sentinel);
        if let Some(color) = &style.color {
            styled = color(&styled);
        }
        if style.bold {
            styled = self.theme.bold(&styled);
        }
        if style.italic {
            styled = self.theme.italic(&styled);
        }
        if style.strikethrough {
            styled = self.theme.strikethrough(&styled);
        }
        if style.underline {
            styled = self.theme.underline(&styled);
        }
        let prefix = match styled.find(sentinel) {
            Some(index) => styled.slice(0, index).clone(),
            None => JsString::default(),
        };
        *self.default_style_prefix.borrow_mut() = Some(prefix.clone());
        prefix
    }

    fn style_prefix(&self, style: &dyn Fn(&JsString) -> JsString) -> JsString {
        let sentinel = "\u{0}";
        let styled = style(&JsString::from(sentinel));
        match styled.find(sentinel) {
            Some(index) => styled.slice(0, index).clone(),
            None => JsString::default(),
        }
    }

    fn render_token(
        &self,
        token: &Token,
        width: usize,
        next_token_type: Option<&str>,
        style_context: Option<&InlineStyleContext>,
    ) -> Vec<JsString> {
        let mut lines: Vec<JsString> = Vec::new();
        match token {
            Token::Heading { depth, tokens, .. } => {
                let heading_prefix = JsString::from("#".repeat(*depth as usize)) + JsString::from(" ");
                let heading_style = |text: &JsString| {
                    if *depth == 1 {
                        self.theme.heading(&self.theme.bold(&self.theme.underline(text)))
                    } else {
                        self.theme.heading(&self.theme.bold(text))
                    }
                };
                let context = InlineStyleContext {
                    apply_text: &heading_style,
                    style_prefix: self.style_prefix(&heading_style),
                };
                let heading_text = self.render_inline_tokens(tokens, Some(&context));
                let styled = if *depth >= 3 {
                    heading_style(&heading_prefix) + heading_text
                } else {
                    heading_text
                };
                lines.push(styled);
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(JsString::default());
                }
            }
            Token::Paragraph { tokens, .. } => {
                lines.push(self.render_inline_tokens(tokens, style_context));
                if next_token_type.is_some_and(|next| next != "list" && next != "space") {
                    lines.push(JsString::default());
                }
            }
            Token::Text { tokens, .. } => {
                let _ = tokens;
                lines.push(self.render_inline_tokens(std::slice::from_ref(token), style_context));
            }
            Token::Code { lang, text, .. } => {
                let indent = self.theme.code_block_indent().clone();
                lines.push(
                    self.theme
                        .code_block_border(&(JsString::from("```") + lang.clone().unwrap_or_default())),
                );
                if let Some(highlighted) = self.theme.highlight_code(text, lang.as_ref()) {
                    for line in highlighted {
                        lines.push(indent.clone() + line);
                    }
                } else {
                    for line in text.split('\n') {
                        lines.push(indent.clone() + self.theme.code_block(&line));
                    }
                }
                lines.push(self.theme.code_block_border(&"```".into()));
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(JsString::default());
                }
            }
            Token::List { .. } => {
                lines.extend(self.render_list(token, 0, width, style_context));
            }
            Token::Table { header, rows, raw, .. } => {
                lines.extend(self.render_table(header, rows, raw, width, next_token_type, style_context));
            }
            Token::Blockquote { tokens, .. } => {
                let quote_style = |text: &JsString| self.theme.quote(&self.theme.italic(text));
                let quote_style_prefix = self.style_prefix(&quote_style);
                let apply_quote_style = |line: &JsString| {
                    if quote_style_prefix.is_empty() {
                        quote_style(line)
                    } else {
                        let reapplied = line.replace("\u{1b}[0m", JsString::from("\x1b[0m") + &quote_style_prefix);
                        quote_style(&reapplied)
                    }
                };
                let quote_content_width = width.saturating_sub(2).max(1);
                let plain = |text: &JsString| text.clone();
                let quote_context = InlineStyleContext {
                    apply_text: &plain,
                    style_prefix: quote_style_prefix.clone(),
                };
                let mut rendered_quote_lines: Vec<JsString> = Vec::new();
                for (index, quote_token) in tokens.iter().enumerate() {
                    let next = tokens.get(index + 1).map(lexer::type_name);
                    rendered_quote_lines.extend(self.render_token(
                        quote_token,
                        quote_content_width,
                        next,
                        Some(&quote_context),
                    ));
                }
                while rendered_quote_lines.last().is_some_and(JsString::is_empty) {
                    rendered_quote_lines.pop();
                }
                for quote_line in rendered_quote_lines {
                    let styled = apply_quote_style(&quote_line);
                    for wrapped in wrap_text_with_ansi(&styled, quote_content_width) {
                        lines.push(self.theme.quote_border(&"│ ".into()) + wrapped);
                    }
                }
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(JsString::default());
                }
            }
            Token::Hr { .. } => {
                lines.push(self.theme.hr(&"─".repeat(width.min(80)).into()));
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(JsString::default());
                }
            }
            Token::Html { raw, .. } => {
                if !raw.is_empty() && !is_html_comment_only(raw) {
                    lines.push(self.apply_default_style(&raw.trim()));
                }
            }
            Token::Space { .. } => lines.push(JsString::default()),
            _ => {
                if let Some(text) = token.has_text() {
                    lines.push(text.clone());
                }
            }
        }
        lines
    }

    fn render_inline_tokens(&self, tokens: &[Token], style_context: Option<&InlineStyleContext>) -> JsString {
        let default_apply = |text: &JsString| self.apply_default_style(text);
        let default_context;
        let context = match style_context {
            Some(context) => context,
            None => {
                default_context = InlineStyleContext {
                    apply_text: &default_apply,
                    style_prefix: self.default_style_prefix(),
                };
                &default_context
            }
        };
        let apply_text = context.apply_text;
        let apply_text_with_newlines =
            |text: &JsString| JsString::join(text.split('\n').map(|line| apply_text(&line)), "\n");

        let mut result = JsString::default();
        for token in tokens {
            match token {
                Token::Escape { raw, text } => {
                    result.push_str(apply_text_with_newlines(if self.options.preserve_backslash_escapes {
                        raw
                    } else {
                        text
                    }));
                }
                Token::Text { text, tokens, .. } => {
                    if let Some(nested) = tokens.as_ref().filter(|nested| !nested.is_empty()) {
                        result.push_str(self.render_inline_tokens(nested, Some(context)));
                    } else {
                        result.push_str(apply_text_with_newlines(text));
                    }
                }
                Token::Paragraph { tokens, .. } => {
                    result.push_str(self.render_inline_tokens(tokens, Some(context)));
                }
                Token::Strong { tokens, .. } => {
                    let content = self.render_inline_tokens(tokens, Some(context));
                    result.push_str(self.theme.bold(&content));
                    result.push_str(&context.style_prefix);
                }
                Token::Em { tokens, .. } => {
                    let content = self.render_inline_tokens(tokens, Some(context));
                    result.push_str(self.theme.italic(&content));
                    result.push_str(&context.style_prefix);
                }
                Token::Codespan { text, .. } => {
                    result.push_str(self.theme.code(text));
                    result.push_str(&context.style_prefix);
                }
                Token::Link { href, text, tokens, .. } => {
                    let link_text = self.render_inline_tokens(tokens, Some(context));
                    let styled_link = self.theme.link(&self.theme.underline(&link_text));
                    if get_capabilities().hyperlinks {
                        result.push_str(hyperlink(&styled_link, href));
                        result.push_str(&context.style_prefix);
                    } else {
                        let href_for_comparison = href.strip_prefix("mailto:").unwrap_or_else(|| href.clone());
                        if text == href || *text == href_for_comparison {
                            result.push_str(&styled_link);
                        } else {
                            result.push_str(&styled_link);
                            result.push_str(
                                self.theme
                                    .link_url(&(JsString::from(" (") + JsString::from(href) + JsString::from(")"))),
                            );
                        }
                        result.push_str(&context.style_prefix);
                    }
                }
                Token::Br { .. } => result.push_str("\n"),
                Token::Del { tokens, .. } => {
                    let content = self.render_inline_tokens(tokens, Some(context));
                    result.push_str(self.theme.strikethrough(&content));
                    result.push_str(&context.style_prefix);
                }
                Token::Html { raw, .. } => {
                    if !raw.is_empty() && !is_html_comment_only(raw) {
                        result.push_str(apply_text_with_newlines(raw));
                    }
                }
                _ => {
                    if let Some(text) = token.has_text() {
                        result.push_str(apply_text_with_newlines(text));
                    }
                }
            }
        }

        let prefix = &context.style_prefix;
        while !prefix.is_empty() && result.ends_with(prefix) {
            result = result.slice(0, result.len() - prefix.len());
        }
        result
    }

    fn render_list(
        &self,
        token: &Token,
        depth: usize,
        width: usize,
        style_context: Option<&InlineStyleContext>,
    ) -> Vec<JsString> {
        let Token::List {
            ordered,
            start,
            items,
            loose,
            ..
        } = token
        else {
            return Vec::new();
        };
        let mut lines: Vec<JsString> = Vec::new();
        let indent = "    ".repeat(depth);
        let start_number = match start {
            ListStart::Number(value) => *value as usize,
            ListStart::Empty(_) => 1,
        };

        for (index, item) in items.iter().enumerate() {
            let is_last_item = index + 1 == items.len();
            let bullet = if *ordered {
                if self.options.preserve_ordered_list_markers {
                    self.ordered_list_marker(item)
                        .unwrap_or_else(|| JsString::from((start_number + index).to_string()) + JsString::from(". "))
                } else {
                    JsString::from((start_number + index).to_string()) + JsString::from(". ")
                }
            } else if self.options.preserve_ordered_list_markers {
                self.unordered_list_marker(item).unwrap_or_else(|| JsString::from("- "))
            } else {
                JsString::from("- ")
            };
            let task_marker = if item.task {
                JsString::from("[")
                    + JsString::from(if item.checked == Some(true) { "x" } else { " " })
                    + JsString::from("] ")
            } else {
                JsString::default()
            };
            let marker = bullet + task_marker;
            let first_prefix = JsString::from(indent.clone()) + self.theme.list_bullet(&marker);
            let continuation_prefix =
                JsString::from(indent.clone()) + JsString::from(" ".repeat(visible_width(&marker)));
            let item_width = width.saturating_sub(visible_width(&first_prefix)).max(1);
            let mut rendered_any_line = false;

            for item_token in &item.tokens {
                if matches!(item_token, Token::List { .. }) {
                    lines.extend(self.render_list(item_token, depth + 1, width, style_context));
                    rendered_any_line = true;
                    continue;
                }
                let item_lines = self.render_token(item_token, item_width, None, style_context);
                for line in item_lines {
                    for wrapped in wrap_text_with_ansi(&line, item_width) {
                        let prefix = if rendered_any_line {
                            &continuation_prefix
                        } else {
                            &first_prefix
                        };
                        lines.push(JsString::from(prefix) + wrapped);
                        rendered_any_line = true;
                    }
                }
            }

            if !rendered_any_line {
                lines.push(first_prefix);
            }
            if *loose && !is_last_item {
                lines.push(JsString::default());
            }
        }

        lines
    }

    fn ordered_list_marker(&self, item: &ListItem) -> Option<JsString> {
        let bytes = item.raw.units();
        let mut index = 0;
        while index < 3 && bytes.get(index) == Some(&32) {
            index += 1;
        }
        let digits_start = index;
        while bytes.get(index).is_some_and(|unit| (48..=57).contains(unit)) {
            index += 1;
        }
        let digits = index - digits_start;
        if digits == 0 || digits > 9 {
            return None;
        }
        if !matches!(bytes.get(index), Some(46) | Some(41)) {
            return None;
        }
        index += 1;
        if !matches!(bytes.get(index), Some(32) | Some(9)) {
            return None;
        }
        Some(item.raw.slice(digits_start, index) + JsString::from(" "))
    }

    fn unordered_list_marker(&self, item: &ListItem) -> Option<JsString> {
        let bytes = item.raw.units();
        let mut index = 0;
        while index < 3 && bytes.get(index) == Some(&32) {
            index += 1;
        }
        let marker = *bytes.get(index)?;
        if !matches!(marker, 45 | 43 | 42) {
            return None;
        }
        index += 1;
        let rest = item.raw.slice(index, item.raw.len());
        let followed = match rest.units().first().copied() {
            Some(32) | Some(9) => true,
            None => true,
            Some(13) => rest.starts_with("\r\n") || rest == "\r",
            Some(10) => true,
            _ => false,
        };
        followed.then(|| {
            JsString::from(char::from_u32(u32::from(marker)).expect("ASCII list marker")) + JsString::from(" ")
        })
    }

    fn get_longest_word_width(text: &JsString, max_width: Option<usize>) -> usize {
        let longest = text
            .units()
            .split(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
            .map(|units| JsString::from_utf16(units.to_vec()))
            .filter(|word| !word.is_empty())
            .map(|word| visible_width(&word))
            .max()
            .unwrap_or(0);
        match max_width {
            Some(max_width) => longest.min(max_width),
            None => longest,
        }
    }

    fn wrap_cell_text(&self, text: &JsString, max_width: usize) -> Vec<JsString> {
        wrap_text_with_ansi(text, max_width.max(1))
    }

    fn render_table(
        &self,
        header: &[TableCell],
        rows: &[Vec<TableCell>],
        raw: &JsString,
        available_width: usize,
        next_token_type: Option<&str>,
        style_context: Option<&InlineStyleContext>,
    ) -> Vec<JsString> {
        let mut lines: Vec<JsString> = Vec::new();
        let num_cols = header.len();
        if num_cols == 0 {
            return lines;
        }

        let border_overhead = 3 * num_cols + 1;
        let available_for_cells = available_width as isize - border_overhead as isize;
        if available_for_cells < num_cols as isize {
            let mut fallback_lines = if raw.is_empty() {
                Vec::new()
            } else {
                wrap_text_with_ansi(raw, available_width)
            };
            if next_token_type.is_some_and(|next| next != "space") {
                fallback_lines.push(JsString::default());
            }
            return fallback_lines;
        }
        let available_for_cells = available_for_cells as usize;

        const MAX_UNBROKEN_WORD_WIDTH: usize = 30;
        let mut natural_widths: Vec<usize> = vec![0; num_cols];
        let mut min_word_widths: Vec<usize> = vec![0; num_cols];
        for (index, cell) in header.iter().enumerate() {
            let text = self.render_inline_tokens(&cell.tokens, style_context);
            natural_widths[index] = visible_width(&text);
            min_word_widths[index] = Self::get_longest_word_width(&text, Some(MAX_UNBROKEN_WORD_WIDTH)).max(1);
        }
        for row in rows {
            for (index, cell) in row.iter().enumerate() {
                let text = self.render_inline_tokens(&cell.tokens, style_context);
                natural_widths[index] = natural_widths[index].max(visible_width(&text));
                min_word_widths[index] =
                    min_word_widths[index].max(Self::get_longest_word_width(&text, Some(MAX_UNBROKEN_WORD_WIDTH)));
            }
        }

        let mut min_column_widths = min_word_widths.clone();
        let mut min_cells_width: usize = min_column_widths.iter().sum();
        if min_cells_width > available_for_cells {
            min_column_widths = vec![1; num_cols];
            let remaining = available_for_cells - num_cols;
            if remaining > 0 {
                let total_weight: usize = min_word_widths.iter().map(|width| width.saturating_sub(1)).sum();
                let growth: Vec<usize> = min_word_widths
                    .iter()
                    .map(|width| {
                        let weight = width.saturating_sub(1);
                        (weight * remaining).checked_div(total_weight).unwrap_or(0)
                    })
                    .collect();
                for (index, value) in growth.iter().enumerate() {
                    min_column_widths[index] += value;
                }
                let allocated: usize = growth.iter().sum();
                let mut leftover = remaining - allocated;
                let mut index = 0;
                while leftover > 0 && index < num_cols {
                    min_column_widths[index] += 1;
                    leftover -= 1;
                    index += 1;
                }
            }
            min_cells_width = min_column_widths.iter().sum();
        }

        let total_natural_width: usize = natural_widths.iter().sum::<usize>() + border_overhead;
        let mut column_widths: Vec<usize>;
        if total_natural_width <= available_width {
            column_widths = natural_widths
                .iter()
                .enumerate()
                .map(|(index, width)| (*width).max(min_column_widths[index]))
                .collect();
        } else {
            let total_grow_potential: usize = natural_widths
                .iter()
                .enumerate()
                .map(|(index, width)| width.saturating_sub(min_column_widths[index]))
                .sum();
            let extra_width = available_for_cells.saturating_sub(min_cells_width);
            column_widths = min_column_widths
                .iter()
                .enumerate()
                .map(|(index, min_width)| {
                    let delta = natural_widths[index].saturating_sub(*min_width);
                    let grow = (delta * extra_width).checked_div(total_grow_potential).unwrap_or(0);
                    min_width + grow
                })
                .collect();

            let allocated: usize = column_widths.iter().sum();
            let mut remaining = available_for_cells.saturating_sub(allocated);
            while remaining > 0 {
                let mut grew = false;
                for index in 0..num_cols {
                    if remaining > 0 && column_widths[index] < natural_widths[index] {
                        column_widths[index] += 1;
                        remaining -= 1;
                        grew = true;
                    }
                }
                if !grew {
                    break;
                }
            }
        }

        let top_cells: Vec<JsString> = column_widths
            .iter()
            .map(|width| JsString::from("─".repeat(*width)))
            .collect();
        lines.push(JsString::from("┌─") + JsString::join(top_cells, "─┬─") + JsString::from("─┐"));

        let header_cell_lines: Vec<Vec<JsString>> = header
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let text = self.render_inline_tokens(&cell.tokens, style_context);
                self.wrap_cell_text(&text, column_widths[index])
            })
            .collect();
        let header_line_count = header_cell_lines.iter().map(Vec::len).max().unwrap_or(0);
        for line_index in 0..header_line_count {
            let parts: Vec<JsString> = header_cell_lines
                .iter()
                .enumerate()
                .map(|(column, cell_lines)| {
                    let text = cell_lines.get(line_index).cloned().unwrap_or_default();
                    let padded = text.clone() + " ".repeat(column_widths[column].saturating_sub(visible_width(text)));
                    self.theme.bold(&padded)
                })
                .collect();
            lines.push(JsString::from("│ ") + JsString::join(parts, " │ ") + JsString::from(" │"));
        }

        let separator_cells: Vec<JsString> = column_widths
            .iter()
            .map(|width| JsString::from("─".repeat(*width)))
            .collect();
        let separator_line = JsString::from("├─") + JsString::join(separator_cells, "─┼─") + JsString::from("─┤");
        lines.push(separator_line.clone());

        for (row_index, row) in rows.iter().enumerate() {
            let row_cell_lines: Vec<Vec<JsString>> = row
                .iter()
                .enumerate()
                .map(|(index, cell)| {
                    let text = self.render_inline_tokens(&cell.tokens, style_context);
                    self.wrap_cell_text(&text, column_widths[index])
                })
                .collect();
            let row_line_count = row_cell_lines.iter().map(Vec::len).max().unwrap_or(0);
            for line_index in 0..row_line_count {
                let parts: Vec<JsString> = row_cell_lines
                    .iter()
                    .enumerate()
                    .map(|(column, cell_lines)| {
                        let text = cell_lines.get(line_index).cloned().unwrap_or_default();
                        text.clone() + " ".repeat(column_widths[column].saturating_sub(visible_width(text)))
                    })
                    .collect();
                lines.push(JsString::from("│ ") + JsString::join(parts, " │ ") + JsString::from(" │"));
            }
            if row_index + 1 < rows.len() {
                lines.push(separator_line.clone());
            }
        }

        let bottom_cells: Vec<JsString> = column_widths
            .iter()
            .map(|width| JsString::from("─".repeat(*width)))
            .collect();
        lines.push(JsString::from("└─") + JsString::join(bottom_cells, "─┴─") + JsString::from("─┘"));

        if next_token_type.is_some_and(|next| next != "space") {
            lines.push(JsString::default());
        }
        lines
    }
}

impl Component for Markdown {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        if self.cached_lines.is_some()
            && self.cached_text.as_ref() == Some(&self.text)
            && self.cached_width == Some(width)
        {
            return self.cached_lines.clone().unwrap_or_default();
        }

        let result = self.render_content(width);
        self.cached_text = Some(self.text.clone());
        self.cached_width = Some(width);
        self.cached_lines = Some(result.clone());
        result
    }

    fn invalidate(&mut self) {
        self.clear_render_cache();
        self.previous_token_caches.clear();
    }
}

fn is_html_comment_only(html: &JsString) -> bool {
    let trimmed = html.trim();
    trimmed.starts_with("<!--") && trimmed.ends_with("-->") && trimmed.len() >= 7
}

pub fn trim_partial_closing_fences(tokens: &mut [Token]) {
    let Some(token) = tokens.last_mut() else {
        return;
    };
    match token {
        Token::List { items, .. } => {
            if let Some(item) = items.last_mut() {
                trim_partial_closing_fences(&mut item.tokens);
            }
        }
        Token::Blockquote { tokens, .. } => trim_partial_closing_fences(tokens),
        Token::Code { raw, text, .. } => {
            let marker_length = raw
                .units()
                .iter()
                .copied()
                .take_while(|ch| *ch == 96 || *ch == 126)
                .count();
            if marker_length < 3 {
                return;
            }
            let marker = raw.slice(0, marker_length);
            let Some(last_line) = raw.split('\n').last() else {
                return;
            };
            let marker_character = marker.units().first().copied().unwrap_or(96);
            if last_line.is_empty()
                || last_line.len() >= marker.len()
                || !last_line.units().iter().copied().all(|ch| ch == marker_character)
            {
                return;
            }
            let remove = last_line.len().min(text.len());
            let mut trimmed = text.slice(0, text.len() - remove).clone();
            if trimmed.ends_with('\n') {
                trimmed = trimmed.slice(0, trimmed.len() - 1);
            }
            *text = trimmed;
        }
        _ => {}
    }
}
