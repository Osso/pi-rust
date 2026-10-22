//! Port of `packages/agent/src/harness/session/jsonl/types.ts` plus the
//! filesystem capability subset required by the JSONL backend.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fs;
use std::io::Write;

use pi_ai::request::BoxFuture;
use pi_ai::types::JsonObject;
use pi_js_compat::number as js_number;
use serde::{Deserialize, Serialize};

use crate::harness::session::types::{JsonValue, SessionCreateOptions};
use crate::harness::types::{FileError, FileErrorCode, FileInfo, FileKind};

/// Filesystem operations used by the JSONL session backend.
pub trait JsonlSessionRepoFileSystem: Send + Sync {
    fn absolute_path<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<JsString, FileError>>;
    fn join_path<'a>(&'a self, parts: &'a [JsString]) -> BoxFuture<'a, Result<JsString, FileError>>;
    fn read_text_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<String, FileError>>;
    fn read_text_lines<'a>(
        &'a self,
        path: &'a JsString,
        max_lines: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<String>, FileError>>;
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a str) -> BoxFuture<'a, Result<(), FileError>>;
    fn append_file<'a>(&'a self, path: &'a JsString, content: &'a str) -> BoxFuture<'a, Result<(), FileError>>;
    fn rename_file<'a>(
        &'a self,
        source: &'a JsString,
        destination: &'a JsString,
    ) -> BoxFuture<'a, Result<(), FileError>>;
    fn file_info<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<FileInfo, FileError>>;
    fn list_dir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>>;
    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<bool, FileError>>;
    fn create_dir<'a>(&'a self, path: &'a JsString, recursive: bool) -> BoxFuture<'a, Result<(), FileError>>;
    fn remove<'a>(&'a self, path: &'a JsString, recursive: bool, force: bool) -> BoxFuture<'a, Result<(), FileError>>;
}

/// JSONL repository options.
#[derive(Clone)]
pub struct JsonlSessionRepoOptions {
    pub fs: std::sync::Arc<dyn JsonlSessionRepoFileSystem>,
    pub sessions_root: JsString,
}

/// Session metadata projected from a JSONL header plus filesystem state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonlSessionMetadata {
    pub id: String,
    pub created_at: i64,
    pub cwd: JsString,
    pub path: JsString,
    #[serde(serialize_with = "js_number::serialize")]
    pub modified_at: f64,
    pub source_format: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_parent_session_path: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonObject>,
}

impl crate::harness::session::types::SessionMetadataValue for JsonlSessionMetadata {
    fn id(&self) -> &str {
        &self.id
    }
}

/// JSONL creation options.
#[derive(Debug, Clone, Default)]
pub struct JsonlSessionCreateOptions {
    pub id: Option<String>,
    pub parent_session_id: Option<String>,
    pub cwd: JsString,
    pub metadata: Option<JsonObject>,
}

impl JsonlSessionCreateOptions {
    pub fn session_create_options(&self) -> SessionCreateOptions {
        SessionCreateOptions {
            id: self.id.clone(),
            parent_session_id: self.parent_session_id.clone(),
        }
    }
}

/// JSONL list options.
#[derive(Debug, Clone, Default)]
pub struct JsonlSessionListOptions {
    pub cwd: Option<JsString>,
}

/// Version 4 session header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonlV4Header {
    pub kind: HeaderKind,
    pub version: u8,
    pub id: String,
    pub created_at: i64,
    pub cwd: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_parent_session_path: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonObject>,
}

/// Header discriminant literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeaderKind {
    Header,
}

/// Native filesystem adapter used by tests and Rust-only callers.
#[derive(Debug, Clone)]
pub struct NativeFileSystem {
    cwd: JsString,
}

impl NativeFileSystem {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self { cwd: cwd.into() }
    }
    pub fn cwd(&self) -> &JsString {
        &self.cwd
    }
    fn resolve(&self, path: &JsString) -> JsString {
        let expanded = expand_home(path);
        let joined = if expanded.starts_with('/') {
            expanded
        } else {
            self.cwd.clone() + "/" + expanded
        };
        normalize(&joined)
    }
    fn info(&self, path: &JsString) -> Result<FileInfo, FileError> {
        file_info_from_path(&self.resolve(path))
    }
}

fn expand_home(path: &JsString) -> JsString {
    let Some(home) = pi_js_compat::process_env::get("HOME") else {
        return path.clone();
    };
    if path == "~" {
        return home.into();
    }
    match path.strip_prefix("~/") {
        Some(rest) => JsString::from(home) + "/" + rest,
        None => path.clone(),
    }
}

fn normalize(path: &JsString) -> JsString {
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part == ".." {
            parts.pop();
        } else if !part.is_empty() && part != "." {
            parts.push(part);
        }
    }
    let joined = JsString::join(parts, "/");
    if path.starts_with('/') {
        JsString::from("/") + joined
    } else {
        joined
    }
}

fn parent_path(path: &JsString) -> Option<JsString> {
    let index = path.units().iter().rposition(|unit| *unit == 0x2f)?;
    (path != "/").then(|| path.slice(0, index.max(1)))
}

