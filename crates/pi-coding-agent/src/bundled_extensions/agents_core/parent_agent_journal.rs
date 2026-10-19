//! Parent agent journal (`extensions/agents-core/src/parent-agent-journal.ts`).

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError};
use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, MultiAgentStore};
use crate::core::session_manager::{FileEntry, SessionEntry, load_entries_from_file, lock_session_manager};

const AGENT_START_CUSTOM_TYPE: &str = "agent_start";
const AGENT_COMPLETE_CUSTOM_TYPE: &str = "agent_complete";
const MAIN_THREAD_AGENT_ID: &str = "main";

pub trait ParentAgentJournalWriter {
    fn append_entry(&self, custom_type: &str, data: Option<Value>) -> Result<(), HandlerError>;
}

impl ParentAgentJournalWriter for ExtensionApi {
    fn append_entry(&self, custom_type: &str, data: Option<Value>) -> Result<(), HandlerError> {
        ExtensionApi::append_entry(self, custom_type, data)
    }
}

/// Send-able writer used by event handlers that cannot capture [`ExtensionApi`].
#[derive(Clone)]
pub struct JournalWriter {
    append_entry: crate::core::extensions::types::AppendEntryHandler,
}

impl JournalWriter {
    pub fn from_api(pi: &ExtensionApi) -> Self {
        let runtime = pi.runtime.clone();
        Self {
            append_entry: Arc::new(move |custom_type, data| {
                runtime
                    .assert_active()
                    .map_err(crate::core::extensions::types::HandlerError::new)?;
                (runtime.actions().append_entry)(custom_type, data)
            }),
        }
    }

    pub fn from_handler(append_entry: crate::core::extensions::types::AppendEntryHandler) -> Self {
        Self { append_entry }
    }
}

impl ParentAgentJournalWriter for JournalWriter {
    fn append_entry(&self, custom_type: &str, data: Option<Value>) -> Result<(), HandlerError> {
        (self.append_entry)(custom_type, data)
    }
}

pub fn is_terminal_agent_lifecycle(lifecycle: AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    )
}

pub fn append_parent_agent_record(
    pi: &dyn ParentAgentJournalWriter,
    custom_type: &str,
    agent: &AgentSnapshot,
) -> Result<(), HandlerError> {
    let Some(transcript) = agent.transcript.as_ref() else {
        return Err(HandlerError::new(format!(
            "Cannot persist {custom_type} for {} without transcript identity",
            agent.id
        )));
    };
    let Some(transcript_path) = transcript.path.as_ref() else {
        return Err(HandlerError::new(format!(
            "Cannot persist {custom_type} for {} without transcript identity",
            agent.id
        )));
    };
    pi.append_entry(
        custom_type,
        Some(json!({
            "agentId": agent.id,
            "childSessionId": transcript.session_id,
            "lifecycle": lifecycle_name(agent.lifecycle),
            "transcriptPath": transcript_path,
        })),
    )
}

fn lifecycle_name(lifecycle: AgentLifecycle) -> &'static str {
    match lifecycle {
        AgentLifecycle::Running => "running",
        AgentLifecycle::WaitingForInput => "waiting_for_input",
        AgentLifecycle::SteeringPending => "steering_pending",
        AgentLifecycle::Cancelling => "cancelling",
        AgentLifecycle::Completed => "completed",
        AgentLifecycle::Failed => "failed",
        AgentLifecycle::Aborted => "aborted",
    }
}

pub fn append_parent_agent_start(pi: &dyn ParentAgentJournalWriter, agent: &AgentSnapshot) -> Result<(), HandlerError> {
    append_parent_agent_record(pi, AGENT_START_CUSTOM_TYPE, agent)
}

pub fn append_parent_agent_completion(
    pi: &dyn ParentAgentJournalWriter,
    agent: &AgentSnapshot,
) -> Result<(), HandlerError> {
    append_parent_agent_record(pi, AGENT_COMPLETE_CUSTOM_TYPE, agent)
}

pub fn read_active_parent_agent_ids_from_entries(entries: &[FileEntry]) -> HashSet<String> {
    let session_entries: Vec<SessionEntry> = entries.iter().filter_map(|entry| entry.entry().cloned()).collect();
    read_active_parent_agent_ids_from_session_entries(&session_entries)
}

