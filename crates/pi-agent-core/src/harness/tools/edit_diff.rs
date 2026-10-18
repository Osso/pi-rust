use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use unicode_normalization::UnicodeNormalization;

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

/// Failure produced by exact-text edit application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    pub message: String,
}

impl EditError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for EditError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EditError {}

/// Display-oriented diff output and the first changed new-file line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStringResult {
    pub diff: String,
    pub first_changed_line: Option<usize>,
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

/// Normalizes text for fuzzy matching: NFKC, trailing whitespace, quotes, dashes, and spaces.
pub fn normalize_for_fuzzy_match(text: &str) -> String {
    let normalized = text.nfkc().collect::<String>();
    normalized
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .map(normalize_fuzzy_character)
        .collect()
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
pub fn fuzzy_find_text(content: &str, old_text: &str) -> FuzzyMatchResult {
    match find_text(content, old_text) {
        Some(found) => FuzzyMatchResult {
            found: true,
            index: utf16_length(&found.content_for_replacement[..found.byte_index]) as f64,
            match_length: utf16_length(
                &found.content_for_replacement[found.byte_index..found.byte_index + found.byte_length],
            ) as f64,
            used_fuzzy_match: found.used_fuzzy_match,
            content_for_replacement: found.content_for_replacement,
        },
        None => FuzzyMatchResult {
            found: false,
            index: -1.0,
            match_length: 0.0,
            used_fuzzy_match: false,
            content_for_replacement: content.to_owned(),
        },
    }
}

struct TextMatch {
    byte_index: usize,
    byte_length: usize,
    used_fuzzy_match: bool,
    content_for_replacement: String,
}

fn find_text(content: &str, old_text: &str) -> Option<TextMatch> {
    if let Some(byte_index) = content.find(old_text) {
        return Some(TextMatch {
            byte_index,
            byte_length: old_text.len(),
            used_fuzzy_match: false,
            content_for_replacement: content.to_owned(),
        });
    }

    let fuzzy_content = normalize_for_fuzzy_match(content);
    let fuzzy_old_text = normalize_for_fuzzy_match(old_text);
    let byte_index = fuzzy_content.find(&fuzzy_old_text)?;
    Some(TextMatch {
        byte_index,
        byte_length: fuzzy_old_text.len(),
        used_fuzzy_match: true,
        content_for_replacement: fuzzy_content,
    })
}

fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

fn count_occurrences(content: &str, old_text: &str) -> usize {
    let fuzzy_content = normalize_for_fuzzy_match(content);
    let fuzzy_old_text = normalize_for_fuzzy_match(old_text);
    fuzzy_content.match_indices(&fuzzy_old_text).count()
}

/// Applies one or more exact-text replacements to LF-normalized content.
pub fn apply_edits_to_normalized_content(
    normalized_content: &str,
    edits: &[Edit],
    path: &str,
) -> Result<AppliedEditsResult, EditError> {
    let normalized_edits = edits
        .iter()
        .map(|edit| Edit {
            old_text: normalize_to_lf(&edit.old_text),
            new_text: normalize_to_lf(&edit.new_text),
        })
        .collect::<Vec<_>>();

    for (index, edit) in normalized_edits.iter().enumerate() {
        if edit.old_text.is_empty() {
            return Err(empty_old_text_error(path, index, normalized_edits.len()));
        }
    }

    let used_fuzzy_match = normalized_edits
        .iter()
        .any(|edit| find_text(normalized_content, &edit.old_text).is_some_and(|found| found.used_fuzzy_match));
    let replacement_base_content = if used_fuzzy_match {
        normalize_for_fuzzy_match(normalized_content)
    } else {
        normalized_content.to_owned()
    };

    let matched_edits = match_edits(&normalized_edits, &replacement_base_content, path)?;
    let new_content = if used_fuzzy_match {
        apply_replacements_preserving_unchanged_lines(normalized_content, &replacement_base_content, &matched_edits)?
    } else {
        apply_replacements(&replacement_base_content, &matched_edits, 0)
    };

    if normalized_content == new_content {
        return Err(no_change_error(path, normalized_edits.len()));
    }
    Ok(AppliedEditsResult {
        base_content: normalized_content.to_owned(),
        new_content,
    })
}

#[derive(Clone)]
struct MatchedEdit {
    edit_index: usize,
    match_index: usize,
    match_length: usize,
    new_text: String,
}

fn match_edits(edits: &[Edit], replacement_base_content: &str, path: &str) -> Result<Vec<MatchedEdit>, EditError> {
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
            match_index: found.byte_index,
            match_length: found.byte_length,
            new_text: edit.new_text.clone(),
        });
    }

    matched.sort_by_key(|edit| edit.match_index);
    for pair in matched.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.match_index + previous.match_length > current.match_index {
            return Err(EditError::new(format!(
                "edits[{}] and edits[{}] overlap in {path}. Merge them into one edit or target disjoint regions.",
                previous.edit_index, current.edit_index
            )));
        }
    }
    Ok(matched)
}

