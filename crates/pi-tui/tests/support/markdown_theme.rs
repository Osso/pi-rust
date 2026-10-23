//! Chalk level-3 test theme matching `packages/tui/test/test-themes.ts`.
//!
//! This is a test double for chalk's nested-style re-open behavior; the
//! production theme is supplied by the caller.

#![allow(dead_code)]

use pi_js_compat::js_json::JsString;
use pi_tui::components::markdown::MarkdownTheme;

#[derive(Clone, Copy)]
pub struct Style {
    open: &'static str,
    close: &'static str,
}

pub const BOLD: Style = Style {
    open: "\u{1b}[1m",
    close: "\u{1b}[22m",
};
pub const DIM: Style = Style {
    open: "\u{1b}[2m",
    close: "\u{1b}[22m",
};
pub const ITALIC: Style = Style {
    open: "\u{1b}[3m",
    close: "\u{1b}[23m",
};
pub const UNDERLINE: Style = Style {
    open: "\u{1b}[4m",
    close: "\u{1b}[24m",
};
pub const STRIKETHROUGH: Style = Style {
    open: "\u{1b}[9m",
    close: "\u{1b}[29m",
};
pub const GREEN: Style = Style {
    open: "\u{1b}[32m",
    close: "\u{1b}[39m",
};
pub const YELLOW: Style = Style {
    open: "\u{1b}[33m",
    close: "\u{1b}[39m",
};
pub const MAGENTA: Style = Style {
    open: "\u{1b}[35m",
    close: "\u{1b}[39m",
};
pub const BLUE: Style = Style {
    open: "\u{1b}[34m",
    close: "\u{1b}[39m",
};
pub const CYAN: Style = Style {
    open: "\u{1b}[36m",
    close: "\u{1b}[39m",
};
pub const GRAY: Style = Style {
    open: "\u{1b}[90m",
    close: "\u{1b}[39m",
};
pub const BG_BLUE: Style = Style {
    open: "\u{1b}[44m",
    close: "\u{1b}[49m",
};

/// Applies a chalk style chain, including chalk's embedded close-code re-open
/// and per-line close/re-open behavior.
pub fn apply(styles: &[Style], text: impl Into<JsString>) -> JsString {
    let text = text.into();
    if text.is_empty() || styles.is_empty() {
        return text;
    }
    let open_all: String = styles.iter().map(|style| style.open).collect();
    let close_all: String = styles.iter().rev().map(|style| style.close).collect();
    let mut styled = text;
    if styled.contains('\u{1b}') {
        for style in styles.iter().rev() {
            styled = styled.replace(style.close, format!("{}{}", style.close, style.open));
        }
    }
    styled = encase_newlines(&styled, &close_all, &open_all);
    JsString::from(open_all) + styled + close_all
}

fn encase_newlines(text: &JsString, close: &str, open: &str) -> JsString {
    if !text.contains('\n') {
        return text.clone();
    }
    let mut result = JsString::default();
    let mut start = 0;
    for (index, _) in text.units().iter().enumerate().filter(|(_, unit)| **unit == 10) {
        let carriage = index > 0 && text.units()[index - 1] == 13;
        let end = if carriage { index - 1 } else { index };
        result.push_str(text.slice(start, end));
        result.push_str(close);
        if carriage {
            result.push_str("\r");
        }
        result.push_str("\n");
        result.push_str(open);
        start = index + 1;
    }
    result.push_str(text.slice(start, text.len()));
    result
}

pub struct ChalkTheme;

impl MarkdownTheme for ChalkTheme {
    fn heading(&self, text: &JsString) -> JsString {
        apply(&[BOLD, CYAN], text)
    }
    fn link(&self, text: &JsString) -> JsString {
        apply(&[BLUE], text)
    }
    fn link_url(&self, text: &JsString) -> JsString {
        apply(&[DIM], text)
    }
    fn code(&self, text: &JsString) -> JsString {
        apply(&[YELLOW], text)
    }
    fn code_block(&self, text: &JsString) -> JsString {
        apply(&[GREEN], text)
    }
    fn code_block_border(&self, text: &JsString) -> JsString {
        apply(&[DIM], text)
    }
    fn quote(&self, text: &JsString) -> JsString {
        apply(&[ITALIC], text)
    }
    fn quote_border(&self, text: &JsString) -> JsString {
        apply(&[DIM], text)
    }
    fn hr(&self, text: &JsString) -> JsString {
        apply(&[DIM], text)
    }
    fn list_bullet(&self, text: &JsString) -> JsString {
        apply(&[CYAN], text)
    }
    fn bold(&self, text: &JsString) -> JsString {
        apply(&[BOLD], text)
    }
    fn italic(&self, text: &JsString) -> JsString {
        apply(&[ITALIC], text)
    }
    fn strikethrough(&self, text: &JsString) -> JsString {
        apply(&[STRIKETHROUGH], text)
    }
    fn underline(&self, text: &JsString) -> JsString {
        apply(&[UNDERLINE], text)
    }
}
