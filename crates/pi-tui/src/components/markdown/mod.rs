//! Port of `packages/tui/src/components/markdown.ts`.
//!
//! Rendering walks the ported marked token stream and applies a caller-provided
//! [`MarkdownTheme`]; the token stream itself lives in [`lexer`].

pub mod lexer;
pub mod tokens;

use crate::latex::{RenderLatexOptions, render_latex};
use crate::terminal_image::{get_capabilities, hyperlink, is_image_line};
use crate::tui::Component;
use crate::utils::{apply_background_to_line, visible_width, wrap_text_with_ansi};

use self::tokens::{ListItem, ListStart, TableCell, Token};

/// Caller-supplied text styling function.
pub type StyleFn = Box<dyn Fn(&str) -> String>;

/// Caller-supplied Markdown source transform.
pub type TransformFn = Box<dyn Fn(&str, usize) -> String>;

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
    fn heading(&self, text: &str) -> String;
    fn link(&self, text: &str) -> String;
    fn link_url(&self, text: &str) -> String;
    fn code(&self, text: &str) -> String;
    fn code_block(&self, text: &str) -> String;
    fn code_block_border(&self, text: &str) -> String;
    fn quote(&self, text: &str) -> String;
    fn quote_border(&self, text: &str) -> String;
    fn hr(&self, text: &str) -> String;
    fn list_bullet(&self, text: &str) -> String;
    fn bold(&self, text: &str) -> String;
    fn italic(&self, text: &str) -> String;
    fn strikethrough(&self, text: &str) -> String;
    fn underline(&self, text: &str) -> String;

    fn highlight_code(&self, _code: &str, _lang: Option<&str>) -> Option<Vec<String>> {
        None
    }

    fn code_block_indent(&self) -> &str {
        "  "
    }
}

/// Options mirroring `MarkdownOptions`.
pub struct MarkdownOptions {
    pub preserve_ordered_list_markers: bool,
    pub preserve_backslash_escapes: bool,
    pub transform: Option<TransformFn>,
    pub render_latex: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            preserve_ordered_list_markers: false,
            preserve_backslash_escapes: false,
            transform: None,
            render_latex: true,
        }
    }
}

struct InlineStyleContext<'a> {
    apply_text: &'a dyn Fn(&str) -> String,
    style_prefix: String,
}

pub struct Markdown {
    text: String,
    padding_x: usize,
    padding_y: usize,
    theme: Box<dyn MarkdownTheme>,
    default_text_style: Option<DefaultTextStyle>,
    options: MarkdownOptions,
    cached_text: Option<String>,
    cached_width: Option<usize>,
    cached_lines: Option<Vec<String>>,
}

