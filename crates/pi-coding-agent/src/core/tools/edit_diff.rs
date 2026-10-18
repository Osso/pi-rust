//! Shared diff computation utilities for the edit and similar tools.

use std::io;

use tokio::io::AsyncReadExt;

use super::path_utils::resolve_to_cwd;
use crate::utils::paths::PathError;

use pi_agent_core::harness::tools::edit_diff::generate_diff_string as core_generate_diff_string;
pub use pi_agent_core::harness::tools::edit_diff::{
    AppliedEditsResult, Edit, EditError, FuzzyMatchResult, LineEnding, StripBomResult, TextReplacement,
    apply_replacements, apply_replacements_preserving_unchanged_lines, detect_line_ending, empty_old_text_error,
    fuzzy_find_text, generate_unified_patch, no_change_error, normalize_for_fuzzy_match, normalize_to_lf,
    not_found_error, restore_line_endings, strip_bom,
};
pub use pi_agent_core::jsdiff;

const DEFAULT_DUPLICATE_CONTEXT_MATCHES: usize = 5;
const DEFAULT_DUPLICATE_CONTEXT_LINES: usize = 2;
const DEFAULT_CONTEXT_LINE_MAX_LENGTH: usize = 160;

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

/// Generates a display-oriented diff with line numbers and context.
pub fn generate_diff_string(old_content: &str, new_content: &str, context_lines: usize) -> EditDiffResult {
    let result = core_generate_diff_string(old_content, new_content, context_lines);
    EditDiffResult {
        diff: result.diff,
        first_changed_line: result.first_changed_line,
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
        let found = fuzzy_find_text(replacement_base_content, &edit.old_text);
        if !found.found {
            return Err(not_found_error(path, index, normalized_edits.len()));
        }
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
        let start = utf16_offset_to_byte(&found.content_for_replacement, found.index as usize);
        let end = utf16_offset_to_byte(
            &found.content_for_replacement,
            found.index as usize + found.match_length as usize,
        );
        matched.push(MatchedEdit {
            edit_index: index,
            replacement: TextReplacement {
                match_index: start,
                match_length: end - start,
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

fn pad_left(value: usize, width: usize) -> String {
    format!("{value:>width$}")
}

fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

fn utf16_offset_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += character.len_utf16();
    }
    text.len()
}
