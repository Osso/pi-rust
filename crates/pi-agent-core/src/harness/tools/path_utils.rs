use unicode_normalization::UnicodeNormalization;

use crate::harness::types::{ExecutionEnv, FileError};
use tokio_util::sync::CancellationToken;

const NARROW_NO_BREAK_SPACE: char = '\u{202f}';
const RIGHT_SINGLE_QUOTATION_MARK: char = '\u{2019}';

/// Normalizes Unicode spaces and strips one leading `@` from a model-supplied path.
pub fn normalize_tool_path(path: &str) -> String {
    let normalized = path
        .chars()
        .map(|character| if is_unicode_space(character) { ' ' } else { character })
        .collect::<String>();
    match normalized.strip_prefix('@') {
        Some(rest) => rest.to_owned(),
        None => normalized,
    }
}

/// Resolves a tool path through [`ExecutionEnv::absolute_path`].
pub async fn resolve_tool_path(
    env: &dyn ExecutionEnv,
    path: &str,
    signal: Option<&CancellationToken>,
) -> Result<String, FileError> {
    env.absolute_path(&normalize_tool_path(path), signal).await
}

/// Resolves a read path, trying source-compatible Unicode and apostrophe variants.
pub async fn resolve_read_tool_path(
    env: &dyn ExecutionEnv,
    path: &str,
    signal: Option<&CancellationToken>,
) -> Result<String, FileError> {
    let resolved = resolve_tool_path(env, path, signal).await?;
    let mut variants = Vec::new();
    push_variant(&mut variants, resolved.clone());
    push_variant(&mut variants, replace_am_pm(&resolved));
    push_variant(&mut variants, resolved.nfd().collect());
    push_variant(&mut variants, replace_apostrophes(&resolved));
    push_variant(&mut variants, replace_apostrophes(&resolved.nfd().collect::<String>()));

    for variant in variants {
        if env.exists(&variant, signal).await? {
            return Ok(variant);
        }
    }
    Ok(resolved)
}

fn push_variant(variants: &mut Vec<String>, candidate: String) {
    if !variants.contains(&candidate) {
        variants.push(candidate);
    }
}

fn is_unicode_space(character: char) -> bool {
    matches!(character, '\u{00a0}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
        || ('\u{2000}'..='\u{200a}').contains(&character)
}

fn replace_am_pm(text: &str) -> String {
    let characters = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        if is_am_pm_marker(&characters, index) {
            output.push(NARROW_NO_BREAK_SPACE);
            output.push(characters[index + 1]);
            output.push(characters[index + 2]);
            output.push('.');
            index += 4;
            continue;
        }
        output.push(characters[index]);
        index += 1;
    }
    output
}

fn is_am_pm_marker(characters: &[char], index: usize) -> bool {
    if characters[index] != ' ' || index + 3 >= characters.len() || characters[index + 3] != '.' {
        return false;
    }
    let first = characters[index + 1];
    let second = characters[index + 2];
    matches!((first, second), ('A' | 'a', 'M' | 'm') | ('P' | 'p', 'M' | 'm'))
}

fn replace_apostrophes(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character == '\'' {
                RIGHT_SINGLE_QUOTATION_MARK
            } else {
                character
            }
        })
        .collect()
}
