//! Runtime mailbox and shared-channel prompt formatting (`core/runtime-coordination-format.ts`).

use crate::core::multi_agent_store::AgentFileReference;
use crate::core::session_control_db::{RuntimeMailboxMessage, SharedChannelMessage};

/// `formatRuntimeMailboxPrompt`.
pub fn format_runtime_mailbox_prompt(message: &RuntimeMailboxMessage, recipient_session_id: &str) -> String {
    let sender_lines = sender_lines(
        &message.sender.session_id,
        message.sender.agent_id.as_deref(),
        recipient_session_id,
    );
    let body = message_body(&message.body);
    let mut sections: Vec<String> = vec!["From:".to_owned()];
    sections.extend(sender_lines);
    sections.push(String::new());
    sections.push("Message:".to_owned());
    sections.push(body.to_owned());
    sections.extend(format_runtime_mailbox_file_references(message.file_refs.as_deref()));
    sections.join("\n")
}

/// `formatSharedChannelPrompt`.
pub fn format_shared_channel_prompt(messages: &[SharedChannelMessage], recipient_session_id: &str) -> String {
    messages
        .iter()
        .map(|message| format_shared_channel_message(message, recipient_session_id))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_shared_channel_message(message: &SharedChannelMessage, recipient_session_id: &str) -> String {
    let sender_lines = sender_lines(
        &message.sender.session_id,
        message.sender.agent_id.as_deref(),
        recipient_session_id,
    );
    let body = message_body(&message.body);
    let mut sections: Vec<String> = vec!["From shared channel:".to_owned()];
    sections.extend(sender_lines);
    sections.push(String::new());
    sections.push("Message:".to_owned());
    sections.push(body.to_owned());
    sections.join("\n")
}

fn sender_lines(sender_session_id: &str, sender_agent_id: Option<&str>, recipient_session_id: &str) -> Vec<String> {
    let sender_session = if sender_session_id.is_empty() {
        "unknown-session"
    } else {
        sender_session_id
    };
    let sender_agent = sender_agent_id
        .filter(|agent_id| !agent_id.is_empty())
        .unwrap_or("main");
    if sender_session == recipient_session_id {
        vec![format!("- agent: {sender_agent}")]
    } else {
        vec![
            format!("- session: {sender_session}"),
            format!("- agent: {sender_agent}"),
        ]
    }
}

fn message_body(body: &str) -> &str {
    let trimmed = pi_js_compat::string::js_trim(body);
    if trimmed.is_empty() {
        "No message body."
    } else {
        trimmed
    }
}

fn format_runtime_mailbox_file_references(file_refs: Option<&[AgentFileReference]>) -> Vec<String> {
    let mut lines = Vec::new();
    for file_ref in file_refs.unwrap_or_default() {
        lines.push(format_runtime_mailbox_file_reference(file_ref));
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let mut sections = vec!["Attached files:".to_owned()];
    sections.extend(lines);
    sections
}

fn format_runtime_mailbox_file_reference(file_ref: &AgentFileReference) -> String {
    let label = match file_ref.label.as_deref().filter(|label| !label.is_empty()) {
        Some(label) => format!("{label} — "),
        None => String::new(),
    };
    format!("- {label}{}", file_ref.path)
}
