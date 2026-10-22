//! Frozen session/memory-storage.ts and session/memory-repo.ts.

use indexmap::IndexMap;
use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::facade::Session;
use super::repo_utils::{create_session_id, create_timestamp, get_entries_to_fork};
use super::state::TreeState;
use super::types::*;

#[derive(Default)]
pub struct InMemorySessionStorageOptions {
    pub entries: Vec<SessionTreeEntry>,
    pub metadata: Option<SessionMetadata>,
}

pub struct InMemorySessionStorage<M: SessionMetadataValue = SessionMetadata> {
    metadata: M,
    state: Mutex<TreeState>,
    clock: Clock,
    ids: IdGenerator,
}

impl InMemorySessionStorage<SessionMetadata> {
    pub fn new(options: InMemorySessionStorageOptions) -> Result<Self, SessionError> {
        let state = TreeState::from_entries(options.entries);
        state.validate_leaf()?;
        let metadata = options.metadata.unwrap_or_else(|| SessionMetadata {
            id: create_session_id(),
            created_at: create_timestamp(),
        });
        Ok(Self {
            metadata,
            state: Mutex::new(state),
            clock: system_clock(),
            ids: uuid_generator(),
        })
    }
}

impl<M: SessionMetadataValue> InMemorySessionStorage<M> {
    pub fn with_metadata(metadata: M, entries: Vec<SessionTreeEntry>) -> Result<Self, SessionError> {
        Self::with_runtime(metadata, entries, system_clock(), uuid_generator())
    }
    pub fn with_runtime(
        metadata: M,
        entries: Vec<SessionTreeEntry>,
        clock: Clock,
        ids: IdGenerator,
    ) -> Result<Self, SessionError> {
        let state = TreeState::from_entries(entries);
        state.validate_leaf()?;
        Ok(Self {
            metadata,
            state: Mutex::new(state),
            clock,
            ids,
        })
    }
}

impl<M: SessionMetadataValue> SessionStorage<M> for InMemorySessionStorage<M> {
    fn get_metadata(&self) -> BoxFuture<'_, Result<M, SessionError>> {
        Box::pin(async { Ok(self.metadata.clone()) })
    }
    fn get_leaf_id(&self) -> BoxFuture<'_, Result<Option<JsString>, SessionError>> {
        Box::pin(async { self.state.lock().await.validate_leaf() })
    }
    fn create_entry_id(&self) -> BoxFuture<'_, Result<JsString, SessionError>> {
        Box::pin(async { Ok(self.state.lock().await.create_entry_id(&self.ids)) })
    }
    fn set_leaf_id<'a>(&'a self, leaf: Option<&'a JsString>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            state.validate_target(leaf)?;
            let entry = SessionTreeEntry::from_known(KnownSessionEntry::Leaf {
                base: SessionTreeEntryBase {
                    id: state.create_entry_id(&self.ids),
                    parent_id: state.leaf_id.clone(),
                    timestamp: (self.clock)(),
                },
                target_id: leaf.cloned(),
            });
            state.append(entry);
            Ok(())
        })
    }
    fn append_entry(&self, entry: SessionTreeEntry) -> BoxFuture<'_, Result<(), SessionError>> {
        Box::pin(async move {
            self.state.lock().await.append(entry);
            Ok(())
        })
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

pub struct InMemorySessionRepo {
    sessions: Mutex<IndexMap<JsString, Session<SessionMetadata>>>,
    clock: Clock,
    ids: IdGenerator,
}
impl Default for InMemorySessionRepo {
    fn default() -> Self {
        Self::with_runtime(system_clock(), uuid_generator())
    }
}
impl InMemorySessionRepo {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_runtime(clock: Clock, ids: IdGenerator) -> Self {
        Self {
            sessions: Mutex::new(IndexMap::new()),
            clock,
            ids,
        }
    }
    pub async fn create(&self, options: SessionCreateOptions) -> Result<Session<SessionMetadata>, SessionError> {
        let metadata = SessionMetadata {
            id: options.id.unwrap_or_else(|| (self.ids)()),
            created_at: (self.clock)(),
        };
        self.persist_new_session(metadata, Vec::new()).await
    }
    async fn persist_new_session(
        &self,
        metadata: SessionMetadata,
        entries: Vec<SessionTreeEntry>,
    ) -> Result<Session<SessionMetadata>, SessionError> {
        let id = metadata.id.clone();
        let storage =
            InMemorySessionStorage::with_runtime(metadata, entries, Arc::clone(&self.clock), Arc::clone(&self.ids))?;
        let session = Session::with_clock(Arc::new(storage), Arc::clone(&self.clock));
        self.sessions.lock().await.insert(id, session.clone());
        Ok(session)
    }
    pub async fn open(&self, metadata: &SessionMetadata) -> Result<Session<SessionMetadata>, SessionError> {
        self.sessions.lock().await.get(&metadata.id).cloned().ok_or_else(|| {
            SessionError::new(
                SessionErrorCode::NotFound,
                js_format!("Session not found: {}", metadata.id),
            )
        })
    }
    pub async fn list(&self) -> Result<Vec<SessionMetadata>, SessionError> {
        let sessions = self.sessions.lock().await.values().cloned().collect::<Vec<_>>();
        let mut metadata = Vec::new();
        for session in sessions {
            metadata.push(session.get_metadata().await?);
        }
        Ok(metadata)
    }
    pub async fn delete(&self, metadata: &SessionMetadata) -> Result<(), SessionError> {
        self.sessions.lock().await.shift_remove(&metadata.id);
        Ok(())
    }
    pub async fn fork(
        &self,
        metadata: &SessionMetadata,
        options: SessionForkOptions,
    ) -> Result<Session<SessionMetadata>, SessionError> {
        let source = self.open(metadata).await?;
        let entries = get_entries_to_fork(source.get_storage(), &options).await?;
        let metadata = SessionMetadata {
            id: options.id.unwrap_or_else(|| (self.ids)()),
            created_at: (self.clock)(),
        };
        self.persist_new_session(metadata, entries).await
    }
}
