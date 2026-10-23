//! Frozen session/jsonl-storage.ts: v3 header plus raw tree entries.

use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, from_value, parse_lossless, stringify_lossless, to_value};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::super::repo_utils::get_file_system_result_or_throw;
use super::super::state::TreeState;
use super::super::types::*;
use super::types::{JsonlSessionMetadata, JsonlSessionStorageCreateOptions, SessionHeader};
use crate::harness::types::{FileContent, FileSystem, ReadTextLinesOptions};

fn invalid_session(path: &JsString, message: &str) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidSession,
        js_format!("Invalid JSONL session file {}: {}", path, message),
    )
}
fn invalid_entry(path: &JsString, line: usize, message: &str) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidEntry,
        js_format!("Invalid JSONL session file {}: line {} {}", path, line, message),
    )
}

fn parse_header_line(line: &JsString, path: &JsString) -> Result<SessionHeader, SessionError> {
    let value = parse_lossless(line).map_err(|cause| {
        let mut error = invalid_session(path, "first line is not a valid session header");
        error.cause = Some(Box::new(cause));
        error
    })?;
    let Some(header) = value.as_object() else {
        return Err(invalid_session(path, "first line is not a valid session header"));
    };
    if header
        .get("type")
        .and_then(JsValue::as_string)
        .is_none_or(|kind| kind != "session")
    {
        return Err(invalid_session(path, "first line is not a valid session header"));
    }
    if header.get("version").and_then(JsValue::as_f64) != Some(3.0) {
        return Err(invalid_session(path, "unsupported session version"));
    }
    for (key, message) in [
        ("id", "session header is missing id"),
        ("timestamp", "session header is missing timestamp"),
        ("cwd", "session header is missing cwd"),
    ] {
        if header
            .get(key)
            .and_then(JsValue::as_string)
            .is_none_or(JsString::is_empty)
        {
            return Err(invalid_session(path, message));
        }
    }
    if header
        .get("parentSession")
        .is_some_and(|parent| parent.as_string().is_none())
    {
        return Err(invalid_session(path, "session header parentSession must be a string"));
    }
    from_value(&value).map_err(|error| {
        SessionError::with_cause(SessionErrorCode::InvalidSession, "Invalid native session header", error)
    })
}

fn parse_entry_line(line: &JsString, path: &JsString, number: usize) -> Result<SessionTreeEntry, SessionError> {
    let value = parse_lossless(line).map_err(|cause| {
        let mut error = invalid_entry(path, number, "is not valid JSON");
        error.cause = Some(Box::new(cause));
        error
    })?;
    validate_entry(&value).map_err(|message| invalid_entry(path, number, message))?;
    SessionTreeEntry::from_value(value)
}

pub async fn load_jsonl_session_metadata(
    fs: &dyn FileSystem,
    path: &JsString,
) -> Result<JsonlSessionMetadata, SessionFailure> {
    let lines = get_file_system_result_or_throw(
        fs.read_text_lines(
            path,
            ReadTextLinesOptions {
                max_lines: Some(1.0),
                abort: None,
            },
        )
        .await,
        &js_format!("Failed to read session header {}", path),
    )?;
    let Some(line) = lines.first().map(JsString::from).filter(|line| !line.trim().is_empty()) else {
        return Err(invalid_session(path, "missing session header").into());
    };
    Ok(parse_header_line(&line, path)?.metadata(path))
}

