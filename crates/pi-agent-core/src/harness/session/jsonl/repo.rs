//! Port of `packages/agent/src/harness/session/jsonl/repo.ts`.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::harness::session::facade::Session;
use crate::harness::session::jsonl::codec::{metadata_from_header, parse_header};
use crate::harness::session::jsonl::errors::file_result;
use crate::harness::session::jsonl::storage::JsonlSessionStorage;
use crate::harness::session::jsonl::types::{
    HeaderKind, JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionMetadata, JsonlSessionRepoFileSystem,
    JsonlSessionRepoOptions, JsonlV4Header,
};
use crate::harness::session::types::{
    Clock, ForkOptions, IdGenerator, SessionError, SessionErrorCode, SessionStorage, UuidV7IdGenerator, system_clock,
};

fn validate_session_id(id: &str) -> Result<(), SessionError> {
    if is_valid_session_id(id) {
        return Ok(());
    }
    Err(SessionError::new(
        SessionErrorCode::InvalidPayload,
        "Session id must be non-empty, contain only alphanumeric characters, '-', '_', and '.', and start and end with an alphanumeric character",
    ))
}

fn is_valid_session_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    let Some(last) = bytes.last() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Encode a working directory the way coding-agent session directories are named.
pub fn jsonl_session_directory_name(cwd: &JsString) -> JsString {
    let without_root = cwd
        .strip_prefix('/')
        .or_else(|| cwd.strip_prefix('\\'))
        .unwrap_or_else(|| cwd.clone());
    let encoded = JsString::from_utf16(
        without_root
            .units()
            .iter()
            .map(|unit| match *unit {
                0x2f | 0x5c | 0x3a => 0x2d,
                other => other,
            })
            .collect(),
    );
    JsString::from("--") + encoded + "--"
}

fn iso_timestamp(created_at: i64) -> String {
    pi_js_compat::date::to_iso_string(created_at).replace([':', '.'], "-")
}

/// Build the durable session file name.
pub fn session_file_name(created_at: i64, id: &str) -> JsString {
    js_format!("{}_{}.jsonl", iso_timestamp(created_at), id)
}

async fn jsonl_sessions_root(options: &JsonlSessionRepoOptions) -> Result<JsString, SessionError> {
    file_result(
        options.fs.absolute_path(&options.sessions_root).await,
        &js_format!("Failed to resolve sessions root {}", &options.sessions_root),
    )
}

async fn jsonl_session_directory(
    fs: &Arc<dyn JsonlSessionRepoFileSystem>,
    sessions_root: &JsString,
    cwd: &JsString,
) -> Result<JsString, SessionError> {
    file_result(
        fs.join_path(&[sessions_root.to_owned(), jsonl_session_directory_name(cwd)])
            .await,
        &js_format!("Failed to resolve sessions directory for {}", cwd),
    )
}

async fn jsonl_session_directories(
    options: &JsonlSessionRepoOptions,
    cwd: Option<&JsString>,
) -> Result<Vec<JsString>, SessionError> {
    let sessions_root = jsonl_sessions_root(options).await?;
    match cwd {
        Some(cwd) => session_directory_for_cwd(options, &sessions_root, cwd).await,
        None => child_session_directories(options, &sessions_root).await,
    }
}

async fn session_directory_for_cwd(
    options: &JsonlSessionRepoOptions,
    sessions_root: &JsString,
    cwd: &JsString,
) -> Result<Vec<JsString>, SessionError> {
    let resolved_cwd = file_result(
        options.fs.absolute_path(cwd).await,
        &js_format!("Failed to resolve session cwd {}", cwd),
    )?;
    let directory = jsonl_session_directory(&options.fs, sessions_root, &resolved_cwd).await?;
    let exists = file_result(
        options.fs.exists(&directory).await,
        &js_format!("Failed to check sessions directory {}", &directory),
    )?;
    Ok(if exists { vec![directory] } else { Vec::new() })
}

async fn child_session_directories(
    options: &JsonlSessionRepoOptions,
    sessions_root: &JsString,
) -> Result<Vec<JsString>, SessionError> {
    let exists = file_result(
        options.fs.exists(sessions_root).await,
        &js_format!("Failed to check sessions directory {}", sessions_root),
    )?;
    if !exists {
        return Ok(Vec::new());
    }
    let entries = file_result(
        options.fs.list_dir(sessions_root).await,
        &js_format!("Failed to list sessions directory {}", sessions_root),
    )?;
    Ok(entries
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                crate::harness::types::FileKind::Directory | crate::harness::types::FileKind::Symlink
            )
        })
        .map(|entry| entry.path)
        .collect())
}

