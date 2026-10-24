//! Frozen `idle-session-archive.ts`, within the existing archive-storage authority.

use std::collections::HashSet;
use std::fs::{self, File, FileTimes, Metadata, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, SystemTime};

use pi_ai::request::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use rusqlite::Connection;

use crate::core::detached_job_cleanup::read_linux_process_references;
use crate::core::resident_session::is_resident_session;
use crate::core::session_control_db::{
    WritableSessionMetadata, list_runtime_mailbox_listeners, read_session_metadata, relocate_and_archive_session,
    relocate_and_unarchive_session, write_session_metadata,
};
use crate::core::session_manager::build_session_info;
use crate::core::sqlite::{
    ControlDbError, DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS, configure_read_only_sqlite_database,
    create_read_only_sqlite_database,
};
use crate::utils::paths::{node_join, node_resolve};

const IDLE_AGE_MS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 1000.0;
const COMPARE_BUFFER_SIZE: usize = 64 * 1024;
const ACTIVE_AGENT_TRANSCRIPTS_SQL: &str = "
 SELECT session_path AS path FROM multi_agent_agents
 WHERE NOT json_valid(data) OR COALESCE(json_extract(data, '$.lifecycle'), '') NOT IN ('completed', 'failed', 'aborted')
 UNION SELECT json_extract(data, '$.transcript.path') AS path FROM multi_agent_agents
 WHERE json_valid(data) AND COALESCE(json_extract(data, '$.lifecycle'), '') NOT IN ('completed', 'failed', 'aborted')
 UNION SELECT session_path AS path FROM multi_agent_runtime_owners WHERE process_identity IS NOT NULL
";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdleSessionArchiveResult {
    pub archived_paths: Vec<JsString>,
    pub errors: Vec<JsString>,
}

#[derive(Debug, Clone, Default)]
pub struct IdleSessionArchiveOptions {
    pub agent_dir: Option<JsString>,
    pub now: Option<f64>,
    pub current_session_path: Option<JsString>,
}

/// Runs blocking compression/IO off the caller's async executor thread.
pub async fn archive_idle_session_transcripts(
    db: &JsString,
    options: IdleSessionArchiveOptions,
) -> Result<IdleSessionArchiveResult, CallbackError> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || archive_pass(&db, &options))
        .await
        .map_err(|error| CallbackError::new(error.to_string()))?
}

/// Frozen fire-and-forget wrapper; runtime creation does not await archive completion.
pub fn run_idle_session_archive(db: &JsString, agent_dir: &JsString, current: Option<&JsString>) {
    let db = db.clone();
    let options = IdleSessionArchiveOptions {
        agent_dir: Some(agent_dir.clone()),
        current_session_path: current.cloned(),
        now: None,
    };
    tokio::spawn(async move {
        match archive_idle_session_transcripts(&db, options).await {
            Ok(result) => {
                for error in result.errors {
                    eprintln!("Idle session archive: {}", encode_node_utf8(&error));
                }
            }
            Err(error) => eprintln!(
                "Idle session archive failed: {}",
                encode_node_utf8(&crate::utils::uncaught::format_uncaught(&error))
            ),
        }
    });
}

fn archive_pass(db: &JsString, options: &IdleSessionArchiveOptions) -> Result<IdleSessionArchiveResult, CallbackError> {
    let mut result = IdleSessionArchiveResult::default();
    let agent_dir = match &options.agent_dir {
        Some(path) => path.clone(),
        None => crate::config::get_agent_dir().map_err(CallbackError::from)?,
    };
    let root = node_join(agent_dir, "sessions");
    if !exists(&root) {
        return Ok(result);
    }
    let Some(references) = read_linux_process_references().map_err(CallbackError::from)? else {
        return Ok(result);
    };
    let mut protected = read_protected_paths(db)?;
    if let Some(current) = options.current_session_path.as_ref().filter(|path| !path.is_empty()) {
        protected.insert(node_resolve("", current).map_err(CallbackError::from)?);
    }
    let cutoff = options.now.unwrap_or_else(pi_js_compat::date::js_now_millis) - IDLE_AGE_MS;
    for path in list_project_transcripts(&root)? {
        let outcome = (|| {
            if references.contains(&realpath(&path)?)
                || protected.contains(&node_resolve("", &path).map_err(CallbackError::from)?)
            {
                return Ok(false);
            }
            archive_transcript(db, &path, cutoff)
        })();
        match outcome {
            Ok(true) => result.archived_paths.push(path + ".zst"),
            Ok(false) => {}
            Err(error) => result
                .errors
                .push(pi_js_compat::js_format!("{}: {}", path, error.message)),
        }
    }
    Ok(result)
}