fn file_kind(metadata: &fs::Metadata, path: &JsString) -> Result<FileKind, FileError> {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Ok(FileKind::Symlink);
    }
    if file_type.is_dir() {
        return Ok(FileKind::Directory);
    }
    if file_type.is_file() {
        return Ok(FileKind::File);
    }
    Err(FileError::new(
        FileErrorCode::Invalid,
        "Unsupported file type",
        Some(path.clone()),
        None,
    ))
}

fn file_info_from_path(path: &JsString) -> Result<FileInfo, FileError> {
    let metadata = fs::symlink_metadata(encode_node_utf8(path)).map_err(|error| file_error(&error, path))?;
    let kind = file_kind(&metadata, path)?;
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    Ok(FileInfo {
        name: path.split('/').last().unwrap_or_default(),
        path: path.clone(),
        kind,
        size: metadata.len() as f64,
        mtime_ms,
    })
}

fn file_error(error: &std::io::Error, path: &JsString) -> FileError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => FileErrorCode::NotFound,
        std::io::ErrorKind::PermissionDenied => FileErrorCode::PermissionDenied,
        std::io::ErrorKind::NotADirectory => FileErrorCode::NotDirectory,
        std::io::ErrorKind::IsADirectory => FileErrorCode::IsDirectory,
        std::io::ErrorKind::InvalidInput => FileErrorCode::Invalid,
        std::io::ErrorKind::Unsupported => FileErrorCode::NotSupported,
        _ => FileErrorCode::Unknown,
    };
    FileError::new(code, error.to_string(), Some(path.clone()), None)
}

impl JsonlSessionRepoFileSystem for NativeFileSystem {
    fn absolute_path<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move { Ok(self.resolve(path)) })
    }

    fn join_path<'a>(&'a self, parts: &'a [JsString]) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move {
            let mut joined = JsString::default();
            for part in parts {
                if part.starts_with('/') {
                    joined = part.clone();
                } else if !part.is_empty() {
                    if !joined.is_empty() {
                        joined.push_str("/");
                    }
                    joined.push_str(part);
                }
            }
            Ok(normalize(&joined))
        })
    }

    fn read_text_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let bytes = fs::read(&encode_node_utf8(&resolved)).map_err(|error| file_error(&error, &resolved))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })
    }

    fn read_text_lines<'a>(
        &'a self,
        path: &'a JsString,
        max_lines: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<String>, FileError>> {
        Box::pin(async move {
            if max_lines == Some(0) {
                return Ok(Vec::new());
            }
            let content = self.read_text_file(path).await?;
            let mut lines: Vec<String> = content
                .split('\n')
                .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
                .collect();
            if content.ends_with('\n') {
                lines.pop();
            }
            if let Some(max_lines) = max_lines {
                lines.truncate(max_lines);
            }
            Ok(lines)
        })
    }

    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            if let Some(parent) = parent_path(&resolved) {
                fs::create_dir_all(encode_node_utf8(&parent)).map_err(|error| file_error(&error, &parent))?;
            }
            fs::write(&encode_node_utf8(&resolved), content).map_err(|error| file_error(&error, &resolved))
        })
    }

    fn append_file<'a>(&'a self, path: &'a JsString, content: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            if let Some(parent) = parent_path(&resolved) {
                fs::create_dir_all(encode_node_utf8(&parent)).map_err(|error| file_error(&error, &parent))?;
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(encode_node_utf8(&resolved))
                .map_err(|error| file_error(&error, &resolved))?;
            file.write_all(content.as_bytes())
                .map_err(|error| file_error(&error, &resolved))
        })
    }

    fn rename_file<'a>(
        &'a self,
        source: &'a JsString,
        destination: &'a JsString,
    ) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let source = self.resolve(source);
            let destination = self.resolve(destination);
            fs::rename(encode_node_utf8(&source), encode_node_utf8(&destination))
                .map_err(|error| file_error(&error, &source))
        })
    }

    fn file_info<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<FileInfo, FileError>> {
        Box::pin(async move { self.info(path) })
    }

    fn list_dir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let entries = fs::read_dir(&encode_node_utf8(&resolved)).map_err(|error| file_error(&error, &resolved))?;
            let mut infos = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|error| file_error(&error, &resolved))?;
                let entry_path =
                    normalize(&(resolved.clone() + "/" + entry.file_name().to_string_lossy().into_owned()));
                let info = file_info_from_path(&entry_path)?;
                infos.push(info);
            }
            Ok(infos)
        })
    }

    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<bool, FileError>> {
        Box::pin(async move {
            match self.info(path) {
                Ok(_) => Ok(true),
                Err(error) if error.code == FileErrorCode::NotFound => Ok(false),
                Err(error) => Err(error),
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a JsString, recursive: bool) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let result = if recursive {
                fs::create_dir_all(&encode_node_utf8(&resolved))
            } else {
                fs::create_dir(&encode_node_utf8(&resolved))
            };
            result.map_err(|error| file_error(&error, &resolved))
        })
    }

    fn remove<'a>(&'a self, path: &'a JsString, recursive: bool, force: bool) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let result = if recursive {
                fs::remove_dir_all(&encode_node_utf8(&resolved))
            } else {
                fs::remove_file(&encode_node_utf8(&resolved))
            };
            match result {
                Ok(()) => Ok(()),
                Err(error) if force && error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(file_error(&error, &resolved)),
            }
        })
    }
}

/// JSONL metadata accepts arbitrary JSON object metadata.
pub type JsonlMetadataValue = JsonValue;