impl Markdown {
    pub fn new(
        text: impl Into<String>,
        padding_x: usize,
        padding_y: usize,
        theme: Box<dyn MarkdownTheme>,
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
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.invalidate();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn render_content(&self, width: usize) -> Vec<String> {
        let content_width = width.saturating_sub(self.padding_x * 2).max(1);
        let text = match &self.options.transform {
            Some(transform) => transform(&self.text, content_width),
            None => self.text.clone(),
        };

        if lexer::js_trim(&text).is_empty() {
            return Vec::new();
        }

        let normalized = text.replace('\t', "   ");
        let mut tokens = lexer::lex(&normalized);
        trim_partial_closing_fences(&mut tokens);

        let mut rendered_lines: Vec<String> = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let next_type = tokens.get(index + 1).map(lexer::type_name);
            rendered_lines.extend(self.render_token(token, content_width, next_type, None));
        }

        let mut wrapped_lines: Vec<String> = Vec::new();
        for line in rendered_lines {
            if is_image_line(&line) {
                wrapped_lines.push(line);
            } else {
                wrapped_lines.extend(wrap_text_with_ansi(&line, content_width));
            }
        }

        let left_margin = " ".repeat(self.padding_x);
        let right_margin = left_margin.clone();
        let background = self
            .default_text_style
            .as_ref()
            .and_then(|style| style.bg_color.as_ref());
        let mut content_lines: Vec<String> = Vec::new();

        for line in wrapped_lines {
            if is_image_line(&line) {
                content_lines.push(line);
                continue;
            }
            let line_with_margins = format!("{left_margin}{line}{right_margin}");
            if let Some(background) = background {
                content_lines.push(apply_background_to_line(&line_with_margins, width, |text| {
                    background(text)
                }));
            } else {
                let visible_len = visible_width(&line_with_margins);
                let padding = width.saturating_sub(visible_len);
                content_lines.push(format!("{line_with_margins}{}", " ".repeat(padding)));
            }
        }

        let empty_line = " ".repeat(width);
        let mut empty_lines: Vec<String> = Vec::new();
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
            result.push(String::new());
        }
        result
    }

    fn apply_default_style(&self, text: &str) -> String {
        let Some(style) = &self.default_text_style else {
            return text.to_string();
        };
        let mut styled = text.to_string();
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

    fn default_style_prefix(&self) -> String {
        let Some(style) = &self.default_text_style else {
            return String::new();
        };
        let sentinel = "\u{0}";
        let mut styled = sentinel.to_string();
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
        match styled.find(sentinel) {
            Some(index) => styled[..index].to_string(),
            None => String::new(),
        }
    }

    fn style_prefix(&self, style: &dyn Fn(&str) -> String) -> String {
        let sentinel = "\u{0}";
        let styled = style(sentinel);
        match styled.find(sentinel) {
            Some(index) => styled[..index].to_string(),
            None => String::new(),
        }
    }

    fn render_token(
        &self,
        token: &Token,
        width: usize,
        next_token_type: Option<&str>,
        style_context: Option<&InlineStyleContext>,
    ) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        match token {
            Token::Heading { depth, tokens, .. } => {
                let heading_prefix = format!("{} ", "#".repeat(*depth as usize));
                let heading_style = |text: &str| {
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
                    format!("{}{heading_text}", heading_style(&heading_prefix))
                } else {
                    heading_text
                };
                lines.push(styled);
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(String::new());
                }
            }
            Token::Paragraph { tokens, .. } => {
                lines.push(self.render_inline_tokens(tokens, style_context));
                if next_token_type.is_some_and(|next| next != "list" && next != "space") {
                    lines.push(String::new());
                }
            }
            Token::Text { tokens, .. } => {
                let _ = tokens;
                lines.push(self.render_inline_tokens(std::slice::from_ref(token), style_context));
            }
            Token::LatexBlock { raw, text, pending, .. } => {
                let rendered = if !*pending && self.options.render_latex {
                    render_latex(text, RenderLatexOptions { display: true })
                        .unwrap_or_else(|| lexer::js_trim(raw).to_string())
                } else {
                    lexer::js_trim(raw).to_string()
                };
                for line in rendered.split('\n') {
                    lines.push(self.apply_default_style(line));
                }
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(String::new());
                }
            }
            Token::Code { lang, text, .. } => {
                let indent = self.theme.code_block_indent().to_string();
                lines.push(
                    self.theme
                        .code_block_border(&format!("```{}", lang.as_deref().unwrap_or(""))),
                );
                if let Some(highlighted) = self.theme.highlight_code(text, lang.as_deref()) {
                    for line in highlighted {
                        lines.push(format!("{indent}{line}"));
                    }
                } else {
                    for line in text.split('\n') {
                        lines.push(format!("{indent}{}", self.theme.code_block(line)));
                    }
                }
                lines.push(self.theme.code_block_border("```"));
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(String::new());
                }
            }
            Token::List { .. } => {
                lines.extend(self.render_list(token, 0, width, style_context));
            }
            Token::Table { header, rows, raw, .. } => {
                lines.extend(self.render_table(header, rows, raw, width, next_token_type, style_context));
            }
            Token::Blockquote { tokens, .. } => {
                let quote_style = |text: &str| self.theme.quote(&self.theme.italic(text));
                let quote_style_prefix = self.style_prefix(&quote_style);
                let apply_quote_style = |line: &str| {
                    if quote_style_prefix.is_empty() {
                        quote_style(line)
                    } else {
                        let reapplied = line.replace("\u{1b}[0m", &format!("\u{1b}[0m{quote_style_prefix}"));
                        quote_style(&reapplied)
                    }
                };
                let quote_content_width = width.saturating_sub(2).max(1);
                let plain = |text: &str| text.to_string();
                let quote_context = InlineStyleContext {
                    apply_text: &plain,
                    style_prefix: quote_style_prefix.clone(),
                };
                let mut rendered_quote_lines: Vec<String> = Vec::new();
                for (index, quote_token) in tokens.iter().enumerate() {
                    let next = tokens.get(index + 1).map(lexer::type_name);
                    rendered_quote_lines.extend(self.render_token(
                        quote_token,
                        quote_content_width,
                        next,
                        Some(&quote_context),
                    ));
                }
                while rendered_quote_lines.last().is_some_and(String::is_empty) {
                    rendered_quote_lines.pop();
                }
                for quote_line in rendered_quote_lines {
                    let styled = apply_quote_style(&quote_line);
                    for wrapped in wrap_text_with_ansi(&styled, quote_content_width) {
                        lines.push(format!("{}{wrapped}", self.theme.quote_border("│ ")));
                    }
                }
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(String::new());
                }
            }
            Token::Hr { .. } => {
                lines.push(self.theme.hr(&"─".repeat(width.min(80))));
                if next_token_type.is_some_and(|next| next != "space") {
                    lines.push(String::new());
                }
            }
            Token::Html { raw, .. } => {
                if !raw.is_empty() && !is_html_comment_only(raw) {
                    lines.push(self.apply_default_style(lexer::js_trim(raw)));
                }
            }
            Token::Space { .. } => lines.push(String::new()),
            _ => {
                if let Some(text) = token.has_text() {
                    lines.push(text.to_string());
                }
            }
        }
        lines
    }

    fn render_inline_tokens(&self, tokens: &[Token], style_context: Option<&InlineStyleContext>) -> String {
        let default_apply = |text: &str| self.apply_default_style(text);
        let default_context = InlineStyleContext {
            apply_text: &default_apply,
            style_prefix: self.default_style_prefix(),
        };
        let context = style_context.unwrap_or(&default_context);
        let apply_text = context.apply_text;
        let apply_text_with_newlines = |text: &str| text.split('\n').map(apply_text).collect::<Vec<_>>().join("\n");

        let mut result = String::new();
        for token in tokens {
            match token {
                Token::Latex { raw, text, pending, .. } => {
                    let rendered = if !*pending && self.options.render_latex {
                        render_latex(text, RenderLatexOptions { display: false }).unwrap_or_else(|| raw.clone())
                    } else {
                        raw.clone()
                    };
                    result.push_str(&apply_text_with_newlines(&rendered));
                }
                Token::Escape { raw, text } => {
                    result.push_str(&apply_text_with_newlines(if self.options.preserve_backslash_escapes {
                        raw
                    } else {
                        text
                    }));
                }
                Token::Text { text, tokens, .. } => {
                    if let Some(nested) = tokens.as_ref().filter(|nested| !nested.is_empty()) {
                        result.push_str(&self.render_inline_tokens(nested, Some(context)));
                    } else {
                        result.push_str(&apply_text_with_newlines(text));
                    }
                }
                Token::Paragraph { tokens, .. } => {
                    result.push_str(&self.render_inline_tokens(tokens, Some(context)));
                }
                Token::Strong { tokens, .. } => {
                    let content = self.render_inline_tokens(tokens, Some(context));
                    result.push_str(&self.theme.bold(&content));
                    result.push_str(&context.style_prefix);
                }
                Token::Em { tokens, .. } => {
                    let content = self.render_inline_tokens(tokens, Some(context));
                    result.push_str(&self.theme.italic(&content));
                    result.push_str(&context.style_prefix);
                }
                Token::Codespan { text, .. } => {
                    result.push_str(&self.theme.code(text));
                    result.push_str(&context.style_prefix);
                }
                Token::Link { href, text, tokens, .. } => {
                    let link_text = self.render_inline_tokens(tokens, Some(context));
                    let styled_link = self.theme.link(&self.theme.underline(&link_text));
                    if get_capabilities().hyperlinks {
                        result.push_str(&hyperlink(&styled_link, href));
                        result.push_str(&context.style_prefix);
                    } else {
                        let href_for_comparison = href.strip_prefix("mailto:").unwrap_or(href);
                        if text.as_str() == href.as_str() || text.as_str() == href_for_comparison {
                            result.push_str(&styled_link);
                        } else {
                            result.push_str(&styled_link);
                            result.push_str(&self.theme.link_url(&format!(" ({href})")));
                        }
                        result.push_str(&context.style_prefix);
                    }
                }
                Token::Br { .. } => result.push('\n'),
                Token::Del { tokens, .. } => {
                    let content = self.render_inline_tokens(tokens, Some(context));
                    result.push_str(&self.theme.strikethrough(&content));
                    result.push_str(&context.style_prefix);
                }
                Token::Html { raw, .. } => {
                    if !raw.is_empty() && !is_html_comment_only(raw) {
                        result.push_str(&apply_text_with_newlines(raw));
                    }
                }
                _ => {
                    if let Some(text) = token.has_text() {
                        result.push_str(&apply_text_with_newlines(text));
                    }
                }
            }
        }

        let prefix = &context.style_prefix;
        while !prefix.is_empty() && result.ends_with(prefix) {
            result.truncate(result.len() - prefix.len());
        }
        result
    }

    fn render_list(
        &self,
        token: &Token,
        depth: usize,
        width: usize,
        style_context: Option<&InlineStyleContext>,
    ) -> Vec<String> {
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
        let mut lines: Vec<String> = Vec::new();
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
                        .unwrap_or_else(|| format!("{}. ", start_number + index))
                } else {
                    format!("{}. ", start_number + index)
                }
            } else if self.options.preserve_ordered_list_markers {
                self.unordered_list_marker(item).unwrap_or_else(|| "- ".to_string())
            } else {
                "- ".to_string()
            };
            let task_marker = if item.task {
                format!("[{}] ", if item.checked == Some(true) { "x" } else { " " })
            } else {
                String::new()
            };
            let marker = format!("{bullet}{task_marker}");
            let first_prefix = format!("{indent}{}", self.theme.list_bullet(&marker));
            let continuation_prefix = format!("{indent}{}", " ".repeat(visible_width(&marker)));
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
                        lines.push(format!("{prefix}{wrapped}"));
                        rendered_any_line = true;
                    }
                }
            }

            if !rendered_any_line {
                lines.push(first_prefix);
            }
            if *loose && !is_last_item {
                lines.push(String::new());
            }
        }

        lines
    }

    fn ordered_list_marker(&self, item: &ListItem) -> Option<String> {
        let bytes = item.raw.as_bytes();
        let mut index = 0;
        while index < 3 && bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        let digits_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        let digits = index - digits_start;
        if digits == 0 || digits > 9 {
            return None;
        }
        if !matches!(bytes.get(index), Some(b'.') | Some(b')')) {
            return None;
        }
        index += 1;
        if !matches!(bytes.get(index), Some(b' ') | Some(b'\t')) {
            return None;
        }
        Some(format!("{} ", &item.raw[digits_start..index]))
    }

    fn unordered_list_marker(&self, item: &ListItem) -> Option<String> {
        let bytes = item.raw.as_bytes();
        let mut index = 0;
        while index < 3 && bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        let marker = *bytes.get(index)?;
        if !matches!(marker, b'-' | b'+' | b'*') {
            return None;
        }
        index += 1;
        let rest = &item.raw[index..];
        let followed = match rest.chars().next() {
            Some(' ') | Some('\t') => true,
            None => true,
            Some('\r') => rest.starts_with("\r\n") || rest == "\r",
            Some('\n') => true,
            _ => false,
        };
        followed.then(|| format!("{} ", marker as char))
    }

    fn get_longest_word_width(text: &str, max_width: Option<usize>) -> usize {
        let longest = text
            .split(lexer::is_js_whitespace)
            .filter(|word| !word.is_empty())
            .map(visible_width)
            .max()
            .unwrap_or(0);
        match max_width {
            Some(max_width) => longest.min(max_width),
            None => longest,
        }
    }

    fn wrap_cell_text(&self, text: &str, max_width: usize) -> Vec<String> {
        wrap_text_with_ansi(text, max_width.max(1))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_table(
        &self,
        header: &[TableCell],
        rows: &[Vec<TableCell>],
        raw: &str,
        available_width: usize,
        next_token_type: Option<&str>,
        style_context: Option<&InlineStyleContext>,
    ) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
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
                fallback_lines.push(String::new());
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

        let top_cells: Vec<String> = column_widths.iter().map(|width| "─".repeat(*width)).collect();
        lines.push(format!("┌─{}─┐", top_cells.join("─┬─")));

        let header_cell_lines: Vec<Vec<String>> = header
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let text = self.render_inline_tokens(&cell.tokens, style_context);
                self.wrap_cell_text(&text, column_widths[index])
            })
            .collect();
        let header_line_count = header_cell_lines.iter().map(Vec::len).max().unwrap_or(0);
        for line_index in 0..header_line_count {
            let parts: Vec<String> = header_cell_lines
                .iter()
                .enumerate()
                .map(|(column, cell_lines)| {
                    let text = cell_lines.get(line_index).map(String::as_str).unwrap_or("");
                    let padded = format!(
                        "{text}{}",
                        " ".repeat(column_widths[column].saturating_sub(visible_width(text)))
                    );
                    self.theme.bold(&padded)
                })
                .collect();
            lines.push(format!("│ {} │", parts.join(" │ ")));
        }

        let separator_cells: Vec<String> = column_widths.iter().map(|width| "─".repeat(*width)).collect();
        let separator_line = format!("├─{}─┤", separator_cells.join("─┼─"));
        lines.push(separator_line.clone());

        for (row_index, row) in rows.iter().enumerate() {
            let row_cell_lines: Vec<Vec<String>> = row
                .iter()
                .enumerate()
                .map(|(index, cell)| {
                    let text = self.render_inline_tokens(&cell.tokens, style_context);
                    self.wrap_cell_text(&text, column_widths[index])
                })
                .collect();
            let row_line_count = row_cell_lines.iter().map(Vec::len).max().unwrap_or(0);
            for line_index in 0..row_line_count {
                let parts: Vec<String> = row_cell_lines
                    .iter()
                    .enumerate()
                    .map(|(column, cell_lines)| {
                        let text = cell_lines.get(line_index).map(String::as_str).unwrap_or("");
                        format!(
                            "{text}{}",
                            " ".repeat(column_widths[column].saturating_sub(visible_width(text)))
                        )
                    })
                    .collect();
                lines.push(format!("│ {} │", parts.join(" │ ")));
            }
            if row_index + 1 < rows.len() {
                lines.push(separator_line.clone());
            }
        }

        let bottom_cells: Vec<String> = column_widths.iter().map(|width| "─".repeat(*width)).collect();
        lines.push(format!("└─{}─┘", bottom_cells.join("─┴─")));

        if next_token_type.is_some_and(|next| next != "space") {
            lines.push(String::new());
        }
        lines
    }
}

impl Component for Markdown {
    fn render(&mut self, width: usize) -> Vec<String> {
        if self.cached_lines.is_some()
            && self.cached_text.as_deref() == Some(self.text.as_str())
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
        self.cached_text = None;
        self.cached_width = None;
        self.cached_lines = None;
    }
}

fn is_html_comment_only(html: &str) -> bool {
    let trimmed = lexer::js_trim(html);
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
            let marker_length = raw.chars().take_while(|ch| *ch == '`' || *ch == '~').count();
            if marker_length < 3 {
                return;
            }
            let marker = &raw[..marker_length];
            let Some(last_line) = raw.split('\n').next_back() else {
                return;
            };
            let marker_character = marker.chars().next().unwrap_or('`');
            if last_line.is_empty()
                || last_line.len() >= marker.len()
                || !last_line.chars().all(|ch| ch == marker_character)
            {
                return;
            }
            let remove = last_line.len().min(text.len());
            let mut trimmed = text[..text.len() - remove].to_string();
            if trimmed.ends_with('\n') {
                trimmed.pop();
            }
            *text = trimmed;
        }
        _ => {}
    }
}
