//! Port of `packages/agent/src/harness/session/session.ts`.

use std::sync::Arc;

use pi_ai::types::Usage;
use pi_js_compat::js_json::JsValue as Value;

use crate::agent::AgentMessage;
use crate::harness::session::state::{assert_valid_cursor, assert_valid_limit};
use crate::harness::session::types::{
    BranchQuery, Entry, EntryBody, EntryQuery, IdGenerator, LanePointer, LaneRecord, LogItem, LogOptions,
    OptionalValue, RecordBody, RecordQuery, SessionError, SessionErrorCode, SessionMetadataValue, SessionStats,
    SessionStorage, StorageBranchQuery, UuidV7IdGenerator,
};

/// Optional metadata recorded with a compaction entry.
#[derive(Debug, Clone, Default)]
pub struct CompactionAppendOptions {
    pub duration_ms: Option<f64>,
    pub details: OptionalValue,
    pub usage: Option<Usage>,
}

/// Session facade over a storage backend.
#[derive(Clone)]
pub struct Session<M: SessionMetadataValue> {
    storage: Arc<dyn SessionStorage<M>>,
    id_generator: Arc<dyn IdGenerator>,
}

impl<M: SessionMetadataValue> std::fmt::Debug for Session<M> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Session")
    }
}

impl<M: SessionMetadataValue> Session<M> {
    pub fn new(storage: Arc<dyn SessionStorage<M>>) -> Self {
        Self {
            storage,
            id_generator: Arc::new(UuidV7IdGenerator),
        }
    }

    pub fn with_id_generator(storage: Arc<dyn SessionStorage<M>>, id_generator: Arc<dyn IdGenerator>) -> Self {
        Self { storage, id_generator }
    }

    pub fn id_generator(&self) -> &Arc<dyn IdGenerator> {
        &self.id_generator
    }

    pub async fn get_metadata(&self) -> Result<M, SessionError> {
        self.storage.get_metadata().await
    }

    /// Bind a lane view. `main` views behave like the session itself.
    pub fn view(&self, lane: &str) -> SessionView<M> {
        SessionView {
            session: self.clone(),
            lane: lane.to_owned(),
        }
    }

