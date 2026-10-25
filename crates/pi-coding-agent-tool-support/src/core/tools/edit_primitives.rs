use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fmt::{self, Display, Formatter};

use super::jsdiff::{self, Part};
use super::unicode::normalize_nfkc;

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
    pub bom: JsString,
    pub text: JsString,
}

/// A direct-module result record from exact or fuzzy text matching.
#[derive(Debug)]
pub struct FuzzyMatchResult {
    pub found: bool,
    pub index: f64,
    pub match_length: f64,
    pub used_fuzzy_match: bool,
    pub content_for_replacement: JsString,
}

/// A direct-module edit record with replacement text.
#[derive(Debug)]
pub struct Edit {
    pub old_text: JsString,
    pub new_text: JsString,
}

/// A replacement expressed in UTF-16 unit offsets of the base content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextReplacement {
    pub match_index: usize,
    pub match_length: usize,
    pub new_text: JsString,
}

/// A direct-module result record from edit application.
#[derive(Debug)]
pub struct AppliedEditsResult {
    pub base_content: JsString,
    pub new_content: JsString,
}

/// Failure produced by exact-text edit application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    pub message: JsString,
}

impl EditError {
    /// Creates an edit failure with the source message text.
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for EditError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for EditError {}

/// Display-oriented diff output and the first changed new-file line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStringResult {
    pub diff: JsString,
    pub first_changed_line: Option<usize>,
}

