//! Port of `packages/coding-agent/extensions/bwrap/src/fs-worker.cjs` as an
//! internal pi process mode.
//!
//! Deviation (approved 2026-10-18): the TypeScript extension runs this worker
//! with Node. Rust has no Node runtime, so bwrap invokes the pi executable with
//! `--internal-bwrap-fs-worker`; operations, payloads, stdout, and exit
//! behavior are identical.

use std::io::Write;
use std::path::{Path, PathBuf};

use pi_js_compat::js_regex::JsRegex;
use pi_js_compat::minimatch::{MinimatchOptions, minimatch};
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsString, JsValue as Value, encode_node_utf8},
};

use crate::utils::child_process::node_error_code;

const EXPECTED_GREP_READ_ERROR_CODES: [&str; 5] = ["EACCES", "EISDIR", "ENOENT", "ENOTDIR", "EPERM"];

fn is_missing_path_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

fn is_expected_grep_read_error(error: &std::io::Error) -> bool {
    EXPECTED_GREP_READ_ERROR_CODES.contains(&node_error_code(error).as_str())
}

fn is_inside_workspace(candidate: &Path, workspace: &Path) -> bool {
    let Ok(relative) = candidate.strip_prefix(workspace) else {
        return false;
    };
    if relative.as_os_str().is_empty() {
        return true;
    }
    !relative
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn read_nearest_existing_real_path(target: &Path) -> Result<PathBuf, String> {
    let mut current = target.parent().unwrap_or(target).to_path_buf();
    while current.parent().is_some() {
        match std::fs::canonicalize(&current) {
            Ok(real) => return Ok(real),
            Err(error) if is_missing_path_error(&error) => {
                let Some(parent) = current.parent() else {
                    break;
                };
                current = parent.to_path_buf();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    std::fs::canonicalize(&current).map_err(|error| error.to_string())
}

fn resolve_workspace_path(value: Option<&JsString>, workspace: &Path, allow_missing: bool) -> Result<PathBuf, String> {
    let encoded = value.map(encode_node_utf8);
    let value = encoded.as_deref();
    let resolved = match value {
        Some(value) if !value.is_empty() => {
            if Path::new(value).is_absolute() {
                absolute(value)
            } else {
                absolute(&workspace.join(value).display().to_string())
            }
        }
        _ => workspace.to_path_buf(),
    };
    let display = value.unwrap_or_default();
    if !is_inside_workspace(&resolved, workspace) {
        return Err(format!("sandbox path escapes workspace: {display}"));
    }
    let real_workspace = std::fs::canonicalize(workspace).map_err(|error| error.to_string())?;
    match std::fs::canonicalize(&resolved) {
        Ok(real_target) => {
            if is_inside_workspace(&real_target, &real_workspace) {
                Ok(resolved)
            } else {
                Err(format!("sandbox path symlink escapes workspace: {display}"))
            }
        }
        Err(error) => {
            if !allow_missing || !is_missing_path_error(&error) {
                return Err(error.to_string());
            }
            let real_parent = read_nearest_existing_real_path(&resolved)?;
            if is_inside_workspace(&real_parent, &real_workspace) {
                Ok(resolved)
            } else {
                Err(format!("sandbox path parent escapes workspace: {display}"))
            }
        }
    }
}

fn absolute(path: &str) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path))
}

fn read_files_recursively(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|error| error.to_string())?;
    let mut visible = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == "node_modules" {
            continue;
        }
        visible.push(entry.path());
    }
    visible.sort();
    for path in visible {
        if path.is_dir() {
            files.extend(read_files_recursively(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

fn matches_glob(file_path: impl Into<JsString>, pattern: &JsString) -> Result<bool, JsString> {
    if pattern.len() > 64 * 1024 {
        return Err(JsString::from("TypeError: pattern is too long"));
    }
    // Frozen worker uses path.posix.matchesGlob on Linux: Node's matcher treats
    // backslashes in the pattern as separators, not backslashes in the path.
    let normalized = file_path.into();
    let base = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or_default();
    let pattern = pattern.replace("\\", "/");
    let options = MinimatchOptions {
        nonegate: true,
        nocomment: true,
        nocase_magic_only: true,
        optimization_level: 2,
        ..Default::default()
    };
    Ok(minimatch(&normalized, &pattern, &options) || minimatch(&base, &pattern, &options))
}

fn truncate_line(line: &str) -> (String, bool) {
    if pi_js_compat::string::utf16_len(line) <= 500 {
        return (line.to_owned(), false);
    }
    let mut length = 0;
    let mut truncated = String::new();
    for character in line.chars() {
        length += character.len_utf16();
        if length > 500 {
            break;
        }
        truncated.push(character);
    }
    (truncated, true)
}

fn normalize_grep_lines(content: &str) -> Vec<String> {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

struct GrepOptions {
    context: usize,
    glob: Option<JsString>,
    ignore_case: bool,
    limit: usize,
    literal: bool,
    pattern: JsString,
    root_is_directory: bool,
    root_path: PathBuf,
}

enum LineMatcher {
    Literal { needle: JsString, ignore_case: bool },
    Regex(JsRegex),
}

impl LineMatcher {
    fn is_match(&self, line: impl Into<JsString>) -> Result<bool, JsString> {
        let line = line.into();
        match self {
            Self::Literal { needle, ignore_case } => {
                let matched = if *ignore_case {
                    line.to_lowercase().contains(needle.to_lowercase())
                } else {
                    line.contains(needle)
                };
                Ok(matched)
            }
            Self::Regex(regex) => Ok(regex.is_match(line)),
        }
    }
}

fn compile_matcher(options: &GrepOptions) -> Result<LineMatcher, JsString> {
    if options.literal {
        return Ok(LineMatcher::Literal {
            needle: options.pattern.clone(),
            ignore_case: options.ignore_case,
        });
    }
    let flags = if options.ignore_case { "i" } else { "" };
    JsRegex::with_flags(&options.pattern, flags)
        .map(LineMatcher::Regex)
        .map_err(|error| error.message().clone())
}

struct GrepFileResult {
    count: usize,
    lines_truncated: bool,
    text: String,
}

fn relative_grep_path(file_path: &Path, options: &GrepOptions) -> String {
    if !options.root_is_directory {
        return file_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    file_path
        .strip_prefix(&options.root_path)
        .map(|path| path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
        .unwrap_or_else(|_| file_path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
}

fn format_grep_match(relative_path: &str, lines: &[String], match_index: usize, context: usize) -> (String, bool) {
    let start = if context > 0 {
        match_index.saturating_sub(context)
    } else {
        match_index
    };
    let end = if context > 0 {
        (match_index + context).min(lines.len().saturating_sub(1))
    } else {
        match_index
    };
    let mut rendered = Vec::new();
    let mut truncated_any = false;
    for current in start..=end {
        let (text, truncated) = truncate_line(lines.get(current).map(String::as_str).unwrap_or(""));
        truncated_any |= truncated;
        let separator = if current == match_index { ":" } else { "-" };
        rendered.push(format!("{relative_path}{separator}{}{separator} {text}", current + 1));
    }
    (rendered.join("\n"), truncated_any)
}

fn format_grep_file(
    file_path: &Path,
    lines: &[String],
    options: &GrepOptions,
    matcher: &LineMatcher,
    limit: usize,
) -> Result<GrepFileResult, JsString> {
    let relative_path = relative_grep_path(file_path, options);
    let mut match_indexes = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if match_indexes.len() >= limit {
            break;
        }
        if matcher.is_match(line)? {
            match_indexes.push(index);
        }
    }
    let mut text_parts = Vec::new();
    let mut lines_truncated = false;
    for match_index in &match_indexes {
        let (text, truncated) = format_grep_match(&relative_path, lines, *match_index, options.context);
        lines_truncated |= truncated;
        if !text.is_empty() {
            text_parts.push(text);
        }
    }
    Ok(GrepFileResult {
        count: match_indexes.len(),
        lines_truncated,
        text: text_parts.join("\n"),
    })
}

fn read_grep_lines(file_path: &Path) -> Result<Option<Vec<String>>, String> {
    // readFile(filePath, "utf8")
    match std::fs::read(file_path) {
        Ok(content) => Ok(Some(normalize_grep_lines(&String::from_utf8_lossy(&content)))),
        Err(error) if is_expected_grep_read_error(&error) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn collect_grep_results(
    files: &[PathBuf],
    options: &GrepOptions,
    matcher: &LineMatcher,
    file_index: usize,
    match_count: usize,
) -> Result<(usize, bool, Vec<String>), JsString> {
    if file_index >= files.len() || match_count >= options.limit {
        return Ok((match_count, false, Vec::new()));
    }
    let file_path = &files[file_index];
    let relative_path = relative_grep_path(file_path, options);
    if let Some(glob) = &options.glob
        && !matches_glob(&relative_path, glob)?
    {
        return collect_grep_results(files, options, matcher, file_index + 1, match_count);
    }
    let remaining_limit = options.limit - match_count;
    let Some(lines) = read_grep_lines(file_path)? else {
        return collect_grep_results(files, options, matcher, file_index + 1, match_count);
    };
    let file_result = format_grep_file(file_path, &lines, options, matcher, remaining_limit)?;
    let (count, remaining_truncated, mut remaining_parts) =
        collect_grep_results(files, options, matcher, file_index + 1, match_count + file_result.count)?;
    let mut text_parts = Vec::new();
    if !file_result.text.is_empty() {
        text_parts.push(file_result.text);
    }
    text_parts.append(&mut remaining_parts);
    Ok((count, file_result.lines_truncated || remaining_truncated, text_parts))
}

fn payload_string(payload: &Value, key: &str) -> Option<JsString> {
    payload.get(key).and_then(Value::as_string).cloned()
}

fn payload_usize(payload: &Value, key: &str) -> Option<usize> {
    payload
        .get(key)
        .and_then(Value::as_f64)
        .map(|value| value.max(0.0) as usize)
}

/// Executes one worker operation; exposed for the Node-vs-Rust differential.
pub fn run_operation_json(operation: &str, payload: &Value) -> Result<Value, pi_ai::request::CallbackError> {
    let workspace = match payload_string(payload, "workspace").filter(|workspace| !workspace.is_empty()) {
        Some(workspace) => crate::utils::paths::node_resolve(JsString::default(), workspace)
            .map_err(pi_ai::request::CallbackError::from)?,
        None => pi_js_compat::process_cwd::cwd()
            .map_err(|error| pi_ai::request::CallbackError::from_error(error.message.clone(), error))?,
    };
    run_worker_operation(operation, payload, &PathBuf::from(encode_node_utf8(&workspace)))
        .map_err(pi_ai::request::CallbackError::from)
}

fn run_worker_operation(operation: &str, payload: &Value, workspace: &Path) -> Result<Value, JsString> {
    let path_value = payload_string(payload, "path");
    match operation {
        "access" => {
            resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            Ok(json!({"ok": true}))
        }
        "exists" => {
            let exists = resolve_workspace_path(path_value.as_ref(), workspace, false)
                .ok()
                .is_some_and(|path| path.exists());
            Ok(json!({"exists": exists}))
        }
        "readFile" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            let content = std::fs::read(&path).map_err(|error| error.to_string())?;
            Ok(json!({"data": pi_js_compat::base64::buffer_to_base64(&content)}))
        }
        "readRange" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            let start = payload_usize(payload, "start").unwrap_or(0);
            let end = payload_usize(payload, "end").unwrap_or(0);
            let length = end.saturating_sub(start);
            let content = std::fs::read(&path).map_err(|error| error.to_string())?;
            let slice = content
                .get(start..(start + length).min(content.len()))
                .unwrap_or_default();
            Ok(json!({"data": pi_js_compat::base64::buffer_to_base64(slice)}))
        }
        "writeFile" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, true)?;
            let content = payload_string(payload, "content").unwrap_or_default();
            std::fs::write(&path, encode_node_utf8(&content)).map_err(|error| error.to_string())?;
            Ok(json!({"ok": true}))
        }
        "mkdir" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, true)?;
            std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            Ok(json!({"ok": true}))
        }
        "stat" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
            Ok(json!({"isDirectory": metadata.is_dir(), "size": metadata.len()}))
        }
        "readdir" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&path).map_err(|error| error.to_string())? {
                entries.push(
                    entry
                        .map_err(|error| error.to_string())?
                        .file_name()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            entries.sort();
            Ok(json!({"entries": entries}))
        }
        "find" => {
            let cwd = resolve_workspace_path(payload_string(payload, "cwd").as_ref(), workspace, false)?;
            let pattern = payload_string(payload, "pattern").unwrap_or_default();
            let limit = payload_usize(payload, "limit");
            let mut results = Vec::new();
            for file_path in read_files_recursively(&cwd)? {
                let relative = file_path
                    .strip_prefix(&cwd)
                    .map(|path| path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
                    .unwrap_or_else(|_| file_path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
                if matches_glob(&relative, &pattern)? {
                    results.push(relative);
                }
            }
            if let Some(limit) = limit {
                results.truncate(limit);
            }
            Ok(json!({"results": results}))
        }
        "grep" => {
            let root_path = resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            let root_is_directory = std::fs::metadata(&root_path)
                .map_err(|error| error.to_string())?
                .is_dir();
            let files = if root_is_directory {
                read_files_recursively(&root_path)?
            } else {
                vec![root_path.clone()]
            };
            let context = payload.get("context").and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as usize;
            let limit = payload
                .get("limit")
                .and_then(Value::as_f64)
                .filter(|limit| *limit != 0.0)
                .map(|limit| limit.max(1.0) as usize)
                .unwrap_or(100);
            let options = GrepOptions {
                context,
                glob: payload_string(payload, "glob"),
                ignore_case: payload.get("ignoreCase").and_then(Value::as_bool).unwrap_or(false),
                limit,
                literal: payload.get("literal").and_then(Value::as_bool).unwrap_or(false),
                pattern: payload_string(payload, "pattern").unwrap_or_default(),
                root_is_directory,
                root_path,
            };
            let matcher = compile_matcher(&options).map_err(|error| JsString::from("SyntaxError: ") + error)?;
            let (count, lines_truncated, text_parts) = collect_grep_results(&files, &options, &matcher, 0, 0)?;
            if count == 0 {
                return Ok(json!({"text": "No matches found"}));
            }
            let mut details = pi_js_compat::js_json::JsObject::new();
            if count >= options.limit {
                details.insert("matchLimitReached".into(), json!(options.limit));
            }
            if lines_truncated {
                details.insert("linesTruncated".into(), json!(true));
            }
            let text = text_parts.join("\n");
            Ok(json!({"text": text, "details": Value::Object(details)}))
        }
        "readText" => {
            let path = resolve_workspace_path(path_value.as_ref(), workspace, false)?;
            // readFile(filePath, "utf8")
            let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
            let text = String::from_utf8_lossy(&bytes);
            Ok(json!({"text": text.as_ref()}))
        }
        other => Err(format!("unknown op: {other}").into()),
    }
}

/// `main()` of the worker. Returns the process exit code.
pub fn run_bwrap_fs_worker(args: &[String]) -> i32 {
    let operation = args.first().map(String::as_str).unwrap_or_default();
    let payload: Value = args
        .get(1)
        .map(|payload| js_json::parse_lossless(payload).unwrap_or_else(|_| json!({})))
        .unwrap_or_else(|| json!({}));
    match run_operation_json(operation, &payload) {
        Ok(value) => {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(serde_json::to_string(&value).unwrap_or_default().as_bytes());
            0
        }
        Err(error) => {
            eprint!("{}", encode_node_utf8(&crate::utils::uncaught::format_uncaught(&error)));
            1
        }
    }
}

#[cfg(test)]
#[path = "worker_matching_tests.rs"]
mod matching_tests;

#[cfg(test)]
mod utf8_tests {
    use super::*;

    #[test]
    fn grep_lines_replace_invalid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"before\xe2\x82after\xff\nkept\n").unwrap();
        assert_eq!(
            read_grep_lines(&path).unwrap(),
            Some(vec!["before\u{fffd}after\u{fffd}".into(), "kept".into(), "".into()])
        );
    }

    #[test]
    fn read_text_replaces_invalid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"before\xe2\x82after\xff end").unwrap();
        let payload = json!({"workspace": dir.path().display().to_string(), "path":"notes.txt"});
        assert_eq!(
            run_operation_json("readText", &payload).unwrap(),
            json!({"text":"before\u{fffd}after\u{fffd} end"})
        );
    }
}
