//! Frozen `agents-core/src/session-message-route.ts`.
use super::mailbox_runtime::SendAgentMessageParams;
use crate::core::extensions::types::HandlerError;
use crate::core::session_control_db::{is_known_local_session, read_live_runtime_mailbox_session_agent_ids};
use pi_js_compat::js_json::JsString;
pub fn is_remote_session_message(params: &SendAgentMessageParams) -> bool {
    params
        .to_host
        .as_ref()
        .is_some_and(|host| host != gethostname::gethostname().to_string_lossy().as_ref())
}
pub fn session_message_route_error(
    db: Option<&JsString>,
    params: &SendAgentMessageParams,
) -> Result<Option<JsString>, HandlerError> {
    if params.to_host.as_ref().is_some_and(|host| host.trim().is_empty()) {
        return Ok(Some("toHost must be non-empty.".into()));
    }
    if is_remote_session_message(params) {
        return Ok(params
            .to_session_id
            .as_ref()
            .filter(|id| !id.is_empty())
            .is_none()
            .then(|| "toHost requires toSessionId.".into()));
    }
    let (Some(db), Some(session)) = (
        db.filter(|path| !path.is_empty()),
        params.to_session_id.as_ref().filter(|id| !id.is_empty()),
    ) else {
        return Ok(None);
    };
    if !is_known_local_session(db, session).map_err(|error| HandlerError::new(error.to_string()))? {
        return Ok(Some(
            JsString::from("Unknown local session ") + session + "; pass toHost for a session on another host.",
        ));
    }
    if params.to_agent_id != "main" && params.to_agent_id != "supervisor" {
        return Ok(None);
    }
    let ids = read_live_runtime_mailbox_session_agent_ids(db, session)
        .map_err(|error| HandlerError::new(error.to_string()))?;
    if ids.is_empty() || ids.contains(&None) {
        return Ok(None);
    }
    let ids = ids.into_iter().map(|id| id.unwrap_or_default()).collect::<Vec<_>>();
    let ids = JsString::join(ids, ", ");
    Ok(Some(
        JsString::from("Session ")
            + session
            + " has child-agent listeners ("
            + ids
            + ") but no main-thread listener. Set toAgentId to the intended child agent ID; \"main\" and \"supervisor\" address only a main thread.",
    ))
}