/// Detects the first line-ending style using the source's first-LF ordering.
pub fn detect_line_ending(content: impl Into<JsString>) -> LineEnding {
    let content = content.into();
    let content = &content;

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
pub fn normalize_to_lf(text: impl Into<JsString>) -> JsString {
    let text = text.into();
    let text = &text;

    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Restores LF to CRLF, including any CRLF already present in the input.
pub fn restore_line_endings(text: impl Into<JsString>, ending: LineEnding) -> JsString {
    let text = text.into();
    let text = &text;

    match ending {
        LineEnding::CrLf => text.replace('\n', "\r\n"),
        LineEnding::Lf => text.to_owned(),
    }
}

/// Removes exactly one leading UTF-8 BOM and returns it separately.
pub fn strip_bom(content: impl Into<JsString>) -> StripBomResult {
    let content = content.into();
    let content = &content;

    match content.strip_prefix('\u{feff}') {
        Some(text) => StripBomResult {
            bom: "\u{feff}".into(),
            text: text.to_owned(),
        },
        None => StripBomResult {
            bom: JsString::default(),
            text: content.to_owned(),
        },
    }
}

/// Normalizes text for fuzzy matching: NFKC, trailing whitespace, quotes, dashes, and spaces.
pub fn normalize_for_fuzzy_match(text: impl Into<JsString>) -> JsString {
    let text = text.into();
    let normalized = normalize_nfkc(&text);
    let lines = normalized.split('\n').map(|line| line.trim_end());
    let joined = JsString::join(lines, "\n");
    JsString::from_utf16(
        joined
            .units()
            .iter()
            .map(|unit| {
                char::from_u32(u32::from(*unit)).map_or(*unit, |character| normalize_fuzzy_character(character) as u16)
            })
            .collect(),
    )
}

fn normalize_fuzzy_character(character: char) -> char {
    match character {
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' => '\'',
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
        '\u{00a0}' | '\u{2002}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' => ' ',
        other => other,
    }
}

/// Finds `old_text` exactly first, then through fuzzy normalization.
pub fn fuzzy_find_text(content: impl Into<JsString>, old_text: impl Into<JsString>) -> FuzzyMatchResult {
    let content = content.into();
    let old_text = old_text.into();
    match find_text(&content, &old_text) {
        Some(found) => FuzzyMatchResult {
            found: true,
            index: found.unit_index as f64,
            match_length: found.unit_length as f64,
            used_fuzzy_match: found.used_fuzzy_match,
            content_for_replacement: found.content_for_replacement,
        },
        None => FuzzyMatchResult {
            found: false,
            index: -1.0,
            match_length: 0.0,
            used_fuzzy_match: false,
            content_for_replacement: content,
        },
    }
}

struct TextMatch {
    unit_index: usize,
    unit_length: usize,
    used_fuzzy_match: bool,
    content_for_replacement: JsString,
}

fn find_text(content: &JsString, old_text: &JsString) -> Option<TextMatch> {
    if let Some(unit_index) = content.find(old_text) {
        return Some(TextMatch {
            unit_index,
            unit_length: old_text.len(),
            used_fuzzy_match: false,
            content_for_replacement: content.to_owned(),
        });
    }

    let fuzzy_content = normalize_for_fuzzy_match(content);
    let fuzzy_old_text = normalize_for_fuzzy_match(old_text);
    let unit_index = fuzzy_content.find(&fuzzy_old_text)?;
    Some(TextMatch {
        unit_index,
        unit_length: fuzzy_old_text.len(),
        used_fuzzy_match: true,
        content_for_replacement: fuzzy_content,
    })
}

fn count_occurrences(content: &JsString, old_text: &JsString) -> usize {
    let content = normalize_for_fuzzy_match(content);
    let old_text = normalize_for_fuzzy_match(old_text);
    if old_text.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut offset = 0;
    while let Some(index) = content.find_from(&old_text, offset) {
        count += 1;
        offset = index + old_text.len();
    }
    count
}

/// Applies one or more exact-text replacements to LF-normalized content.
pub fn apply_edits_to_normalized_content(
    normalized_content: impl Into<JsString>,
    edits: &[Edit],
    path: impl Into<JsString>,
) -> Result<AppliedEditsResult, EditError> {
    let normalized_content = normalized_content.into();
    let normalized_content = &normalized_content;
    let path = path.into();
    let path = &path;

    let normalized_edits = normalize_edits(edits);
    ensure_nonempty_edits(&normalized_edits, path)?;

    let used_fuzzy_match = normalized_edits
        .iter()
        .any(|edit| find_text(normalized_content, &edit.old_text).is_some_and(|found| found.used_fuzzy_match));
    let replacement_base_content = if used_fuzzy_match {
        normalize_for_fuzzy_match(normalized_content)
    } else {
        normalized_content.to_owned()
    };

    let matched_edits = match_edits(&normalized_edits, &replacement_base_content, path)?;
    let replacements = matched_edits
        .iter()
        .map(|matched| matched.replacement.clone())
        .collect::<Vec<_>>();
    let new_content = if used_fuzzy_match {
        apply_replacements_preserving_unchanged_lines(normalized_content, &replacement_base_content, &replacements)?
    } else {
        apply_replacements(&replacement_base_content, &replacements, 0)
    };

    if normalized_content == &new_content {
        return Err(no_change_error(path, normalized_edits.len()));
    }
    Ok(AppliedEditsResult {
        base_content: normalized_content.to_owned(),
        new_content,
    })
}

fn normalize_edits(edits: &[Edit]) -> Vec<Edit> {
    edits
        .iter()
        .map(|edit| Edit {
            old_text: normalize_to_lf(&edit.old_text),
            new_text: normalize_to_lf(&edit.new_text),
        })
        .collect()
}

fn ensure_nonempty_edits(edits: &[Edit], path: &JsString) -> Result<(), EditError> {
    for (index, edit) in edits.iter().enumerate() {
        if edit.old_text.is_empty() {
            return Err(empty_old_text_error(path, index, edits.len()));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct MatchedEdit {
    edit_index: usize,
    replacement: TextReplacement,
}

fn match_edits(
    edits: &[Edit],
    replacement_base_content: &JsString,
    path: &JsString,
) -> Result<Vec<MatchedEdit>, EditError> {
    let mut matched = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let Some(found) = find_text(replacement_base_content, &edit.old_text) else {
            return Err(not_found_error(path, index, edits.len()));
        };
        let occurrences = count_occurrences(replacement_base_content, &edit.old_text);
        if occurrences > 1 {
            return Err(duplicate_error(path, index, edits.len(), occurrences));
        }
        matched.push(MatchedEdit {
            edit_index: index,
            replacement: TextReplacement {
                match_index: found.unit_index,
                match_length: found.unit_length,
                new_text: edit.new_text.clone(),
            },
        });
    }

    matched.sort_by_key(|edit| edit.replacement.match_index);
    for pair in matched.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.replacement.match_index + previous.replacement.match_length > current.replacement.match_index {
            return Err(EditError::new(
                JsString::from(format!(
                    "edits[{}] and edits[{}] overlap in ",
                    previous.edit_index, current.edit_index
                )) + path
                    + ". Merge them into one edit or target disjoint regions.",
            ));
        }
    }
    Ok(matched)
}

pub fn empty_old_text_error(path: impl Into<JsString>, edit_index: usize, total_edits: usize) -> EditError {
    let path = path.into();
    let path = &path;

    let prefix = if total_edits == 1 {
        "oldText must not be empty in ".to_owned()
    } else {
        format!("edits[{edit_index}].oldText must not be empty in ")
    };
    EditError::new(JsString::from(prefix) + path + ".")
}

pub fn not_found_error(path: impl Into<JsString>, edit_index: usize, total_edits: usize) -> EditError {
    let path = path.into();
    let path = &path;

    if total_edits == 1 {
        EditError::new(
            JsString::from("Could not find the exact text in ")
                + path
                + ". The old text must match exactly including all whitespace and newlines.",
        )
    } else {
        EditError::new(
            JsString::from(format!("Could not find edits[{edit_index}] in "))
                + path
                + ". The oldText must match exactly including all whitespace and newlines.",
        )
    }
}

fn duplicate_error(path: &JsString, edit_index: usize, total_edits: usize, occurrences: usize) -> EditError {
    if total_edits == 1 {
        EditError::new(
            JsString::from(format!("Found {occurrences} occurrences of the text in "))
                + path
                + ". The text must be unique. Please provide more context to make it unique.",
        )
    } else {
        EditError::new(
            JsString::from(format!("Found {occurrences} occurrences of edits[{edit_index}] in "))
                + path
                + ". Each oldText must be unique. Please provide more context to make it unique.",
        )
    }
}

pub fn no_change_error(path: impl Into<JsString>, total_edits: usize) -> EditError {
    let path = path.into();
    let path = &path;

    if total_edits == 1 {
        EditError::new(
            JsString::from("No changes made to ")
                + path
                + ". The replacement produced identical content. This might indicate an issue with special characters or the text not existing as expected.",
        )
    } else {
        EditError::new(JsString::from("No changes made to ") + path + ". The replacements produced identical content.")
    }
}

fn split_lines_with_endings(content: &JsString) -> Vec<JsString> {
    content.split_inclusive('\n').collect()
}

struct LineSpan {
    start: usize,
    end: usize,
}

fn get_line_spans(content: &JsString) -> Vec<LineSpan> {
    let mut offset = 0;
    split_lines_with_endings(content)
        .into_iter()
        .map(|line| {
            let span = LineSpan {
                start: offset,
                end: offset + line.len(),
            };
            offset = span.end;
            span
        })
        .collect()
}

fn get_replacement_line_range(lines: &[LineSpan], replacement: &TextReplacement) -> Result<(usize, usize), EditError> {
    let replacement_start = replacement.match_index;
    let replacement_end = replacement.match_index + replacement.match_length;

    let start_line = lines
        .iter()
        .position(|line| replacement_start >= line.start && replacement_start < line.end)
        .ok_or_else(|| EditError::new("Replacement range is outside the base content."))?;

    let mut end_line = start_line;
    while end_line < lines.len() && lines[end_line].end < replacement_end {
        end_line += 1;
    }
    if end_line >= lines.len() {
        return Err(EditError::new("Replacement range is outside the base content."));
    }
    Ok((start_line, end_line + 1))
}

pub fn apply_replacements(content: impl Into<JsString>, replacements: &[TextReplacement], offset: usize) -> JsString {
    let content = content.into();
    let content = &content;

    let mut result = content.to_owned();
    for replacement in replacements.iter().rev() {
        let start = replacement.match_index - offset;
        result.replace_range(start..start + replacement.match_length, &replacement.new_text);
    }
    result
}

/// Applies replacements matched against `base_content` to `original_content`, preserving
/// unchanged line blocks from the original.
pub fn apply_replacements_preserving_unchanged_lines(
    original_content: impl Into<JsString>,
    base_content: impl Into<JsString>,
    replacements: &[TextReplacement],
) -> Result<JsString, EditError> {
    let original_content = original_content.into();
    let original_content = &original_content;
    let base_content = base_content.into();
    let base_content = &base_content;

    let original_lines = split_lines_with_endings(original_content);
    let base_lines = get_line_spans(base_content);
    if original_lines.len() != base_lines.len() {
        return Err(EditError::new(
            "Cannot preserve unchanged lines because the base content has a different line count.",
        ));
    }
    let groups = group_replacements(&base_lines, replacements)?;
    Ok(render_preserved_groups(
        base_content,
        &original_lines,
        &base_lines,
        &groups,
    ))
}

struct ReplacementGroup {
    start_line: usize,
    end_line: usize,
    replacements: Vec<TextReplacement>,
}

fn group_replacements(
    base_lines: &[LineSpan],
    replacements: &[TextReplacement],
) -> Result<Vec<ReplacementGroup>, EditError> {
    let mut groups: Vec<ReplacementGroup> = Vec::new();
    let mut sorted = replacements.to_vec();
    sorted.sort_by_key(|replacement| replacement.match_index);
    for replacement in sorted {
        let (start_line, end_line) = get_replacement_line_range(base_lines, &replacement)?;
        match groups.last_mut() {
            Some(current) if start_line < current.end_line => {
                current.end_line = current.end_line.max(end_line);
                current.replacements.push(replacement);
            }
            _ => groups.push(ReplacementGroup {
                start_line,
                end_line,
                replacements: vec![replacement],
            }),
        }
    }
    Ok(groups)
}

fn render_preserved_groups(
    base_content: &JsString,
    original_lines: &[JsString],
    base_lines: &[LineSpan],
    groups: &[ReplacementGroup],
) -> JsString {
    let mut original_line_index = 0;
    let mut result = JsString::default();
    for group in groups {
        result.push_str(JsString::join(
            &original_lines[original_line_index..group.start_line],
            "",
        ));
        let group_start_offset = base_lines[group.start_line].start;
        let group_end_offset = base_lines[group.end_line - 1].end;
        result.push_str(apply_replacements(
            base_content.slice(group_start_offset, group_end_offset),
            &group.replacements,
            group_start_offset,
        ));
        original_line_index = group.end_line;
    }
    result.push_str(JsString::join(&original_lines[original_line_index..], ""));
    result
}

/// Generates a standard unified patch with file headers only.
pub fn generate_unified_patch(
    path: impl Into<JsString>,
    old_content: impl Into<JsString>,
    new_content: impl Into<JsString>,
    context_lines: usize,
) -> JsString {
    let path = path.into();
    let path = &path;
    let old_content = old_content.into();
    let old_content = &old_content;
    let new_content = new_content.into();
    let new_content = &new_content;

    jsdiff::create_two_files_patch(path, old_content, new_content, context_lines)
}

/// Generates a display-oriented diff string with line numbers and context.
pub fn generate_diff_string(
    old_content: impl Into<JsString>,
    new_content: impl Into<JsString>,
    context_lines: usize,
) -> DiffStringResult {
    let old_content = old_content.into();
    let old_content = &old_content;
    let new_content = new_content.into();
    let new_content = &new_content;

    let parts = jsdiff::diff_lines(old_content, new_content);
    let old_lines = old_content.split('\n').collect::<Vec<_>>();
    let new_lines = new_content.split('\n').collect::<Vec<_>>();
    let line_num_width = old_lines.len().max(new_lines.len()).to_string().len();

    let mut renderer = DiffRenderer {
        output: Vec::new(),
        old_line_num: 1,
        new_line_num: 1,
        line_num_width,
        first_changed_line: None,
    };
    let mut last_was_change = false;
    for (index, part) in parts.iter().enumerate() {
        if part.added || part.removed {
            renderer.push_change(part);
            last_was_change = true;
            continue;
        }
        let next_part_is_change = parts.get(index + 1).is_some_and(|next| next.added || next.removed);
        renderer.push_context(part, context_lines, last_was_change, next_part_is_change);
        last_was_change = false;
    }

    DiffStringResult {
        diff: JsString::join(renderer.output, "\n"),
        first_changed_line: renderer.first_changed_line,
    }
}

struct DiffRenderer {
    output: Vec<JsString>,
    old_line_num: usize,
    new_line_num: usize,
    line_num_width: usize,
    first_changed_line: Option<usize>,
}

impl DiffRenderer {
    fn push_change(&mut self, part: &Part) {
        self.first_changed_line.get_or_insert(self.new_line_num);
        for line in split_part_lines(&part.value) {
            if part.added {
                self.output.push(
                    JsString::from(format!("+{:>width$} ", self.new_line_num, width = self.line_num_width)) + line,
                );
                self.new_line_num += 1;
            } else {
                self.output.push(
                    JsString::from(format!("-{:>width$} ", self.old_line_num, width = self.line_num_width)) + line,
                );
                self.old_line_num += 1;
            }
        }
    }

    fn push_context(&mut self, part: &Part, context_lines: usize, has_leading_change: bool, has_trailing_change: bool) {
        let raw = split_part_lines(&part.value);
        if has_leading_change && has_trailing_change {
            self.push_surrounded_context(&raw, context_lines);
        } else if has_leading_change {
            self.push_trailing_context(&raw, context_lines);
        } else if has_trailing_change {
            self.push_leading_context(&raw, context_lines);
        } else {
            self.skip_lines(raw.len());
        }
    }

    fn push_surrounded_context(&mut self, raw: &[JsString], context_lines: usize) {
        if raw.len() <= context_lines * 2 {
            self.push_context_lines(raw);
            return;
        }
        self.push_context_lines(&raw[..context_lines]);
        let skipped = raw.len() - context_lines * 2;
        self.push_skipped(skipped);
        self.push_context_lines(&raw[raw.len() - context_lines..]);
    }

    fn push_trailing_context(&mut self, raw: &[JsString], context_lines: usize) {
        let shown = context_lines.min(raw.len());
        self.push_context_lines(&raw[..shown]);
        let skipped = raw.len() - shown;
        if skipped > 0 {
            self.push_skipped(skipped);
        }
    }

    fn push_leading_context(&mut self, raw: &[JsString], context_lines: usize) {
        let skipped = raw.len().saturating_sub(context_lines);
        if skipped > 0 {
            self.push_skipped(skipped);
        }
        self.push_context_lines(&raw[skipped..]);
    }

    fn push_context_lines(&mut self, lines: &[JsString]) {
        for line in lines {
            self.output
                .push(JsString::from(format!(" {:>width$} ", self.old_line_num, width = self.line_num_width)) + line);
            self.old_line_num += 1;
            self.new_line_num += 1;
        }
    }

    fn push_skipped(&mut self, skipped: usize) {
        self.output
            .push(format!(" {} ...", " ".repeat(self.line_num_width)).into());
        self.skip_lines(skipped);
    }

    fn skip_lines(&mut self, skipped: usize) {
        self.old_line_num += skipped;
        self.new_line_num += skipped;
    }
}

fn split_part_lines(value: &JsString) -> Vec<JsString> {
    let mut raw = value.split('\n').collect::<Vec<_>>();
    if raw.last().is_some_and(JsString::is_empty) {
        raw.pop();
    }
    raw
}
