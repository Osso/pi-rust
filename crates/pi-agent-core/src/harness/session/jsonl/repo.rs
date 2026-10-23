//! Frozen session/jsonl-repo.ts.

use pi_js_compat::array_sort::js_array_sort;
use pi_js_compat::date::date_parse_utf16;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::super::facade::Session;
use super::super::repo_utils::{get_entries_to_fork, get_file_system_result_or_throw};
use super::super::types::*;
use super::storage::{JsonlSessionStorage, load_jsonl_session_metadata};
use super::types::*;
use crate::harness::types::{CreateDirOptions, FileKind, FileSystem, RemoveOptions};

#[derive(Debug, Clone)]
pub struct JsonlSessionForkOptions {
    pub create: JsonlSessionCreateOptions,
    pub entry_id: Option<JsString>,
    pub position: ForkPosition,
}

pub fn jsonl_session_directory_name(cwd: &JsString) -> JsString {
    let cwd = if matches!(cwd.units().first(), Some(0x2f | 0x5c)) {
        cwd.slice(1, cwd.len())
    } else {
        cwd.clone()
    };
    let cwd = ['/', '\\', ':']
        .into_iter()
        .fold(cwd, |cwd, separator| cwd.replace(separator, "-"));
    js_format!("--{}--", cwd)
}

