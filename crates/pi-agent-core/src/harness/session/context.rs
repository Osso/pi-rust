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

fn derive_session_context_state(path_entries: &[Entry]) -> (String, Option<SessionModel>, Option<Vec<String>>) {
    let mut thinking_level = "off".to_owned();
    let mut model = None;
    let mut active_tool_names = None;

    for entry in path_entries {
        match &entry.body {
            crate::harness::session::types::EntryBody::ThinkingLevelChange {
                thinking_level: level, ..
            } => {
                thinking_level = level.clone();
            }
            crate::harness::session::types::EntryBody::ModelChange { provider, model_id, .. } => {
                model = Some(SessionModel {
                    provider: provider.clone(),
                    model_id: model_id.clone(),
                });
            }
            crate::harness::session::types::EntryBody::Message { message, .. } => {
                if let Some(assistant) = message.as_assistant() {
                    model = Some(SessionModel {
                        provider: assistant.provider.clone(),
                        model_id: assistant.model.clone(),
                    });
                }
            }
            crate::harness::session::types::EntryBody::ActiveToolsChange {
                active_tool_names: names,
                ..
            } => {
                active_tool_names = Some(names.clone());
            }
            _ => {}
        }
    }

    (thinking_level, model, active_tool_names)
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
        crate::harness::session::types::EntryBody::Message { message, .. } => {
            if message
                .as_assistant()
                .is_some_and(|assistant| assistant.stop_reason == pi_ai::types::StopReason::Deferred)
            {
                return Vec::new();
            }
            vec![message.clone()]
        }
        crate::harness::session::types::EntryBody::Compaction {
            summary,
            tokens_before,
            retained_tail,
            ..
        } => {
            let mut messages = vec![create_compaction_summary_message(
                summary,
                *tokens_before,
                entry.timestamp,
                &CompactionSummaryOptions::default(),
            )];
            messages.extend(retained_tail.iter().cloned());
            messages
        }
        crate::harness::session::types::EntryBody::BranchSummary { summary, from_id, .. } => {
            if summary.is_empty() {
                return Vec::new();
            }
            vec![create_branch_summary_message(summary, from_id, entry.timestamp)]
        }
        crate::harness::session::types::EntryBody::Custom { custom_type, .. } => options
            .entry_projectors
            .get(custom_type)
            .and_then(|projector| projector(entry, index, entries))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Build the full session context from a branch path.
pub fn build_session_context(path_entries: &[Entry], options: &SessionContextBuildOptions) -> SessionContext {
    let (thinking_level, model, active_tool_names) = derive_session_context_state(path_entries);
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
        thinking_level,
        model,
        active_tool_names,
    }
}
