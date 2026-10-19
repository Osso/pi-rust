//! Node `util.styleText` for the ANSI formats used by the eval report.
//!
//! Node decides whether to emit escapes from `process.stderr` (the default
//! `styleText` stream): `FORCE_COLOR` wins over `NO_COLOR`, then
//! `NODE_DISABLE_COLORS`/`TERM=dumb`, then TTY detection. The Rust port keeps
//! that decision in [`StyleText::detect`] and exposes explicit choices so
//! differential tests can compare both modes without mutating process state.

use std::io::IsTerminal;

use pi_js_compat::process_env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Bold,
    Gray,
    Green,
    Red,
    Yellow,
}

impl TextStyle {
    /// Opening and closing SGR codes from Node's `inspect.colors`.
    fn codes(self) -> (&'static str, &'static str) {
        match self {
            Self::Bold => ("\u{1b}[1m", "\u{1b}[22m"),
            Self::Gray => ("\u{1b}[90m", "\u{1b}[39m"),
            Self::Green => ("\u{1b}[32m", "\u{1b}[39m"),
            Self::Red => ("\u{1b}[31m", "\u{1b}[39m"),
            Self::Yellow => ("\u{1b}[33m", "\u{1b}[39m"),
        }
    }
}

/// Color support resolved once per report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleText {
    colorize: bool,
}

impl StyleText {
    pub fn always() -> Self {
        Self { colorize: true }
    }

    pub fn never() -> Self {
        Self { colorize: false }
    }

    /// `process.stderr.hasColors()` as consulted by `styleText`.
    pub fn detect() -> Self {
        if let Some(force) = process_env::get("FORCE_COLOR") {
            let colorize = matches!(force.as_str(), "" | "true" | "1" | "2" | "3");
            return Self { colorize };
        }
        if process_env::get("NODE_DISABLE_COLORS").is_some()
            || process_env::get("NO_COLOR").is_some()
            || process_env::get("TERM").as_deref() == Some("dumb")
        {
            return Self::never();
        }
        Self {
            colorize: std::io::stderr().is_terminal(),
        }
    }

    pub fn colorize(&self) -> bool {
        self.colorize
    }

    /// `styleText(format, text)`.
    pub fn style(self, style: TextStyle, text: &str) -> String {
        if !self.colorize {
            return text.to_owned();
        }
        let (open, close) = style.codes();
        format!("{open}{text}{close}")
    }
}
