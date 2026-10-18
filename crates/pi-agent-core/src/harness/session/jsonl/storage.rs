//! Port of `packages/agent/src/harness/session/jsonl/storage.ts`.

use std::sync::Arc;

use pi_ai::request::BoxFuture;
use tokio::sync::Mutex;

use crate::harness::session::jsonl::codec::{
    encode_header, encode_mutation, metadata_from_header, parse_header, parse_mutation,
};
use crate::harness::session::jsonl::errors::{file_result, invalid_file, invalid_mutation_file};
use crate::harness::session::jsonl::types::{JsonlSessionMetadata, JsonlSessionRepoFileSystem, JsonlV4Header};
use crate::harness::session::state::SessionState;
use crate::harness::session::types::{
    Clock, Entry, EntryBody, EntryQuery, FactItem, ForkOptions, JsonlDecodeKind, LanePointer, LaneRecord, LogItem,
    LogOptions, RecordBody, RecordQuery, SessionError, SessionErrorCode, SessionMutation, SessionStats, SessionStorage,
    StorageBranchQuery, system_clock,
};

/// JSONL-backed per-session storage.
pub struct JsonlSessionStorage {
    fs: Arc<dyn JsonlSessionRepoFileSystem>,
    metadata: JsonlSessionMetadata,
    state: Mutex<SessionState>,
    clock: Clock,
}

impl JsonlSessionStorage {
    pub fn new(fs: Arc<dyn JsonlSessionRepoFileSystem>, metadata: JsonlSessionMetadata) -> Self {
        Self::with_clock(fs, metadata, system_clock())
    }

    pub fn with_clock(fs: Arc<dyn JsonlSessionRepoFileSystem>, metadata: JsonlSessionMetadata, clock: Clock) -> Self {
        Self {
            fs,
            metadata,
            state: Mutex::new(SessionState::new()),
            clock,
        }
    }

    /// Create a new session file from its header.
    pub async fn create(
        fs: Arc<dyn JsonlSessionRepoFileSystem>,
        path: &str,
        header: &JsonlV4Header,
        clock: Clock,
    ) -> Result<Self, SessionError> {
        file_result(
            fs.write_file(path, &encode_header(header)).await,
            &format!("Failed to initialize session {path}"),
        )?;
        let file_info = file_result(
            fs.file_info(path).await,
            &format!("Failed to read session metadata {path}"),
        )?;
        Ok(Self::with_clock(
            fs,
            metadata_from_header(header, path.to_owned(), file_info.mtime_ms),
            clock,
        ))
    }

    /// Load and replay a session file, repairing a torn final append.
    pub async fn load(fs: Arc<dyn JsonlSessionRepoFileSystem>, path: &str, clock: Clock) -> Result<Self, SessionError> {
        let content = file_result(fs.read_text_file(path).await, &format!("Failed to read session {path}"))?;
        let physical_lines = split_physical_lines(&content);
        let storage = load_storage_from_lines(Arc::clone(&fs), path, clock, &physical_lines).await?;
        if replay_mutations(&storage, path, &physical_lines).await? {
            return Ok(storage);
        }
        if !content.ends_with('\n') {
            file_result(
                fs.append_file(path, "\n").await,
                &format!("Failed to repair unterminated session tail {path}"),
            )?;
        }
        Ok(storage)
    }

    /// Fork this storage into `path`, copying entries, lanes, and facts.
    pub async fn fork(&self, path: &str, header: &JsonlV4Header, options: &ForkOptions) -> Result<Self, SessionError> {
        let mutations = self.state.lock().await.create_fork_mutations(options)?;
        let temp_path = format!("{path}.tmp");
        let populate_result = async {
            let target = Self::create(Arc::clone(&self.fs), &temp_path, header, Arc::clone(&self.clock)).await?;
            for mutation in &mutations {
                target.append_mutation(mutation).await?;
                target.state.lock().await.apply_mutation(mutation)?;
            }
            file_result(
                self.fs.rename_file(&temp_path, path).await,
                &format!("Failed to publish staged file {path}"),
            )
        }
        .await;
        if let Err(error) = populate_result {
            let _ = self.fs.remove(&temp_path, false, true).await;
            return Err(error);
        }
        Self::load(Arc::clone(&self.fs), path, Arc::clone(&self.clock)).await
    }

