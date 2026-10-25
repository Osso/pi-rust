//! Agent listing and viewer (`extensions/agents-core/src/viewer.ts`).

use pi_agent_core::AgentToolResult;
use pi_js_compat::{
    js_format, js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value},
};

use crate::core::agent_lifecycle_trace::{
    AgentLifecycleTrace, format_agent_lifecycle_trace, read_agent_lifecycle_trace,
};
use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, MultiAgentStore, is_active_lifecycle};
use crate::core::session_control_db::{list_session_metadata, read_multi_agent_state};

use super::tool_results::tool_result;

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAgentsParams {
    pub parent_id: Option<JsString>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentViewerParams {
    pub agent_id: JsString,
    pub session_id: Option<JsString>,
    pub store_session_id: Option<JsString>,
    pub trace: Option<bool>,
}

pub fn list_agents(store: &MultiAgentStore, params: &ListAgentsParams) -> Result<AgentToolResult, HandlerError> {
    let agents = list_matching_agents(store, params);
    let details = json!({
        "activeCount": store.get_active_agent_count(),
        "agents": agents,
    });
    Ok(tool_result(format_agent_list_content(&agents), details))
}

fn list_matching_agents(store: &MultiAgentStore, params: &ListAgentsParams) -> Vec<AgentSnapshot> {
    let agents = match params.parent_id.as_ref().filter(|id| !id.is_empty()) {
        Some(parent_id) => store.list_descendants(parent_id),
        None => store.list_agents(),
    };
    agents
        .into_iter()
        .filter(|agent| is_active_lifecycle(agent.lifecycle))
        .collect()
}

fn format_agent_list_content(agents: &[AgentSnapshot]) -> JsString {
    let suffix = if agents.len() == 1 { "" } else { "s" };
    let header = JsString::from(format!("Found {} agent{suffix}.", agents.len()));
    let entries = agents.iter().map(|agent| {
        js_format!(
            "id={} name={} type={} status={} lifecycle={}",
            agent.id,
            quote(&agent.display_name),
            agent.agent_type,
            agent_status_label(agent),
            lifecycle_name(agent.lifecycle)
        )
    });
    JsString::join(std::iter::once(header).chain(entries).collect::<Vec<_>>(), "\n")
}

fn quote(value: impl Into<JsString>) -> String {
    js_json::stringify_lossless(&Value::String(value.into()))
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

pub fn agent_viewer(
    store: &MultiAgentStore,
    params: &AgentViewerParams,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    if params.trace == Some(true) && params.store_session_id.as_ref().is_none_or(JsString::is_empty) {
        return Ok(tool_result(
            "Historical lifecycle trace requires storeSessionId.",
            json!({"agentId":params.agent_id,"error":"trace_requires_store_session"}),
        ));
    }
    if params.store_session_id.as_ref().is_some_and(|id| !id.is_empty()) {
        return agent_viewer_from_persisted_session(params, ctx);
    }

    let Some(agent) = store.get_agent(&params.agent_id) else {
        return Ok(tool_result(
            js_format!("Agent not found: {}.", params.agent_id),
            json!({"agentId": params.agent_id, "error": "not_found"}),
        ));
    };
    if let Some(error) = validate_transcript_session(&agent, params) {
        return Ok(error);
    }
    agent_viewer_result(&agent, &store.list_agents(), None)
}

fn agent_viewer_from_persisted_session(
    params: &AgentViewerParams,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    let (agents, control_db_path, session_path) = match load_persisted_agents_for_viewer(params, ctx)? {
        AgentPersistedLoad::Agents {
            agents,
            control_db_path,
            session_path,
        } => (agents, control_db_path, session_path),
        AgentPersistedLoad::Error(error) => return Ok(*error),
    };

    let Some(agent) = agents.iter().find(|candidate| candidate.id == params.agent_id) else {
        return Ok(tool_result(
            js_format!("Agent not found: {}.", params.agent_id),
            json!({
                "agentId": params.agent_id,
                "error": "not_found",
                "storeSessionId": params.store_session_id,
            }),
        ));
    };
    if let Some(error) = validate_transcript_session(agent, params) {
        return Ok(error);
    }

    let trace = if params.trace == Some(true) {
        Some(
            read_agent_lifecycle_trace(agent, &agents, &control_db_path, &session_path)
                .map_err(|error| HandlerError::new(error.to_string()))?,
        )
    } else {
        None
    };
    agent_viewer_result(agent, &agents, trace.as_ref())
}

fn validate_transcript_session(agent: &AgentSnapshot, params: &AgentViewerParams) -> Option<AgentToolResult> {
    let session_id = params.session_id.as_ref().filter(|id| !id.is_empty())?;
    if agent.transcript.as_ref().map(|transcript| &transcript.session_id) == Some(session_id) {
        return None;
    }
    Some(tool_result(
        js_format!("Agent {} is not attached to session {}.", params.agent_id, session_id),
        json!({"agentId":params.agent_id,"error":"session_mismatch","sessionId":session_id}),
    ))
}

enum AgentPersistedLoad {
    Agents {
        agents: Vec<AgentSnapshot>,
        control_db_path: JsString,
        session_path: JsString,
    },
    Error(Box<AgentToolResult>),
}

fn load_persisted_agents_for_viewer(
    params: &AgentViewerParams,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentPersistedLoad, HandlerError> {
    let control_db_path = match ctx {
        Some(ctx) => ctx.control_db_path()?,
        None => None,
    };
    let Some(control_db_path) = control_db_path.filter(|path| !path.is_empty()) else {
        return Ok(AgentPersistedLoad::Error(Box::new(tool_result(
            "Cannot view a persisted agent without a control DB.",
            json!({
                "agentId": params.agent_id,
                "error": "missing_control_db",
                "storeSessionId": params.store_session_id,
            }),
        ))));
    };

    let sessions = list_session_metadata(&control_db_path).map_err(|error| HandlerError::new(error.message()))?;
    let Some(session) = sessions
        .iter()
        .find(|metadata| Some(&metadata.id) == params.store_session_id.as_ref())
    else {
        return Ok(AgentPersistedLoad::Error(Box::new(tool_result(
            js_format!(
                "Session not found: {}.",
                params.store_session_id.clone().unwrap_or_default()
            ),
            json!({
                "agentId": params.agent_id,
                "error": "session_not_found",
                "storeSessionId": params.store_session_id,
            }),
        ))));
    };

    let state = read_multi_agent_state(&control_db_path, &session.session_path)
        .map_err(|error| HandlerError::new(error.message()))?;
    let agents = state
        .map(|state| {
            state
                .agents
                .into_iter()
                .filter(is_persisted_agent_snapshot)
                .map(|value| js_json::from_value(&value).map_err(|error| HandlerError::new(error.to_string())))
                .collect::<Result<Vec<AgentSnapshot>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(AgentPersistedLoad::Agents {
        agents,
        control_db_path,
        session_path: session.session_path.clone(),
    })
}

fn is_persisted_agent_snapshot(value: &Value) -> bool {
    let Some(candidate) = value.as_object() else {
        return false;
    };
    candidate.get("id").is_some_and(Value::is_string)
        && candidate.get("lifecycle").is_some_and(Value::is_string)
        && candidate
            .get("revision")
            .is_some_and(|value| matches!(value, Value::Number(_)))
}

fn agent_viewer_result(
    agent: &AgentSnapshot,
    agents: &[AgentSnapshot],
    trace: Option<&AgentLifecycleTrace>,
) -> Result<AgentToolResult, HandlerError> {
    let children: Vec<JsString> = agents
        .iter()
        .filter(|candidate| candidate.parent_id.as_ref() == Some(&agent.id))
        .map(|child| child.id.clone())
        .collect();

    let mut details = Map::new();
    details.insert(
        "agent".into(),
        js_json::to_value(agent).map_err(|error| HandlerError::new(error.to_string()))?,
    );
    details.insert("children".into(), json!(children));
    details.insert("commands".into(), json!(list_viewer_commands(agent)));
    if let Some(parent_id) = agent.parent_id.as_ref() {
        details.insert("parentId".into(), json!(parent_id));
    }
    details.insert("status".into(), view_status(agent));
    let mut content = format_agent_viewer_content(agent);
    if let Some(trace) = trace {
        details.insert(
            "trace".into(),
            js_json::to_value(trace).map_err(|error| HandlerError::new(error.to_string()))?,
        );
        content.push_unit(10);
        content.push_str(format_agent_lifecycle_trace(trace));
    }
    if let Some(transcript) = view_transcript(agent) {
        details.insert("transcript".into(), transcript);
    }
    Ok(tool_result(content, Value::Object(details)))
}

fn format_agent_viewer_content(agent: &AgentSnapshot) -> JsString {
    let status = agent_status_label(agent);
    let terminal = status == "terminal";
    let mut lines = vec![js_format!(
        "Viewing agent {}: name={} type={} status={} lifecycle={}",
        agent.id,
        quote(&agent.display_name),
        agent.agent_type,
        status,
        lifecycle_name(agent.lifecycle)
    )];
    if terminal
        && let Some(summary) = agent.result.as_ref().and_then(|result| result.summary.as_ref())
        && !summary.is_empty()
    {
        lines.push(JsString::from("Summary: ") + summary);
    }
    if terminal
        && let Some(message) = agent.error.as_ref().map(|error| &error.message)
        && !message.is_empty()
    {
        let code = agent
            .error
            .as_ref()
            .and_then(|error| error.code.as_deref())
            .map(|code| format!(" ({code})"))
            .unwrap_or_default();
        lines.push(JsString::from("Error: ") + message + code);
    }
    JsString::join(lines, "\n")
}

fn agent_status_label(agent: &AgentSnapshot) -> &'static str {
    if is_active_lifecycle(agent.lifecycle) {
        "active"
    } else {
        "terminal"
    }
}

fn view_status(agent: &AgentSnapshot) -> Value {
    json!({
        "agentId": agent.id,
        "lifecycle": lifecycle_name(agent.lifecycle),
        "revision": agent.revision,
        "terminal": matches!(
            agent.lifecycle,
            AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
        ),
    })
}

fn view_transcript(agent: &AgentSnapshot) -> Option<Value> {
    let transcript = agent.transcript.as_ref()?;
    let mut details = Map::new();
    details.insert("agentId".into(), json!(agent.id));
    if let Some(path) = transcript.path.as_ref() {
        details.insert("path".into(), json!(path));
    }
    details.insert("sessionId".into(), json!(transcript.session_id));
    Some(Value::Object(details))
}

fn list_viewer_commands(agent: &AgentSnapshot) -> Value {
    json!([
        {"agentId": agent.id, "command": "stop", "tool": "close_agent"},
        {"agentId": agent.id, "command": "steer", "tool": "steer_agent"}
    ])
}
