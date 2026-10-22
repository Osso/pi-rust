//! Frozen `core/resident-session.ts`: resident discovery and fresh-file preparation.

use std::fmt;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_fs::{NodeFsError, NodePathArgumentError};
#[cfg(unix)]
use pi_js_compat::node_fs::{node_fs_error_record, node_path_argument_error};
use serde::Serialize;

#[cfg(unix)]
use super::runtime_process::is_pi_runtime_process_alive;
use super::session_control_db::ControlDbError;
#[cfg(unix)]
use super::session_control_db::{
    RuntimeMailboxAddress, SessionMetadata, list_session_metadata, read_runtime_mailbox_listener,
    remove_session_metadata,
};
use super::session_manager::SessionInfo;
use crate::utils::paths::PathError;
#[cfg(unix)]
use crate::utils::paths::{node_basename, node_dirname, node_join, node_resolve};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentRole {
    Architect,
    Supervisor,
}

#[cfg(unix)]
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
    pub session_dir: JsString,
    pub session_path: JsString,
}

#[derive(Debug)]
pub enum ResidentSessionError {
    PathArgument(NodePathArgumentError),
    Filesystem(NodeFsError),
    ControlDb(ControlDbError),
    Path(PathError),
}

impl ResidentSessionError {
    /// The thrown `Error.message`, keeping unmatched UTF-16 units.
    pub fn message(&self) -> JsString {
        match self {
            Self::PathArgument(error) => error.message.clone(),
            Self::Filesystem(error) => error.message.clone(),
            Self::ControlDb(error) => error.message().clone(),
            Self::Path(error) => error.message().clone(),
        }
    }
}

impl fmt::Display for ResidentSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for ResidentSessionError {}

impl From<ControlDbError> for ResidentSessionError {
    fn from(error: ControlDbError) -> Self {
        Self::ControlDb(error)
    }
}

/// `/(?:^|[\\/])name(?:[\\/]|$)/.test(path)`: some `/`- or `\\`-delimited segment equals `name`.
fn has_path_segment(path: &JsString, name: &str) -> bool {
    path.split('/')
        .flat_map(|part| part.split('\\').collect::<Vec<_>>())
        .any(|segment| segment == *name)
}

pub fn is_resident_session(session: &SessionInfo) -> bool {
    session.id == *"supervisor"
        || session.id == *"architect"
        || has_path_segment(&session.path, "supervisor-sessions")
        || has_path_segment(&session.path, "architect-sessions")
}

/// Select a resident file without creating its contents, preserving any live listener.
/// Unix-only preparation is an approved gap; the existing predicate remains cross-platform.
#[cfg(unix)]
pub fn prepare_resident_session_file(
    agent_dir: &JsString,
    role: ResidentRole,
    control_db_path: &JsString,
) -> Result<PreparedResidentSessionFile, ResidentSessionError> {
    let session_dir = node_join(agent_dir.clone(), js_format!("{}-sessions", role.as_str()));
    if let Some(error) = node_path_argument_error(&session_dir) {
        return Err(ResidentSessionError::PathArgument(error));
    }
    fs::create_dir_all(encode_node_utf8(&session_dir))
        .map_err(|error| filesystem_error(error, "mkdir", &session_dir))?;
    let canonical_file = js_format!("{}.jsonl", role.as_str());
    let suffix = JsString::from("_") + &canonical_file;
    let mut matching_files = matching_session_files(&session_dir, &canonical_file, &suffix)?;
    let live_session_path = find_live_resident_session_path(control_db_path, role, &session_dir)?;
    let live_file = live_session_path.as_ref().map(node_basename);
    if let Some(file) = &live_file
        && !matching_files.contains(file)
    {
        matching_files.push(file.clone());
    }
    let latest_file = matching_files
        .iter()
        .filter(|file| file.ends_with(suffix.clone()))
        .max_by(|left, right| left.units().cmp(right.units()))
        .cloned();
    let retained_file = live_file.or(latest_file).unwrap_or(canonical_file);
    for file in &matching_files {
        if *file == retained_file {
            continue;
        }
        let stale_path = node_join(session_dir.clone(), file.clone());
        remove_session_metadata(control_db_path, &stale_path)?;
        fs::remove_file(encode_node_utf8(&stale_path))
            .map_err(|error| filesystem_error(error, "unlink", &stale_path))?;
    }
    for metadata in list_session_metadata(control_db_path)? {
        if !is_resident_session_for_role(&metadata, role) || Some(&metadata.session_path) == live_session_path.as_ref()
        {
            continue;
        }
        // Node existsSync follows symlinks and returns false for stat failures.
        if !Path::new(&encode_node_utf8(&metadata.session_path)).exists() {
            remove_session_metadata(control_db_path, &metadata.session_path)?;
        }
    }
    Ok(PreparedResidentSessionFile {
        session_path: node_join(session_dir.clone(), retained_file),
        session_dir,
    })
}

/// `readdirSync(..., { withFileTypes: true })` names decode lossily, in libuv byte order.
#[cfg(unix)]
fn matching_session_files(
    session_dir: &JsString,
    canonical_file: &JsString,
    suffix: &JsString,
) -> Result<Vec<JsString>, ResidentSessionError> {
    let encoded_dir = encode_node_utf8(session_dir);
    let entries = fs::read_dir(&encoded_dir).map_err(|error| filesystem_error(error, "scandir", session_dir))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| filesystem_error(error, "scandir", session_dir))?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut matching = Vec::new();
    for entry in entries {
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        let file_type = entry
            .file_type()
            .map_err(|error| filesystem_error(error, "lstat", &node_join(session_dir.clone(), name.clone())))?;
        if file_type.is_file() && (name == *canonical_file || name.ends_with(suffix.clone())) {
            matching.push(name);
        }
    }
    Ok(matching)
}

#[cfg(unix)]
fn find_live_resident_session_path(
    control_db_path: &JsString,
    role: ResidentRole,
    session_dir: &JsString,
) -> Result<Option<JsString>, ResidentSessionError> {
    let listener = read_runtime_mailbox_listener(
        control_db_path,
        &RuntimeMailboxAddress {
            agent_id: None,
            session_id: role.as_str().into(),
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
    let session_path = resolve_path(&JsString::from(path))?;
    Ok(
        (node_dirname(session_path.clone()) == resolve_path(session_dir)? && session_path.ends_with(".jsonl"))
            .then_some(session_path),
    )
}

#[cfg(unix)]
fn is_resident_session_for_role(session: &SessionMetadata, role: ResidentRole) -> bool {
    session.id == *role.as_str() || has_path_segment(&session.session_path, &format!("{}-sessions", role.as_str()))
}

#[cfg(unix)]
fn filesystem_error(error: std::io::Error, syscall: &str, path: &JsString) -> ResidentSessionError {
    ResidentSessionError::Filesystem(node_fs_error_record(&error, syscall, path))
}

/// `path.resolve`; a failing `process.cwd()` propagates.
#[cfg(unix)]
fn resolve_path(path: &JsString) -> Result<JsString, ResidentSessionError> {
    node_resolve("", path.clone()).map_err(ResidentSessionError::Path)
}