    /// Wait for queued writes to finish. Mutex-based serialization has no tail.
    pub async fn drain(&self) {}

    async fn append_mutation(&self, mutation: &SessionMutation) -> Result<(), SessionError> {
        let line = encode_mutation(mutation)?;
        file_result(
            self.fs.append_file(&self.metadata.path, &line).await,
            &format!("Failed to append session {}", self.metadata.path),
        )
    }
}

async fn publish_file_atomically(
    fs: &Arc<dyn JsonlSessionRepoFileSystem>,
    destination: &str,
    content: &str,
    label: &str,
) -> Result<(), SessionError> {
    let temp_path = format!("{destination}.tmp");
    let result = async {
        file_result(
            fs.write_file(&temp_path, content).await,
            &format!("Failed to stage {label} {destination}"),
        )?;
        file_result(
            fs.rename_file(&temp_path, destination).await,
            &format!("Failed to publish staged file {destination}"),
        )
    }
    .await;
    if result.is_err() {
        let _ = fs.remove(&temp_path, false, true).await;
    }
    result
}

fn split_physical_lines(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = content.split('\n').map(str::to_owned).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

async fn load_storage_from_lines(
    fs: Arc<dyn JsonlSessionRepoFileSystem>,
    path: &str,
    clock: Clock,
    physical_lines: &[String],
) -> Result<JsonlSessionStorage, SessionError> {
    let Some(first_line) = physical_lines.first().filter(|line| !line.is_empty()) else {
        return Err(invalid_file(
            path,
            1,
            &crate::harness::session::types::JsonlDecodeError::new(JsonlDecodeKind::Schema, "is missing a header"),
        ));
    };
    let header = parse_header(first_line).map_err(|error| invalid_file(path, 1, &error))?;
    let file_info = file_result(
        fs.file_info(path).await,
        &format!("Failed to read session metadata {path}"),
    )?;
    Ok(JsonlSessionStorage::with_clock(
        fs,
        metadata_from_header(&header, path.to_owned(), file_info.mtime_ms),
        clock,
    ))
}

async fn replay_mutations(
    storage: &JsonlSessionStorage,
    path: &str,
    physical_lines: &[String],
) -> Result<bool, SessionError> {
    for (index, line) in physical_lines.iter().enumerate().skip(1) {
        let mutation = match parse_mutation(line) {
            Ok(mutation) => mutation,
            Err(error) => {
                let is_torn_tail = index == physical_lines.len() - 1 && error.kind == JsonlDecodeKind::Syntax;
                if is_torn_tail {
                    let valid_prefix = format!("{}\n", physical_lines[..index].join("\n"));
                    publish_file_atomically(&storage.fs, path, &valid_prefix, "torn-tail repair").await?;
                    return Ok(true);
                }
                return Err(invalid_file(path, index + 1, &error));
            }
        };
        if let Err(error) = storage.state.lock().await.apply_mutation(&mutation) {
            if error.code() == SessionErrorCode::InvalidEntry {
                return Err(invalid_mutation_file(path, index + 1, &error));
            }
            return Err(error);
        }
    }
    Ok(false)
}

macro_rules! lock_state {
    ($self:ident) => {
        $self.state.lock().await
    };
}

impl SessionStorage<JsonlSessionMetadata> for JsonlSessionStorage {
    fn get_metadata<'a>(&'a self) -> BoxFuture<'a, Result<JsonlSessionMetadata, SessionError>> {
        Box::pin(async move { Ok(self.metadata.clone()) })
    }

    fn get_lanes<'a>(&'a self) -> BoxFuture<'a, Result<Vec<LanePointer>, SessionError>> {
        Box::pin(async move { Ok(lock_state!(self).get_lanes()) })
    }

    fn create_lane<'a>(&'a self, lane: &'a str, at: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = lock_state!(self);
            state.validate_new_lane(lane)?;
            state.validate_target(at)?;
            let mutation = SessionMutation::Lane {
                seq: state.next_sequence(),
                lane: lane.to_owned(),
                leaf_id: at.map(str::to_owned),
            };
            self.append_mutation(&mutation).await?;
            state.apply_mutation(&mutation)
        })
    }

    fn move_lane<'a>(&'a self, lane: &'a str, to: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = lock_state!(self);
            state.require_lane(lane)?;
            state.validate_target(to)?;
            let mutation = SessionMutation::Lane {
                seq: state.next_sequence(),
                lane: lane.to_owned(),
                leaf_id: to.map(str::to_owned),
            };
            self.append_mutation(&mutation).await?;
            state.apply_mutation(&mutation)
        })
    }

    fn append_entry<'a>(&'a self, entry: EntryBody, lane: &'a str) -> BoxFuture<'a, Result<Entry, SessionError>> {
        Box::pin(async move {
            let mut state = lock_state!(self);
            let parent_id = state.require_lane(lane)?;
            state.validate_unused_id(entry.id())?;
            let entry = Entry {
                body: entry,
                parent_id,
                seq: state.next_sequence(),
                timestamp: (self.clock)(),
            };
            let mutation = SessionMutation::Entry {
                lane: Some(lane.to_owned()),
                entry: entry.clone(),
            };
            self.append_mutation(&mutation).await?;
            state.apply_mutation(&mutation)?;
            Ok(entry)
        })
    }

    fn append_record<'a>(&'a self, record: RecordBody) -> BoxFuture<'a, Result<LaneRecord, SessionError>> {
        Box::pin(async move {
            let mut state = lock_state!(self);
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
            let mutation = SessionMutation::Record { record: record.clone() };
            self.append_mutation(&mutation).await?;
            state.apply_mutation(&mutation)?;
            Ok(record)
        })
    }

    fn get_entry<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Entry>, SessionError>> {
        Box::pin(async move { Ok(lock_state!(self).get_entry(id).cloned()) })
    }

    fn find_entries<'a>(&'a self, query: EntryQuery) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move { lock_state!(self).find_entries(&query) })
    }

    fn find_entries_on_branch<'a>(
        &'a self,
        query: StorageBranchQuery,
    ) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move { lock_state!(self).find_entries_on_branch(&query) })
    }

    fn find_records<'a>(&'a self, query: RecordQuery) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>> {
        Box::pin(async move { lock_state!(self).find_records(&query) })
    }

    fn find_open_operations<'a>(
        &'a self,
        lane: &'a str,
        limit: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>> {
        Box::pin(async move { lock_state!(self).find_open_operations(lane, limit) })
    }

    fn get_log<'a>(&'a self, options: LogOptions) -> BoxFuture<'a, Result<Vec<LogItem>, SessionError>> {
        Box::pin(async move { lock_state!(self).get_log(&options) })
    }

    fn get_name<'a>(&'a self) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move { Ok(lock_state!(self).get_name()) })
    }

    fn set_name<'a>(&'a self, name: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = lock_state!(self);
            let mutation = SessionMutation::Fact {
                seq: state.next_sequence(),
                fact: FactItem::Name {
                    name: name.map(str::to_owned),
                },
            };
            self.append_mutation(&mutation).await?;
            state.apply_mutation(&mutation)
        })
    }

    fn get_label<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move { Ok(lock_state!(self).get_label(id)) })
    }

    fn set_label<'a>(&'a self, id: &'a str, label: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let mut state = lock_state!(self);
            state.validate_target(Some(id))?;
            let mutation = SessionMutation::Fact {
                seq: state.next_sequence(),
                fact: FactItem::Label {
                    target_id: id.to_owned(),
                    label: label.map(str::to_owned),
                },
            };
            self.append_mutation(&mutation).await?;
            state.apply_mutation(&mutation)
        })
    }

    fn get_stats<'a>(&'a self) -> BoxFuture<'a, Result<SessionStats, SessionError>> {
        Box::pin(async move { Ok(lock_state!(self).get_stats()) })
    }
}
