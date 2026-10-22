//! Tool path expansion and read-path variant resolution.

use std::path::Path;
use std::sync::OnceLock;

use super::unicode::normalize_nfd;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::js_regex::JsRegex;

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
pub fn expand_path(file_path: impl Into<JsString>) -> Result<JsString, PathError> {
    normalize_path(file_path, &tool_path_options())
}

/// Resolves a path relative to `cwd`, expanding `~` and accepting absolute paths.
pub fn resolve_to_cwd(file_path: impl Into<JsString>, cwd: impl Into<JsString>) -> Result<JsString, PathError> {
    resolve_path(file_path, cwd, &tool_path_options())
}

/// Synchronous existence check equivalent to `access(path, F_OK)`.
pub fn path_exists_sync(file_path: &JsString) -> bool {
    Path::new(&encode_node_utf8(file_path)).exists()
}

/// Asynchronous existence check equivalent to `access(path, F_OK)`.
pub async fn path_exists(file_path: &JsString) -> bool {
    tokio::fs::metadata(encode_node_utf8(file_path)).await.is_ok()
}

/// Resolves a read path, falling back to macOS screenshot, NFD, and curly-quote filename variants.
pub fn resolve_read_path(file_path: impl Into<JsString>, cwd: impl Into<JsString>) -> Result<JsString, PathError> {
    let resolved = resolve_to_cwd(file_path, cwd)?;
    Ok(read_path_variants(&resolved)
        .into_iter()
        .find(path_exists_sync)
        .unwrap_or(resolved))
}

/// Asynchronous form of [`resolve_read_path`].
pub async fn resolve_read_path_async(
    file_path: impl Into<JsString>,
    cwd: impl Into<JsString>,
) -> Result<JsString, PathError> {
    let resolved = resolve_to_cwd(file_path, cwd)?;
    for variant in read_path_variants(&resolved) {
        if path_exists(&variant).await {
            return Ok(variant);
        }
    }
    Ok(resolved)
}

/// Candidate paths in source order: the resolved path, then each distinct variant.
fn read_path_variants(resolved: &JsString) -> Vec<JsString> {
    let am_pm = replace_am_pm(resolved);
    let nfd = normalize_nfd(resolved);
    let curly = replace_with_curly_quote(resolved);
    let nfd_curly = replace_with_curly_quote(&nfd);
    let mut variants = vec![resolved.clone()];
    for candidate in [am_pm, nfd, curly, nfd_curly] {
        if !variants.contains(&candidate) {
            variants.push(candidate);
        }
    }
    variants
}

fn replace_am_pm(file_path: &JsString) -> JsString {
    static AM_PM: OnceLock<JsRegex> = OnceLock::new();
    let pattern = AM_PM.get_or_init(|| JsRegex::with_flags(r" (AM|PM)\.", "gi").expect("AM/PM pattern must compile"));
    pattern.replace_all(file_path, js_format!("{}$1.", NARROW_NO_BREAK_SPACE))
}

fn replace_with_curly_quote(file_path: &JsString) -> JsString {
    file_path.replace("'", "\u{2019}")
}
