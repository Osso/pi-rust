//! Durable recipient acceptance (`runtime-mailbox-acceptance.ts` at the frozen pin).

use std::collections::VecDeque;

use pi_agent_core::AgentMessage;
use pi_js_compat::js_json::JsValue;
use serde::Deserialize;
use serde_json::json;

use super::runtime_mailbox_transcript::{
    RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE, mailbox_store_refs_equal, read_accepted_runtime_mailbox_store_refs,
};
use super::session_control_db::{
    ControlDbError, RuntimeMailboxMessage, RuntimeMailboxStoreRef, deliver_runtime_mailbox_message,
    read_runtime_mailbox_message_for_delivery, release_runtime_mailbox_message_claim,
};
use super::session_manager::SessionManager;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxAcceptanceRef {
    pub id: i64,
    pub store_ref: RuntimeMailboxStoreRef,
}

impl From<&RuntimeMailboxMessage> for RuntimeMailboxAcceptanceRef {
    fn from(message: &RuntimeMailboxMessage) -> Self {
        Self {
            id: message.id,
            store_ref: message.store_ref.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxToolDelivery {
    pub control_db_path: String,
    pub messages: Vec<RuntimeMailboxAcceptanceRef>,
}

/// Rust events clone messages, so JS WeakSet identity cannot survive dispatch.
/// Keep accepted clones until their first fully equal MessageEnd. Equal messages
/// serialize identically; consuming either identical clone produces the same bytes.
/// Session disposal drops any queued messages that never reached MessageEnd.
#[derive(Default)]
pub struct RuntimeMailboxAcceptance {
    persisted_messages: VecDeque<AgentMessage>,
}

impl RuntimeMailboxAcceptance {
    pub fn consume_persisted_message(&mut self, message: &AgentMessage) -> bool {
        let Some(index) = self.persisted_messages.iter().position(|accepted| accepted == message) else {
            return false;
        };
        self.persisted_messages.remove(index);
        true
    }

    pub fn clear(&mut self) {
        self.persisted_messages.clear();
    }

    pub fn accept_messages(
        &mut self,
        session: &mut SessionManager,
        control_db_path: &str,
        messages: &[RuntimeMailboxAcceptanceRef],
        accepted_message: Option<&AgentMessage>,
    ) -> Result<(), ControlDbError> {
        if messages.is_empty() {
            return Ok(());
        }
        if !session.is_persisted() || session.get_session_file().is_none() {
            return Err(ControlDbError::new(
                "Mailbox acceptance requires a persisted recipient session",
            ));
        }
        session.persist_for_recovery().map_err(control_error)?;
        let refs = messages
            .iter()
            .map(|message| message.store_ref.clone())
            .collect::<Vec<_>>();
        if let Some(message) = accepted_message {
            if !matches!(message, AgentMessage::User(_) | AgentMessage::ToolResult(_)) {
                return Err(ControlDbError::new("Invalid mailbox acceptance message"));
            }
            session
                .append_message_with_mailbox_store_refs(message.clone(), Some(&refs))
                .map_err(control_error)?;
            self.persisted_messages.push_back(message.clone());
        } else {
            session
                .append_custom_entry(
                    RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE,
                    Some(json!({"mailboxStoreRefs": refs})),
                )
                .map_err(control_error)?;
        }
        session.fsync().map_err(control_error)?;
        delete_accepted_claims(control_db_path, messages)
    }
}

fn control_error(error: impl std::fmt::Display) -> ControlDbError {
    ControlDbError::new(error.to_string())
}

fn delete_accepted_claims(
    control_db_path: &str,
    messages: &[RuntimeMailboxAcceptanceRef],
) -> Result<(), ControlDbError> {
    for message in messages {
        let Some(snapshot) = read_runtime_mailbox_message_for_delivery(control_db_path, message.id)? else {
            continue;
        };
        if snapshot.payload_valid && snapshot.message.store_ref == message.store_ref {
            deliver_runtime_mailbox_message(control_db_path, message.id, snapshot.payload_data.as_deref())?;
        }
    }
    Ok(())
}

pub fn release_unaccepted_runtime_mailbox_messages(
    session: &SessionManager,
    control_db_path: &str,
    messages: &[RuntimeMailboxAcceptanceRef],
) -> Result<(), ControlDbError> {
    if messages.is_empty() {
        return Ok(());
    }
    let accepted = read_accepted_runtime_mailbox_store_refs(session.get_session_file())?;
    for message in messages {
        let expected = JsValue::from(&json!(message.store_ref));
        let mut has_receipt = false;
        for reference in &accepted {
            if mailbox_store_refs_equal(reference, &expected)? {
                has_receipt = true;
                break;
            }
        }
        if !has_receipt {
            release_runtime_mailbox_message_claim(control_db_path, message.id)?;
        }
    }
    Ok(())
}

pub fn read_runtime_mailbox_tool_delivery(
    message: &AgentMessage,
) -> Result<Option<RuntimeMailboxToolDelivery>, ControlDbError> {
    let AgentMessage::ToolResult(message) = message else {
        return Ok(None);
    };
    message
        .details
        .as_ref()
        .and_then(|details| details.get("runtimeMailboxDelivery"))
        .map(|delivery| serde_json::from_value(delivery.clone()).map_err(control_error))
        .transpose()
}
