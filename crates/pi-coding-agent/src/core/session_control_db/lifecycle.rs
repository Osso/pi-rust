//! Detached-job finalization and self-restart control-db preparation.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::agents::{
    PreparedTerminalTransport, TerminalAgentCasInput, has_active_persisted_descendant,
    persist_prepared_terminal_transport, prepare_detached_agent_terminal_transport, update_terminal_agent_if_current,
};
use super::{
    ControlDbError, MultiAgentRuntimeOwnerIdentity, MultiAgentRuntimeOwnershipIdentity, MultiAgentRuntimeOwnershipRow,
    parse_agent_snapshot, parse_file_refs, parse_stored_json_object, read_multi_agent_runtime_ownership_row,
    runtime_owner_matches, serialize_process_identity, validate_persisted_agent_payload, with_control_db,
    with_immediate_transaction,
};
use crate::core::multi_agent_store::{AgentError, AgentFileReference, AgentSnapshot};
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedJobOwnershipIdentity {
    pub job_id: String,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub output_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedJobTerminalOutput {
    pub label: String,
    pub path: String,
    pub size: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum DetachedJobOutcome {
    Completed {
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<JsString>,
    },
    Failed {
        error: AgentError,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
    },
    Aborted {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl DetachedJobOutcome {
    fn kind(&self) -> &'static str {
        match self {
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
            Self::Aborted { .. } => "aborted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedJobTerminalInput {
    pub job_id: String,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub output_label: String,
    pub terminal_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub outcome: DetachedJobOutcome,
    pub output: DetachedJobTerminalOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizeDetachedJobInput {
    pub session_path: String,
    pub terminal: DetachedJobTerminalInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeDetachedJobResult {
    Ok {
        terminal_agent: Box<AgentSnapshot>,
        terminal_revision: i64,
    },
    Err {
        error: FinalizeDetachedJobError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeDetachedJobError {
    AgentNotFound,
    InvalidTransition,
    MutationMismatch,
}

impl FinalizeDetachedJobError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentNotFound => "agent_not_found",
            Self::InvalidTransition => "invalid_transition",
            Self::MutationMismatch => "mutation_mismatch",
        }
    }
}

struct FinalizeDetachedJobPlan {
    agent_data: String,
    event_kind: String,
    process_identity_data: String,
    session_path: String,
    terminal_agent: AgentSnapshot,
    terminal_agent_data: String,
    terminal_revision: i64,
    terminal_transport: Option<PreparedTerminalTransport>,
}

/// `prepareControlDbForSelfRestart`.
pub fn prepare_control_db_for_self_restart(control_db_path: &str, process_id: u32) -> Result<(), ControlDbError> {
    let db = crate::core::sqlite::create_sqlite_database(control_db_path)?;
    let result = (|| {
        crate::core::sqlite::configure_shared_sqlite_database(
            &db,
            crate::core::sqlite::DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS,
        )?;
        super::initialize_schema(&db, Some(process_id))
    })();
    drop(db);
    result
}

/// `finalizeDetachedJob`.
pub fn finalize_detached_job(
    control_db_path: &str,
    input: &FinalizeDetachedJobInput,
) -> Result<FinalizeDetachedJobResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
        for _ in 0..3 {
            let Some(plan) = prepare_detached_job_finalization(db, input)? else {
                continue;
            };
            match plan {
                FinalizePreflight::Result(result) => return Ok(*result),
                FinalizePreflight::Plan(plan) => {
                    if let Some(result) = persist_detached_job_finalization(db, &input.terminal, &plan)? {
                        return Ok(result);
                    }
                }
            }
        }
        Ok(FinalizeDetachedJobResult::Err {
            error: FinalizeDetachedJobError::MutationMismatch,
        })
    })
}

enum FinalizePreflight {
    Plan(Box<FinalizeDetachedJobPlan>),
    Result(Box<FinalizeDetachedJobResult>),
}

fn find_detached_job_session_path(
    db: &Connection,
    input: &FinalizeDetachedJobInput,
) -> Result<Option<String>, ControlDbError> {
    let mut statement = db.prepare("SELECT session_path FROM multi_agent_runtime_owners WHERE agent_id = ?")?;
    let candidates = statement
        .query_map(params![input.terminal.job_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut matching_paths = Vec::new();
    for session_path in candidates {
        let ownership = read_multi_agent_runtime_ownership_row(db, &session_path, &input.terminal.job_id)?;
        if ownership
            .as_ref()
            .is_some_and(|ownership| detached_job_ownership_matches(ownership, &session_path, &input.terminal))
        {
            matching_paths.push(session_path);
        }
    }
    Ok((matching_paths.len() == 1).then(|| matching_paths[0].clone()))
}

fn detached_job_lookup_failure(
    db: &Connection,
    input: &FinalizeDetachedJobInput,
) -> Result<FinalizeDetachedJobResult, ControlDbError> {
    let agent_exists = db
        .query_row(
            "SELECT 1 FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![input.session_path, input.terminal.job_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(FinalizeDetachedJobResult::Err {
        error: if agent_exists {
            FinalizeDetachedJobError::MutationMismatch
        } else {
            FinalizeDetachedJobError::AgentNotFound
        },
    })
}

fn prepare_detached_job_finalization(
    db: &Connection,
    input: &FinalizeDetachedJobInput,
) -> Result<Option<FinalizePreflight>, ControlDbError> {
    let Some(session_path) = find_detached_job_session_path(db, input)? else {
        return Ok(Some(FinalizePreflight::Result(Box::new(detached_job_lookup_failure(
            db, input,
        )?))));
    };
    let terminal = &input.terminal;
    let row: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, terminal.job_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(agent_data) = row else {
        return Ok(Some(FinalizePreflight::Result(Box::new(
            FinalizeDetachedJobResult::Err {
                error: FinalizeDetachedJobError::AgentNotFound,
            },
        ))));
    };
    let context = format!("multi_agent_agents:{session_path}#{}", terminal.job_id);
    let agent = parse_stored_json_object(&agent_data, &context)?;
    let terminal_lifecycle = resolve_detached_job_terminal_lifecycle(&agent, terminal);
    let event_kind = format!("detached_job_{terminal_lifecycle}");
    let ownership = read_multi_agent_runtime_ownership_row(db, &session_path, &terminal.job_id)?;
    let Some(ownership) = ownership else {
        return Ok(Some(FinalizePreflight::Result(Box::new(
            FinalizeDetachedJobResult::Err {
                error: FinalizeDetachedJobError::MutationMismatch,
            },
        ))));
    };
    if !detached_job_ownership_matches(&ownership, &session_path, terminal) {
        return Ok(Some(FinalizePreflight::Result(Box::new(
            FinalizeDetachedJobResult::Err {
                error: FinalizeDetachedJobError::MutationMismatch,
            },
        ))));
    }
    let current_lifecycle = agent.get("lifecycle").and_then(Value::as_str).unwrap_or("");
    if current_lifecycle == terminal_lifecycle {
        return Ok(Some(FinalizePreflight::Result(Box::new(detached_job_replay_result(
            &session_path,
            terminal,
            &agent,
            &agent_data,
            agent.get("revision").and_then(Value::as_i64).unwrap_or(0),
        )?))));
    }
    let terminal_revision = agent.get("revision").and_then(Value::as_i64).unwrap_or(0) + 1;
    let can_finalize = current_lifecycle == "running" || current_lifecycle == "cancelling";
    if !can_finalize || has_active_persisted_descendant(db, &session_path, &terminal.job_id)? {
        return Ok(Some(FinalizePreflight::Result(Box::new(
            FinalizeDetachedJobResult::Err {
                error: FinalizeDetachedJobError::InvalidTransition,
            },
        ))));
    }
    let mut terminal_agent = super::parse_stored_json_object_lossless(&agent_data, &context)?;
    for (key, value) in detached_job_agent_details(&agent, terminal, terminal_lifecycle)? {
        terminal_agent.insert(key, value);
    }
    terminal_agent.shift_remove("currentActivity");
    terminal_agent.insert("lifecycle".into(), JsValue::String(terminal_lifecycle.into()));
    terminal_agent.insert("revision".into(), JsValue::Number(terminal_revision as f64));
    terminal_agent.insert("updatedAt".into(), JsValue::String(terminal.terminal_at.clone().into()));
    terminal_agent.shift_remove("worker");
    let terminal_agent_data = js_json::stringify_lossless(&JsValue::Object(terminal_agent));
    let terminal_agent_snapshot = parse_agent_snapshot(&terminal_agent_data, &context)?;
    let terminal_transport = if super::agents::should_notify_detached_job_owner(&agent) {
        let owner = MultiAgentRuntimeOwnerIdentity {
            agent_id: ownership.owner_agent_id.clone(),
            session_id: ownership.owner_session_id.clone().unwrap_or_default(),
        };
        Some(prepare_detached_agent_terminal_transport(
            &session_path,
            &terminal.job_id,
            &owner,
            terminal_revision,
            &event_kind,
            &terminal.terminal_at,
        )?)
    } else {
        None
    };
    Ok(Some(FinalizePreflight::Plan(Box::new(FinalizeDetachedJobPlan {
        agent_data,
        event_kind,
        process_identity_data: serialize_process_identity(&terminal.process_identity),
        session_path,
        terminal_agent_data,
        terminal_agent: terminal_agent_snapshot,
        terminal_revision,
        terminal_transport,
    }))))
}

fn resolve_detached_job_terminal_lifecycle(
    agent: &Map<String, Value>,
    terminal: &DetachedJobTerminalInput,
) -> &'static str {
    match agent.get("lifecycle").and_then(Value::as_str) {
        Some("completed") => "completed",
        Some("failed") => "failed",
        Some("aborted") => "aborted",
        Some("cancelling") => "aborted",
        _ => terminal.outcome.kind(),
    }
}

fn detached_job_ownership_matches(
    ownership: &MultiAgentRuntimeOwnershipRow,
    session_path: &str,
    terminal: &DetachedJobTerminalInput,
) -> bool {
    let identity = MultiAgentRuntimeOwnershipIdentity {
        session_path: session_path.to_owned(),
        agent_id: terminal.job_id.clone(),
        process_identity: terminal.process_identity.clone(),
        owner: terminal.owner.clone(),
    };
    runtime_owner_matches(Some(ownership), &identity)
}

fn detached_job_replay_result(
    session_path: &str,
    terminal: &DetachedJobTerminalInput,
    terminal_agent: &Map<String, Value>,
    agent_data: &str,
    terminal_revision: i64,
) -> Result<FinalizeDetachedJobResult, ControlDbError> {
    let lifecycle = terminal_agent
        .get("lifecycle")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    let expected_details = detached_job_agent_details(terminal_agent, terminal, lifecycle)?;
    let raw = super::parse_stored_json_object_lossless(agent_data, "detached job replay")?;
    for (key, value) in expected_details {
        if js_json::stringify_lossless(raw.get(&key).unwrap_or(&JsValue::Null)) != js_json::stringify_lossless(&value) {
            return Ok(FinalizeDetachedJobResult::Err {
                error: FinalizeDetachedJobError::MutationMismatch,
            });
        }
    }
    let context = format!("multi_agent_agents:{session_path}#{}", terminal.job_id);
    validate_persisted_agent_payload(terminal_agent, &context)?;
    Ok(FinalizeDetachedJobResult::Ok {
        terminal_agent: Box::new(parse_agent_snapshot(
            &js_json::stringify_lossless(&JsValue::Object(terminal_agent.clone())),
            &context,
        )?),
        terminal_revision,
    })
}

fn persist_detached_job_finalization(
    db: &Connection,
    terminal: &DetachedJobTerminalInput,
    plan: &FinalizeDetachedJobPlan,
) -> Result<Option<FinalizeDetachedJobResult>, ControlDbError> {
    with_immediate_transaction(db, |db| {
        if !update_terminal_agent_if_current(
            db,
            &TerminalAgentCasInput {
                agent_data: &plan.agent_data,
                agent_id: &terminal.job_id,
                owner: &terminal.owner,
                process_identity_data: &plan.process_identity_data,
                require_unique_ownership: true,
                session_path: &plan.session_path,
                updated_agent_data: &plan.terminal_agent_data,
                updated_at: &terminal.terminal_at,
            },
        )? {
            return Ok(None);
        }
        db.execute(
            "INSERT INTO multi_agent_terminal_outbox
				(session_path, agent_id, terminal_revision, event_kind, status, attempt_count, updated_at)
			 VALUES (?, ?, ?, ?, 'pending', 0, ?)",
            params![
                plan.session_path,
                terminal.job_id,
                plan.terminal_revision,
                plan.event_kind,
                terminal.terminal_at
            ],
        )?;
        // Attended jobs deliver their result in-band through the waiting tool call;
        // only jobs explicitly detached from their tool call notify the supervisor.
        if let Some(transport) = &plan.terminal_transport {
            persist_prepared_terminal_transport(db, transport)?;
        }
        Ok(Some(FinalizeDetachedJobResult::Ok {
            terminal_agent: Box::new(plan.terminal_agent.clone()),
            terminal_revision: plan.terminal_revision,
        }))
    })
}

fn detached_job_agent_details(
    agent: &Map<String, Value>,
    terminal: &DetachedJobTerminalInput,
    terminal_lifecycle: &str,
) -> Result<JsObject, ControlDbError> {
    let existing_file_refs = match agent.get("result") {
        Some(JsValue::Object(result)) => parse_file_refs(
            result.get("fileRefs"),
            &format!("detached job {} result", terminal.job_id),
        )?
        .unwrap_or_default(),
        _ => Vec::new(),
    };
    let output_ref = AgentFileReference {
        label: Some(terminal.output.label.clone()),
        path: terminal.output.path.clone(),
    };
    let mut file_refs = vec![output_ref.clone()];
    file_refs.extend(
        existing_file_refs
            .into_iter()
            .filter(|file_ref| file_ref.label != output_ref.label || file_ref.path != output_ref.path),
    );
    let file_refs_value = js_json::to_value(&file_refs).map_err(|error| ControlDbError::new(error.to_string()))?;
    let mut result = JsObject::new();
    result.insert("fileRefs".into(), file_refs_value);
    if terminal_lifecycle == "aborted" {
        insert_optional(
            &mut result,
            "durationMs",
            terminal.duration_ms.map(|value| JsValue::Number(value as f64)),
        );
        insert_optional(
            &mut result,
            "toolCallId",
            terminal.tool_call_id.clone().map(|value| JsValue::String(value.into())),
        );
        let mut details = JsObject::new();
        details.insert("result".into(), JsValue::Object(result));
        return Ok(details);
    }
    match &terminal.outcome {
        DetachedJobOutcome::Completed { summary, .. } => {
            if let Some(summary) = summary {
                result.insert("summary".into(), JsValue::String(summary.clone()));
            }
            insert_optional(
                &mut result,
                "durationMs",
                terminal.duration_ms.map(|value| JsValue::Number(value as f64)),
            );
            insert_optional(
                &mut result,
                "toolCallId",
                terminal.tool_call_id.clone().map(|value| JsValue::String(value.into())),
            );
            let mut details = JsObject::new();
            details.insert("result".into(), JsValue::Object(result));
            Ok(details)
        }
        DetachedJobOutcome::Failed { error, .. } => {
            result.insert("summary".into(), JsValue::String(error.message.clone()));
            insert_optional(
                &mut result,
                "durationMs",
                terminal.duration_ms.map(|value| JsValue::Number(value as f64)),
            );
            insert_optional(
                &mut result,
                "toolCallId",
                terminal.tool_call_id.clone().map(|value| JsValue::String(value.into())),
            );
            let mut details = JsObject::new();
            details.insert(
                "error".into(),
                js_json::to_value(error).map_err(|error| ControlDbError::new(error.to_string()))?,
            );
            details.insert("result".into(), JsValue::Object(result));
            Ok(details)
        }
        DetachedJobOutcome::Aborted { .. } => {
            insert_optional(
                &mut result,
                "durationMs",
                terminal.duration_ms.map(|value| JsValue::Number(value as f64)),
            );
            insert_optional(
                &mut result,
                "toolCallId",
                terminal.tool_call_id.clone().map(|value| JsValue::String(value.into())),
            );
            let mut details = JsObject::new();
            details.insert("result".into(), JsValue::Object(result));
            Ok(details)
        }
    }
}

fn insert_optional(map: &mut JsObject, key: &str, value: Option<JsValue>) {
    if let Some(value) = value {
        map.insert(key.into(), value);
    }
}
