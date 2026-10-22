//! Runtime helpers (`extensions/agents-core/src/runtime-utils.ts`).

use crate::core::multi_agent_store::{
    AgentMailboxMessage, MailboxMessageKindValue, MailboxMessageStatusValue, MultiAgentStore,
};

pub fn abort_agent_handle_safely(store: &MultiAgentStore, agent_id: &str) {
    let _ = store.abort_agent_handle(agent_id);
}

pub fn create_failed_mailbox_message(
    body: &pi_js_compat::js_json::JsString,
    from_agent_id: &str,
    kind: MailboxMessageKindValue,
    to_agent_id: &str,
) -> AgentMailboxMessage {
    let timestamp = "1970-01-01T00:00:00.000Z".to_owned();
    AgentMailboxMessage {
        extra: serde_json::Map::new(),
        id: String::new(),
        thread_id: None,
        from_agent_id: from_agent_id.to_owned(),
        to_agent_id: to_agent_id.to_owned(),
        kind,
        status: MailboxMessageStatusValue::Failed,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        body: Some(body.to_owned()),
        file_refs: None,
        target_checkpoint: None,
        error: None,
    }
}
