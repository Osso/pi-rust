//! Port of `packages/coding-agent/extensions/bwrap/src/fs-worker.cjs` as an
//! internal pi process mode.
//!
//! Deviation (approved 2026-10-18): the TypeScript extension runs this worker
//! with Node. Rust has no Node runtime, so bwrap invokes the pi executable with
//! `--internal-bwrap-fs-worker`; operations, payloads, stdout, and exit
//! behavior are identical.

use std::io::Write;
use std::path::{Path, PathBuf};

use pi_js_compat::minimatch::{MinimatchOptions, minimatch};
use serde_json::{Value, json};

const EXPECTED_GREP_READ_ERROR_CODES: [&str; 5] = ["EACCES", "EISDIR", "ENOENT", "ENOTDIR", "EPERM"];

fn is_missing_path_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

fn is_expected_grep_read_error(error: &std::io::Error) -> bool {
    let code = error.raw_os_error().map(|code| match code {
        2 => "ENOENT",
        13 => "EACCES",
        20 => "ENOTDIR",
        21 => "EISDIR",
        1 => "EPERM",
        _ => "",
    });
    code.is_some_and(|code| EXPECTED_GREP_READ_ERROR_CODES.contains(&code))
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

fn resolve_workspace_path(value: Option<&str>, workspace: &Path, allow_missing: bool) -> Result<PathBuf, String> {
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

fn matches_glob(file_path: &str, pattern: &str) -> bool {
    let normalized = file_path.replace('\\', "/");
    let base = Path::new(&normalized)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| normalized.clone());
    let options = MinimatchOptions::default();
    if minimatch(&normalized, pattern, &options) {
        return true;
    }
    minimatch(&base, pattern, &options)
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
    glob: Option<String>,
    ignore_case: bool,
    limit: usize,
    literal: bool,
    pattern: String,
    root_is_directory: bool,
    root_path: PathBuf,
}

fn make_matcher(options: &GrepOptions, line: &str) -> bool {
    if options.literal {
        let (haystack, needle) = if options.ignore_case {
            (line.to_lowercase(), options.pattern.to_lowercase())
        } else {
            (line.to_owned(), options.pattern.clone())
        };
        return haystack.contains(&needle);
    }
    let pattern = if options.ignore_case {
        format!("(?i){}", options.pattern)
    } else {
        options.pattern.clone()
    };
    regex::Regex::new(&pattern)
        .map(|regex| regex.is_match(line))
        .unwrap_or(false)
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
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file_path.to_string_lossy().replace('\\', "/"))
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

fn format_grep_file(file_path: &Path, lines: &[String], options: &GrepOptions, limit: usize) -> GrepFileResult {
    let relative_path = relative_grep_path(file_path, options);
    let match_indexes: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| make_matcher(options, line))
        .map(|(index, _)| index)
        .take(limit)
        .collect();
    let mut text_parts = Vec::new();
    let mut lines_truncated = false;
    for match_index in &match_indexes {
        let (text, truncated) = format_grep_match(&relative_path, lines, *match_index, options.context);
        lines_truncated |= truncated;
        if !text.is_empty() {
            text_parts.push(text);
        }
    }
    GrepFileResult {
        count: match_indexes.len(),
        lines_truncated,
        text: text_parts.join("\n"),
    }
}

