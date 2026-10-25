//! Shared diff computation utilities for the edit and similar tools.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tokio::io::AsyncReadExt;

use super::node_fs::node_promises_read_error;
use super::path_utils::resolve_to_cwd;
use crate::utils::child_process::node_error_code;
use crate::utils::paths::PathError;

use super::edit_primitives::generate_diff_string as core_generate_diff_string;
pub use super::edit_primitives::{
    AppliedEditsResult, Edit, EditError, FuzzyMatchResult, LineEnding, StripBomResult, TextReplacement,
    apply_replacements, apply_replacements_preserving_unchanged_lines, detect_line_ending, empty_old_text_error,
    fuzzy_find_text, generate_unified_patch, no_change_error, normalize_for_fuzzy_match, normalize_to_lf,
    not_found_error, restore_line_endings, strip_bom,
};
pub use super::jsdiff;

const DEFAULT_DUPLICATE_CONTEXT_MATCHES: usize = 5;
const DEFAULT_DUPLICATE_CONTEXT_LINES: usize = 2;
const DEFAULT_CONTEXT_LINE_MAX_LENGTH: usize = 160;

/// A diff with the first changed line in the new content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditDiffResult {
    pub diff: JsString,
    pub first_changed_line: Option<usize>,
}

/// Result of a preview computation: either a diff or the source error text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditDiffOutcome {
    Diff(EditDiffResult),
    Error(JsString),
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

    if normalized_content == &new_content {
        return Err(no_change_error(path, normalized_edits.len()));
    }
    Ok(AppliedEditsResult {
        base_content: normalized_content.to_owned(),
        new_content,
    })
}

/// Generates a display-oriented diff with line numbers and context.
pub fn generate_diff_string(
    old_content: impl Into<JsString>,
    new_content: impl Into<JsString>,
    context_lines: usize,
) -> EditDiffResult {
    let old_content = old_content.into();
    let old_content = &old_content;
    let new_content = new_content.into();
    let new_content = &new_content;

    let result = core_generate_diff_string(old_content, new_content, context_lines);
    EditDiffResult {
        diff: result.diff,
        first_changed_line: result.first_changed_line,
    }
}

/// Computes the diff for one or more edits without applying them.
pub async fn compute_edits_diff(
    path: impl Into<JsString>,
    edits: &[Edit],
    cwd: impl Into<JsString>,
) -> Result<EditDiffOutcome, PathError> {
    let path = path.into();
    let path = &path;

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
            Err(error) => EditDiffOutcome::Error(error.message),
        },
    )
}

/// Computes the diff for a single edit without applying it.
pub async fn compute_edit_diff(
    path: impl Into<JsString>,
    old_text: impl Into<JsString>,
    new_text: impl Into<JsString>,
    cwd: impl Into<JsString>,
) -> Result<EditDiffOutcome, PathError> {
    let path = path.into();
    let path = &path;
    let old_text = old_text.into();
    let old_text = &old_text;
    let new_text = new_text.into();
    let new_text = &new_text;

    let edits = [Edit {
        old_text: old_text.to_owned(),
        new_text: new_text.to_owned(),
    }];
    compute_edits_diff(path, &edits, cwd).await
}

async fn read_editable_file(absolute_path: &JsString, display_path: &JsString) -> Result<String, JsString> {
    let mut file = match tokio::fs::File::open(&encode_node_utf8(absolute_path)).await {
        Ok(file) => file,
        Err(error) => {
            return Err(JsString::from("Could not edit file: ")
                + display_path
                + format!(". Error code: {}.", node_error_code(&error)));
        }
    };
    let mut bytes = Vec::new();
    match file.read_to_end(&mut bytes).await {
        Ok(_) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(error) => Err(node_promises_read_error(&error, absolute_path)),
    }
}

struct MatchedEdit {
    edit_index: usize,
    replacement: TextReplacement,
}