pub struct JsonlSessionStorage {
    fs: Arc<dyn FileSystem>,
    path: JsString,
    metadata: JsonlSessionMetadata,
    state: Mutex<TreeState>,
    clock: Clock,
    ids: IdGenerator,
}
impl JsonlSessionStorage {
    pub async fn open(fs: Arc<dyn FileSystem>, path: JsString) -> Result<Self, SessionFailure> {
        Self::open_with_runtime(fs, path, system_clock(), uuid_generator()).await
    }
    pub async fn open_with_runtime(
        fs: Arc<dyn FileSystem>,
        path: JsString,
        clock: Clock,
        ids: IdGenerator,
    ) -> Result<Self, SessionFailure> {
        let content = get_file_system_result_or_throw(
            fs.read_text_file(&path, None).await,
            &js_format!("Failed to read session {}", path),
        )?;
        let lines = JsString::from(content)
            .split('\n')
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        let Some(first) = lines.first() else {
            return Err(invalid_session(&path, "missing session header").into());
        };
        let header = parse_header_line(first, &path)?;
        let entries = lines
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, line)| parse_entry_line(line, &path, index + 2))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            metadata: header.metadata(&path),
            fs,
            path,
            state: Mutex::new(TreeState::from_entries(entries)),
            clock,
            ids,
        })
    }
    pub async fn create(
        fs: Arc<dyn FileSystem>,
        path: JsString,
        options: JsonlSessionStorageCreateOptions,
    ) -> Result<Self, SessionFailure> {
        Self::create_with_runtime(fs, path, options, system_clock(), uuid_generator()).await
    }
    pub async fn create_with_runtime(
        fs: Arc<dyn FileSystem>,
        path: JsString,
        options: JsonlSessionStorageCreateOptions,
        clock: Clock,
        ids: IdGenerator,
    ) -> Result<Self, SessionFailure> {
        let header = SessionHeader {
            kind: "session".into(),
            version: 3,
            id: options.session_id,
            timestamp: clock(),
            cwd: options.cwd,
            parent_session: options.parent_session_path,
        };
        let text = JsString::from(stringify_lossless(&to_value(&header).expect("header serializes")) + "\n");
        get_file_system_result_or_throw(
            fs.write_file(&path, FileContent::Text(&text), None).await,
            &js_format!("Failed to create session {}", path),
        )?;
        Ok(Self {
            metadata: header.metadata(&path),
            fs,
            path,
            state: Mutex::new(TreeState::default()),
            clock,
            ids,
        })
    }
    async fn persist_entry(&self, entry: SessionTreeEntry, leaf: bool) -> Result<(), SessionFailure> {
        let text = JsString::from(stringify_lossless(&entry.value()) + "\n");
        let message = if leaf {
            js_format!("Failed to append session leaf {}", entry.id())
        } else {
            js_format!("Failed to append session entry {}", entry.id())
        };
        get_file_system_result_or_throw(
            self.fs.append_file(&self.path, FileContent::Text(&text), None).await,
            &message,
        )?;
        self.state.lock().await.append(entry);
        Ok(())
    }
}
impl SessionStorage<JsonlSessionMetadata> for JsonlSessionStorage {
    fn get_metadata(&self) -> BoxFuture<'_, Result<JsonlSessionMetadata, SessionError>> {
        Box::pin(async { Ok(self.metadata.clone()) })
    }
    fn get_leaf_id(&self) -> BoxFuture<'_, Result<Option<JsString>, SessionError>> {
        Box::pin(async { self.state.lock().await.validate_leaf() })
    }
    fn create_entry_id(&self) -> BoxFuture<'_, Result<JsString, SessionError>> {
        Box::pin(async { Ok(self.state.lock().await.create_entry_id(&self.ids)) })
    }
    fn set_leaf_id<'a>(&'a self, leaf: Option<&'a JsString>) -> BoxFuture<'a, Result<(), SessionFailure>> {
        Box::pin(async move {
            let entry = {
                let state = self.state.lock().await;
                state.validate_target(leaf)?;
                SessionTreeEntry::from_known(KnownSessionEntry::Leaf {
                    base: SessionTreeEntryBase {
                        id: state.create_entry_id(&self.ids),
                        parent_id: state.leaf_id.clone(),
                        timestamp: (self.clock)(),
                    },
                    target_id: leaf.cloned(),
                })
            };
            self.persist_entry(entry, true).await
        })
    }
    fn append_entry(&self, entry: SessionTreeEntry) -> BoxFuture<'_, Result<(), SessionFailure>> {
        Box::pin(async move { self.persist_entry(entry, false).await })
    }
    fn get_entry<'a>(&'a self, id: &'a JsString) -> BoxFuture<'a, Result<Option<SessionTreeEntry>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.by_id.get(id).cloned()) })
    }
    fn find_entries<'a>(&'a self, kind: &'a JsString) -> BoxFuture<'a, Result<Vec<SessionTreeEntry>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.find_entries(kind)) })
    }
    fn get_label<'a>(&'a self, id: &'a JsString) -> BoxFuture<'a, Result<Option<JsString>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.labels.get(id).cloned()) })
    }
    fn get_path_to_root<'a>(
        &'a self,
        leaf: Option<&'a JsString>,
    ) -> BoxFuture<'a, Result<Vec<SessionTreeEntry>, SessionError>> {
        Box::pin(async move { self.state.lock().await.path_to_root(leaf) })
    }
    fn get_entries(&self) -> BoxFuture<'_, Result<Vec<SessionTreeEntry>, SessionError>> {
        Box::pin(async { Ok(self.state.lock().await.entries.clone()) })
    }
}
