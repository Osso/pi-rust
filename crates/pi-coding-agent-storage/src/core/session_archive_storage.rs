//! Frozen `session-archive-storage.ts`: file publication and atomic metadata relocation.

mod idle;
pub use idle::{
    IdleSessionArchiveOptions, IdleSessionArchiveResult, archive_idle_session_transcripts, run_idle_session_archive,
};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use pi_js_compat::js_json::{JsString, encode_node_utf8};

use super::session_control_db::{read_session_metadata, relocate_and_archive_session, relocate_and_unarchive_session};
use super::session_manager::{SessionManagerError, control_db_error};
use super::session_transcript::{
    decode_session_zstd_bytes, read_session_transcript_path, session_transcript_read_error,
};

pub const ARCHIVED_SESSION_SUFFIX: &str = ".zst";
pub fn is_archived_session_file(path: &JsString) -> bool {
    path.ends_with(ARCHIVED_SESSION_SUFFIX)
}
fn error(cause: impl std::fmt::Display) -> SessionManagerError {
    SessionManagerError::new(cause.to_string())
}
fn archive_io_error(cause: io::Error) -> SessionManagerError {
    if let Some(native) = cause
        .get_ref()
        .and_then(|source| source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>())
    {
        return SessionManagerError::from(Box::new(native.clone()));
    }
    error(cause)
}
fn exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}
fn with_archive_suffix(path: &JsString) -> JsString {
    path.clone() + ARCHIVED_SESSION_SUFFIX
}
fn without_archive_suffix(path: &JsString) -> JsString {
    path.slice(0, path.len() - ARCHIVED_SESSION_SUFFIX.len())
}

pub fn archive_persisted_session(db: &JsString, path: &JsString) -> Result<JsString, SessionManagerError> {
    let archived = archive_session_file(path)?;
    if let Err(cause) = relocate_and_archive_session(db, path, &archived) {
        restore_archived_session_file(&archived)?;
        return Err(control_db_error(cause));
    }
    Ok(archived)
}

pub fn restore_archived_session(db: &JsString, path: &JsString) -> Result<JsString, SessionManagerError> {
    let path = read_session_transcript_path(path);
    let archived = if is_archived_session_file(&path) {
        path.clone()
    } else {
        with_archive_suffix(&path)
    };
    let restored = without_archive_suffix(&archived);
    if exists(&restored) {
        if read_session_metadata(db, &archived)
            .map_err(control_db_error)?
            .is_some()
        {
            relocate_and_unarchive_session(db, &archived, &restored).map_err(control_db_error)?;
        }
        remove_if_exists(&encode_node_utf8(&archived)).map_err(error)?;
        return Ok(restored);
    }
    if !is_archived_session_file(&path) {
        return Ok(path);
    }
    if !exists(&path) && read_session_metadata(db, &path).map_err(control_db_error)?.is_none() {
        return Ok(restored);
    }
    restore_archived_session_file(&path)?;
    if let Err(cause) = relocate_and_unarchive_session(db, &path, &restored) {
        archive_session_file(&restored)?;
        return Err(control_db_error(cause));
    }
    Ok(restored)
}

pub fn archive_session_file(path: &JsString) -> Result<JsString, SessionManagerError> {
    if is_archived_session_file(path) {
        return Ok(path.clone());
    }
    let archived = with_archive_suffix(path);
    if !exists(path) {
        return Err(SessionManagerError::new(
            JsString::from("Session file does not exist: ") + path,
        ));
    }
    if exists(&archived) {
        return Err(SessionManagerError::new(
            JsString::from("Archived session file already exists: ") + &archived,
        ));
    }
    let original = read_archive_file_bytes(path)?;
    let compressed = zstd::stream::encode_all(original.as_slice(), 0).map_err(error)?;
    if decode_session_zstd_bytes(compressed.as_slice()).map_err(error)? != original {
        return Err(SessionManagerError::new(
            JsString::from("Compressed session validation failed: ") + path,
        ));
    }
    replace_session_file(&encode_node_utf8(path), &encode_node_utf8(&archived), &compressed)
        .map_err(archive_io_error)?;
    Ok(archived)
}

pub fn restore_archived_session_file(path: &JsString) -> Result<JsString, SessionManagerError> {
    if !is_archived_session_file(path) {
        return Ok(path.clone());
    }
    let restored = without_archive_suffix(path);
    if !exists(path) {
        if exists(&restored) {
            return Ok(restored);
        }
        return Err(SessionManagerError::new(
            JsString::from("Archived session file does not exist: ") + path,
        ));
    }
    if exists(&restored) {
        return Err(SessionManagerError::new(
            JsString::from("Restored session file already exists: ") + &restored,
        ));
    }
    let content = decode_session_zstd_bytes(&read_archive_file_bytes(path)?)
        .map_err(|cause| SessionManagerError::from(session_transcript_read_error(path, cause)))?;
    replace_session_file(&encode_node_utf8(path), &encode_node_utf8(&restored), &content).map_err(archive_io_error)?;
    Ok(restored)
}

fn read_archive_file_bytes(path: &JsString) -> Result<Vec<u8>, SessionManagerError> {
    fs::read(encode_node_utf8(path)).map_err(|cause| {
        let record = pi_js_compat::node_fs::node_read_error_record(
            &cause,
            path,
            pi_js_compat::node_fs::NodeReadApi::ReadFileSync,
        );
        SessionManagerError::from(Box::new(pi_js_compat::node_error::NativeNodeError::from(record)))
    })
}

/// Filesystem boundary: paths arrive already encoded as Node would pass them to libuv.
fn remove_if_exists(path: &str) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn replace_session_file(source: &str, target: &str, content: &[u8]) -> io::Result<()> {
    let metadata = fs::metadata(source)?;
    let temporary = format!(
        "{target}.archive-temp-{}",
        pi_ai::utils::random_uuid::random_uuid().map_err(io::Error::other)?
    );
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            options.mode(metadata.mode() & 0o777);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            fs::set_permissions(&temporary, fs::Permissions::from_mode(metadata.mode() & 0o777))?;
        }
        fs::rename(&temporary, target)?;
        fs::remove_file(source)
    })();
    if let Err(cause) = result {
        remove_if_exists(&temporary)?;
        if Path::new(source).exists() && Path::new(target).exists() {
            remove_if_exists(target)?;
        }
        return Err(cause);
    }
    Ok(())
}
