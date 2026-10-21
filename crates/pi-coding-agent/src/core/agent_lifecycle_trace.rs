//! Whole frozen `core/agent-lifecycle-trace.ts` historical trace reader/formatter.
use super::multi_agent_store::AgentSnapshot;
use super::session_manager::load_entries_from_file;
use super::sqlite::{ControlDbError, configure_read_only_sqlite_database, create_read_only_sqlite_database};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize)]
pub struct AgentLifecycleTrace {
    pub events: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<Value>,
}
fn err(error: impl std::fmt::Display) -> ControlDbError {
    ControlDbError::new(error.to_string())
}
fn lifecycle(agent: &AgentSnapshot) -> Value {
    serde_json::to_value(agent.lifecycle).expect("lifecycle enum")
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn optional_string(target: &mut Value, key: &str, source: &Value) {
    if source.is_string() {
        target[key] = source.clone();
    }
}
fn lifecycle_value(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some("running" | "waiting_for_input" | "steering_pending" | "cancelling" | "completed" | "failed" | "aborted")
    )
}
fn descendants<'a>(agents: &'a [AgentSnapshot], parent: &str) -> Vec<&'a AgentSnapshot> {
    let mut children = HashMap::<&str, Vec<&AgentSnapshot>>::new();
    for agent in agents {
        if let Some(parent) = &agent.parent_id {
            children.entry(parent).or_default().push(agent);
        }
    }
    let mut pending = VecDeque::from([parent]);
    let mut visited = HashSet::from([parent]);
    let mut result = Vec::new();
    while let Some(parent) = pending.pop_front() {
        if parent.is_empty() {
            continue;
        }
        for child in children.get(parent).into_iter().flatten() {
            if visited.insert(&child.id) {
                result.push(*child);
                pending.push_back(&child.id);
            }
        }
    }
    result
}
fn descendant_events(agent: &AgentSnapshot) -> [Value; 2] {
    [
        json!({"agentId":agent.id,"agentType":agent.agent_type,"detached":agent.detached==Some(true),"kind":"descendant_admitted","parentId":agent.parent_id,"source":"control_db","timestamp":agent.created_at,"timestampSource":"descendant.createdAt"}),
        json!({"agentId":agent.id,"kind":"descendant_snapshot","lifecycle":lifecycle(agent),"parentId":agent.parent_id,"revision":agent.revision,"source":"control_db","timestamp":agent.updated_at,"timestampSource":"descendant.updatedAt"}),
    ]
}
fn read_ownership(db: &Connection, path: &str, id: &str) -> Result<Option<Value>, ControlDbError> {
    let row:Option<(String,Option<String>,Option<String>,Option<String>,String)>=db.query_row("SELECT agent_id, owner_agent_id, owner_session_id, process_identity, session_path FROM multi_agent_runtime_owners WHERE session_path = ? AND agent_id = ?",[path,id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional().map_err(err)?;
    let Some((id, owner_agent, owner_session, process, path)) = row else {
        return Ok(None);
    };
    let mut result = json!({"agentId":id,"owner":{"agentId":owner_agent},"sessionPath":path});
    if let Some(session) = owner_session {
        result["owner"]["sessionId"] = json!(session);
    }
    if let Some(serialized) = process.filter(|process| !process.is_empty()) {
        let process: Value = serde_json::from_str(&serialized).map_err(err)?;
        if process.is_object() && process["pid"].is_number() && process["startTimeTicks"].is_number() {
            let mut identity = json!({"pid":process["pid"],"startTimeTicks":process["startTimeTicks"]});
            optional_string(&mut identity, "incarnation", &process["incarnation"]);
            result["processIdentity"] = identity;
        }
    }
    Ok(Some(result))
}
fn read_outbox(db: &Connection, path: &str, ids: &[&str]) -> Result<Vec<Value>, ControlDbError> {
    let mut events = Vec::new();
    let mut query=db.prepare("SELECT agent_id,terminal_revision,event_kind,status,claim_id,claimed_at,attempt_count,last_error,updated_at FROM multi_agent_terminal_outbox WHERE session_path = ? AND agent_id = ? ORDER BY terminal_revision,event_kind").map_err(err)?;
    for id in ids {
        let rows=query.query_map([path,*id],|row| {
            let mut value=json!({"agentId":row.get::<_,String>(0)?,"attemptCount":row.get::<_,i64>(6)?,"eventKind":row.get::<_,String>(2)?,"kind":"terminal_outbox","source":"control_db","status":row.get::<_,String>(3)?,"terminalRevision":row.get::<_,i64>(1)?,"timestamp":row.get::<_,String>(8)?,"timestampSource":"outbox.updated_at"});
            for (index,key) in [(4,"claimId"),(5,"claimedAt"),(7,"lastError")] {if let Some(value_text)=row.get::<_,Option<String>>(index)? {value[key]=json!(value_text);}}
            Ok(value)
        }).map_err(err)?;
        for row in rows {
            events.push(row.map_err(err)?);
        }
    }
    Ok(events)
}
fn entries(path: &str) -> Result<Vec<Value>, ControlDbError> {
    load_entries_from_file(path)
        .map_err(err)?
        .into_iter()
        .map(|entry| serde_json::to_value(entry).map_err(err))
        .collect()
}
fn parent_events(path: &str, id: &str) -> Result<Vec<Value>, ControlDbError> {
    let mut result = Vec::new();
    for entry in entries(path)? {
        if entry["type"] != "custom"
            || !matches!(entry["customType"].as_str(), Some("agent_start" | "agent_complete"))
            || entry["data"]["agentId"] != id
        {
            continue;
        }
        let data = &entry["data"];
        let kind = if entry["customType"] == "agent_start" {
            "parent_agent_start"
        } else {
            "parent_agent_complete"
        };
        let mut event = json!({"entryId":entry["id"],"kind":kind,"source":"parent_transcript","timestamp":entry["timestamp"],"timestampSource":"entry.timestamp"});
        optional_string(&mut event, "childSessionId", &data["childSessionId"]);
        optional_string(&mut event, "transcriptPath", &data["transcriptPath"]);
        if lifecycle_value(&data["lifecycle"]) {
            event["lifecycle"] = data["lifecycle"].clone();
        }
        result.push(event);
    }
    Ok(result)
}
fn child_events(agent: &AgentSnapshot) -> Result<Vec<Value>, ControlDbError> {
    let Some(transcript) = &agent.transcript else {
        return Ok(Vec::new());
    };
    let Some(path) = transcript.path.as_deref().filter(|path| !path.is_empty()) else {
        return Ok(Vec::new());
    };
    let entries = entries(path)?;
    let Some(header) = entries.iter().find(|entry| entry["type"] == "session") else {
        return Ok(Vec::new());
    };
    if header["id"] != transcript.session_id {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in &entries {
        let message = &entry["message"];
        if text(entry, "timestamp") < text(header, "timestamp")
            || entry["type"] != "message"
            || message["role"] != "toolResult"
            || message["toolName"] != "end_turn"
            || message["isError"] == true
        {
            continue;
        }
        let mut event = json!({"entryId":entry["id"],"kind":"child_end_turn","source":"child_transcript","timestamp":entry["timestamp"],"timestampSource":"entry.timestamp","toolCallId":message["toolCallId"]});
        optional_string(&mut event, "reason", &message["details"]["reason"]);
        result.push(event);
    }
    Ok(result)
}
fn event_order(event: &Value) -> u8 {
    match text(event, "kind") {
        "agent_snapshot" => 0,
        "parent_agent_start" => 1,
        "descendant_admitted" => 2,
        "descendant_snapshot" => 3,
        "child_end_turn" => 4,
        "terminal_outbox" => 5,
        "parent_agent_complete" => 6,
        _ => unreachable!("trace event"),
    }
}
pub fn read_agent_lifecycle_trace(
    agent: &AgentSnapshot,
    agents: &[AgentSnapshot],
    db_path: &str,
    path: &str,
) -> Result<AgentLifecycleTrace, ControlDbError> {
    let db = create_read_only_sqlite_database(db_path)?;
    configure_read_only_sqlite_database(&db, 5_000)?;
    let ownership = read_ownership(&db, path, &agent.id)?;
    let descendants = descendants(agents, &agent.id);
    let mut events = vec![
        json!({"kind":"agent_snapshot","lifecycle":lifecycle(agent),"revision":agent.revision,"source":"control_db","timestamp":agent.updated_at,"timestampSource":"agent.updatedAt"}),
    ];
    for descendant in &descendants {
        events.extend(descendant_events(descendant));
    }
    events.extend(parent_events(path, &agent.id)?);
    events.extend(child_events(agent)?);
    let mut ids = vec![agent.id.as_str()];
    ids.extend(descendants.iter().map(|agent| agent.id.as_str()));
    events.extend(read_outbox(&db, path, &ids)?);
    events.sort_by(|left, right| {
        pi_js_compat::locale_compare(text(left, "timestamp"), text(right, "timestamp"))
            .then_with(|| event_order(left).cmp(&event_order(right)))
    });
    Ok(AgentLifecycleTrace { events, ownership })
}
fn format_ownership(ownership: Option<&Value>) -> String {
    let Some(ownership) = ownership else {
        return "runtime_ownership absent".into();
    };
    let owner_agent = ownership["owner"]["agentId"].as_str().unwrap_or("main");
    let owner_session = ownership["owner"]["sessionId"].as_str().unwrap_or("unknown");
    let process = &ownership["processIdentity"];
    let process = if process.is_object() {
        format!(
            "{}:{}:{}",
            process["pid"],
            process["startTimeTicks"],
            process["incarnation"].as_str().unwrap_or("legacy")
        )
    } else {
        "unbound".into()
    };
    format!("runtime_ownership owner={owner_session}:{owner_agent} process={process}")
}
fn format_event(event: &Value) -> String {
    match text(event, "kind") {
        "agent_snapshot" => format!(
            "agent_snapshot lifecycle={} revision={}",
            text(event, "lifecycle"),
            event["revision"]
        ),
        "descendant_admitted" => format!(
            "descendant_admitted agent={} parent={} type={} detached={}",
            text(event, "agentId"),
            text(event, "parentId"),
            text(event, "agentType"),
            event["detached"]
        ),
        "descendant_snapshot" => format!(
            "descendant_snapshot agent={} parent={} lifecycle={} revision={}",
            text(event, "agentId"),
            text(event, "parentId"),
            text(event, "lifecycle"),
            event["revision"]
        ),
        "parent_agent_start" | "parent_agent_complete" => format!(
            "{} lifecycle={} entry={}",
            text(event, "kind"),
            event["lifecycle"].as_str().unwrap_or("unknown"),
            text(event, "entryId")
        ),
        "child_end_turn" => format!(
            "child_end_turn reason={} entry={}",
            serde_json::to_string(event["reason"].as_str().unwrap_or("")).expect("reason string"),
            text(event, "entryId")
        ),
        "terminal_outbox" => format!(
            "terminal_outbox agent={} event={} revision={} status={}",
            text(event, "agentId"),
            text(event, "eventKind"),
            event["terminalRevision"],
            text(event, "status")
        ),
        _ => unreachable!("trace event"),
    }
}
pub fn format_agent_lifecycle_trace(trace: &AgentLifecycleTrace) -> String {
    let mut lines = vec![
        "Lifecycle trace:".to_owned(),
        format_ownership(trace.ownership.as_ref()),
    ];
    lines.extend(trace.events.iter().map(|event| {
        format!(
            "{} [{}] {}",
            text(event, "timestamp"),
            text(event, "timestampSource"),
            format_event(event)
        )
    }));
    lines.join("\n")
}