fn match_edits(
    replacement_base_content: &JsString,
    normalized_edits: &[Edit],
    path: &JsString,
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
        let start = found.index as usize;
        let end = start + found.match_length as usize;
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

fn ensure_disjoint(matched: &[MatchedEdit], path: &JsString) -> Result<(), EditError> {
    for pair in matched.windows(2) {
        let previous = &pair[0].replacement;
        let current = &pair[1].replacement;
        if previous.match_index + previous.match_length > current.match_index {
            return Err(EditError::new(
                JsString::from(format!(
                    "edits[{}] and edits[{}] overlap in ",
                    pair[0].edit_index, pair[1].edit_index
                )) + path
                    + ". Merge them into one edit or target disjoint regions.",
            ));
        }
    }
    Ok(())
}

fn find_text_occurrences(content: &JsString, old_text: &JsString) -> Vec<(usize, usize)> {
    if old_text.is_empty() {
        return Vec::new();
    }
    let mut occurrences = Vec::new();
    let mut start = 0;
    while let Some(index) = content.find_from(old_text, start) {
        occurrences.push((index, old_text.len()));
        start = index + old_text.len();
    }
    occurrences
}

fn count_occurrences(content: &JsString, old_text: &JsString) -> usize {
    let fuzzy_content = normalize_for_fuzzy_match(content);
    let fuzzy_old_text = normalize_for_fuzzy_match(old_text);
    find_text_occurrences(&fuzzy_content, &fuzzy_old_text).len()
}

fn line_number_at_offset(content: &JsString, offset: usize) -> usize {
    content.units()[..offset.min(content.len())]
        .iter()
        .filter(|unit| **unit == 0x0a)
        .count()
        + 1
}

fn truncate_context_line(line: &JsString, max_length: usize) -> JsString {
    if line.len() <= max_length {
        line.clone()
    } else {
        line.slice(0, max_length - 1) + "…"
    }
}

fn format_duplicate_match_block(
    content: &JsString,
    lines: &[JsString],
    ordinal: usize,
    index: usize,
    length: usize,
) -> JsString {
    let start_line = line_number_at_offset(content, index);
    let end_line = line_number_at_offset(content, index + length.saturating_sub(1));
    let first_context_line = start_line.saturating_sub(DEFAULT_DUPLICATE_CONTEXT_LINES).max(1);
    let last_context_line = (end_line + DEFAULT_DUPLICATE_CONTEXT_LINES).min(lines.len());
    let line_number_width = lines.len().to_string().len();
    let context = (first_context_line..=last_context_line).map(|line_number| {
        let marker = if line_number >= start_line && line_number <= end_line {
            ">"
        } else {
            " "
        };
        let padded = pad_left(line_number, line_number_width);
        let text = lines.get(line_number - 1).cloned().unwrap_or_default();
        JsString::from(format!("{marker} {padded}: ")) + truncate_context_line(&text, DEFAULT_CONTEXT_LINE_MAX_LENGTH)
    });
    JsString::from(format!("{}) line {start_line}\n", ordinal + 1)) + JsString::join(context, "\n")
}

fn format_duplicate_match_contexts(content: &JsString, old_text: &JsString) -> JsString {
    let occurrences = find_text_occurrences(content, old_text);
    if occurrences.is_empty() {
        return JsString::default();
    }
    let lines = content.split('\n').collect::<Vec<_>>();
    let blocks = occurrences
        .iter()
        .take(DEFAULT_DUPLICATE_CONTEXT_MATCHES)
        .enumerate()
        .map(|(ordinal, (index, length))| format_duplicate_match_block(content, &lines, ordinal, *index, *length));
    let omitted_count = occurrences.len().saturating_sub(DEFAULT_DUPLICATE_CONTEXT_MATCHES);
    let omitted = if omitted_count > 0 {
        format!("\n... {omitted_count} more match(es) omitted.")
    } else {
        String::new()
    };
    JsString::from("\n\nDuplicate match context:\n") + JsString::join(blocks, "\n\n") + omitted
}

fn duplicate_error(
    path: &JsString,
    edit_index: usize,
    total_edits: usize,
    occurrences: usize,
    content: &JsString,
    old_text: &JsString,
) -> EditError {
    let context = format_duplicate_match_contexts(content, old_text);
    if total_edits == 1 {
        EditError::new(
            JsString::from(format!("Found {occurrences} occurrences of the text in "))
                + path
                + ". The text must be unique. Please provide more context to make it unique."
                + context,
        )
    } else {
        EditError::new(
            JsString::from(format!("Found {occurrences} occurrences of edits[{edit_index}] in "))
                + path
                + ". Each oldText must be unique. Please provide more context to make it unique."
                + context,
        )
    }
}

fn pad_left(value: usize, width: usize) -> String {
    format!("{value:>width$}")
}
