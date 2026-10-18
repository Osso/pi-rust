//! Tool path expansion and read-path variant resolution.

use std::path::Path;

use icu_normalizer::DecomposingNormalizerBorrowed;
use regex::Regex;

use crate::utils::paths::{PathError, PathInputOptions, normalize_path, resolve_path};

const NARROW_NO_BREAK_SPACE: char = '\u{202F}';

fn tool_path_options() -> PathInputOptions {
    PathInputOptions {
        normalize_unicode_spaces: true,
        strip_at_prefix: true,
        ..PathInputOptions::default()
    }
}

/// Normalizes a tool path input without resolving it against a base directory.
pub fn expand_path(file_path: &str) -> Result<String, PathError> {
    normalize_path(file_path, &tool_path_options())
}

/// Resolves a path relative to `cwd`, expanding `~` and accepting absolute paths.
pub fn resolve_to_cwd(file_path: &str, cwd: &str) -> Result<String, PathError> {
    resolve_path(file_path, cwd, &tool_path_options())
}

/// Synchronous existence check equivalent to `access(path, F_OK)`.
pub fn path_exists_sync(file_path: &str) -> bool {
    Path::new(file_path).exists()
}

/// Asynchronous existence check equivalent to `access(path, F_OK)`.
pub async fn path_exists(file_path: &str) -> bool {
    tokio::fs::metadata(file_path).await.is_ok()
}

/// Resolves a read path, falling back to macOS screenshot, NFD, and curly-quote filename variants.
pub fn resolve_read_path(file_path: &str, cwd: &str) -> Result<String, PathError> {
    resolve_with(file_path, cwd, path_exists_sync)
}

/// Asynchronous form of [`resolve_read_path`].
pub async fn resolve_read_path_async(file_path: &str, cwd: &str) -> Result<String, PathError> {
    let resolved = resolve_to_cwd(file_path, cwd)?;
    for variant in read_path_variants(&resolved) {
        if path_exists(&variant).await {
            return Ok(variant);
        }
    }
    Ok(resolved)
}

fn resolve_with(file_path: &str, cwd: &str, exists: fn(&str) -> bool) -> Result<String, PathError> {
    let resolved = resolve_to_cwd(file_path, cwd)?;
    for variant in read_path_variants(&resolved) {
        if exists(&variant) {
            return Ok(variant);
        }
    }
    Ok(resolved)
}

/// Candidate paths in source order: the resolved path, then each distinct variant.
fn read_path_variants(resolved: &str) -> Vec<String> {
    let am_pm = replace_am_pm(resolved);
    let nfd = to_nfd(resolved);
    let curly = replace_with_curly_quote(resolved);
    let nfd_curly = replace_with_curly_quote(&nfd);
    let mut variants = vec![resolved.to_owned()];
    for candidate in [am_pm, nfd, curly, nfd_curly] {
        if !variants.contains(&candidate) {
            variants.push(candidate);
        }
    }
    variants
}

fn replace_am_pm(file_path: &str) -> String {
    static AM_PM: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let pattern = AM_PM.get_or_init(|| Regex::new(r" ([AaPp][Mm])\.").expect("AM/PM pattern must compile"));
    pattern
        .replace_all(file_path, |captures: &regex::Captures<'_>| {
            format!("{NARROW_NO_BREAK_SPACE}{}.", &captures[1])
        })
        .into_owned()
}

fn replace_with_curly_quote(file_path: &str) -> String {
    file_path.replace('\'', "\u{2019}")
}

fn to_nfd(file_path: &str) -> String {
    DecomposingNormalizerBorrowed::new_nfd()
        .normalize(file_path)
        .into_owned()
}
