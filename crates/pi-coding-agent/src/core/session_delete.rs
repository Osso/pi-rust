//! Frozen `session-delete.ts`: batch trash, unlink remaining files, clean only removed metadata.
use std::path::Path;
use std::process::Stdio;

use pi_js_compat::js_json::{JsString, JsValue, stringify_lossless};
use serde::{Serialize, Serializer};
use tokio::io::AsyncReadExt;

use super::session_control_db::{list_descendant_session_paths, remove_sessions_metadata};
use super::tools::node_fs::node_fs_error;

#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SessionDeleteResult {
    Success {
        ok: bool,
        method: &'static str,
    },
    Failure {
        ok: bool,
        #[serde(serialize_with = "serialize_error")]
        error: JsString,
    },
}

fn failure(error: String) -> SessionDeleteResult {
    SessionDeleteResult::Failure {
        ok: false,
        error: JsString::from(error.as_str()),
    }
}

fn serialize_error<S: Serializer>(error: &JsString, serializer: S) -> Result<S::Ok, S::Error> {
    let json = stringify_lossless(&JsValue::String(error.clone()));
    let raw = serde_json::value::RawValue::from_string(json).map_err(serde::ser::Error::custom)?;
    raw.serialize(serializer)
}

pub async fn delete_session_tree(path: &str, db: Option<&str>) -> SessionDeleteResult {
    let mut deleted = Vec::new();
    let mut result = delete_tree_files(path, db, &mut deleted).await;
    if let Some(db) = db.filter(|_| !deleted.is_empty()) {
        let paths: Vec<&str> = deleted.iter().map(String::as_str).collect();
        if let Err(error) = remove_sessions_metadata(db, &paths) {
            result = failure(format!(
                "Cannot clean metadata for {} deleted transcripts under {path}: Error: {error}",
                deleted.len()
            ));
        }
    }
    result
}

async fn delete_tree_files(path: &str, db: Option<&str>, deleted: &mut Vec<String>) -> SessionDeleteResult {
    let mut paths = match db
        .filter(|db| !db.is_empty())
        .map(|db| list_descendant_session_paths(db, path))
        .transpose()
    {
        Ok(paths) => paths.unwrap_or_default(),
        Err(error) => return failure(format!("Cannot delete session tree {path}: Error: {error}")),
    };
    paths.push(path.to_owned());
    let mut method = "trash";
    for batch in chunk_session_paths(&paths) {
        match delete_session_batch(batch, deleted).await {
            result @ SessionDeleteResult::Failure { .. } => return result,
            SessionDeleteResult::Success { method: "unlink", .. } => method = "unlink",
            _ => {}
        }
    }
    SessionDeleteResult::Success { ok: true, method }
}

fn chunk_session_paths(paths: &[String]) -> Vec<&[String]> {
    const BYTE_BUDGET: usize = 64 * 1024;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (index, path) in paths.iter().enumerate() {
        let path_bytes = path.len() + 1 + 8;
        if index > start && bytes + path_bytes > BYTE_BUDGET {
            chunks.push(&paths[start..index]);
            start = index;
            bytes = 0;
        }
        bytes += path_bytes;
    }
    if start < paths.len() {
        chunks.push(&paths[start..]);
    }
    chunks
}

async fn delete_session_batch(paths: &[String], deleted: &mut Vec<String>) -> SessionDeleteResult {
    let trash_error = trash_session_files(paths).await;
    let (remaining, removed): (Vec<&String>, Vec<&String>) = paths.iter().partition(|path| Path::new(path).exists());
    deleted.extend(removed.into_iter().cloned());
    let method = if remaining.is_empty() { "trash" } else { "unlink" };
    for path in remaining {
        if let Err(error) = tokio::fs::remove_file(path).await {
            let error = node_fs_error(&error, "unlink", path);
            let mut units: Vec<u16> = error.encode_utf16().collect();
            if let Some(trash) = trash_error {
                units.extend(" (trash: ".encode_utf16());
                units.extend_from_slice(trash.units());
                units.push(u16::from(b')'));
            }
            return SessionDeleteResult::Failure {
                ok: false,
                error: JsString::from_utf16(units),
            };
        }
        deleted.push(path.clone());
    }
    SessionDeleteResult::Success { ok: true, method }
}