fn read_grep_lines(file_path: &Path) -> Result<Option<Vec<String>>, String> {
    match std::fs::read_to_string(file_path) {
        Ok(content) => Ok(Some(normalize_grep_lines(&content))),
        Err(error) if is_expected_grep_read_error(&error) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn collect_grep_results(
    files: &[PathBuf],
    options: &GrepOptions,
    file_index: usize,
    match_count: usize,
) -> Result<(usize, bool, Vec<String>), String> {
    if file_index >= files.len() || match_count >= options.limit {
        return Ok((match_count, false, Vec::new()));
    }
    let file_path = &files[file_index];
    let relative_path = relative_grep_path(file_path, options);
    if let Some(glob) = &options.glob
        && !matches_glob(&relative_path, glob)
    {
        return collect_grep_results(files, options, file_index + 1, match_count);
    }
    let remaining_limit = options.limit - match_count;
    let Some(lines) = read_grep_lines(file_path)? else {
        return collect_grep_results(files, options, file_index + 1, match_count);
    };
    let file_result = format_grep_file(file_path, &lines, options, remaining_limit);
    let (count, remaining_truncated, mut remaining_parts) =
        collect_grep_results(files, options, file_index + 1, match_count + file_result.count)?;
    let mut text_parts = Vec::new();
    if !file_result.text.is_empty() {
        text_parts.push(file_result.text);
    }
    text_parts.append(&mut remaining_parts);
    Ok((count, file_result.lines_truncated || remaining_truncated, text_parts))
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn payload_usize(payload: &Value, key: &str) -> Option<usize> {
    payload
        .get(key)
        .and_then(Value::as_f64)
        .map(|value| value.max(0.0) as usize)
}

/// Executes one worker operation; exposed for the Node-vs-Rust differential.
pub fn run_operation_json(operation: &str, payload: &Value) -> Result<Value, String> {
    let workspace = payload_string(payload, "workspace")
        .map(|workspace| absolute(&workspace))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let path_value = payload_string(payload, "path");
    match operation {
        "access" => {
            resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
            Ok(json!({"ok": true}))
        }
        "exists" => {
            let exists = resolve_workspace_path(path_value.as_deref(), &workspace, false)
                .ok()
                .is_some_and(|path| path.exists());
            Ok(json!({"exists": exists}))
        }
        "readFile" => {
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
            let content = std::fs::read(&path).map_err(|error| error.to_string())?;
            Ok(json!({"data": pi_js_compat::base64::buffer_to_base64(&content)}))
        }
        "readRange" => {
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
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
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, true)?;
            let content = payload_string(payload, "content").unwrap_or_default();
            std::fs::write(&path, content).map_err(|error| error.to_string())?;
            Ok(json!({"ok": true}))
        }
        "mkdir" => {
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, true)?;
            std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            Ok(json!({"ok": true}))
        }
        "stat" => {
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
            let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
            Ok(json!({"isDirectory": metadata.is_dir(), "size": metadata.len()}))
        }
        "readdir" => {
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
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
            let cwd = resolve_workspace_path(payload_string(payload, "cwd").as_deref(), &workspace, false)?;
            let pattern = payload_string(payload, "pattern").unwrap_or_default();
            let limit = payload_usize(payload, "limit");
            let mut results = Vec::new();
            for file_path in read_files_recursively(&cwd)? {
                let relative = file_path
                    .strip_prefix(&cwd)
                    .map(|path| path.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| file_path.to_string_lossy().replace('\\', "/"));
                if matches_glob(&relative, &pattern) {
                    results.push(relative);
                }
            }
            if let Some(limit) = limit {
                results.truncate(limit);
            }
            Ok(json!({"results": results}))
        }
        "grep" => {
            let root_path = resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
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
            let (count, lines_truncated, text_parts) = collect_grep_results(&files, &options, 0, 0)?;
            if count == 0 {
                return Ok(json!({"text": "No matches found"}));
            }
            let mut details = serde_json::Map::new();
            if count >= options.limit {
                details.insert("matchLimitReached".to_owned(), json!(options.limit));
            }
            if lines_truncated {
                details.insert("linesTruncated".to_owned(), json!(true));
            }
            let text = text_parts.join("\n");
            Ok(json!({"text": text, "details": Value::Object(details)}))
        }
        "readText" => {
            let path = resolve_workspace_path(path_value.as_deref(), &workspace, false)?;
            let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            Ok(json!({"text": text}))
        }
        other => Err(format!("unknown op: {other}")),
    }
}

/// `main()` of the worker. Returns the process exit code.
pub fn run_bwrap_fs_worker(args: &[String]) -> i32 {
    let operation = args.first().map(String::as_str).unwrap_or_default();
    let payload: Value = args
        .get(1)
        .map(|payload| serde_json::from_str(payload).unwrap_or_else(|_| json!({})))
        .unwrap_or_else(|| json!({}));
    match run_operation_json(operation, &payload) {
        Ok(value) => {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(serde_json::to_string(&value).unwrap_or_default().as_bytes());
            0
        }
        Err(error) => {
            eprint!("{error}");
            1
        }
    }
}