/// List session metadata without opening sessions or acquiring writer claims.
pub async fn list_jsonl_session_metadata(
    options: &JsonlSessionRepoOptions,
    query: &JsonlSessionListOptions,
) -> Result<Vec<JsonlSessionMetadata>, SessionError> {
    let mut metadata = Vec::new();
    for directory in jsonl_session_directories(options, query.cwd.as_ref()).await? {
        for file in list_jsonl_files(options, &directory).await? {
            if let Some(listed) = read_listed_header(options, &file).await? {
                metadata.push(listed);
            }
        }
    }
    metadata.sort_by(|left, right| {
        right
            .modified_at
            .partial_cmp(&left.modified_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(metadata)
}

async fn list_jsonl_files(
    options: &JsonlSessionRepoOptions,
    directory: &JsString,
) -> Result<Vec<crate::harness::types::FileInfo>, SessionError> {
    let files = file_result(
        options.fs.list_dir(directory).await,
        &js_format!("Failed to list sessions directory {}", directory),
    )?;
    Ok(files
        .into_iter()
        .filter(|entry| entry.kind != crate::harness::types::FileKind::Directory && entry.name.ends_with(".jsonl"))
        .collect())
}

async fn read_listed_header(
    options: &JsonlSessionRepoOptions,
    file: &crate::harness::types::FileInfo,
) -> Result<Option<JsonlSessionMetadata>, SessionError> {
    let lines = file_result(
        options.fs.read_text_lines(&file.path, Some(1)).await,
        &js_format!("Failed to read session header {}", &file.path),
    )?;
    let Some(first_line) = lines.first() else {
        return Ok(None);
    };
    let Ok(header) = parse_header(first_line) else {
        return Ok(None);
    };
    Ok(Some(metadata_from_header(&header, file.path.clone(), file.mtime_ms)))
}

/// Load a JSONL storage for a listed metadata record.
pub async fn load_jsonl_session_storage(
    options: &JsonlSessionRepoOptions,
    metadata: &JsonlSessionMetadata,
) -> Result<JsonlSessionStorage, SessionError> {
    load_jsonl_session_storage_with_clock(options, metadata, system_clock()).await
}

/// Load with an injectable clock.
pub async fn load_jsonl_session_storage_with_clock(
    options: &JsonlSessionRepoOptions,
    metadata: &JsonlSessionMetadata,
    clock: Clock,
) -> Result<JsonlSessionStorage, SessionError> {
    let exists = file_result(
        options.fs.exists(&metadata.path).await,
        &js_format!("Failed to check session {}", &metadata.path),
    )?;
    if !exists {
        return Err(SessionError::new(
            SessionErrorCode::NotFound,
            format!("Session not found: {}", metadata.id),
        ));
    }
    let storage = JsonlSessionStorage::load(Arc::clone(&options.fs), &metadata.path, clock).await?;
    let loaded_metadata = storage.get_metadata().await?;
    if loaded_metadata.id != metadata.id {
        return Err(SessionError::new(
            SessionErrorCode::InvalidEntry,
            format!("Session id does not match header: {}", metadata.id),
        ));
    }
    Ok(storage)
}

/// JSONL session repository.
pub struct JsonlSessionRepo {
    fs: Arc<dyn JsonlSessionRepoFileSystem>,
    sessions_root_input: JsString,
    active_create_destinations: Mutex<HashSet<JsString>>,
    clock: Clock,
    id_generator: Arc<dyn IdGenerator>,
}

impl JsonlSessionRepo {
    pub fn new(options: JsonlSessionRepoOptions) -> Self {
        Self::with_clock_and_id_generator(options, system_clock(), Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock(options: JsonlSessionRepoOptions, clock: Clock) -> Self {
        Self::with_clock_and_id_generator(options, clock, Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock_and_id_generator(
        options: JsonlSessionRepoOptions,
        clock: Clock,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            fs: options.fs,
            sessions_root_input: options.sessions_root,
            active_create_destinations: Mutex::new(HashSet::new()),
            clock,
            id_generator,
        }
    }

    fn options(&self) -> JsonlSessionRepoOptions {
        JsonlSessionRepoOptions {
            fs: Arc::clone(&self.fs),
            sessions_root: self.sessions_root_input.clone(),
        }
    }

    pub async fn create(
        &self,
        options: JsonlSessionCreateOptions,
    ) -> Result<Session<JsonlSessionMetadata>, SessionError> {
        let destination = self.resolve_create_destination(&options).await?;
        let key = self.claim(&destination).await?;
        let result = async {
            let (header, path) = self.prepare_create(&destination, &options).await?;
            let storage =
                JsonlSessionStorage::create(Arc::clone(&self.fs), &path, &header, Arc::clone(&self.clock)).await?;
            Ok(Session::new(Arc::new(storage)))
        }
        .await;
        self.release(&key).await;
        result
    }

    pub async fn open(&self, metadata: &JsonlSessionMetadata) -> Result<Session<JsonlSessionMetadata>, SessionError> {
        let storage = load_jsonl_session_storage_with_clock(&self.options(), metadata, Arc::clone(&self.clock)).await?;
        Ok(Session::new(Arc::new(storage)))
    }

    pub async fn list(&self, options: JsonlSessionListOptions) -> Result<Vec<JsonlSessionMetadata>, SessionError> {
        list_jsonl_session_metadata(&self.options(), &options).await
    }

    pub async fn delete(&self, metadata: &JsonlSessionMetadata) -> Result<(), SessionError> {
        file_result(
            self.fs.remove(&metadata.path, false, true).await,
            &js_format!("Failed to delete session {}", &metadata.path),
        )
    }

    pub async fn fork(
        &self,
        source: &JsonlSessionMetadata,
        options: ForkOptions,
        mut create_options: JsonlSessionCreateOptions,
    ) -> Result<Session<JsonlSessionMetadata>, SessionError> {
        let source_storage =
            load_jsonl_session_storage_with_clock(&self.options(), source, Arc::clone(&self.clock)).await?;
        if create_options.parent_session_id.is_none() {
            create_options.parent_session_id = Some(source.id.clone());
        }
        let destination = self.resolve_create_destination(&create_options).await?;
        let key = self.claim(&destination).await?;
        let result = async {
            let (header, path) = self.prepare_create(&destination, &create_options).await?;
            let storage = source_storage.fork(&path, &header, &options).await?;
            Ok(Session::new(Arc::new(storage)))
        }
        .await;
        self.release(&key).await;
        result
    }

    async fn resolve_create_destination(
        &self,
        options: &JsonlSessionCreateOptions,
    ) -> Result<CreateDestination, SessionError> {
        let id = options.id.clone().unwrap_or_else(|| self.id_generator.next());
        validate_session_id(&id)?;
        let cwd = file_result(
            self.fs.absolute_path(&options.cwd).await,
            &js_format!("Failed to resolve session cwd {}", &options.cwd),
        )?;
        Ok(CreateDestination { id, cwd })
    }

    async fn claim(&self, destination: &CreateDestination) -> Result<JsString, SessionError> {
        let key = js_format!("{}\0{}", &destination.cwd, &destination.id);
        let mut active = self.active_create_destinations.lock().await;
        if active.contains(&key) {
            return Err(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Session already exists: {}", destination.id),
            ));
        }
        active.insert(key.clone());
        Ok(key)
    }

    async fn release(&self, key: &JsString) {
        self.active_create_destinations.lock().await.remove(key);
    }

    async fn prepare_create(
        &self,
        destination: &CreateDestination,
        options: &JsonlSessionCreateOptions,
    ) -> Result<(JsonlV4Header, JsString), SessionError> {
        if self.session_id_exists(&destination.id, &destination.cwd).await? {
            return Err(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Session already exists: {}", destination.id),
            ));
        }
        let created_at = (self.clock)();
        let session_directory = self.session_directory(&destination.cwd).await?;
        let path = file_result(
            self.fs
                .join_path(&[
                    session_directory.clone(),
                    session_file_name(created_at, &destination.id),
                ])
                .await,
            &format!("Failed to resolve path for session {}", destination.id),
        )?;
        let header = build_header(destination, created_at, options);
        file_result(
            self.fs.create_dir(&session_directory, true).await,
            "Failed to create sessions directory",
        )?;
        Ok((header, path))
    }

    async fn session_id_exists(&self, id: &str, cwd: &JsString) -> Result<bool, SessionError> {
        let suffix = format!("_{id}.jsonl");
        let directory = self.session_directory(cwd).await?;
        let exists = file_result(
            self.fs.exists(&directory).await,
            &js_format!("Failed to check sessions directory {}", &directory),
        )?;
        if !exists {
            return Ok(false);
        }
        let files = file_result(
            self.fs.list_dir(&directory).await,
            &js_format!("Failed to list sessions directory {}", &directory),
        )?;
        Ok(files
            .iter()
            .any(|entry| entry.kind != crate::harness::types::FileKind::Directory && entry.name.ends_with(&suffix)))
    }

    async fn session_directory(&self, cwd: &JsString) -> Result<JsString, SessionError> {
        let root = jsonl_sessions_root(&self.options()).await?;
        jsonl_session_directory(&self.fs, &root, cwd).await
    }
}

fn build_header(
    destination: &CreateDestination,
    created_at: i64,
    options: &JsonlSessionCreateOptions,
) -> JsonlV4Header {
    JsonlV4Header {
        kind: HeaderKind::Header,
        version: 4,
        id: destination.id.clone(),
        created_at,
        cwd: destination.cwd.clone(),
        parent_session_id: options.parent_session_id.clone(),
        legacy_parent_session_path: None,
        metadata: options.metadata.clone(),
    }
}

struct CreateDestination {
    id: String,
    cwd: JsString,
}
