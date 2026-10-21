//! Frozen `session-archive-storage.ts`: file publication and atomic metadata relocation.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use super::session_control_db::{read_session_metadata, relocate_and_archive_session, relocate_and_unarchive_session};
use super::session_manager::SessionManagerError;
use super::session_transcript::read_session_transcript_path;

pub const ARCHIVED_SESSION_SUFFIX: &str = ".zst";
pub fn is_archived_session_file(path: &str) -> bool {
    path.ends_with(ARCHIVED_SESSION_SUFFIX)
}
fn error(cause: impl std::fmt::Display) -> SessionManagerError {
    SessionManagerError::new(cause.to_string())
}

pub fn archive_persisted_session(db: &str, path: &str) -> Result<String, SessionManagerError> {
    let archived = archive_session_file(path)?;
    if let Err(cause) = relocate_and_archive_session(db, path, &archived) {
        restore_archived_session_file(&archived)?;
        return Err(error(cause));
    }
    Ok(archived)
}

pub fn restore_archived_session(db: &str, path: &str) -> Result<String, SessionManagerError> {
    let path = read_session_transcript_path(path);
    let archived = if is_archived_session_file(&path) {
        path.clone()
    } else {
        format!("{path}{ARCHIVED_SESSION_SUFFIX}")
    };
    let restored = archived[..archived.len() - ARCHIVED_SESSION_SUFFIX.len()].to_owned();
    if Path::new(&restored).exists() {
        if read_session_metadata(db, &archived).map_err(error)?.is_some() {
            relocate_and_unarchive_session(db, &archived, &restored).map_err(error)?;
        }
        remove_if_exists(&archived).map_err(error)?;
        return Ok(restored);
    }
    if !is_archived_session_file(&path) {
        return Ok(path);
    }
    if !Path::new(&path).exists() && read_session_metadata(db, &path).map_err(error)?.is_none() {
        return Ok(restored);
    }
    restore_archived_session_file(&path)?;
    if let Err(cause) = relocate_and_unarchive_session(db, &path, &restored) {
        archive_session_file(&restored)?;
        return Err(error(cause));
    }
    Ok(restored)
}

pub fn archive_session_file(path: &str) -> Result<String, SessionManagerError> {
    if is_archived_session_file(path) {
        return Ok(path.to_owned());
    }
    let archived = format!("{path}{ARCHIVED_SESSION_SUFFIX}");
    if !Path::new(path).exists() {
        return Err(error(format!("Session file does not exist: {path}")));
    }
    if Path::new(&archived).exists() {
        return Err(error(format!("Archived session file already exists: {archived}")));
    }
    let original = fs::read(path).map_err(error)?;
    let compressed = zstd::stream::encode_all(original.as_slice(), 0).map_err(error)?;
    if zstd::stream::decode_all(compressed.as_slice()).map_err(error)? != original {
        return Err(error(format!("Compressed session validation failed: {path}")));
    }
    replace_session_file(path, &archived, &compressed).map_err(error)?;
    Ok(archived)
}

pub fn restore_archived_session_file(path: &str) -> Result<String, SessionManagerError> {
    if !is_archived_session_file(path) {
        return Ok(path.to_owned());
    }
    let restored = path[..path.len() - ARCHIVED_SESSION_SUFFIX.len()].to_owned();
    if !Path::new(path).exists() {
        if Path::new(&restored).exists() {
            return Ok(restored);
        }
        return Err(error(format!("Archived session file does not exist: {path}")));
    }
    if Path::new(&restored).exists() {
        return Err(error(format!("Restored session file already exists: {restored}")));
    }
    let content = zstd::stream::decode_all(fs::read(path).map_err(error)?.as_slice()).map_err(error)?;
    replace_session_file(path, &restored, &content).map_err(error)?;
    Ok(restored)
}

fn remove_if_exists(path: &str) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn replace_session_file(source: &str, target: &str, content: &[u8]) -> io::Result<()> {
    let metadata = fs::metadata(source)?;
    let temporary = format!("{target}.archive-temp-{}", pi_ai::utils::uuid::random_uuid());
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
