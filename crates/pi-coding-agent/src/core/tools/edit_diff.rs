//! Shared diff computation utilities for the edit and similar tools.

use std::io;

use icu_normalizer::ComposingNormalizerBorrowed;
use tokio::io::AsyncReadExt;

use super::jsdiff;
use super::path_utils::resolve_to_cwd;
use crate::core::js_string::trim_end_javascript;
use crate::utils::paths::PathError;

pub use pi_agent_core::harness::tools::edit_diff::{
    AppliedEditsResult, Edit, FuzzyMatchResult, LineEnding, StripBomResult, detect_line_ending, normalize_to_lf,
    restore_line_endings, strip_bom,
};

const DEFAULT_DUPLICATE_CONTEXT_MATCHES: usize = 5;
const DEFAULT_DUPLICATE_CONTEXT_LINES: usize = 2;
const DEFAULT_CONTEXT_LINE_MAX_LENGTH: usize = 160;

/// A replacement expressed in byte offsets of the base content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextReplacement {
    pub match_index: usize,
    pub match_length: usize,
    pub new_text: String,
}

/// A diff with the first changed line in the new content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditDiffResult {
    pub diff: String,
    pub first_changed_line: Option<usize>,
}

/// Result of a preview computation: either a diff or the source error text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditDiffOutcome {
    Diff(EditDiffResult),
    Error(String),
}

/// A message-compatible edit failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    message: String,
}

impl EditError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EditError {}

/// Normalizes text for fuzzy matching: NFKC, per-line trailing-whitespace trim,
/// smart quotes to ASCII, dashes to `-`, and special spaces to a regular space.
pub fn normalize_for_fuzzy_match(text: &str) -> String {
    let normalized = ComposingNormalizerBorrowed::new_nfkc().normalize(text).into_owned();
    let trimmed = normalized
        .split('\n')
        .map(trim_end_javascript)
        .collect::<Vec<_>>()
        .join("\n");
    trimmed.chars().map(normalize_fuzzy_character).collect()
}

fn normalize_fuzzy_character(character: char) -> char {
    match character {
        '\u{2018}'..='\u{201B}' => '\'',
        '\u{201C}'..='\u{201F}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
        '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
        other => other,
    }
}

