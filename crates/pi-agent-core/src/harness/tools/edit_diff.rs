/// The line-ending style selected by the source helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    CrLf,
    Lf,
}

impl LineEnding {
    /// Returns the exact source line-ending literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Lf => "\n",
        }
    }
}

/// The leading BOM, if present, and the remaining text.
#[derive(Debug)]
pub struct StripBomResult {
    pub bom: String,
    pub text: String,
}

/// A direct-module result record from exact or fuzzy text matching.
#[derive(Debug)]
pub struct FuzzyMatchResult {
    pub found: bool,
    pub index: f64,
    pub match_length: f64,
    pub used_fuzzy_match: bool,
    pub content_for_replacement: String,
}

/// A direct-module edit record with replacement text.
#[derive(Debug)]
pub struct Edit {
    pub old_text: String,
    pub new_text: String,
}

/// A direct-module result record from edit application.
#[derive(Debug)]
pub struct AppliedEditsResult {
    pub base_content: String,
    pub new_content: String,
}

/// Detects the first line-ending style using the source's first-LF ordering.
pub fn detect_line_ending(content: &str) -> LineEnding {
    let Some(first_lf) = content.find('\n') else {
        return LineEnding::Lf;
    };
    let Some(first_crlf) = content.find("\r\n") else {
        return LineEnding::Lf;
    };

    if first_crlf < first_lf {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

/// Replaces CRLF and then every remaining CR with LF.
pub fn normalize_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Restores LF to CRLF, including any CRLF already present in the input.
pub fn restore_line_endings(text: &str, ending: LineEnding) -> String {
    match ending {
        LineEnding::CrLf => text.replace('\n', "\r\n"),
        LineEnding::Lf => text.to_owned(),
    }
}

/// Removes exactly one leading UTF-8 BOM and returns it separately.
pub fn strip_bom(content: &str) -> StripBomResult {
    match content.strip_prefix('\u{feff}') {
        Some(text) => StripBomResult {
            bom: "\u{feff}".to_owned(),
            text: text.to_owned(),
        },
        None => StripBomResult {
            bom: String::new(),
            text: content.to_owned(),
        },
    }
}
