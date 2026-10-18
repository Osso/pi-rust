//! Port of `packages/agent/src/harness/session/memory.ts`.

use indexmap::IndexMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use pi_ai::request::BoxFuture;

use crate::harness::session::facade::Session;
use crate::harness::session::state::SessionState;
use crate::harness::session::types::{
    Clock, Entry, EntryBody, EntryQuery, FactItem, ForkOptions, IdGenerator, LanePointer, LaneRecord, LogItem,
    LogOptions, RecordBody, RecordQuery, SessionCreateOptions, SessionError, SessionErrorCode, SessionMetadata,
    SessionMutation, SessionStats, SessionStorage, StorageBranchQuery, UuidV7IdGenerator, system_clock,
};

/// In-memory session storage.
pub struct InMemorySessionStorage {
    metadata: SessionMetadata,
    state: Mutex<SessionState>,
    clock: Clock,
}

impl InMemorySessionStorage {
    pub fn new(metadata: SessionMetadata) -> Self {
        Self::with_clock(metadata, system_clock())
    }

    pub fn with_clock(metadata: SessionMetadata, clock: Clock) -> Self {
        Self {
            metadata,
            state: Mutex::new(SessionState::new()),
            clock,
        }
    }

    /// Create a fork storage by replaying fork mutations into fresh state.
    pub fn fork(&self, metadata: SessionMetadata, options: &ForkOptions) -> Result<Self, SessionError> {
        let storage = Self::with_clock(metadata, Arc::clone(&self.clock));
        let mutations = self
            .state
            .try_lock()
            .expect("in-memory state is not contended during fork")
            .create_fork_mutations(options)?;
        {
            let mut state = storage
                .state
                .try_lock()
                .expect("fresh in-memory state is not contended");
            for mutation in mutations {
                state.apply_mutation(&mutation)?;
            }
        }
        Ok(storage)
    }
}

impl SessionStorage<SessionMetadata> for InMemorySessionStorage {
    fn get_metadata<'a>(&'a self) -> BoxFuture<'a, Result<SessionMetadata, SessionError>> {
        Box::pin(async move { Ok(self.metadata.clone()) })
    }

    fn get_lanes<'a>(&'a self) -> BoxFuture<'a, Result<Vec<LanePointer>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.get_lanes()) })
    }

    fn create_lane<'a>(&'a self, lane: &'a str, at: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            state.validate_new_lane(lane)?;
            state.validate_target(at)?;
            let mutation = SessionMutation::Lane {
                seq: state.next_sequence(),
                lane: lane.to_owned(),
                leaf_id: at.map(str::to_owned),
            };
            state.apply_mutation(&mutation)
        })
    }

    fn move_lane<'a>(&'a self, lane: &'a str, to: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            state.require_lane(lane)?;
            state.validate_target(to)?;
            let mutation = SessionMutation::Lane {
                seq: state.next_sequence(),
                lane: lane.to_owned(),
                leaf_id: to.map(str::to_owned),
            };
            state.apply_mutation(&mutation)
        })
    }

    fn append_entry<'a>(&'a self, entry: EntryBody, lane: &'a str) -> BoxFuture<'a, Result<Entry, SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let parent_id = state.require_lane(lane)?;
            state.validate_unused_id(entry.id())?;
            let entry = Entry {
                body: entry,
                parent_id,
                seq: state.next_sequence(),
                timestamp: (self.clock)(),
            };
            state.apply_mutation(&SessionMutation::Entry {
                lane: Some(lane.to_owned()),
                entry: entry.clone(),
            })?;
            Ok(entry)
        })
    }

    fn append_record<'a>(&'a self, record: RecordBody) -> BoxFuture<'a, Result<LaneRecord, SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            state.require_lane(record.lane())?;
            state.validate_unused_id(record.id())?;
            let open_operation_id = state
                .find_open_operations(record.lane(), Some(1))?
                .first()
                .map(|record| record.id().to_owned());
            if record.record_type() == crate::harness::session::types::RecordType::OperationStarted
                && let Some(open_id) = open_operation_id
            {
                return Err(SessionError::new(
                    SessionErrorCode::Storage,
                    format!("Lane {} already has an open operation {open_id}", record.lane()),
                ));
            }
            let record = LaneRecord {
                body: record,
                seq: state.next_sequence(),
                timestamp: (self.clock)(),
            };
            state.apply_mutation(&SessionMutation::Record { record: record.clone() })?;
            Ok(record)
        })
    }

    fn get_entry<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Entry>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.get_entry(id).cloned()) })
    }

    fn find_entries<'a>(&'a self, query: EntryQuery) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move { self.state.lock().await.find_entries(&query) })
    }

    fn find_entries_on_branch<'a>(
        &'a self,
        query: StorageBranchQuery,
    ) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move { self.state.lock().await.find_entries_on_branch(&query) })
    }

    fn find_records<'a>(&'a self, query: RecordQuery) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>> {
        Box::pin(async move { self.state.lock().await.find_records(&query) })
    }

    fn find_open_operations<'a>(
        &'a self,
        lane: &'a str,
        limit: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>> {
        Box::pin(async move { self.state.lock().await.find_open_operations(lane, limit) })
    }

    fn get_log<'a>(&'a self, options: LogOptions) -> BoxFuture<'a, Result<Vec<LogItem>, SessionError>> {
        Box::pin(async move { self.state.lock().await.get_log(&options) })
    }

    fn get_name<'a>(&'a self) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.get_name()) })
    }

    fn set_name<'a>(&'a self, name: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let mutation = SessionMutation::Fact {
                seq: state.next_sequence(),
                fact: FactItem::Name {
                    name: name.map(str::to_owned),
                },
            };
            state.apply_mutation(&mutation)
        })
    }

    fn get_label<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.get_label(id)) })
    }

    fn set_label<'a>(&'a self, id: &'a str, label: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            state.validate_target(Some(id))?;
            let mutation = SessionMutation::Fact {
                seq: state.next_sequence(),
                fact: FactItem::Label {
                    target_id: id.to_owned(),
                    label: label.map(str::to_owned),
                },
            };
            state.apply_mutation(&mutation)
        })
    }

    fn get_stats<'a>(&'a self) -> BoxFuture<'a, Result<SessionStats, SessionError>> {
        Box::pin(async move { Ok(self.state.lock().await.get_stats()) })
    }
}