/// Finds `old_text` in `content`, preferring an exact match and falling back to
/// fuzzy-normalized matching.
///
/// Offsets are UTF-16 code units, matching the source's string indices.
pub fn fuzzy_find_text(content: &str, old_text: &str) -> FuzzyMatchResult {
    match find_text_bytes(content, old_text) {
        Some(found) => FuzzyMatchResult {
            found: true,
            index: utf16_length(&found.content[..found.index]) as f64,
            match_length: utf16_length(&found.content[found.index..found.index + found.match_length]) as f64,
            used_fuzzy_match: found.used_fuzzy_match,
            content_for_replacement: found.content,
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

struct ByteMatch {
    index: usize,
    match_length: usize,
    used_fuzzy_match: bool,
    content: String,
}

fn find_text_bytes(content: &str, old_text: &str) -> Option<ByteMatch> {
    if let Some(index) = content.find(old_text) {
        return Some(ByteMatch {
            index,
            match_length: old_text.len(),
            used_fuzzy_match: false,
            content: content.to_owned(),
        });
    }

    let fuzzy_content = normalize_for_fuzzy_match(content);
    let fuzzy_old_text = normalize_for_fuzzy_match(old_text);
    let index = fuzzy_content.find(&fuzzy_old_text)?;
    Some(ByteMatch {
        index,
        match_length: fuzzy_old_text.len(),
        used_fuzzy_match: true,
        content: fuzzy_content,
    })
}

/// Applies replacements matched against `base_content` to `original_content`,
/// keeping unchanged line blocks from the original.
pub fn apply_replacements_preserving_unchanged_lines(
    original_content: &str,
    base_content: &str,
    replacements: &[TextReplacement],
) -> Result<String, EditError> {
    let original_lines = split_lines_with_endings(original_content);
    let base_lines = get_line_spans(base_content);
    if original_lines.len() != base_lines.len() {
        return Err(EditError::new(
            "Cannot preserve unchanged lines because the base content has a different line count.",
        ));
    }

    let groups = group_replacements(&base_lines, replacements)?;

    let mut original_line_index = 0;
    let mut result = String::new();
    for group in groups {
        result.push_str(&original_lines[original_line_index..group.start_line].concat());
        let group_start_offset = base_lines[group.start_line].start;
        let group_end_offset = base_lines[group.end_line - 1].end;
        result.push_str(&apply_replacements(
            &base_content[group_start_offset..group_end_offset],
            &group.replacements,
            group_start_offset,
        ));
        original_line_index = group.end_line;
    }
    result.push_str(&original_lines[original_line_index..].concat());
    Ok(result)
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
        .any(|edit| fuzzy_find_text(normalized_content, &edit.old_text).used_fuzzy_match);
    let replacement_base_content = if used_fuzzy_match {
        normalize_for_fuzzy_match(normalized_content)
    } else {
        normalized_content.to_owned()
    };

    let matched_edits = match_edits(&replacement_base_content, &normalized_edits, path)?;
    ensure_disjoint(&matched_edits, path)?;
    let replacements = matched_edits
        .into_iter()
        .map(|matched| matched.replacement)
        .collect::<Vec<_>>();

    let new_content = if used_fuzzy_match {
        apply_replacements_preserving_unchanged_lines(normalized_content, &replacement_base_content, &replacements)?
    } else {
        apply_replacements(&replacement_base_content, &replacements, 0)
    };

    if normalized_content == new_content {
        return Err(no_change_error(path, normalized_edits.len()));
    }
    Ok(AppliedEditsResult {
        base_content: normalized_content.to_owned(),
        new_content,
    })
}

/// Generates a standard unified patch with file headers only.
pub fn generate_unified_patch(path: &str, old_content: &str, new_content: &str, context_lines: usize) -> String {
    jsdiff::create_two_files_patch(path, old_content, new_content, context_lines)
}

/// Generates a display-oriented diff with line numbers and context.
pub fn generate_diff_string(old_content: &str, new_content: &str, context_lines: usize) -> EditDiffResult {
    let parts = jsdiff::diff_lines(old_content, new_content);
    let old_lines = old_content.split('\n').collect::<Vec<_>>();
    let new_lines = new_content.split('\n').collect::<Vec<_>>();
    let line_num_width = old_lines.len().max(new_lines.len()).to_string().len();

    let mut output = Vec::new();
    let mut old_line_num = 1;
    let mut new_line_num = 1;
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
                    output.push(format!("+{} {line}", pad_left(new_line_num, line_num_width)));
                    new_line_num += 1;
                } else {
                    output.push(format!("-{} {line}", pad_left(old_line_num, line_num_width)));
                    old_line_num += 1;
                }
            }
            last_was_change = true;
            continue;
        }

        let next_is_change = parts.get(index + 1).is_some_and(|next| next.added || next.removed);
        let has_leading_change = last_was_change;
        let has_trailing_change = next_is_change;
        if has_leading_change && has_trailing_change {
            if raw.len() <= context_lines * 2 {
                push_context(&mut output, &raw, &mut old_line_num, &mut new_line_num, line_num_width);
            } else {
                push_context(
                    &mut output,
                    &raw[..context_lines],
                    &mut old_line_num,
                    &mut new_line_num,
                    line_num_width,
                );
                let skipped = raw.len() - context_lines * 2;
                output.push(skip_marker(line_num_width));
                old_line_num += skipped;
                new_line_num += skipped;
                push_context(
                    &mut output,
                    &raw[raw.len() - context_lines..],
                    &mut old_line_num,
                    &mut new_line_num,
                    line_num_width,
                );
            }
        } else if has_leading_change {
            let shown = raw.len().min(context_lines);
            push_context(
                &mut output,
                &raw[..shown],
                &mut old_line_num,
                &mut new_line_num,
                line_num_width,
            );
            let skipped = raw.len() - shown;
            if skipped > 0 {
                output.push(skip_marker(line_num_width));
                old_line_num += skipped;
                new_line_num += skipped;
            }
        } else if has_trailing_change {
            let skipped = raw.len().saturating_sub(context_lines);
            if skipped > 0 {
                output.push(skip_marker(line_num_width));
                old_line_num += skipped;
                new_line_num += skipped;
            }
            push_context(
                &mut output,
                &raw[skipped..],
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

    EditDiffResult {
        diff: output.join("\n"),
        first_changed_line,
    }
}

/// Computes the diff for one or more edits without applying them.
pub async fn compute_edits_diff(path: &str, edits: &[Edit], cwd: &str) -> Result<EditDiffOutcome, PathError> {
    let absolute_path = resolve_to_cwd(path, cwd)?;
    let raw_content = match read_editable_file(&absolute_path, path).await {
        Ok(content) => content,
        Err(error) => return Ok(EditDiffOutcome::Error(error)),
    };

    let content = strip_bom(&raw_content).text;
    let normalized_content = normalize_to_lf(&content);
    Ok(
        match apply_edits_to_normalized_content(&normalized_content, edits, path) {
            Ok(applied) => EditDiffOutcome::Diff(generate_diff_string(&applied.base_content, &applied.new_content, 4)),
            Err(error) => EditDiffOutcome::Error(error.to_string()),
        },
    )
}

/// Computes the diff for a single edit without applying it.
pub async fn compute_edit_diff(
    path: &str,
    old_text: &str,
    new_text: &str,
    cwd: &str,
) -> Result<EditDiffOutcome, PathError> {
    let edits = [Edit {
        old_text: old_text.to_owned(),
        new_text: new_text.to_owned(),
    }];
    compute_edits_diff(path, &edits, cwd).await
}

async fn read_editable_file(absolute_path: &str, display_path: &str) -> Result<String, String> {
    let mut file = match tokio::fs::File::open(absolute_path).await {
        Ok(file) => file,
        Err(error) => {
            return Err(format!(
                "Could not edit file: {display_path}. Error code: {}.",
                node_error_code(&error)
            ));
        }
    };
    let mut bytes = Vec::new();
    match file.read_to_end(&mut bytes).await {
        Ok(_) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(error) => Err(node_read_error_message(&error, absolute_path)),
    }
}

fn node_error_code(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "ENOENT",
        io::ErrorKind::PermissionDenied => "EACCES",
        io::ErrorKind::IsADirectory => "EISDIR",
        io::ErrorKind::NotADirectory => "ENOTDIR",
        io::ErrorKind::AlreadyExists => "EEXIST",
        io::ErrorKind::InvalidInput => "EINVAL",
        _ => "EIO",
    }
}

fn node_read_error_message(error: &io::Error, path: &str) -> String {
    let description = match error.kind() {
        io::ErrorKind::IsADirectory => "illegal operation on a directory",
        io::ErrorKind::PermissionDenied => "permission denied",
        io::ErrorKind::NotFound => "no such file or directory",
        _ => return format!("{}: {error}, read '{path}'", node_error_code(error)),
    };
    format!("{}: {description}, read '{path}'", node_error_code(error))
}

fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
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

#[derive(Debug, Clone, Copy)]
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

struct ReplacementGroup {
    start_line: usize,
    end_line: usize,
    replacements: Vec<TextReplacement>,
}

fn group_replacements(
    base_lines: &[LineSpan],
    replacements: &[TextReplacement],
) -> Result<Vec<ReplacementGroup>, EditError> {
    let mut sorted = replacements.to_vec();
    sorted.sort_by_key(|replacement| replacement.match_index);

    let mut groups: Vec<ReplacementGroup> = Vec::new();
    for replacement in sorted {
        let (start_line, end_line) = replacement_line_range(base_lines, &replacement)?;
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

fn replacement_line_range(lines: &[LineSpan], replacement: &TextReplacement) -> Result<(usize, usize), EditError> {
    let replacement_start = replacement.match_index;
    let replacement_end = replacement.match_index + replacement.match_length;
    let outside = || EditError::new("Replacement range is outside the base content.");

    let start_line = lines
        .iter()
        .position(|line| replacement_start >= line.start && replacement_start < line.end)
        .ok_or_else(outside)?;
    let mut end_line = start_line;
    while end_line < lines.len() && lines[end_line].end < replacement_end {
        end_line += 1;
    }
    if end_line >= lines.len() {
        return Err(outside());
    }
    Ok((start_line, end_line + 1))
}

fn apply_replacements(content: &str, replacements: &[TextReplacement], offset: usize) -> String {
    let mut result = content.to_owned();
    for replacement in replacements.iter().rev() {
        let match_index = replacement.match_index - offset;
        result.replace_range(
            match_index..match_index + replacement.match_length,
            &replacement.new_text,
        );
    }
    result
}

struct MatchedEdit {
    edit_index: usize,
    replacement: TextReplacement,
}

fn match_edits(
    replacement_base_content: &str,
    normalized_edits: &[Edit],
    path: &str,
) -> Result<Vec<MatchedEdit>, EditError> {
    let mut matched = Vec::new();
    for (index, edit) in normalized_edits.iter().enumerate() {
        let Some(found) = find_text_bytes(replacement_base_content, &edit.old_text) else {
            return Err(not_found_error(path, index, normalized_edits.len()));
        };
        let occurrences = count_occurrences(replacement_base_content, &edit.old_text);
        if occurrences > 1 {
            return Err(duplicate_error(
                path,
                index,
                normalized_edits.len(),
                occurrences,
                replacement_base_content,
                &edit.old_text,
            ));
        }
        matched.push(MatchedEdit {
            edit_index: index,
            replacement: TextReplacement {
                match_index: found.index,
                match_length: found.match_length,
                new_text: edit.new_text.clone(),
            },
        });
    }
    matched.sort_by_key(|edit| edit.replacement.match_index);
    Ok(matched)
}

fn ensure_disjoint(matched: &[MatchedEdit], path: &str) -> Result<(), EditError> {
    for pair in matched.windows(2) {
        let previous = &pair[0].replacement;
        let current = &pair[1].replacement;
        if previous.match_index + previous.match_length > current.match_index {
            return Err(EditError::new(format!(
                "edits[{}] and edits[{}] overlap in {path}. Merge them into one edit or target disjoint regions.",
                pair[0].edit_index, pair[1].edit_index
            )));
        }
    }
    Ok(())
}

fn find_text_occurrences(content: &str, old_text: &str) -> Vec<(usize, usize)> {
    if old_text.is_empty() {
        return Vec::new();
    }
    let mut occurrences = Vec::new();
    let mut start = 0;
    while start <= content.len() {
        match content[start..].find(old_text) {
            Some(relative) => {
                let index = start + relative;
                occurrences.push((index, old_text.len()));
                start = index + old_text.len();
            }
            None => break,
        }
    }
    occurrences
}

fn count_occurrences(content: &str, old_text: &str) -> usize {
    let fuzzy_content = normalize_for_fuzzy_match(content);
    let fuzzy_old_text = normalize_for_fuzzy_match(old_text);
    find_text_occurrences(&fuzzy_content, &fuzzy_old_text).len()
}

fn line_number_at_offset(content: &str, offset: usize) -> usize {
    // Counting LF bytes avoids slicing at a UTF-16 offset that may fall inside a scalar.
    content.as_bytes()[..offset.min(content.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn truncate_context_line(line: &str, max_length: usize) -> String {
    if utf16_length(line) <= max_length {
        return line.to_owned();
    }
    let mut output = String::new();
    let mut units = 0;
    for character in line.chars() {
        if units + character.len_utf16() > max_length - 1 {
            break;
        }
        units += character.len_utf16();
        output.push(character);
    }
    output.push('…');
    output
}

fn format_duplicate_match_contexts(content: &str, old_text: &str) -> String {
    let occurrences = find_text_occurrences(content, old_text);
    if occurrences.is_empty() {
        return String::new();
    }

    let lines = content.split('\n').collect::<Vec<_>>();
    let line_number_width = lines.len().to_string().len();
    let shown = occurrences.iter().take(DEFAULT_DUPLICATE_CONTEXT_MATCHES).enumerate();
    let blocks = shown
        .map(|(occurrence_index, (index, length))| {
            let start_line = line_number_at_offset(content, *index);
            let end_line = line_number_at_offset(content, index + length.saturating_sub(1));
            let first_context_line = start_line.saturating_sub(DEFAULT_DUPLICATE_CONTEXT_LINES).max(1);
            let last_context_line = (end_line + DEFAULT_DUPLICATE_CONTEXT_LINES).min(lines.len());
            let context = (first_context_line..=last_context_line)
                .map(|line_number| {
                    let marker = if line_number >= start_line && line_number <= end_line {
                        ">"
                    } else {
                        " "
                    };
                    let padded = pad_left(line_number, line_number_width);
                    let text = lines.get(line_number - 1).copied().unwrap_or("");
                    format!(
                        "{marker} {padded}: {}",
                        truncate_context_line(text, DEFAULT_CONTEXT_LINE_MAX_LENGTH)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{}) line {start_line}\n{context}", occurrence_index + 1)
        })
        .collect::<Vec<_>>();

    let omitted_count = occurrences.len().saturating_sub(DEFAULT_DUPLICATE_CONTEXT_MATCHES);
    let omitted = if omitted_count > 0 {
        format!("\n... {omitted_count} more match(es) omitted.")
    } else {
        String::new()
    };
    format!("\n\nDuplicate match context:\n{}{omitted}", blocks.join("\n\n"))
}

fn not_found_error(path: &str, edit_index: usize, total_edits: usize) -> EditError {
    if total_edits == 1 {
        return EditError::new(format!(
            "Could not find the exact text in {path}. The old text must match exactly including all whitespace and newlines."
        ));
    }
    EditError::new(format!(
        "Could not find edits[{edit_index}] in {path}. The oldText must match exactly including all whitespace and newlines."
    ))
}

fn duplicate_error(
    path: &str,
    edit_index: usize,
    total_edits: usize,
    occurrences: usize,
    content: &str,
    old_text: &str,
) -> EditError {
    let context = format_duplicate_match_contexts(content, old_text);
    if total_edits == 1 {
        return EditError::new(format!(
            "Found {occurrences} occurrences of the text in {path}. The text must be unique. Please provide more context to make it unique.{context}"
        ));
    }
    EditError::new(format!(
        "Found {occurrences} occurrences of edits[{edit_index}] in {path}. Each oldText must be unique. Please provide more context to make it unique.{context}"
    ))
}

fn empty_old_text_error(path: &str, edit_index: usize, total_edits: usize) -> EditError {
    if total_edits == 1 {
        return EditError::new(format!("oldText must not be empty in {path}."));
    }
    EditError::new(format!("edits[{edit_index}].oldText must not be empty in {path}."))
}

fn no_change_error(path: &str, total_edits: usize) -> EditError {
    if total_edits == 1 {
        return EditError::new(format!(
            "No changes made to {path}. The replacement produced identical content. This might indicate an issue with special characters or the text not existing as expected."
        ));
    }
    EditError::new(format!(
        "No changes made to {path}. The replacements produced identical content."
    ))
}

fn pad_left(value: usize, width: usize) -> String {
    format!("{value:>width$}")
}

fn skip_marker(line_num_width: usize) -> String {
    format!(" {} ...", " ".repeat(line_num_width))
}

fn push_context(
    output: &mut Vec<String>,
    lines: &[&str],
    old_line_num: &mut usize,
    new_line_num: &mut usize,
    line_num_width: usize,
) {
    for line in lines {
        output.push(format!(" {} {line}", pad_left(*old_line_num, line_num_width)));
        *old_line_num += 1;
        *new_line_num += 1;
    }
}
