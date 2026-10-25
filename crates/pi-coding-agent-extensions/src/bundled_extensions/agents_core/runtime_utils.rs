//! Runtime helpers (`extensions/agents-core/src/runtime-utils.ts`).

use crate::core::multi_agent_store::{
    AgentMailboxMessage, MailboxMessageKindValue, MailboxMessageStatusValue, MultiAgentStore,
};
use pi_js_compat::js_json::JsString;

pub fn abort_agent_handle_safely(store: &MultiAgentStore, agent_id: &JsString) {
    if let Err(error) = store.abort_agent_handle(agent_id) {
        let prefix = pi_js_compat::js_format!("Failed to abort agent runtime {}:", agent_id);
        crate::core::desktop_notification::report_callback_error(
            &pi_js_compat::js_json::encode_node_utf8(&prefix),
            &error,
        );
    }
}

pub fn create_failed_mailbox_message(
    body: &pi_js_compat::js_json::JsString,
    from_agent_id: impl Into<pi_js_compat::js_json::JsString>,
    kind: MailboxMessageKindValue,
    to_agent_id: impl Into<pi_js_compat::js_json::JsString>,
) -> AgentMailboxMessage {
    let timestamp = "1970-01-01T00:00:00.000Z".to_owned();
    AgentMailboxMessage {
        extra: pi_js_compat::js_json::JsObject::new(),
        id: String::new().into(),
        thread_id: None,
        from_agent_id: from_agent_id.into(),
        to_agent_id: to_agent_id.into(),
        kind,
        status: MailboxMessageStatusValue::Failed,
        created_at: timestamp.clone().into(),
        updated_at: Some(timestamp.into()),
        body: Some(body.to_owned()),
        file_refs: None,
        target_checkpoint: None,
        error: None,
    }
}
