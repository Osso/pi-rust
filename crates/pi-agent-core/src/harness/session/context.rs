//! Frozen session/session.ts:21–85 context construction over a root-to-leaf path.

use super::types::{KnownSessionEntry, SessionError, SessionErrorCode, SessionTreeEntry};
use crate::agent::AgentMessage;
use crate::harness::messages::{
    CompactionSummaryOptions, create_branch_summary_message, create_compaction_summary_message, create_custom_message,
};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionModel {
    pub provider: JsString,
    pub model_id: JsString,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SessionContext {
    pub messages: Vec<AgentMessage>,
    pub thinking_level: JsString,
    pub model: Option<SessionModel>,
    pub active_tool_names: Option<Vec<JsString>>,
}

pub fn build_session_context(path_entries: &[SessionTreeEntry]) -> Result<SessionContext, SessionError> {
    let mut context = SessionContext {
        messages: Vec::new(),
        thinking_level: "off".into(),
        model: None,
        active_tool_names: None,
    };
    let mut compaction = None;
    for entry in path_entries {
        match entry.known() {
            Some(KnownSessionEntry::ThinkingLevelChange { thinking_level, .. }) => {
                context.thinking_level = thinking_level.clone()
            }
            Some(KnownSessionEntry::ModelChange { provider, model_id, .. }) => {
                context.model = Some(SessionModel {
                    provider: provider.clone(),
                    model_id: model_id.clone(),
                })
            }
            Some(KnownSessionEntry::Message {
                message: AgentMessage::Assistant(message),
                ..
            }) => {
                context.model = Some(SessionModel {
                    provider: message.provider.clone().into(),
                    model_id: message.model.clone().into(),
                })
            }
            Some(KnownSessionEntry::ActiveToolsChange { active_tool_names, .. }) => {
                context.active_tool_names = Some(active_tool_names.clone())
            }
            Some(KnownSessionEntry::Compaction { .. }) => compaction = Some(entry),
            _ => {}
        }
    }
    if let Some(compaction) = compaction {
        append_compacted_context(&mut context.messages, path_entries, compaction)?;
    } else {
        for entry in path_entries {
            append_context_message(&mut context.messages, entry)?;
        }
    }
    Ok(context)
}

fn append_compacted_context(
    messages: &mut Vec<AgentMessage>,
    entries: &[SessionTreeEntry],
    compaction: &SessionTreeEntry,
) -> Result<(), SessionError> {
    let Some(KnownSessionEntry::Compaction {
        summary,
        tokens_before,
        duration_ms,
        first_kept_entry_id,
        ..
    }) = compaction.known()
    else {
        unreachable!("compaction selected by known projection");
    };
    messages.push(create_compaction_summary_message(
        summary,
        *tokens_before,
        compaction.timestamp(),
        &CompactionSummaryOptions {
            duration_ms: *duration_ms,
            ..Default::default()
        },
    ));
    // DISCREPANCY preserved: findIndex picks the first matching ID even if the selected compaction is later.
    let index = entries
        .iter()
        .position(|entry| entry.kind() == "compaction" && entry.id() == compaction.id())
        .expect("selected compaction belongs to path");
    let mut found_first_kept = false;
    for entry in &entries[..index] {
        if entry.id() == first_kept_entry_id {
            found_first_kept = true;
        }
        if found_first_kept {
            append_context_message(messages, entry)?;
        }
    }
    for entry in &entries[index + 1..] {
        append_context_message(messages, entry)?;
    }
    Ok(())
}

fn append_context_message(messages: &mut Vec<AgentMessage>, entry: &SessionTreeEntry) -> Result<(), SessionError> {
    match entry.known() {
        Some(KnownSessionEntry::Message { message, .. }) => messages.push(message.clone()),
        Some(KnownSessionEntry::CustomMessage {
            custom_type,
            content,
            display,
            details,
            ..
        }) => messages.push(create_custom_message(
            custom_type,
            content.clone(),
            *display,
            details.clone(),
            entry.timestamp(),
        )),
        Some(KnownSessionEntry::BranchSummary { summary, from_id, .. }) if !summary.is_empty() => {
            messages.push(create_branch_summary_message(summary, from_id, entry.timestamp()))
        }
        None if entry.kind() == "message" || entry.kind() == "custom_message" => {
            return Err(SessionError::new(
                SessionErrorCode::InvalidEntry,
                js_format!("Entry {} has an invalid message payload", entry.id()),
            ));
        }
        _ => {}
    }
    Ok(())
}