fn node_io(error: std::io::Error, syscall: &str, path: &JsString) -> CallbackError {
    NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(&error, syscall, path)).into()
}

fn exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

fn realpath(path: &JsString) -> Result<JsString, CallbackError> {
    fs::canonicalize(encode_node_utf8(path))
        .map(|path| pi_js_compat::node_fs::decode_node_path(path.as_os_str()))
        .map_err(|error| node_io(error, "realpath", path))
}

fn list_project_transcripts(root: &JsString) -> Result<Vec<JsString>, CallbackError> {
    let directories = read_directory_entries(root)?;
    let mut paths = Vec::new();
    for directory in directories {
        if !directory
            .file_type()
            .map_err(|error| node_io(error, "scandir", root))?
            .is_dir()
        {
            continue;
        }
        let dir = node_join(
            root,
            pi_js_compat::node_fs::decode_node_path(directory.file_name().as_os_str()),
        );
        for file in read_directory_entries(&dir)? {
            let name = pi_js_compat::node_fs::decode_node_path(file.file_name().as_os_str());
            if file
                .file_type()
                .map_err(|error| node_io(error, "scandir", &dir))?
                .is_file()
                && name.ends_with(".jsonl")
            {
                paths.push(node_join(&dir, name));
            }
        }
    }
    Ok(paths)
}

fn read_directory_entries(path: &JsString) -> Result<Vec<fs::DirEntry>, CallbackError> {
    let entries = fs::read_dir(encode_node_utf8(path)).map_err(|error| node_io(error, "scandir", path))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| node_io(error, "scandir", path))?;
    // Keep deterministic native filename-byte order before the Node UTF-8 decode boundary.
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

fn is_process_present(pid: u32) -> bool {
    let Some(pid) = i32::try_from(pid).ok().and_then(rustix::process::Pid::from_raw) else {
        return true;
    };
    !matches!(rustix::process::test_kill_process(pid), Err(rustix::io::Errno::SRCH))
}

fn read_protected_paths(db: &JsString) -> Result<HashSet<JsString>, CallbackError> {
    let mut paths = HashSet::new();
    for listener in list_runtime_mailbox_listeners(db).map_err(CallbackError::from)? {
        if let Some(path) = listener.session_path.filter(|path| !path.is_empty())
            && is_process_present(listener.pid)
        {
            paths.insert(node_resolve("", JsString::from(path)).map_err(CallbackError::from)?);
        }
    }
    let connection = create_read_only_sqlite_database(&encode_node_utf8(db)).map_err(CallbackError::from)?;
    configure_read_only_sqlite_database(&connection, DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS)
        .map_err(CallbackError::from)?;
    for path in read_path_rows(&connection, ACTIVE_AGENT_TRANSCRIPTS_SQL)? {
        paths.insert(node_resolve("", path).map_err(CallbackError::from)?);
    }
    let mut statement = connection.prepare("SELECT metadata.session_path AS path, health.pid FROM session_metadata AS metadata JOIN session_health AS health ON health.session_id = metadata.id WHERE health.pid IS NOT NULL").map_err(ControlDbError::from).map_err(CallbackError::from)?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, JsString>(0)?, row.get::<_, u32>(1)?)))
        .map_err(ControlDbError::from)
        .map_err(CallbackError::from)?;
    for row in rows {
        let (path, pid) = row.map_err(ControlDbError::from).map_err(CallbackError::from)?;
        if is_process_present(pid) {
            paths.insert(node_resolve("", path).map_err(CallbackError::from)?);
        }
    }
    Ok(paths)
}

fn read_path_rows(connection: &Connection, sql: &str) -> Result<Vec<JsString>, CallbackError> {
    let mut statement = connection
        .prepare(sql)
        .map_err(ControlDbError::from)
        .map_err(CallbackError::from)?;
    let rows = statement
        .query_map([], |row| row.get::<_, Option<JsString>>(0))
        .map_err(ControlDbError::from)
        .map_err(CallbackError::from)?;
    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(ControlDbError::from)
        .map_err(CallbackError::from)?;
    Ok(rows.into_iter().flatten().filter(|path| !path.is_empty()).collect())
}