pub struct JsonlSessionRepo {
    fs: Arc<dyn FileSystem>,
    root_input: JsString,
    root: Mutex<Option<JsString>>,
    clock: Clock,
    ids: IdGenerator,
}
impl JsonlSessionRepo {
    pub fn new(options: JsonlSessionRepoOptions) -> Self {
        Self::with_runtime(options, system_clock(), uuid_generator())
    }
    pub fn with_runtime(options: JsonlSessionRepoOptions, clock: Clock, ids: IdGenerator) -> Self {
        Self {
            fs: options.fs,
            root_input: options.sessions_root,
            root: Mutex::new(None),
            clock,
            ids,
        }
    }
    async fn sessions_root(&self) -> Result<JsString, SessionFailure> {
        if let Some(root) = self.root.lock().await.as_ref().filter(|root| !root.is_empty()).cloned() {
            return Ok(root);
        }
        let root = get_file_system_result_or_throw(
            self.fs.absolute_path(&self.root_input, None).await,
            &js_format!("Failed to resolve sessions root {}", self.root_input),
        )?;
        *self.root.lock().await = Some(root.clone());
        Ok(root)
    }
    async fn session_dir(&self, cwd: &JsString) -> Result<JsString, SessionFailure> {
        get_file_system_result_or_throw(
            self.fs
                .join_path(&[self.sessions_root().await?, jsonl_session_directory_name(cwd)], None)
                .await,
            &js_format!("Failed to resolve session directory for {}", cwd),
        )
    }
    async fn session_file_path(
        &self,
        cwd: &JsString,
        id: &JsString,
        timestamp: &JsString,
    ) -> Result<JsString, SessionFailure> {
        let filename = js_format!(
            "{}_{}.jsonl",
            [':', '.']
                .into_iter()
                .fold(timestamp.clone(), |text, separator| text.replace(separator, "-")),
            id
        );
        get_file_system_result_or_throw(
            self.fs.join_path(&[self.session_dir(cwd).await?, filename], None).await,
            &js_format!("Failed to resolve session file path for {}", id),
        )
    }
    async fn create_session_dir(&self, cwd: &JsString) -> Result<(), SessionFailure> {
        let directory = self.session_dir(cwd).await?;
        get_file_system_result_or_throw(
            self.fs
                .create_dir(&directory, CreateDirOptions { recursive: Some(true) })
                .await,
            &js_format!("Failed to create session directory {}", directory),
        )
    }
    pub async fn create(
        &self,
        options: JsonlSessionCreateOptions,
    ) -> Result<Session<JsonlSessionMetadata>, SessionFailure> {
        let id = options.id.unwrap_or_else(|| (self.ids)());
        let created_at = (self.clock)();
        self.create_session_dir(&options.cwd).await?;
        let path = self.session_file_path(&options.cwd, &id, &created_at).await?;
        let storage = JsonlSessionStorage::create_with_runtime(
            Arc::clone(&self.fs),
            path,
            JsonlSessionStorageCreateOptions {
                cwd: options.cwd,
                session_id: id,
                parent_session_path: options.parent_session_path,
            },
            Arc::clone(&self.clock),
            Arc::clone(&self.ids),
        )
        .await?;
        Ok(Session::with_clock(Arc::new(storage), Arc::clone(&self.clock)))
    }
    pub async fn open(&self, metadata: &JsonlSessionMetadata) -> Result<Session<JsonlSessionMetadata>, SessionFailure> {
        let exists = get_file_system_result_or_throw(
            self.fs.exists(&metadata.path, None).await,
            &js_format!("Failed to check session {}", metadata.path),
        )?;
        if !exists {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                js_format!("Session not found: {}", metadata.path),
            )
            .into());
        }
        let storage = JsonlSessionStorage::open_with_runtime(
            Arc::clone(&self.fs),
            metadata.path.clone(),
            Arc::clone(&self.clock),
            Arc::clone(&self.ids),
        )
        .await?;
        Ok(Session::with_clock(Arc::new(storage), Arc::clone(&self.clock)))
    }
    pub async fn list(&self, options: JsonlSessionListOptions) -> Result<Vec<JsonlSessionMetadata>, SessionFailure> {
        let directories = if let Some(cwd) = options.cwd.as_ref().filter(|cwd| !cwd.is_empty()) {
            vec![self.session_dir(cwd).await?]
        } else {
            self.list_session_dirs().await?
        };
        let mut sessions = Vec::new();
        for directory in directories {
            let exists = get_file_system_result_or_throw(
                self.fs.exists(&directory, None).await,
                &js_format!("Failed to check session directory {}", directory),
            )?;
            if !exists {
                continue;
            }
            let files = get_file_system_result_or_throw(
                self.fs.list_dir(&directory, None).await,
                &js_format!("Failed to list sessions in {}", directory),
            )?;
            for file in files
                .into_iter()
                .filter(|file| file.kind != FileKind::Directory && file.name.ends_with(".jsonl"))
            {
                match load_jsonl_session_metadata(self.fs.as_ref(), &file.path).await {
                    Ok(metadata) => sessions.push(metadata),
                    Err(crate::harness::types::OperationFailure::Returned(error))
                        if error.code == SessionErrorCode::InvalidSession => {}
                    Err(error) => return Err(error),
                }
            }
        }
        js_array_sort(&mut sessions, |left, right| {
            date_parse_utf16(right.created_at.units()) - date_parse_utf16(left.created_at.units())
        });
        Ok(sessions)
    }
    pub async fn delete(&self, metadata: &JsonlSessionMetadata) -> Result<(), SessionFailure> {
        get_file_system_result_or_throw(
            self.fs
                .remove(
                    &metadata.path,
                    RemoveOptions {
                        recursive: None,
                        force: Some(true),
                    },
                )
                .await,
            &js_format!("Failed to delete session {}", metadata.path),
        )
    }
    pub async fn fork(
        &self,
        metadata: &JsonlSessionMetadata,
        options: JsonlSessionForkOptions,
    ) -> Result<Session<JsonlSessionMetadata>, SessionFailure> {
        let source = self.open(metadata).await?;
        let entries = get_entries_to_fork(
            source.get_storage(),
            &SessionForkOptions {
                entry_id: options.entry_id,
                position: options.position,
                id: None,
            },
        )
        .await?;
        let id = options.create.id.unwrap_or_else(|| (self.ids)());
        let created_at = (self.clock)();
        self.create_session_dir(&options.create.cwd).await?;
        let path = self.session_file_path(&options.create.cwd, &id, &created_at).await?;
        let storage = JsonlSessionStorage::create_with_runtime(
            Arc::clone(&self.fs),
            path,
            JsonlSessionStorageCreateOptions {
                cwd: options.create.cwd,
                session_id: id,
                parent_session_path: Some(
                    options
                        .create
                        .parent_session_path
                        .unwrap_or_else(|| metadata.path.clone()),
                ),
            },
            Arc::clone(&self.clock),
            Arc::clone(&self.ids),
        )
        .await?;
        for entry in entries {
            storage.append_entry(entry).await?;
        }
        Ok(Session::with_clock(Arc::new(storage), Arc::clone(&self.clock)))
    }
    async fn list_session_dirs(&self) -> Result<Vec<JsString>, SessionFailure> {
        let root = self.sessions_root().await?;
        let exists = get_file_system_result_or_throw(
            self.fs.exists(&root, None).await,
            &js_format!("Failed to check sessions root {}", root),
        )?;
        if !exists {
            return Ok(Vec::new());
        }
        let entries = get_file_system_result_or_throw(
            self.fs.list_dir(&root, None).await,
            &js_format!("Failed to list sessions root {}", root),
        )?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.kind == FileKind::Directory)
            .map(|entry| entry.path)
            .collect())
    }
}