pub fn read_active_parent_agent_ids_from_session_entries(entries: &[SessionEntry]) -> HashSet<String> {
    let mut started = HashSet::new();
    let mut completed = HashSet::new();
    for entry in entries {
        let SessionEntry::Known(crate::core::session_manager::KnownSessionEntry::Custom(custom)) = entry else {
            continue;
        };
        if custom.custom_type != AGENT_START_CUSTOM_TYPE && custom.custom_type != AGENT_COMPLETE_CUSTOM_TYPE {
            continue;
        }
        let Some(agent_id) = custom
            .data
            .as_ref()
            .and_then(|data| data.as_object())
            .and_then(|data| data.get("agentId"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if custom.custom_type == AGENT_START_CUSTOM_TYPE {
            started.insert(agent_id.to_owned());
        } else {
            completed.insert(agent_id.to_owned());
        }
    }
    for agent_id in completed {
        started.remove(&agent_id);
    }
    started
}

pub fn read_active_parent_agent_ids(ctx: &ExtensionContext) -> Result<HashSet<String>, HandlerError> {
    let session_manager = ctx.session_manager()?;
    let manager = lock_session_manager(&session_manager);
    Ok(read_active_parent_agent_ids_from_session_entries(
        &manager.get_entries(),
    ))
}

fn list_direct_active_agents(
    store: &MultiAgentStore,
    ctx: &ExtensionContext,
) -> Result<Vec<AgentSnapshot>, HandlerError> {
    let parent_id = ctx
        .multi_agent_agent_id()?
        .unwrap_or_else(|| MAIN_THREAD_AGENT_ID.to_owned());
    Ok(store
        .list_active_agents()
        .into_iter()
        .filter(|agent| agent.parent_id.as_deref().unwrap_or(MAIN_THREAD_AGENT_ID) == parent_id)
        .collect())
}

fn append_parent_agent_starts(pi: &dyn ParentAgentJournalWriter, agents: &[AgentSnapshot]) -> Result<(), HandlerError> {
    for agent in agents {
        append_parent_agent_start(pi, agent)?;
    }
    Ok(())
}

fn refresh_active_parent_agent_records_after_compaction(
    pi: &dyn ParentAgentJournalWriter,
    store: &MultiAgentStore,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    let journaled_agent_ids = read_active_parent_agent_ids(ctx)?;
    let active_journaled_agents: Vec<AgentSnapshot> = list_direct_active_agents(store, ctx)?
        .into_iter()
        .filter(|agent| journaled_agent_ids.contains(&agent.id))
        .collect();
    append_parent_agent_starts(pi, &active_journaled_agents)
}

pub fn register_parent_agent_compaction_refresh(pi: &ExtensionApi, store: Arc<MultiAgentStore>) -> Result<(), String> {
    let journal = Arc::new(JournalWriter::from_api(pi));
    pi.on_session_compact(move |_event, ctx| {
        let store = Arc::clone(&store);
        let journal = Arc::clone(&journal);
        let future: crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>> =
            Box::pin(async move {
                refresh_active_parent_agent_records_after_compaction(&*journal, &store, &ctx)?;
                Ok(None)
            });
        future
    })
}

pub fn restore_compacted_parent_agent_records(
    pi: &dyn ParentAgentJournalWriter,
    store: &MultiAgentStore,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    let journaled_agent_ids = read_active_parent_agent_ids(ctx)?;
    let missing_journal_agents: Vec<AgentSnapshot> = list_direct_active_agents(store, ctx)?
        .into_iter()
        .filter(|agent| !journaled_agent_ids.contains(&agent.id))
        .collect();
    if missing_journal_agents.is_empty() {
        return Ok(());
    }
    let session_file = {
        let session_manager = ctx.session_manager()?;
        let manager = lock_session_manager(&session_manager);
        let Some(session_file) = manager.get_session_file() else {
            return Ok(());
        };
        session_file.to_owned()
    };
    let persisted_agent_ids = read_active_parent_agent_ids_from_entries(&load_entries_from_file(&session_file));
    let persisted_missing_journal_agents: Vec<AgentSnapshot> = missing_journal_agents
        .into_iter()
        .filter(|agent| persisted_agent_ids.contains(&agent.id))
        .collect();
    append_parent_agent_starts(pi, &persisted_missing_journal_agents)
}

/// Data shape shared with the dispatch-runtime journal writer.
pub fn parent_agent_record_data(agent: &AgentSnapshot) -> Option<Map<String, Value>> {
    let transcript = agent.transcript.as_ref()?;
    let transcript_path = transcript.path.as_ref()?;
    let mut data = Map::new();
    data.insert("agentId".to_owned(), json!(agent.id));
    data.insert("childSessionId".to_owned(), json!(transcript.session_id));
    data.insert("lifecycle".to_owned(), json!(lifecycle_name(agent.lifecycle)));
    data.insert("transcriptPath".to_owned(), json!(transcript_path));
    Some(data)
}
