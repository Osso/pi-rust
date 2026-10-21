//! Frozen `core/resident-session.ts`: resident discovery and fresh-file preparation.

use std::fmt;
use std::fs;
use std::path::Path;

use pi_js_compat::node_fs::{NodeFsError, NodePathArgumentError, node_fs_error_record, node_path_argument_error};
use serde::Serialize;

use super::runtime_process::is_pi_runtime_process_alive;
use super::session_control_db::{
    ControlDbError, RuntimeMailboxAddress, SessionMetadata, list_session_metadata, read_runtime_mailbox_listener,
    remove_session_metadata,
};
use super::session_manager::SessionInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentRole {
    Architect,
    Supervisor,
}

impl ResidentRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Architect => "architect",
            Self::Supervisor => "supervisor",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedResidentSessionFile {
    pub session_dir: String,
    pub session_path: String,
}

#[derive(Debug)]
pub enum ResidentSessionError {
    PathArgument(NodePathArgumentError),
    Filesystem(NodeFsError),
    ControlDb(ControlDbError),
}

impl fmt::Display for ResidentSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathArgument(error) => formatter.write_str(&error.message.to_utf8_lossy()),
            Self::Filesystem(error) => formatter.write_str(&error.message),
            Self::ControlDb(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ResidentSessionError {}

impl From<ControlDbError> for ResidentSessionError {
    fn from(error: ControlDbError) -> Self {
        Self::ControlDb(error)
    }
}

pub fn is_resident_session(session: &SessionInfo) -> bool {
    matches!(session.id.as_str(), "supervisor" | "architect")
        || session
            .path
            .split(['/', '\\'])
            .any(|segment| matches!(segment, "supervisor-sessions" | "architect-sessions"))
}

/// Select a resident file without creating its contents, preserving any live listener.
pub fn prepare_resident_session_file(
    agent_dir: &str,
    role: ResidentRole,
    control_db_path: &str,
) -> Result<PreparedResidentSessionFile, ResidentSessionError> {
    let session_dir = join_path(agent_dir, &format!("{}-sessions", role.as_str()));
    if let Some(error) = node_path_argument_error(&session_dir) {
        return Err(ResidentSessionError::PathArgument(error));
    }
    fs::create_dir_all(&session_dir).map_err(|error| filesystem_error(error, "mkdir", &session_dir))?;
    let canonical_file = format!("{}.jsonl", role.as_str());
    let suffix = format!("_{canonical_file}");
    let mut matching_files = matching_session_files(&session_dir, &canonical_file, &suffix)?;
    let live_session_path = find_live_resident_session_path(control_db_path, role, &session_dir)?;
    let live_file = live_session_path.as_deref().and_then(|path| path.rsplit('/').next());
    if let Some(file) = live_file
        && !matching_files.iter().any(|existing| existing == file)
    {
        matching_files.push(file.to_owned());
    }
    let latest_file = matching_files
        .iter()
        .filter(|file| file.ends_with(&suffix))
        .max_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
    let retained_file = live_file
        .or_else(|| latest_file.map(String::as_str))
        .unwrap_or(&canonical_file);
    for file in &matching_files {
        if file == retained_file {
            continue;
        }
        let stale_path = join_path(&session_dir, file);
        remove_session_metadata(control_db_path, &stale_path)?;
        fs::remove_file(&stale_path).map_err(|error| filesystem_error(error, "unlink", &stale_path))?;
    }
    for metadata in list_session_metadata(control_db_path)? {
        if !is_resident_session_for_role(&metadata, role)
            || Some(metadata.session_path.as_str()) == live_session_path.as_deref()
        {
            continue;
        }
        // Node existsSync follows symlinks and returns false for stat failures.
        if !Path::new(&metadata.session_path).exists() {
            remove_session_metadata(control_db_path, &metadata.session_path)?;
        }
    }
    Ok(PreparedResidentSessionFile {
        session_path: join_path(&session_dir, retained_file),
        session_dir,
    })
}

fn matching_session_files(
    session_dir: &str,
    canonical_file: &str,
    suffix: &str,
) -> Result<Vec<String>, ResidentSessionError> {
    let entries = fs::read_dir(session_dir).map_err(|error| filesystem_error(error, "scandir", session_dir))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| filesystem_error(error, "scandir", session_dir))?;
    // libuv scandir returns byte-ordered entries; latest-file selection below is JS UTF-16 ordered.
    entries.sort_by_key(|entry| entry.file_name());
    let mut matching = Vec::new();
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| filesystem_error(error, "lstat", &entry.path().to_string_lossy()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_file() && (name == canonical_file || name.ends_with(suffix)) {
            matching.push(name);
        }
    }
    Ok(matching)
}

fn find_live_resident_session_path(
    control_db_path: &str,
    role: ResidentRole,
    session_dir: &str,
) -> Result<Option<String>, ResidentSessionError> {
    let listener = read_runtime_mailbox_listener(
        control_db_path,
        &RuntimeMailboxAddress {
            agent_id: None,
            session_id: role.as_str().to_owned(),
        },
    )?;
    let Some(listener) = listener else {
        return Ok(None);
    };
    let Some(path) = listener.session_path.filter(|path| !path.is_empty()) else {
        return Ok(None);
    };
    if !is_pi_runtime_process_alive(listener.pid) {
        return Ok(None);
    }
    let session_path = resolve_path(&path)?;
    let directory = session_path
        .rsplit_once('/')
        .map(|(parent, _)| if parent.is_empty() { "/" } else { parent });
    Ok(
        (directory == Some(resolve_path(session_dir)?.as_str()) && session_path.ends_with(".jsonl"))
            .then_some(session_path),
    )
}

fn is_resident_session_for_role(session: &SessionMetadata, role: ResidentRole) -> bool {
    session.id == role.as_str()
        || session
            .session_path
            .split(['/', '\\'])
            .any(|segment| segment == format!("{}-sessions", role.as_str()))
}

fn filesystem_error(error: std::io::Error, syscall: &str, path: &str) -> ResidentSessionError {
    ResidentSessionError::Filesystem(node_fs_error_record(&error, syscall, path))
}

// POSIX Node path.join/resolve are lexical: no realpath, tilde expansion, or dropped leading '..'.
// utils::paths::node_join drops relative parent components and node_resolve does not anchor to cwd.
fn join_path(base: &str, tail: &str) -> String {
    if base.is_empty() {
        normalize_path(tail)
    } else {
        normalize_path(&format!("{base}/{tail}"))
    }
}

fn resolve_path(path: &str) -> Result<String, ResidentSessionError> {
    if path.starts_with('/') {
        return Ok(normalize_path(path));
    }
    let cwd = std::env::current_dir().map_err(|error| filesystem_error(error, "uv_cwd", ""))?;
    Ok(join_path(&cwd.to_string_lossy(), path))
}

fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|last| *last != "..") => {
                segments.pop();
            }
            ".." if absolute => {}
            _ => segments.push(segment),
        }
    }
    let normalized = segments.join("/");
    if absolute {
        format!("/{normalized}")
    } else if normalized.is_empty() {
        ".".to_owned()
    } else {
        normalized
    }
}
