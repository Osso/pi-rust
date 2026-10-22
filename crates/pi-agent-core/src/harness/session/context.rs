//! Port of `packages/agent/src/harness/session/context.ts`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agent::AgentMessage;
use crate::harness::messages::{
    CompactionSummaryOptions, create_branch_summary_message, create_compaction_summary_message,
};
use crate::harness::session::types::{Entry, EntryType};

/// Model identity derived from the branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModel {
    pub provider: String,
    pub model_id: String,
}

/// Materialized session context.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionContext {
    pub messages: Vec<AgentMessage>,
    pub thinking_level: String,
    pub model: Option<SessionModel>,
    pub active_tool_names: Option<Vec<String>>,
}

/// Caller transform applied after the compaction boundary.
pub type ContextEntryTransform = Arc<dyn Fn(&[Entry]) -> Vec<Entry> + Send + Sync>;

/// Projector for one custom entry type.
pub type CustomEntryContextMessageProjector =
    Arc<dyn Fn(&Entry, usize, &[Entry]) -> Option<Vec<AgentMessage>> + Send + Sync>;

/// Context build options.
#[derive(Default, Clone)]
pub struct SessionContextBuildOptions {
    pub entry_transforms: Vec<ContextEntryTransform>,
    pub entry_projectors: HashMap<String, CustomEntryContextMessageProjector>,
}

struct ContextState {
    thinking_level: String,
    model: Option<SessionModel>,
    active_tool_names: Option<Vec<String>>,
}

impl Default for ContextState {
    fn default() -> Self {
        Self {
            thinking_level: "off".to_owned(),
            model: None,
            active_tool_names: None,
        }
    }
}

fn derive_session_context_state(path_entries: &[Entry]) -> ContextState {
    let mut state = ContextState::default();
    for entry in path_entries {
        apply_context_state(&mut state, entry);
    }
    state
}

fn apply_context_state(state: &mut ContextState, entry: &Entry) {
    match &entry.body {
        crate::harness::session::types::EntryBody::ThinkingLevelChange { thinking_level, .. } => {
            state.thinking_level = thinking_level.clone();
        }
        crate::harness::session::types::EntryBody::ModelChange { provider, model_id, .. } => {
            state.model = Some(SessionModel {
                provider: provider.clone(),
                model_id: model_id.clone(),
            });
        }
        crate::harness::session::types::EntryBody::Message { message, .. } => {
            if let Some(assistant) = message.as_assistant() {
                state.model = Some(SessionModel {
                    provider: assistant.provider.clone(),
                    model_id: assistant.model.clone(),
                });
            }
        }
        crate::harness::session::types::EntryBody::ActiveToolsChange { active_tool_names, .. } => {
            state.active_tool_names = Some(active_tool_names.clone());
        }
        _ => {}
    }
}

/// Keep the latest compaction and everything after it.
pub fn default_context_entry_transform(path_entries: &[Entry]) -> Vec<Entry> {
    let compaction_index = path_entries
        .iter()
        .rposition(|entry| entry.entry_type() == EntryType::Compaction);
    match compaction_index {
        None => path_entries.to_vec(),
        Some(index) => {
            let mut entries = Vec::with_capacity(path_entries.len() - index);
            entries.push(path_entries[index].clone());
            entries.extend_from_slice(&path_entries[index + 1..]);
            entries
        }
    }
}

/// Apply the default transform and every caller transform in order.
pub fn build_context_entries(path_entries: &[Entry], options: &SessionContextBuildOptions) -> Vec<Entry> {
    let mut entries = default_context_entry_transform(path_entries);
    for transform in &options.entry_transforms {
        entries = transform(&entries);
    }
    entries
}

/// Project one entry to context messages.
pub fn session_entry_to_context_messages(
    entry: &Entry,
    index: usize,
    entries: &[Entry],
    options: &SessionContextBuildOptions,
) -> Vec<AgentMessage> {
    match &entry.body {
        crate::harness::session::types::EntryBody::Message { message, .. } => message_entry_messages(message),
        crate::harness::session::types::EntryBody::Compaction {
            summary,
            tokens_before,
            retained_tail,
            ..
        } => compaction_messages(summary, *tokens_before, entry.timestamp, retained_tail),
        crate::harness::session::types::EntryBody::BranchSummary { summary, from_id, .. } => {
            branch_summary_messages(summary, from_id, entry.timestamp)
        }
        crate::harness::session::types::EntryBody::Custom { custom_type, .. } => {
            custom_entry_messages(entry, custom_type, index, entries, options)
        }
        _ => Vec::new(),
    }
}

fn message_entry_messages(message: &AgentMessage) -> Vec<AgentMessage> {
    if message
        .as_assistant()
        .is_some_and(|assistant| assistant.stop_reason == pi_ai::types::StopReason::Deferred)
    {
        return Vec::new();
    }
    vec![message.clone()]
}

fn compaction_messages(
    summary: &pi_js_compat::js_json::JsString,
    tokens_before: f64,
    timestamp: i64,
    retained_tail: &[AgentMessage],
) -> Vec<AgentMessage> {
    let mut messages = vec![create_compaction_summary_message(
        summary,
        tokens_before,
        timestamp,
        &CompactionSummaryOptions::default(),
    )];
    messages.extend(retained_tail.iter().cloned());
    messages
}

fn branch_summary_messages(
    summary: &pi_js_compat::js_json::JsString,
    from_id: &str,
    timestamp: i64,
) -> Vec<AgentMessage> {
    if summary.is_empty() {
        return Vec::new();
    }
    vec![create_branch_summary_message(
        summary,
        &JsString::from(from_id),
        timestamp,
    )]
}

fn custom_entry_messages(
    entry: &Entry,
    custom_type: &str,
    index: usize,
    entries: &[Entry],
    options: &SessionContextBuildOptions,
) -> Vec<AgentMessage> {
    options
        .entry_projectors
        .get(custom_type)
        .and_then(|projector| projector(entry, index, entries))
        .unwrap_or_default()
}

/// Build the full session context from a branch path.
pub fn build_session_context(path_entries: &[Entry], options: &SessionContextBuildOptions) -> SessionContext {
    let state = derive_session_context_state(path_entries);
    let context_entries = build_context_entries(path_entries, options);
    let mut messages = Vec::new();
    for (index, entry) in context_entries.iter().enumerate() {
        messages.extend(session_entry_to_context_messages(
            entry,
            index,
            &context_entries,
            options,
        ));
    }
    SessionContext {
        messages,
        thinking_level: state.thinking_level,
        model: state.model,
        active_tool_names: state.active_tool_names,
    }
}