fn spawn_trash(paths: &[String]) -> std::io::Result<tokio::process::Child> {
    let spawn = |command: &mut tokio::process::Command| {
        command
            .arg("--")
            .args(paths)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
    };
    let result = spawn(&mut tokio::process::Command::new("trash"));
    match result {
        // Node/libuv uses execvp: an executable text file without a shebang runs under /bin/sh.
        // Only ENOEXEC selects this path; positional arguments never become shell source.
        Err(error) if error.raw_os_error() == Some(rustix::io::Errno::NOEXEC.raw_os_error()) => {
            spawn(tokio::process::Command::new("/bin/sh").args(["-c", "exec trash \"$@\"", "trash"]))
        }
        other => other,
    }
}

async fn trash_session_files(paths: &[String]) -> Option<JsString> {
    let child = spawn_trash(paths);
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            let diagnostic = node_fs_error(&error, "spawn", "trash");
            let code = diagnostic.split(':').next().unwrap_or_default();
            return Some(JsString::from(format!("spawn trash {code}").as_str()));
        }
    };
    let stderr = child.stderr.take().expect("trash stderr is piped");
    let (status, stderr) = tokio::join!(child.wait(), read_stderr_prefix(stderr));
    match (status, stderr) {
        (Ok(status), _) if status.success() => None,
        (Ok(status), Ok(stderr)) => Some(format_trash_error(status, &stderr)),
        (Err(error), _) | (_, Err(error)) => Some(JsString::from(error.to_string().as_str())),
    }
}

async fn read_stderr_prefix(mut stderr: tokio::process::ChildStderr) -> std::io::Result<Vec<u8>> {
    // Four UTF-8 bytes per retained UTF-16 unit is sufficient for the source's 200-unit prefix.
    const PREFIX_BYTES: usize = 200 * 4;
    let mut prefix = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stderr.read(&mut buffer).await?;
        if count == 0 {
            return Ok(prefix);
        }
        let remaining = PREFIX_BYTES.saturating_sub(prefix.len());
        prefix.extend_from_slice(&buffer[..count.min(remaining)]);
    }
}

fn format_trash_error(status: std::process::ExitStatus, stderr: &[u8]) -> JsString {
    let prefix = String::from_utf8_lossy(stderr)
        .encode_utf16()
        .take(200)
        .collect::<Vec<_>>();
    let first_line = trim_utf16(&prefix)
        .split(|unit| *unit == u16::from(b'\n'))
        .next()
        .unwrap_or_default();
    if first_line.is_empty() {
        JsString::from(format!("trash exited with {}", exit_status_text(status)).as_str())
    } else {
        JsString::from_utf16(first_line.to_vec())
    }
}

fn trim_utf16(units: &[u16]) -> &[u16] {
    use pi_js_compat::string::is_js_whitespace_unit;
    let start = units
        .iter()
        .position(|unit| !is_js_whitespace_unit(*unit))
        .unwrap_or(units.len());
    let rest = &units[start..];
    let end = rest
        .iter()
        .rposition(|unit| !is_js_whitespace_unit(*unit))
        .map_or(0, |index| index + 1);
    &rest[..end]
}

fn exit_status_text(status: std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    if let Some(signal) = status.signal() {
        return rustix::process::Signal::from_named_raw(signal).map_or_else(
            || signal.to_string(),
            |signal| {
                let debug = format!("{signal:?}");
                let name = debug.trim_matches('"').trim_start_matches("Signal::");
                let name = match name {
                    "ABORT" => "ABRT",
                    "ALARM" => "ALRM",
                    other => other,
                };
                format!("SIG{name}")
            },
        );
    }
    status.code().map_or_else(|| "null".to_owned(), |code| code.to_string())
}