fn empty_old_text_error(path: &str, edit_index: usize, total_edits: usize) -> EditError {
    if total_edits == 1 {
        EditError::new(format!("oldText must not be empty in {path}."))
    } else {
        EditError::new(format!("edits[{edit_index}].oldText must not be empty in {path}."))
    }
}

fn not_found_error(path: &str, edit_index: usize, total_edits: usize) -> EditError {
    if total_edits == 1 {
        EditError::new(format!(
            "Could not find the exact text in {path}. The old text must match exactly including all whitespace and newlines."
        ))
    } else {
        EditError::new(format!(
            "Could not find edits[{edit_index}] in {path}. The oldText must match exactly including all whitespace and newlines."
        ))
    }
}

fn duplicate_error(path: &str, edit_index: usize, total_edits: usize, occurrences: usize) -> EditError {
    if total_edits == 1 {
        EditError::new(format!(
            "Found {occurrences} occurrences of the text in {path}. The text must be unique. Please provide more context to make it unique."
        ))
    } else {
        EditError::new(format!(
            "Found {occurrences} occurrences of edits[{edit_index}] in {path}. Each oldText must be unique. Please provide more context to make it unique."
        ))
    }
}

fn no_change_error(path: &str, total_edits: usize) -> EditError {
    if total_edits == 1 {
        EditError::new(format!(
            "No changes made to {path}. The replacement produced identical content. This might indicate an issue with special characters or the text not existing as expected."
        ))
    } else {
        EditError::new(format!(
            "No changes made to {path}. The replacements produced identical content."
        ))
    }
}

fn split_lines_with_endings(content: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            lines.push(&content[start..=index]);
            start = index + 1;
        }
    }
    if start < content.len() {
        lines.push(&content[start..]);
    }
    lines
}

struct LineSpan {
    start: usize,
    end: usize,
}

