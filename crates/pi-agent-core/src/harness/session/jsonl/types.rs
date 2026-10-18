//! Port of `packages/agent/src/harness/session/jsonl/types.ts` plus the
//! filesystem capability subset required by the JSONL backend.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use pi_ai::js_number;
use pi_ai::request::BoxFuture;
use pi_ai::types::JsonObject;
use serde::{Deserialize, Serialize};

use crate::harness::session::types::{JsonValue, SessionCreateOptions};
use crate::harness::types::{FileError, FileErrorCode, FileInfo, FileKind};

/// Filesystem operations used by the JSONL session backend.
pub trait JsonlSessionRepoFileSystem: Send + Sync {
    fn absolute_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>>;
    fn join_path<'a>(&'a self, parts: &'a [String]) -> BoxFuture<'a, Result<String, FileError>>;
    fn read_text_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>>;
    fn read_text_lines<'a>(
        &'a self,
        path: &'a str,
        max_lines: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<String>, FileError>>;
    fn write_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, Result<(), FileError>>;
    fn append_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, Result<(), FileError>>;
    fn rename_file<'a>(&'a self, source: &'a str, destination: &'a str) -> BoxFuture<'a, Result<(), FileError>>;
    fn file_info<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<FileInfo, FileError>>;
    fn list_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>>;
    fn exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<bool, FileError>>;
    fn create_dir<'a>(&'a self, path: &'a str, recursive: bool) -> BoxFuture<'a, Result<(), FileError>>;
    fn remove<'a>(&'a self, path: &'a str, recursive: bool, force: bool) -> BoxFuture<'a, Result<(), FileError>>;
}

/// JSONL repository options.
#[derive(Clone)]
pub struct JsonlSessionRepoOptions {
    pub fs: std::sync::Arc<dyn JsonlSessionRepoFileSystem>,
    pub sessions_root: String,
}

/// Session metadata projected from a JSONL header plus filesystem state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonlSessionMetadata {
    pub id: String,
    pub created_at: i64,
    pub cwd: String,
    pub path: String,
    #[serde(serialize_with = "js_number::serialize")]
    pub modified_at: f64,
    pub source_format: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_parent_session_path: Option<String>,
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
    pub cwd: String,
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
    pub cwd: Option<String>,
}

/// Version 4 session header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonlV4Header {
    pub kind: HeaderKind,
    pub version: u8,
    pub id: String,
    pub created_at: i64,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_parent_session_path: Option<String>,
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
    cwd: PathBuf,
}

impl NativeFileSystem {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { cwd: cwd.into() }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let expanded = expand_home(path);
        let candidate = Path::new(&expanded);
        if candidate.is_absolute() {
            normalize(candidate)
        } else {
            normalize(&self.cwd.join(candidate))
        }
    }

    fn info(&self, path: &str) -> Result<FileInfo, FileError> {
        let resolved = self.resolve(path);
        file_info_from_path(&resolved)
    }
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_owned());
    }
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var("HOME") {
            Ok(home) => format!("{home}/{rest}"),
            Err(_) => path.to_owned(),
        },
        None => path.to_owned(),
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_owned()),
        }
    }
    parts.iter().collect()
}

fn file_info_from_path(path: &Path) -> Result<FileInfo, FileError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| file_error(&error, path))?;
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        FileKind::Symlink
    } else if file_type.is_dir() {
        FileKind::Directory
    } else if file_type.is_file() {
        FileKind::File
    } else {
        return Err(FileError::new(
            FileErrorCode::Invalid,
            "Unsupported file type",
            Some(path.to_string_lossy().into_owned()),
            None,
        ));
    };
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    Ok(FileInfo {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        kind,
        size: metadata.len() as f64,
        mtime_ms,
    })
}

fn file_error(error: &std::io::Error, path: &Path) -> FileError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => FileErrorCode::NotFound,
        std::io::ErrorKind::PermissionDenied => FileErrorCode::PermissionDenied,
        std::io::ErrorKind::NotADirectory => FileErrorCode::NotDirectory,
        std::io::ErrorKind::IsADirectory => FileErrorCode::IsDirectory,
        std::io::ErrorKind::InvalidInput => FileErrorCode::Invalid,
        std::io::ErrorKind::Unsupported => FileErrorCode::NotSupported,
        _ => FileErrorCode::Unknown,
    };
    FileError::new(code, error.to_string(), Some(path.to_string_lossy().into_owned()), None)
}

impl JsonlSessionRepoFileSystem for NativeFileSystem {
    fn absolute_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move { Ok(self.resolve(path).to_string_lossy().into_owned()) })
    }

    fn join_path<'a>(&'a self, parts: &'a [String]) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let mut joined = PathBuf::new();
            for part in parts {
                joined.push(part);
            }
            Ok(normalize(&joined).to_string_lossy().into_owned())
        })
    }

    fn read_text_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let bytes = fs::read(&resolved).map_err(|error| file_error(&error, &resolved))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })
    }

    fn read_text_lines<'a>(
        &'a self,
        path: &'a str,
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

    fn write_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            if let Some(parent) = resolved.parent() {
                fs::create_dir_all(parent).map_err(|error| file_error(&error, parent))?;
            }
            fs::write(&resolved, content).map_err(|error| file_error(&error, &resolved))
        })
    }

    fn append_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            if let Some(parent) = resolved.parent() {
                fs::create_dir_all(parent).map_err(|error| file_error(&error, parent))?;
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&resolved)
                .map_err(|error| file_error(&error, &resolved))?;
            file.write_all(content.as_bytes())
                .map_err(|error| file_error(&error, &resolved))
        })
    }

    fn rename_file<'a>(&'a self, source: &'a str, destination: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let source = self.resolve(source);
            let destination = self.resolve(destination);
            fs::rename(&source, &destination).map_err(|error| file_error(&error, &source))
        })
    }

    fn file_info<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<FileInfo, FileError>> {
        Box::pin(async move { self.info(path) })
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let entries = fs::read_dir(&resolved).map_err(|error| file_error(&error, &resolved))?;
            let mut infos = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|error| file_error(&error, &resolved))?;
                let entry_path = resolved.join(entry.file_name());
                let info = file_info_from_path(&entry_path)?;
                infos.push(info);
            }
            Ok(infos)
        })
    }

    fn exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<bool, FileError>> {
        Box::pin(async move {
            match self.info(path) {
                Ok(_) => Ok(true),
                Err(error) if error.code == FileErrorCode::NotFound => Ok(false),
                Err(error) => Err(error),
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a str, recursive: bool) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let result = if recursive {
                fs::create_dir_all(&resolved)
            } else {
                fs::create_dir(&resolved)
            };
            result.map_err(|error| file_error(&error, &resolved))
        })
    }

    fn remove<'a>(&'a self, path: &'a str, recursive: bool, force: bool) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = self.resolve(path);
            let result = if recursive {
                fs::remove_dir_all(&resolved)
            } else {
                fs::remove_file(&resolved)
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