/// In-memory session repository.
pub struct InMemorySessionRepo {
    sessions: Mutex<IndexMap<String, Arc<InMemorySessionStorage>>>,
    clock: Clock,
    id_generator: Arc<dyn IdGenerator>,
}

impl Default for InMemorySessionRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemorySessionRepo {
    pub fn new() -> Self {
        Self::with_clock_and_id_generator(system_clock(), Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock(clock: Clock) -> Self {
        Self::with_clock_and_id_generator(clock, Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock_and_id_generator(clock: Clock, id_generator: Arc<dyn IdGenerator>) -> Self {
        Self {
            sessions: Mutex::new(IndexMap::new()),
            clock,
            id_generator,
        }
    }

    pub async fn create(&self, options: SessionCreateOptions) -> Result<Session<SessionMetadata>, SessionError> {
        let id = options.id.unwrap_or_else(|| self.id_generator.next());
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(&id) {
            return Err(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Session already exists: {id}"),
            ));
        }
        let storage = Arc::new(InMemorySessionStorage::with_clock(
            SessionMetadata {
                id: id.clone(),
                created_at: (self.clock)(),
                parent_session_id: options.parent_session_id,
            },
            Arc::clone(&self.clock),
        ));
        sessions.insert(id, Arc::clone(&storage));
        Ok(Session::new(storage))
    }

    pub async fn open(&self, metadata: &SessionMetadata) -> Result<Session<SessionMetadata>, SessionError> {
        Ok(Session::new(self.require_storage(&metadata.id).await?))
    }

    pub async fn list(&self) -> Result<Vec<SessionMetadata>, SessionError> {
        let sessions: Vec<_> = self.sessions.lock().await.values().cloned().collect();
        let mut metadata = Vec::with_capacity(sessions.len());
        for storage in sessions {
            metadata.push(storage.get_metadata().await?);
        }
        Ok(metadata)
    }

    pub async fn delete(&self, metadata: &SessionMetadata) -> Result<(), SessionError> {
        self.sessions.lock().await.shift_remove(&metadata.id);
        Ok(())
    }

    pub async fn fork(
        &self,
        source: &SessionMetadata,
        options: ForkOptions,
        create_options: SessionCreateOptions,
    ) -> Result<Session<SessionMetadata>, SessionError> {
        let source_storage = self.require_storage(&source.id).await?;
        let id = create_options.id.unwrap_or_else(|| self.id_generator.next());
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(&id) {
            return Err(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Session already exists: {id}"),
            ));
        }
        let parent_session_id = create_options.parent_session_id.or_else(|| Some(source.id.clone()));
        let storage = Arc::new(source_storage.fork(
            SessionMetadata {
                id: id.clone(),
                created_at: (self.clock)(),
                parent_session_id,
            },
            &options,
        )?);
        sessions.insert(id, Arc::clone(&storage));
        Ok(Session::new(storage))
    }

    async fn require_storage(&self, id: &str) -> Result<Arc<InMemorySessionStorage>, SessionError> {
        match self.sessions.lock().await.get(id) {
            Some(storage) => Ok(Arc::clone(storage)),
            None => Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Session not found: {id}"),
            )),
        }
    }
}