fn get_line_spans(content: &str) -> Vec<LineSpan> {
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

fn get_replacement_line_range(lines: &[LineSpan], replacement: &MatchedEdit) -> Result<(usize, usize), EditError> {
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

fn apply_replacements(content: &str, replacements: &[MatchedEdit], offset: usize) -> String {
    let mut result = content.to_owned();
    for replacement in replacements.iter().rev() {
        let start = replacement.match_index - offset;
        result.replace_range(start..start + replacement.match_length, replacement.new_text.as_str());
    }
    result
}

/// Applies replacements matched against `base_content` to `original_content`, preserving
/// unchanged line blocks from the original.
fn apply_replacements_preserving_unchanged_lines(
    original_content: &str,
    base_content: &str,
    replacements: &[MatchedEdit],
) -> Result<String, EditError> {
    let original_lines = split_lines_with_endings(original_content);
    let base_lines = get_line_spans(base_content);
    if original_lines.len() != base_lines.len() {
        return Err(EditError::new(
            "Cannot preserve unchanged lines because the base content has a different line count.",
        ));
    }

    let mut groups: Vec<(usize, usize, Vec<MatchedEdit>)> = Vec::new();
    let mut sorted = replacements.to_vec();
    sorted.sort_by_key(|replacement| replacement.match_index);
    for replacement in sorted {
        let (start_line, end_line) = get_replacement_line_range(&base_lines, &replacement)?;
        match groups.last_mut() {
            Some(current) if start_line < current.1 => {
                current.1 = current.1.max(end_line);
                current.2.push(replacement);
            }
            _ => groups.push((start_line, end_line, vec![replacement])),
        }
    }

    let mut original_line_index = 0;
    let mut result = String::new();
    for (start_line, end_line, group_replacements) in groups {
        result.push_str(&original_lines[original_line_index..start_line].concat());
        let group_start_offset = base_lines[start_line].start;
        let group_end_offset = base_lines[end_line - 1].end;
        result.push_str(&apply_replacements(
            &base_content[group_start_offset..group_end_offset],
            &group_replacements,
            group_start_offset,
        ));
        original_line_index = end_line;
    }
    result.push_str(&original_lines[original_line_index..].concat());
    Ok(result)
}

/// Generates a standard unified patch with file headers only.
pub fn generate_unified_patch(path: &str, old_content: &str, new_content: &str, context_lines: usize) -> String {
    let hunks = structured_patch(old_content, new_content, context_lines);
    let mut output = vec![format!("--- {path}"), format!("+++ {path}")];
    for hunk in hunks {
        let old_start = if hunk.old_lines == 0 {
            hunk.old_start - 1
        } else {
            hunk.old_start
        };
        let new_start = if hunk.new_lines == 0 {
            hunk.new_start - 1
        } else {
            hunk.new_start
        };
        output.push(format!(
            "@@ -{},{} +{},{} @@",
            old_start, hunk.old_lines, new_start, hunk.new_lines
        ));
        output.extend(hunk.lines);
    }
    format!("{}\n", output.join("\n"))
}

struct Hunk {
    old_start: i64,
    old_lines: usize,
    new_start: i64,
    new_lines: usize,
    lines: Vec<String>,
}

struct PatchChange {
    added: bool,
    removed: bool,
    lines: Vec<String>,
}

fn structured_patch(old_content: &str, new_content: &str, context: usize) -> Vec<Hunk> {
    let mut diff = diff_lines(old_content, new_content)
        .into_iter()
        .map(|change| PatchChange {
            lines: split_lines_for_patch(&change.value),
            added: change.added,
            removed: change.removed,
        })
        .collect::<Vec<_>>();
    diff.push(PatchChange {
        added: false,
        removed: false,
        lines: Vec::new(),
    });

    let mut hunks = Vec::new();
    let mut old_range_start: Option<(i64, i64)> = None;
    let mut current_range: Vec<String> = Vec::new();
    let mut old_line: i64 = 1;
    let mut new_line: i64 = 1;

    for index in 0..diff.len() {
        let current = &diff[index];
        let lines = current.lines.clone();
        if current.added || current.removed {
            if old_range_start.is_none() {
                let mut old_start = old_line;
                let mut new_start = new_line;
                if index > 0 {
                    let previous = &diff[index - 1];
                    if context > 0 {
                        let start = previous.lines.len().saturating_sub(context);
                        current_range = previous.lines[start..].iter().map(|line| format!(" {line}")).collect();
                    }
                    let skipped = current_range.len() as i64;
                    old_start -= skipped;
                    new_start -= skipped;
                }
                old_range_start = Some((old_start, new_start));
            }
            for line in &lines {
                current_range.push(format!("{}{line}", if current.added { '+' } else { '-' }));
            }
            if current.added {
                new_line += lines.len() as i64;
            } else {
                old_line += lines.len() as i64;
            }
        } else {
            if old_range_start.is_some() {
                if lines.len() <= context * 2 && index < diff.len() - 2 {
                    for line in &lines {
                        current_range.push(format!(" {line}"));
                    }
                } else {
                    let context_size = lines.len().min(context);
                    for line in &lines[..context_size] {
                        current_range.push(format!(" {line}"));
                    }
                    let (old_start, new_start) = old_range_start.expect("active range");
                    hunks.push(Hunk {
                        old_start,
                        old_lines: (old_line - old_start + context_size as i64) as usize,
                        new_start,
                        new_lines: (new_line - new_start + context_size as i64) as usize,
                        lines: std::mem::take(&mut current_range),
                    });
                    old_range_start = None;
                }
            }
            old_line += lines.len() as i64;
            new_line += lines.len() as i64;
        }
    }

    for hunk in &mut hunks {
        let mut index = 0;
        while index < hunk.lines.len() {
            if hunk.lines[index].ends_with('\n') {
                hunk.lines[index].pop();
            } else {
                hunk.lines.insert(index + 1, "\\ No newline at end of file".to_owned());
                index += 1;
            }
            index += 1;
        }
    }
    hunks
}

fn split_lines_for_patch(text: &str) -> Vec<String> {
    let has_trailing_newline = text.ends_with('\n');
    let mut result = text.split('\n').map(|line| format!("{line}\n")).collect::<Vec<_>>();
    if has_trailing_newline {
        result.pop();
    } else if let Some(last) = result.pop() {
        result.push(last.trim_end_matches('\n').to_owned());
    }
    result
}

/// Generates a display-oriented diff string with line numbers and context.
pub fn generate_diff_string(old_content: &str, new_content: &str, context_lines: usize) -> DiffStringResult {
    let parts = diff_lines(old_content, new_content);
    let mut output = Vec::new();
    let old_lines = old_content.split('\n').collect::<Vec<_>>();
    let new_lines = new_content.split('\n').collect::<Vec<_>>();
    let max_line_num = old_lines.len().max(new_lines.len());
    let line_num_width = max_line_num.to_string().len();

    let mut old_line_num = 1usize;
    let mut new_line_num = 1usize;
    let mut last_was_change = false;
    let mut first_changed_line = None;

    for (index, part) in parts.iter().enumerate() {
        let mut raw = part.value.split('\n').collect::<Vec<_>>();
        if raw.last() == Some(&"") {
            raw.pop();
        }

        if part.added || part.removed {
            first_changed_line.get_or_insert(new_line_num);
            for line in raw {
                if part.added {
                    output.push(format!("+{:>width$} {line}", new_line_num, width = line_num_width));
                    new_line_num += 1;
                } else {
                    output.push(format!("-{:>width$} {line}", old_line_num, width = line_num_width));
                    old_line_num += 1;
                }
            }
            last_was_change = true;
            continue;
        }

        let next_part_is_change = parts.get(index + 1).is_some_and(|next| next.added || next.removed);
        let has_leading_change = last_was_change;
        let has_trailing_change = next_part_is_change;

        if has_leading_change && has_trailing_change {
            if raw.len() <= context_lines * 2 {
                push_context_lines(&mut output, raw, &mut old_line_num, &mut new_line_num, line_num_width);
            } else {
                let leading = &raw[..context_lines];
                let trailing = &raw[raw.len() - context_lines..];
                let skipped = raw.len() - leading.len() - trailing.len();
                push_context_lines(
                    &mut output,
                    leading.to_vec(),
                    &mut old_line_num,
                    &mut new_line_num,
                    line_num_width,
                );
                output.push(format!(" {} ...", " ".repeat(line_num_width)));
                old_line_num += skipped;
                new_line_num += skipped;
                push_context_lines(
                    &mut output,
                    trailing.to_vec(),
                    &mut old_line_num,
                    &mut new_line_num,
                    line_num_width,
                );
            }
        } else if has_leading_change {
            let shown = &raw[..context_lines.min(raw.len())];
            let skipped = raw.len() - shown.len();
            push_context_lines(
                &mut output,
                shown.to_vec(),
                &mut old_line_num,
                &mut new_line_num,
                line_num_width,
            );
            if skipped > 0 {
                output.push(format!(" {} ...", " ".repeat(line_num_width)));
                old_line_num += skipped;
                new_line_num += skipped;
            }
        } else if has_trailing_change {
            let skipped = raw.len().saturating_sub(context_lines);
            if skipped > 0 {
                output.push(format!(" {} ...", " ".repeat(line_num_width)));
                old_line_num += skipped;
                new_line_num += skipped;
            }
            push_context_lines(
                &mut output,
                raw[skipped..].to_vec(),
                &mut old_line_num,
                &mut new_line_num,
                line_num_width,
            );
        } else {
            old_line_num += raw.len();
            new_line_num += raw.len();
        }
        last_was_change = false;
    }

    DiffStringResult {
        diff: output.join("\n"),
        first_changed_line,
    }
}

fn push_context_lines(
    output: &mut Vec<String>,
    lines: Vec<&str>,
    old_line_num: &mut usize,
    new_line_num: &mut usize,
    line_num_width: usize,
) {
    for line in lines {
        output.push(format!(" {:>width$} {line}", *old_line_num, width = line_num_width));
        *old_line_num += 1;
        *new_line_num += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Change {
    value: String,
    added: bool,
    removed: bool,
}

fn diff_lines(old_content: &str, new_content: &str) -> Vec<Change> {
    let old_tokens = tokenize_lines(old_content);
    let new_tokens = tokenize_lines(new_content);
    diff_tokens(&old_tokens, &new_tokens)
}

fn tokenize_lines(value: &str) -> Vec<String> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '\r' && characters.get(index + 1) == Some(&'\n') {
            current.push_str("\r\n");
            tokens.push(std::mem::take(&mut current));
            index += 2;
        } else if character == '\n' {
            current.push('\n');
            tokens.push(std::mem::take(&mut current));
            index += 1;
        } else {
            current.push(character);
            index += 1;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[derive(Debug, Clone)]
struct Component {
    count: usize,
    added: bool,
    removed: bool,
    previous: Option<usize>,
}

#[derive(Debug, Clone)]
struct PathState {
    old_pos: i64,
    last_component: Option<usize>,
}

fn diff_tokens(old_tokens: &[String], new_tokens: &[String]) -> Vec<Change> {
    let old_len = old_tokens.len() as i64;
    let new_len = new_tokens.len() as i64;
    let mut components = Vec::new();
    let mut best: HashMap<i64, PathState> = HashMap::new();
    best.insert(
        0,
        PathState {
            old_pos: -1,
            last_component: None,
        },
    );

    let new_pos = extract_common(
        best.get_mut(&0).expect("initial path"),
        new_tokens,
        old_tokens,
        0,
        &mut components,
    );
    if best[&0].old_pos + 1 >= old_len && new_pos + 1 >= new_len {
        return build_values(best[&0].last_component, &components, new_tokens, old_tokens);
    }

    let max_edit_length = old_len + new_len;
    let mut min_diagonal = i64::MIN / 4;
    let mut max_diagonal = i64::MAX / 4;
    let mut edit_length = 1i64;
    while edit_length <= max_edit_length {
        let mut diagonal = min_diagonal.max(-edit_length);
        while diagonal <= max_diagonal.min(edit_length) {
            let remove_path = best.get(&(diagonal - 1)).cloned();
            let add_path = best.get(&(diagonal + 1)).cloned();
            if remove_path.is_some() {
                best.remove(&(diagonal - 1));
            }
            let can_add = add_path.as_ref().is_some_and(|path| {
                let add_new_pos = path.old_pos - diagonal;
                (0..new_len).contains(&add_new_pos)
            });
            let can_remove = remove_path.as_ref().is_some_and(|path| path.old_pos + 1 < old_len);
            if !can_add && !can_remove {
                best.remove(&diagonal);
                diagonal += 2;
                continue;
            }

            let use_add = match (&remove_path, &add_path) {
                (None, Some(_)) => true,
                (Some(remove), Some(add)) => remove.old_pos < add.old_pos,
                _ => false,
            };
            let mut base = if use_add {
                add_to_path(add_path.as_ref().expect("add path"), true, false, 0, &mut components)
            } else {
                add_to_path(
                    remove_path.as_ref().expect("remove path"),
                    false,
                    true,
                    1,
                    &mut components,
                )
            };
            let new_pos = extract_common(&mut base, new_tokens, old_tokens, diagonal, &mut components);
            if base.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
                return build_values(base.last_component, &components, new_tokens, old_tokens);
            }
            if base.old_pos + 1 >= old_len {
                max_diagonal = max_diagonal.min(diagonal - 1);
            }
            if new_pos + 1 >= new_len {
                min_diagonal = min_diagonal.max(diagonal + 1);
            }
            best.insert(diagonal, base);
            diagonal += 2;
        }
        edit_length += 1;
    }
    Vec::new()
}

fn add_to_path(
    path: &PathState,
    added: bool,
    removed: bool,
    old_pos_increment: i64,
    components: &mut Vec<Component>,
) -> PathState {
    let merged = path.last_component.is_some_and(|index| {
        let last = &components[index];
        last.added == added && last.removed == removed
    });
    let (count, previous) = match path.last_component {
        Some(index) if merged => (components[index].count + 1, components[index].previous),
        _ => (1, path.last_component),
    };
    components.push(Component {
        count,
        added,
        removed,
        previous,
    });
    PathState {
        old_pos: path.old_pos + old_pos_increment,
        last_component: Some(components.len() - 1),
    }
}

fn extract_common(
    path: &mut PathState,
    new_tokens: &[String],
    old_tokens: &[String],
    diagonal: i64,
    components: &mut Vec<Component>,
) -> i64 {
    let new_len = new_tokens.len() as i64;
    let old_len = old_tokens.len() as i64;
    let mut old_pos = path.old_pos;
    let mut new_pos = old_pos - diagonal;
    let mut common = 0;
    while new_pos + 1 < new_len
        && old_pos + 1 < old_len
        && old_tokens[(old_pos + 1) as usize] == new_tokens[(new_pos + 1) as usize]
    {
        new_pos += 1;
        old_pos += 1;
        common += 1;
    }
    if common > 0 {
        components.push(Component {
            count: common as usize,
            added: false,
            removed: false,
            previous: path.last_component,
        });
        path.last_component = Some(components.len() - 1);
    }
    path.old_pos = old_pos;
    new_pos
}

fn build_values(
    last_component: Option<usize>,
    components: &[Component],
    new_tokens: &[String],
    old_tokens: &[String],
) -> Vec<Change> {
    let mut ordered = Vec::new();
    let mut current = last_component;
    while let Some(index) = current {
        ordered.push(&components[index]);
        current = components[index].previous;
    }
    ordered.reverse();

    let mut changes = Vec::with_capacity(ordered.len());
    let mut new_pos = 0;
    let mut old_pos = 0;
    for component in ordered {
        let value = if component.removed {
            let value = old_tokens[old_pos..old_pos + component.count].concat();
            old_pos += component.count;
            value
        } else {
            let value = new_tokens[new_pos..new_pos + component.count].concat();
            new_pos += component.count;
            if !component.added {
                old_pos += component.count;
            }
            value
        };
        changes.push(Change {
            value,
            added: component.added,
            removed: component.removed,
        });
    }
    changes
}