    pub async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        self.get_leaf_id_for_lane("main").await
    }

    pub async fn get_entry(&self, id: &str) -> Result<Option<Entry>, SessionError> {
        self.storage.get_entry(id).await
    }

    pub async fn get_stats(&self) -> Result<SessionStats, SessionError> {
        self.storage.get_stats().await
    }

    pub async fn get_name(&self) -> Result<Option<String>, SessionError> {
        self.storage.get_name().await
    }

    pub async fn set_name(&self, name: Option<&str>) -> Result<(), SessionError> {
        self.storage.set_name(name).await
    }

    pub async fn get_label(&self, target_id: &str) -> Result<Option<String>, SessionError> {
        self.storage.get_label(target_id).await
    }

    pub async fn set_label(&self, target_id: &str, label: Option<&str>) -> Result<(), SessionError> {
        self.storage.set_label(target_id, label).await
    }

    pub async fn find_entries(&self, query: EntryQuery) -> Result<Vec<Entry>, SessionError> {
        self.query_entries(query, None).await
    }

    pub async fn find_entry(&self, query: EntryQuery) -> Result<Option<Entry>, SessionError> {
        Ok(self.query_entries(query, Some(1)).await?.into_iter().next())
    }

    pub async fn find_entries_on_branch(&self, query: BranchQuery) -> Result<Vec<Entry>, SessionError> {
        self.query_branch_entries("main", query, None).await
    }

    pub async fn find_entry_on_branch(&self, query: BranchQuery) -> Result<Option<Entry>, SessionError> {
        Ok(self
            .query_branch_entries("main", query, Some(1))
            .await?
            .into_iter()
            .next())
    }

    pub async fn append_message(&self, message: AgentMessage) -> Result<String, SessionError> {
        self.append_message_to_lane("main", message).await
    }

    pub async fn append_compaction(
        &self,
        summary: &str,
        tokens_before: f64,
        retained_tail: Vec<AgentMessage>,
        options: CompactionAppendOptions,
    ) -> Result<String, SessionError> {
        self.append_compaction_to_lane("main", summary, tokens_before, retained_tail, options)
            .await
    }

    pub async fn append_custom_entry(&self, custom_type: &str, data: Option<Value>) -> Result<String, SessionError> {
        self.append_custom_entry_to_lane("main", custom_type, data).await
    }

    pub async fn get_lanes(&self) -> Result<Vec<LanePointer>, SessionError> {
        self.storage.get_lanes().await
    }

    pub async fn create_lane(&self, lane: &str, at: Option<&str>) -> Result<(), SessionError> {
        self.storage.create_lane(lane, at).await
    }

    pub async fn move_lane(&self, lane: &str, to: Option<&str>) -> Result<(), SessionError> {
        self.storage.move_lane(lane, to).await
    }

    pub async fn append_entry(&self, entry: EntryBody, lane: &str) -> Result<Entry, SessionError> {
        validate_entry_payload(&entry)?;
        self.storage.append_entry(entry, lane).await
    }

    pub async fn append_record(&self, record: RecordBody) -> Result<LaneRecord, SessionError> {
        validate_record_payload(&record)?;
        self.storage.append_record(record).await
    }

    pub async fn find_records(&self, query: RecordQuery) -> Result<Vec<LaneRecord>, SessionError> {
        assert_valid_limit(query.limit)?;
        assert_valid_cursor(query.after_seq)?;
        if query.operation_kind.is_some()
            && query.record_type != Some(crate::harness::session::types::RecordType::OperationStarted)
        {
            return Err(SessionError::new(
                SessionErrorCode::InvalidQuery,
                "operationKind requires type \"operation_started\"",
            ));
        }
        self.storage.find_records(query).await
    }

    pub async fn find_open_operations(
        &self,
        lane: &str,
        limit: Option<usize>,
    ) -> Result<Vec<LaneRecord>, SessionError> {
        assert_valid_limit(limit)?;
        self.storage.find_open_operations(lane, limit).await
    }

    pub async fn get_log(&self, options: LogOptions) -> Result<Vec<LogItem>, SessionError> {
        assert_valid_limit(options.limit)?;
        assert_valid_cursor(options.after_seq)?;
        self.storage.get_log(options).await
    }

    async fn get_leaf_id_for_lane(&self, lane: &str) -> Result<Option<String>, SessionError> {
        match self.get_lanes().await?.into_iter().find(|pointer| pointer.lane == lane) {
            Some(pointer) => Ok(pointer.leaf_id),
            None => Err(SessionError::new(
                SessionErrorCode::InvalidLane,
                format!("Lane not found: {lane}"),
            )),
        }
    }

    async fn query_entries(
        &self,
        mut query: EntryQuery,
        result_limit: Option<usize>,
    ) -> Result<Vec<Entry>, SessionError> {
        assert_valid_limit(query.limit)?;
        assert_valid_cursor(query.cursor.map(|cursor| cursor.after_seq))?;
        if let Some(limit) = result_limit {
            query.limit = Some(limit);
        }
        self.storage.find_entries(query).await
    }

    async fn query_branch_entries(
        &self,
        default_lane: &str,
        mut query: BranchQuery,
        result_limit: Option<usize>,
    ) -> Result<Vec<Entry>, SessionError> {
        assert_valid_limit(query.entry.limit)?;
        assert_valid_cursor(query.entry.cursor.map(|cursor| cursor.after_seq))?;
        let start = match query.bounds.start.take() {
            Some(start) => Some(start),
            None => self.get_leaf_id_for_lane(default_lane).await?,
        };
        let Some(start) = start else {
            return Ok(Vec::new());
        };
        if let Some(limit) = result_limit {
            query.entry.limit = Some(limit);
        }
        self.storage
            .find_entries_on_branch(StorageBranchQuery {
                entry: query.entry,
                start,
                stop_at_type: query.bounds.stop_at_type,
                stop_at_id: query.bounds.stop_at_id,
            })
            .await
    }

    async fn append_message_to_lane(&self, lane: &str, message: AgentMessage) -> Result<String, SessionError> {
        let entry = self
            .append_entry(
                EntryBody::Message {
                    id: self.id_generator.next(),
                    message,
                    terminate: None,
                },
                lane,
            )
            .await?;
        Ok(entry.id().to_owned())
    }

    async fn append_compaction_to_lane(
        &self,
        lane: &str,
        summary: &str,
        tokens_before: f64,
        retained_tail: Vec<AgentMessage>,
        options: CompactionAppendOptions,
    ) -> Result<String, SessionError> {
        let entry = self
            .append_entry(
                EntryBody::Compaction {
                    id: self.id_generator.next(),
                    summary: summary.to_owned().into(),
                    tokens_before,
                    retained_tail,
                    duration_ms: options.duration_ms,
                    details: options.details,
                    usage: options.usage,
                },
                lane,
            )
            .await?;
        Ok(entry.id().to_owned())
    }

    async fn append_custom_entry_to_lane(
        &self,
        lane: &str,
        custom_type: &str,
        data: Option<Value>,
    ) -> Result<String, SessionError> {
        let entry = self
            .append_entry(
                EntryBody::Custom {
                    id: self.id_generator.next(),
                    custom_type: custom_type.to_owned(),
                    data: data.into(),
                },
                lane,
            )
            .await?;
        Ok(entry.id().to_owned())
    }
}