fn archive_transcript(db: &JsString, path: &JsString, cutoff: f64) -> Result<bool, CallbackError> {
    let before = fs::symlink_metadata(encode_node_utf8(path)).map_err(|error| node_io(error, "lstat", path))?;
    if !before.is_file() {
        return Ok(false);
    }
    let archive = path.clone() + ".zst";
    let metadata = match read_session_metadata(db, path).map_err(CallbackError::from)? {
        Some(metadata) => Some(metadata),
        None => read_session_metadata(db, &archive).map_err(CallbackError::from)?,
    };
    if metadata.as_ref().is_some_and(|metadata| resident(&metadata.id, path)) {
        return Ok(false);
    }
    if exists(&archive) {
        return recover_published_archive(db, path, &before);
    }
    if mtime_ms(&before) >= cutoff {
        return Ok(false);
    }
    let temp = path.clone() + ".archive-temp-" + &pi_ai::utils::random_uuid::random_uuid();
    let outcome = (|| {
        compress_transcript(path, &temp, before.mode() & 0o777)?;
        if !archive_matches_original(path, &temp)? {
            return Err(CallbackError::new(pi_js_compat::js_format!(
                "Archive differs from original: {}",
                path
            )));
        }
        if metadata.is_none() && !index_transcript(db, path)? {
            return Ok(false);
        }
        if !can_publish(db, path, &before)? {
            return Ok(false);
        }
        fs::set_permissions(
            encode_node_utf8(&temp),
            fs::Permissions::from_mode(before.mode() & 0o777),
        )
        .map_err(|error| node_io(error, "chmod", &temp))?;
        let times = FileTimes::new()
            .set_accessed(stat_date_time(before.atime(), before.atime_nsec()))
            .set_modified(stat_date_time(before.mtime(), before.mtime_nsec()));
        File::open(encode_node_utf8(&temp))
            .map_err(|error| node_io(error, "open", &temp))?
            .set_times(times)
            .map_err(|error| node_io(error, "utime", &temp))?;
        publish_archive(db, path, &temp)?;
        Ok(true)
    })();
    remove_force(&temp)?;
    outcome
}

fn resident(id: &JsString, path: &JsString) -> bool {
    id == "supervisor"
        || id == "architect"
        || path
            .split('/')
            .flat_map(|part| part.split('\\').collect::<Vec<_>>())
            .any(|part| part == "supervisor-sessions" || part == "architect-sessions")
}

fn compress_transcript(source: &JsString, target: &JsString, mode: u32) -> Result<(), CallbackError> {
    let mut input = File::open(encode_node_utf8(source)).map_err(|error| node_io(error, "open", source))?;
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(encode_node_utf8(target))
        .map_err(|error| node_io(error, "open", target))?;
    let mut encoder = zstd::stream::Encoder::new(output, 0).map_err(|error| CallbackError::new(error.to_string()))?;
    std::io::copy(&mut input, &mut encoder).map_err(|error| CallbackError::new(error.to_string()))?;
    encoder
        .finish()
        .map_err(|error| CallbackError::new(error.to_string()))?;
    Ok(())
}

fn recover_published_archive(db: &JsString, path: &JsString, before: &Metadata) -> Result<bool, CallbackError> {
    let archive = path.clone() + ".zst";
    let identical = archive_matches_original(path, &archive)?;
    if !can_publish(db, path, before)? {
        return Ok(false);
    }
    if !identical {
        if read_session_metadata(db, &archive)
            .map_err(CallbackError::from)?
            .is_some()
        {
            relocate_and_unarchive_session(db, &archive, path).map_err(CallbackError::from)?;
        }
        unlink(&archive)?;
        return Ok(false);
    }
    if read_session_metadata(db, &archive)
        .map_err(CallbackError::from)?
        .is_none()
    {
        if read_session_metadata(db, path).map_err(CallbackError::from)?.is_none() && !index_transcript(db, path)? {
            return Ok(false);
        }
        relocate_and_archive_session(db, path, &archive).map_err(CallbackError::from)?;
    }
    unlink(path)?;
    Ok(true)
}

fn can_publish(db: &JsString, path: &JsString, before: &Metadata) -> Result<bool, CallbackError> {
    let Some(references) = read_linux_process_references().map_err(CallbackError::from)? else {
        return Ok(false);
    };
    if references.contains(&realpath(path)?)
        || read_protected_paths(db)?.contains(&node_resolve("", path).map_err(CallbackError::from)?)
    {
        return Ok(false);
    }
    let after = fs::symlink_metadata(encode_node_utf8(path)).map_err(|error| node_io(error, "lstat", path))?;
    Ok(before.dev() as f64 == after.dev() as f64
        && before.ino() as f64 == after.ino() as f64
        && before.len() as f64 == after.len() as f64
        && mtime_ms(before) == mtime_ms(&after)
        && ctime_ms(before) == ctime_ms(&after))
}

