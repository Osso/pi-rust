//! Whole frozen `core/agent-lifecycle-trace.ts` historical trace reader/formatter.
use super::multi_agent_store::AgentSnapshot;
use super::session_manager::load_entries_from_file;
use super::sqlite::{ControlDbError, configure_read_only_sqlite_database, create_read_only_sqlite_database};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize)]
pub struct AgentLifecycleTrace {
    pub events: Vec<JsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<TraceOwnership>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceOwnership {
    agent_id: JsString,
    owner: TraceOwner,
    session_path: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    process_identity: Option<TraceProcessIdentity>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceOwner {
    agent_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<JsString>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceProcessIdentity {
    #[serde(skip_serializing_if = "Option::is_none")]
    incarnation: Option<JsString>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pid: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    start_time_ticks: f64,
}
const AGENT_LIFECYCLE_STATES: [&str; 7] = [
    "running",
    "waiting_for_input",
    "steering_pending",
    "cancelling",
    "completed",
    "failed",
    "aborted",
];
const AGENT_START_CUSTOM_TYPE: &str = "agent_start";
const AGENT_COMPLETE_CUSTOM_TYPE: &str = "agent_complete";
const END_TURN_TOOL_NAME: &str = "end_turn";

fn err(error: impl std::fmt::Display) -> ControlDbError {
    ControlDbError::new(error.to_string())
}
fn lifecycle(agent: &AgentSnapshot) -> JsValue {
    js_json::to_value(&agent.lifecycle).expect("lifecycle enum")
}
fn text(value: &str) -> JsValue {
    JsValue::String(value.into())
}
fn field<'a>(value: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    value.get(key)
}
fn string_field<'a>(value: &'a JsValue, key: &str) -> Option<&'a JsString> {
    field(value, key).and_then(JsValue::as_string)
}
fn is_string_field(value: &JsValue, key: &str, expected: &str) -> bool {
    string_field(value, key).is_some_and(|text| *text == expected)
}
/// Template-literal interpolation of a trace event property (`${event.key}`).
fn interpolate(value: &JsValue, key: &str) -> JsString {
    match field(value, key) {
        None => "undefined".into(),
        Some(JsValue::String(text)) => text.clone(),
        Some(JsValue::Number(number)) => pi_js_compat::number::js_number_to_string(*number).into(),
        Some(JsValue::Bool(flag)) => flag.to_string().into(),
        Some(JsValue::Null) => "null".into(),
        Some(other) => js_json::stringify_lossless(other).into(),
    }
}
fn insert_string(target: &mut JsValue, key: &str, value: Option<&JsString>) {
    if let (Some(object), Some(value)) = (target.as_object_mut(), value) {
        object.insert(key.into(), JsValue::String(value.clone()));
    }
}
fn lifecycle_value(value: Option<&JsValue>) -> Option<&JsString> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| AGENT_LIFECYCLE_STATES.iter().any(|state| **text == **state))
}
fn list_trace_descendants<'a>(agents: &'a [AgentSnapshot], parent_id: &JsString) -> Vec<&'a AgentSnapshot> {
    let mut children_by_parent = HashMap::<&JsString, Vec<&AgentSnapshot>>::new();
    for agent in agents {
        if let Some(parent) = &agent.parent_id {
            children_by_parent.entry(parent).or_default().push(agent);
        }
    }
    let mut descendants = Vec::new();
    let mut pending_parent_ids = VecDeque::from([parent_id]);
    let mut visited_agent_ids = HashSet::from([parent_id]);
    while let Some(current_parent_id) = pending_parent_ids.pop_front() {
        if current_parent_id.is_empty() {
            continue;
        }
        for child in children_by_parent.get(current_parent_id).into_iter().flatten() {
            if visited_agent_ids.insert(&child.id) {
                descendants.push(*child);
                pending_parent_ids.push_back(&child.id);
            }
        }
    }
    descendants
}
fn agent_snapshot_event(agent: &AgentSnapshot) -> JsValue {
    pi_js_compat::js_json!({
        "kind": "agent_snapshot",
        "lifecycle": lifecycle(agent),
        "revision": agent.revision,
        "source": "control_db",
        "timestamp": agent.updated_at,
        "timestampSource": "agent.updatedAt",
    })
}
fn descendant_trace_events(agent: &AgentSnapshot) -> [JsValue; 2] {
    [
        pi_js_compat::js_json!({
            "agentId": agent.id,
            "agentType": agent.agent_type,
            "detached": agent.detached == Some(true),
            "kind": "descendant_admitted",
            "parentId": agent.parent_id,
            "source": "control_db",
            "timestamp": agent.created_at,
            "timestampSource": "descendant.createdAt",
        }),
        pi_js_compat::js_json!({
            "agentId": agent.id,
            "kind": "descendant_snapshot",
            "lifecycle": lifecycle(agent),
            "parentId": agent.parent_id,
            "revision": agent.revision,
            "source": "control_db",
            "timestamp": agent.updated_at,
            "timestampSource": "descendant.updatedAt",
        }),
    ]
}
fn parse_process_identity(serialized: &JsString) -> Result<Option<TraceProcessIdentity>, ControlDbError> {
    let JsValue::Object(process) = js_json::parse_lossless(serialized).map_err(err)? else {
        return Ok(None);
    };
    let (Some(JsValue::Number(pid)), Some(JsValue::Number(ticks))) =
        (process.get("pid"), process.get("startTimeTicks"))
    else {
        return Ok(None);
    };
    let incarnation = match process.get("incarnation") {
        Some(JsValue::String(value)) => Some(value.clone()),
        _ => None,
    };
    Ok(Some(TraceProcessIdentity {
        incarnation,
        pid: *pid,
        start_time_ticks: *ticks,
    }))
}
fn read_runtime_ownership(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
) -> Result<Option<TraceOwnership>, ControlDbError> {
    let row = db
        .query_row(
            "SELECT session_path, agent_id, process_identity, owner_session_id, owner_agent_id
			 FROM multi_agent_runtime_owners
			 WHERE session_path = ? AND agent_id = ?",
            params![session_path, agent_id],
            |row| {
                Ok((
                    row.get::<_, JsString>(0)?,
                    row.get::<_, JsString>(1)?,
                    row.get::<_, Option<JsString>>(2)?,
                    row.get::<_, Option<JsString>>(3)?,
                    row.get::<_, Option<JsString>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(err)?;
    let Some((session_path, agent_id, process_identity, owner_session_id, owner_agent_id)) = row else {
        return Ok(None);
    };
    let process_identity = match process_identity.filter(|serialized| !serialized.is_empty()) {
        Some(serialized) => parse_process_identity(&serialized)?,
        None => None,
    };
    Ok(Some(TraceOwnership {
        agent_id,
        owner: TraceOwner {
            agent_id: owner_agent_id,
            session_id: owner_session_id,
        },
        session_path,
        process_identity,
    }))
}
fn read_terminal_outbox_events(
    db: &Connection,
    session_path: &JsString,
    agent_ids: &[&JsString],
) -> Result<Vec<JsValue>, ControlDbError> {
    let mut statement = db
        .prepare(
            "SELECT agent_id, terminal_revision, event_kind, status, claim_id, claimed_at,
			        attempt_count, last_error, updated_at
			 FROM multi_agent_terminal_outbox
			 WHERE session_path = ? AND agent_id = ?
			 ORDER BY terminal_revision, event_kind",
        )
        .map_err(err)?;
    let mut events = Vec::new();
    for agent_id in agent_ids {
        let rows = statement
            .query_map(params![session_path, agent_id], |row| {
                let mut event = pi_js_compat::js_json!({
                    "agentId": row.get::<_, JsString>(0)?,
                    "attemptCount": row.get::<_, i64>(6)?,
                });
                insert_string(&mut event, "claimId", row.get::<_, Option<JsString>>(4)?.as_ref());
                insert_string(&mut event, "claimedAt", row.get::<_, Option<JsString>>(5)?.as_ref());
                insert_string(&mut event, "eventKind", Some(&row.get::<_, JsString>(2)?));
                insert_string(&mut event, "kind", Some(&"terminal_outbox".into()));
                insert_string(&mut event, "lastError", row.get::<_, Option<JsString>>(7)?.as_ref());
                let object = event.as_object_mut().expect("outbox event object");
                object.insert("source".into(), text("control_db"));
                object.insert("status".into(), JsValue::String(row.get(3)?));
                object.insert("terminalRevision".into(), pi_js_compat::js_json!(row.get::<_, i64>(1)?));
                object.insert("timestamp".into(), JsValue::String(row.get(8)?));
                object.insert("timestampSource".into(), text("outbox.updated_at"));
                Ok(event)
            })
            .map_err(err)?;
        for row in rows {
            events.push(row.map_err(err)?);
        }
    }
    Ok(events)
}
fn load_entry_values(path: &JsString) -> Result<Vec<JsValue>, ControlDbError> {
    load_entries_from_file(path)
        .map_err(|error| ControlDbError::new(error.message().clone()))?
        .into_iter()
        .map(|entry| js_json::to_value(&entry).map_err(err))
        .collect()
}
fn read_parent_agent_events(session_path: &JsString, agent_id: &JsString) -> Result<Vec<JsValue>, ControlDbError> {
    Ok(load_entry_values(session_path)?
        .iter()
        .filter_map(|entry| parent_agent_event(entry, agent_id))
        .collect())
}
fn parent_agent_event(entry: &JsValue, agent_id: &JsString) -> Option<JsValue> {
    let custom_type = string_field(entry, "customType");
    let is_start = custom_type.is_some_and(|kind| *kind == AGENT_START_CUSTOM_TYPE);
    let is_complete = custom_type.is_some_and(|kind| *kind == AGENT_COMPLETE_CUSTOM_TYPE);
    if !is_string_field(entry, "type", "custom") || !(is_start || is_complete) {
        return None;
    }
    let data = field(entry, "data").filter(|data| data.as_object().is_some());
    if data.and_then(|data| string_field(data, "agentId")) != Some(agent_id) {
        return None;
    }
    let data = data.expect("parent agent record data");
    let mut event = JsValue::Object(JsObject::new());
    insert_string(&mut event, "childSessionId", string_field(data, "childSessionId"));
    let object = event.as_object_mut().expect("parent event object");
    object.insert("entryId".into(), field(entry, "id").cloned().unwrap_or_default());
    object.insert(
        "kind".into(),
        text(if is_start {
            "parent_agent_start"
        } else {
            "parent_agent_complete"
        }),
    );
    insert_string(&mut event, "lifecycle", lifecycle_value(field(data, "lifecycle")));
    let object = event.as_object_mut().expect("parent event object");
    object.insert("source".into(), text("parent_transcript"));
    object.insert(
        "timestamp".into(),
        field(entry, "timestamp").cloned().unwrap_or_default(),
    );
    object.insert("timestampSource".into(), text("entry.timestamp"));
    insert_string(&mut event, "transcriptPath", string_field(data, "transcriptPath"));
    Some(event)
}
fn read_child_end_turn_events(agent: &AgentSnapshot) -> Result<Vec<JsValue>, ControlDbError> {
    let Some(transcript) = &agent.transcript else {
        return Ok(Vec::new());
    };
    let Some(path) = transcript.path.as_ref().filter(|path| !path.is_empty()) else {
        return Ok(Vec::new());
    };
    let entries = load_entry_values(path)?;
    let Some(header) = entries.iter().find(|entry| is_string_field(entry, "type", "session")) else {
        return Ok(Vec::new());
    };
    if string_field(header, "id") != Some(&transcript.session_id) {
        return Ok(Vec::new());
    }
    let header_timestamp = string_field(header, "timestamp");
    Ok(entries
        .iter()
        .filter(|entry| string_field(entry, "timestamp") >= header_timestamp)
        .filter_map(child_end_turn_event)
        .collect())
}
fn child_end_turn_event(entry: &JsValue) -> Option<JsValue> {
    let message = field(entry, "message")?;
    if !is_string_field(entry, "type", "message") || !is_string_field(message, "role", "toolResult") {
        return None;
    }
    if !is_string_field(message, "toolName", END_TURN_TOOL_NAME)
        || field(message, "isError") == Some(&JsValue::Bool(true))
    {
        return None;
    }
    let mut event = pi_js_compat::js_json!({
        "entryId": field(entry, "id").cloned().unwrap_or_default(),
        "kind": "child_end_turn",
    });
    let reason = field(message, "details")
        .filter(|details| details.as_object().is_some())
        .and_then(|details| string_field(details, "reason"));
    insert_string(&mut event, "reason", reason);
    let object = event.as_object_mut().expect("child event object");
    object.insert("source".into(), text("child_transcript"));
    object.insert(
        "timestamp".into(),
        field(entry, "timestamp").cloned().unwrap_or_default(),
    );
    object.insert("timestampSource".into(), text("entry.timestamp"));
    object.insert(
        "toolCallId".into(),
        field(message, "toolCallId").cloned().unwrap_or_default(),
    );
    Some(event)
}
fn trace_event_order(event: &JsValue) -> u8 {
    let kind = string_field(event, "kind").cloned().unwrap_or_default();
    [
        "agent_snapshot",
        "parent_agent_start",
        "descendant_admitted",
        "descendant_snapshot",
        "child_end_turn",
        "terminal_outbox",
        "parent_agent_complete",
    ]
    .iter()
    .position(|candidate| kind == *candidate)
    .expect("trace event kind") as u8
}
fn compare_trace_events(left: &JsValue, right: &JsValue) -> std::cmp::Ordering {
    let timestamp = |event: &JsValue| string_field(event, "timestamp").cloned().unwrap_or_default();
    pi_js_compat::collator::locale_compare_utf16(timestamp(left).units(), timestamp(right).units())
        .then_with(|| trace_event_order(left).cmp(&trace_event_order(right)))
}
pub fn read_agent_lifecycle_trace(
    agent: &AgentSnapshot,
    agents: &[AgentSnapshot],
    control_db_path: &JsString,
    session_path: &JsString,
) -> Result<AgentLifecycleTrace, ControlDbError> {
    let db = create_read_only_sqlite_database(&js_json::encode_node_utf8(control_db_path))?;
    configure_read_only_sqlite_database(&db, 5_000)?;
    let ownership = read_runtime_ownership(&db, session_path, &agent.id)?;
    let descendants = list_trace_descendants(agents, &agent.id);
    let mut events = vec![agent_snapshot_event(agent)];
    for descendant in &descendants {
        events.extend(descendant_trace_events(descendant));
    }
    events.extend(read_parent_agent_events(session_path, &agent.id)?);
    events.extend(read_child_end_turn_events(agent)?);
    let mut agent_ids = vec![&agent.id];
    agent_ids.extend(descendants.iter().map(|descendant| &descendant.id));
    events.extend(read_terminal_outbox_events(&db, session_path, &agent_ids)?);
    events.sort_by(compare_trace_events);
    Ok(AgentLifecycleTrace { events, ownership })
}
fn format_ownership(ownership: Option<&TraceOwnership>) -> JsString {
    let Some(ownership) = ownership else {
        return "runtime_ownership absent".into();
    };
    let owner_agent_id = ownership.owner.agent_id.clone().unwrap_or_else(|| "main".into());
    let owner_session_id = ownership.owner.session_id.clone().unwrap_or_else(|| "unknown".into());
    let process = match &ownership.process_identity {
        Some(process) => js_format!(
            "{}:{}:{}",
            pi_js_compat::number::js_number_to_string(process.pid),
            pi_js_compat::number::js_number_to_string(process.start_time_ticks),
            process.incarnation.clone().unwrap_or_else(|| "legacy".into())
        ),
        None => "unbound".into(),
    };
    js_format!(
        "runtime_ownership owner={}:{} process={}",
        owner_session_id,
        owner_agent_id,
        process
    )
}
fn format_trace_event(event: &JsValue) -> JsString {
    let kind = string_field(event, "kind").cloned().unwrap_or_default();
    let value = |key: &str| interpolate(event, key);
    if kind == "agent_snapshot" {
        js_format!(
            "agent_snapshot lifecycle={} revision={}",
            value("lifecycle"),
            value("revision")
        )
    } else if kind == "descendant_admitted" {
        js_format!(
            "descendant_admitted agent={} parent={} type={} detached={}",
            value("agentId"),
            value("parentId"),
            value("agentType"),
            value("detached")
        )
    } else if kind == "descendant_snapshot" {
        js_format!(
            "descendant_snapshot agent={} parent={} lifecycle={} revision={}",
            value("agentId"),
            value("parentId"),
            value("lifecycle"),
            value("revision")
        )
    } else if kind == "parent_agent_start" || kind == "parent_agent_complete" {
        let lifecycle = string_field(event, "lifecycle")
            .cloned()
            .unwrap_or_else(|| "unknown".into());
        js_format!("{} lifecycle={} entry={}", kind, lifecycle, value("entryId"))
    } else if kind == "child_end_turn" {
        let reason = string_field(event, "reason").cloned().unwrap_or_default();
        js_format!(
            "child_end_turn reason={} entry={}",
            js_json::stringify_lossless(&JsValue::String(reason)),
            value("entryId")
        )
    } else if kind == "terminal_outbox" {
        js_format!(
            "terminal_outbox agent={} event={} revision={} status={}",
            value("agentId"),
            value("eventKind"),
            value("terminalRevision"),
            value("status")
        )
    } else {
        unreachable!("trace event kind")
    }
}
pub fn format_agent_lifecycle_trace(trace: &AgentLifecycleTrace) -> JsString {
    let mut lines = vec![
        JsString::from("Lifecycle trace:"),
        format_ownership(trace.ownership.as_ref()),
    ];
    lines.extend(trace.events.iter().map(|event| {
        js_format!(
            "{} [{}] {}",
            interpolate(event, "timestamp"),
            interpolate(event, "timestampSource"),
            format_trace_event(event)
        )
    }));
    JsString::join(&lines, "\n")
}