/// Lane-bound view of a session.
pub struct SessionView<M: SessionMetadataValue> {
    session: Session<M>,
    lane: String,
}

impl<M: SessionMetadataValue> SessionView<M> {
    pub async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        self.session.get_leaf_id_for_lane(&self.lane).await
    }

    pub async fn get_entry(&self, id: &str) -> Result<Option<Entry>, SessionError> {
        self.session.get_entry(id).await
    }

    pub async fn get_stats(&self) -> Result<SessionStats, SessionError> {
        self.session.get_stats().await
    }

    pub async fn get_name(&self) -> Result<Option<String>, SessionError> {
        self.session.get_name().await
    }

    pub async fn set_name(&self, name: Option<&str>) -> Result<(), SessionError> {
        self.session.set_name(name).await
    }

    pub async fn get_label(&self, target_id: &str) -> Result<Option<String>, SessionError> {
        self.session.get_label(target_id).await
    }

    pub async fn set_label(&self, target_id: &str, label: Option<&str>) -> Result<(), SessionError> {
        self.session.set_label(target_id, label).await
    }

    pub async fn find_entries(&self, query: EntryQuery) -> Result<Vec<Entry>, SessionError> {
        self.session.query_entries(query, None).await
    }

    pub async fn find_entry(&self, query: EntryQuery) -> Result<Option<Entry>, SessionError> {
        Ok(self.session.query_entries(query, Some(1)).await?.into_iter().next())
    }

    pub async fn find_entries_on_branch(&self, query: BranchQuery) -> Result<Vec<Entry>, SessionError> {
        self.session.query_branch_entries(&self.lane, query, None).await
    }

    pub async fn find_entry_on_branch(&self, query: BranchQuery) -> Result<Option<Entry>, SessionError> {
        Ok(self
            .session
            .query_branch_entries(&self.lane, query, Some(1))
            .await?
            .into_iter()
            .next())
    }

    pub async fn append_message(&self, message: AgentMessage) -> Result<String, SessionError> {
        self.session.append_message_to_lane(&self.lane, message).await
    }

    pub async fn append_compaction(
        &self,
        summary: &str,
        tokens_before: f64,
        retained_tail: Vec<AgentMessage>,
        options: CompactionAppendOptions,
    ) -> Result<String, SessionError> {
        self.session
            .append_compaction_to_lane(&self.lane, summary, tokens_before, retained_tail, options)
            .await
    }

    pub async fn append_custom_entry(&self, custom_type: &str, data: Option<Value>) -> Result<String, SessionError> {
        self.session
            .append_custom_entry_to_lane(&self.lane, custom_type, data)
            .await
    }
}

fn validate_entry_payload(entry: &EntryBody) -> Result<(), SessionError> {
    match entry {
        EntryBody::Compaction {
            tokens_before,
            duration_ms,
            usage,
            ..
        } => {
            require_finite(*tokens_before)?;
            if let Some(duration_ms) = duration_ms {
                require_finite(*duration_ms)?;
            }
            if let Some(usage) = usage {
                validate_usage(usage)?;
            }
        }
        EntryBody::BranchSummary { usage: Some(usage), .. } => validate_usage(usage)?,
        _ => {}
    }
    Ok(())
}

fn validate_record_payload(record: &RecordBody) -> Result<(), SessionError> {
    match record {
        RecordBody::StepAttempt { attempt, .. } => require_finite(*attempt)?,
        RecordBody::ToolStarted { tool_index, .. } => require_finite(*tool_index)?,
        RecordBody::Usage { attempt, usage, .. } => {
            if let Some(attempt) = attempt {
                require_finite(*attempt)?;
            }
            validate_usage(usage)?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_usage(usage: &Usage) -> Result<(), SessionError> {
    for value in [
        usage.input,
        usage.output,
        usage.cache_read,
        usage.cache_write,
        usage.total_tokens,
        usage.cost.input,
        usage.cost.output,
        usage.cost.cache_read,
        usage.cost.cache_write,
        usage.cost.total,
    ] {
        require_finite(value)?;
    }
    if let Some(value) = usage.cache_write_1h {
        require_finite(value)?;
    }
    if let Some(value) = usage.reasoning {
        require_finite(value)?;
    }
    Ok(())
}

fn require_finite(value: f64) -> Result<(), SessionError> {
    if value.is_finite() {
        return Ok(());
    }
    Err(SessionError::new(
        SessionErrorCode::InvalidPayload,
        "Durable payload contains a non-finite number",
    ))
}