fn index_transcript(db: &JsString, path: &JsString) -> Result<bool, CallbackError> {
    let Some(info) = build_session_info(path) else {
        return Ok(false);
    };
    if is_resident_session(&info) {
        return Ok(false);
    }
    let created = pi_js_compat::date::to_iso_string_value(info.created)
        .map_err(|_| CallbackError::from(NativeNodeError::new("RangeError", "Invalid time value")))?;
    let modified = pi_js_compat::date::to_iso_string_value(info.modified)
        .map_err(|_| CallbackError::from(NativeNodeError::new("RangeError", "Invalid time value")))?;
    let metadata = WritableSessionMetadata {
        session_path: path.clone(),
        id: info.id,
        cwd: info.cwd,
        name: None,
        parent_session_path: info.parent_session_path,
        archived_at: None,
        is_archived: None,
        is_subagent: None,
        subagent_name: None,
        model_provider: None,
        model_id: None,
        thinking_level: None,
        created_at: created.into(),
        modified_at: modified.into(),
        message_count: info.message_count as i64,
        first_message: info.first_message,
    };
    write_session_metadata(db, &metadata).map_err(CallbackError::from)?;
    Ok(true)
}

fn archive_matches_original(original: &JsString, archive: &JsString) -> Result<bool, CallbackError> {
    let mut source = File::open(encode_node_utf8(original)).map_err(|error| node_io(error, "open", original))?;
    let compressed = File::open(encode_node_utf8(archive)).map_err(|error| node_io(error, "open", archive))?;
    let mut decoded =
        zstd::stream::read::Decoder::new(compressed).map_err(|error| CallbackError::new(error.to_string()))?;
    let mut bytes = [0u8; COMPARE_BUFFER_SIZE];
    let mut expected = [0u8; COMPARE_BUFFER_SIZE];
    let mut position = 0u64;
    loop {
        let count = decoded
            .read(&mut bytes)
            .map_err(|error| CallbackError::new(error.to_string()))?;
        if count == 0 {
            break;
        }
        let mut read = 0;
        while read < count {
            let part = source
                .read(&mut expected[read..count])
                .map_err(|error| node_io(error, "read", original))?;
            if part == 0 {
                return Ok(false);
            }
            read += part;
        }
        if bytes[..count] != expected[..count] {
            return Ok(false);
        }
        position += count as u64;
    }
    Ok(source
        .metadata()
        .map_err(|error| node_io(error, "fstat", original))?
        .len()
        == position)
}

fn publish_archive(db: &JsString, source: &JsString, temp: &JsString) -> Result<(), CallbackError> {
    let target = source.clone() + ".zst";
    fs::hard_link(encode_node_utf8(temp), encode_node_utf8(&target)).map_err(|error| {
        let mut native = NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(&error, "link", temp));
        native.message = pi_js_compat::node_fs::node_fs_dest_error(&error, "link", temp, &target);
        CallbackError::from(native.with_property("dest", pi_js_compat::js_json::JsValue::String(target.clone())))
    })?;
    let outcome = (|| {
        relocate_and_archive_session(db, source, &target).map_err(CallbackError::from)?;
        if let Err(error) = unlink(source) {
            relocate_and_unarchive_session(db, &target, source).map_err(CallbackError::from)?;
            return Err(error);
        }
        Ok(())
    })();
    if outcome.is_err() {
        remove_force(&target)?;
    }
    outcome
}

fn unlink(path: &JsString) -> Result<(), CallbackError> {
    fs::remove_file(encode_node_utf8(path)).map_err(|error| node_io(error, "unlink", path))
}

fn remove_force(path: &JsString) -> Result<(), CallbackError> {
    match fs::remove_file(encode_node_utf8(path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(node_io(error, "unlink", path)),
    }
}

fn mtime_ms(metadata: &Metadata) -> f64 {
    metadata.mtime() as f64 * 1000.0 + metadata.mtime_nsec() as f64 / 1_000_000.0
}
fn ctime_ms(metadata: &Metadata) -> f64 {
    metadata.ctime() as f64 * 1000.0 + metadata.ctime_nsec() as f64 / 1_000_000.0
}
fn stat_date_time(seconds: i64, nanoseconds: i64) -> SystemTime {
    let millis = (seconds as f64 * 1000.0 + nanoseconds as f64 / 1_000_000.0).trunc();
    let duration = Duration::from_millis(millis.abs() as u64);
    if millis < 0.0 {
        SystemTime::UNIX_EPOCH - duration
    } else {
        SystemTime::UNIX_EPOCH + duration
    }
}
