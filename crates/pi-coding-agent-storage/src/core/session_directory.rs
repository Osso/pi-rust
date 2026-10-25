//! Session inventory and broadcast (`core/session-directory.ts`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::collator::{locale_compare, locale_compare_utf16};
use pi_js_compat::js_json::{JsString, JsValue};

use crate::core::runtime_process::is_pi_runtime_process_alive;
use crate::core::session_control_db::{
    ControlDbError, EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind,
    RuntimeMailboxStoreRef, SessionMetadata, enqueue_stored_runtime_mailbox_message, list_active_session_metadata,
    list_runtime_mailbox_listeners, list_session_health, retire_runtime_mailbox_listener, write_session_health,
};
use crate::core::session_health::{
    SESSION_CHECK_STALE_MS, SessionBroadcastOutcome, SessionBroadcastResult, SessionCheckStatus, SessionDirectoryEntry,
    SessionHealthRecord, SessionLiveStatus, derive_live_status, empty_session_health, end_session_health,
    is_session_eligible_to_receive, is_sticky_dead, parse_goal_objective,
};
use crate::utils::iso_time::format_iso_millis;

/// Runtime process liveness probe, injected for deterministic tests.
pub type RuntimeProcessProbe = Arc<dyn Fn(u32) -> bool + Send + Sync>;

#[derive(Clone, Default)]
pub struct SessionDirectoryOptions {
    /// Fixed `Date.now()` milliseconds; defaults to the wall clock.
    pub now_ms: Option<i64>,
    pub include_ended: Option<bool>,
    pub is_runtime_process_alive: Option<RuntimeProcessProbe>,
}

pub struct BroadcastSessionFilters {
    pub session_ids: Option<Vec<JsString>>,
    pub cwd: Option<JsString>,
    pub name: Option<JsString>,
    pub status: Option<Vec<SessionLiveStatus>>,
}

pub struct BroadcastSessionsInput {
    pub message: JsString,
    pub filters: Option<BroadcastSessionFilters>,
    pub sender_session_id: JsString,
    pub sender_session_path: JsString,
    pub sender_agent_id: Option<JsString>,
}

#[derive(Debug, Clone)]
struct CurrentMainSessionBinding {
    pid: u32,
    updated_at: String,
    heartbeat_fresh: bool,
}

fn resolve_now_ms(options: &SessionDirectoryOptions) -> i64 {
    options.now_ms.unwrap_or_else(now_millis)
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch");
    i64::try_from(elapsed.as_millis()).expect("timestamp fits in i64")
}

fn runtime_probe(options: &SessionDirectoryOptions) -> RuntimeProcessProbe {
    options
        .is_runtime_process_alive
        .clone()
        .unwrap_or_else(|| Arc::new(is_pi_runtime_process_alive))
}

fn health_by_session_id(control_db_path: &JsString) -> Result<HashMap<JsString, SessionHealthRecord>, ControlDbError> {
    let mut map = HashMap::new();
    for health in list_session_health(control_db_path)? {
        map.insert(health.session_id.clone(), health);
    }
    Ok(map)
}

fn newest_metadata_by_session_id(metadata: Vec<SessionMetadata>) -> Vec<SessionMetadata> {
    let mut seen_session_ids = HashSet::new();
    metadata
        .into_iter()
        .filter(|row| seen_session_ids.insert(row.id.clone()))
        .collect()
}

/// `reconcileSessionRuntimeBindings`.
pub fn reconcile_session_runtime_bindings(
    control_db_path: &JsString,
    options: &SessionDirectoryOptions,
) -> Result<(), ControlDbError> {
    let now_ms = resolve_now_ms(options);
    reconcile_current_main_session_bindings(control_db_path, now_ms, &runtime_probe(options)).map(|_| ())
}

fn reconcile_current_main_session_bindings(
    control_db_path: &JsString,
    now_ms: i64,
    is_runtime_process_alive: &RuntimeProcessProbe,
) -> Result<HashMap<JsString, CurrentMainSessionBinding>, ControlDbError> {
    let mut listeners = list_runtime_mailbox_listeners(control_db_path)?;
    listeners.retain(|listener| listener.agent_id.is_none());
    listeners.sort_by(|left, right| {
        let updated_at_order = locale_compare(&right.updated_at, &left.updated_at);
        if updated_at_order.is_ne() {
            return updated_at_order;
        }
        locale_compare_utf16(left.session_id.units(), right.session_id.units())
    });

    let mut seen_pids = HashSet::new();
    let mut bindings = HashMap::new();
    for listener in listeners {
        let heartbeat_fresh = binding_heartbeat_is_fresh(&listener.updated_at, now_ms);
        let is_superseded = !seen_pids.insert(listener.pid);
        let process_unavailable = !is_runtime_process_alive(listener.pid);
        if is_superseded || process_unavailable {
            retire_runtime_mailbox_listener(
                control_db_path,
                &RuntimeMailboxAddress {
                    agent_id: None,
                    session_id: listener.session_id,
                },
                listener.pid,
            )?;
            continue;
        }
        bindings.insert(
            listener.session_id,
            CurrentMainSessionBinding {
                pid: listener.pid,
                updated_at: listener.updated_at,
                heartbeat_fresh,
            },
        );
    }
    Ok(bindings)
}

fn binding_heartbeat_is_fresh(updated_at: &str, now_ms: i64) -> bool {
    let heartbeat_ms = pi_js_compat::date::date_parse(updated_at);
    let heartbeat_age_ms = now_ms as f64 - heartbeat_ms;
    heartbeat_age_ms.is_finite() && heartbeat_age_ms >= 0.0 && heartbeat_age_ms <= SESSION_CHECK_STALE_MS as f64
}

fn synchronized_bound_session_health(
    existing: &SessionHealthRecord,
    binding: &CurrentMainSessionBinding,
    now_iso: &str,
) -> Option<SessionHealthRecord> {
    let same_generation = existing.pid == Some(binding.pid) && existing.agent_generation > 0;
    let agent_generation = if same_generation {
        existing.agent_generation
    } else {
        existing.agent_generation + 1
    };
    let check_status = if binding.heartbeat_fresh {
        SessionCheckStatus::Ok
    } else {
        SessionCheckStatus::Timeout
    };
    let last_checked_at = if binding.heartbeat_fresh {
        binding.updated_at.clone()
    } else {
        now_iso.to_owned()
    };
    let already_synced = existing.pid == Some(binding.pid)
        && existing.last_active_at.as_deref() == Some(binding.updated_at.as_str())
        && existing.last_checked_at.as_deref() == Some(last_checked_at.as_str())
        && existing.check_status == check_status
        && existing.checked_generation == Some(agent_generation);
    if already_synced {
        return None;
    }
    Some(SessionHealthRecord {
        pid: Some(binding.pid),
        agent_generation,
        last_active_at: Some(binding.updated_at.clone()),
        last_checked_at: Some(last_checked_at),
        check_status,
        checked_generation: Some(agent_generation),
        check_latency_ms: Some(0),
        updated_at: now_iso.to_owned(),
        ..existing.clone()
    })
}

fn sync_bound_session_health(
    control_db_path: &JsString,
    session_id: &JsString,
    binding: &CurrentMainSessionBinding,
    health_map: &mut HashMap<JsString, SessionHealthRecord>,
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let existing = health_map
        .get(session_id)
        .cloned()
        .unwrap_or_else(|| empty_session_health(session_id, now_iso));
    let Some(next) = synchronized_bound_session_health(&existing, binding, now_iso) else {
        return Ok(());
    };
    write_session_health(control_db_path, &next)?;
    health_map.insert(session_id.clone(), next);
    Ok(())
}

fn retire_unbound_session_health_if_needed(
    control_db_path: &JsString,
    metadata: &SessionMetadata,
    health_map: &mut HashMap<JsString, SessionHealthRecord>,
    bindings: &HashMap<JsString, CurrentMainSessionBinding>,
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let existing = health_map
        .get(&metadata.id)
        .cloned()
        .unwrap_or_else(|| empty_session_health(&metadata.id, now_iso));
    if bindings.contains_key(&metadata.id) || existing.pid.is_none() {
        health_map.insert(metadata.id.clone(), existing);
        return Ok(());
    }
    let retired = end_session_health(&existing, now_iso);
    write_session_health(control_db_path, &retired)?;
    health_map.insert(metadata.id.clone(), retired);
    Ok(())
}

fn ensure_health_synced_from_listeners(
    control_db_path: &JsString,
    metadata: &[SessionMetadata],
    health_map: &mut HashMap<JsString, SessionHealthRecord>,
    bindings: &HashMap<JsString, CurrentMainSessionBinding>,
    now_ms: i64,
) -> Result<(), ControlDbError> {
    let now_iso = format_iso_millis(now_ms);
    for (session_id, binding) in bindings {
        sync_bound_session_health(control_db_path, session_id, binding, health_map, &now_iso)?;
    }
    for row in metadata {
        retire_unbound_session_health_if_needed(control_db_path, row, health_map, bindings, &now_iso)?;
    }
    Ok(())
}

fn to_directory_entry(metadata: &SessionMetadata, health: &SessionHealthRecord, now_ms: i64) -> SessionDirectoryEntry {
    let last_active_at = health
        .last_active_at
        .clone()
        .unwrap_or_else(|| metadata.modified_at.clone());
    SessionDirectoryEntry {
        session_id: metadata.id.clone(),
        session_path: metadata.session_path.clone(),
        pid: health.pid,
        status: derive_live_status(health, Some(&last_active_at), now_ms, SESSION_CHECK_STALE_MS),
        started_at: metadata.created_at.clone(),
        last_active_at: Some(last_active_at),
        name: metadata.name.clone(),
        goal: parse_goal_objective(metadata.goal_json.as_ref()),
        cwd: (!metadata.cwd.is_empty()).then(|| metadata.cwd.clone()),
        last_checked_at: health.last_checked_at.clone(),
        check_status: health.check_status,
        check_latency_ms: health.check_latency_ms,
        agent_generation: health.agent_generation,
        checked_generation: health.checked_generation,
        eligible_to_receive: is_session_eligible_to_receive(health),
    }
}

/// `listSessions`.
pub fn list_sessions(
    control_db_path: &JsString,
    options: &SessionDirectoryOptions,
) -> Result<Vec<SessionDirectoryEntry>, ControlDbError> {
    let now_ms = resolve_now_ms(options);
    let now_iso = format_iso_millis(now_ms);
    let metadata = newest_metadata_by_session_id(
        list_active_session_metadata(control_db_path)?
            .into_iter()
            .filter(|row| row.is_subagent != Some(true))
            .collect(),
    );
    let mut health_map = health_by_session_id(control_db_path)?;
    let bindings = reconcile_current_main_session_bindings(control_db_path, now_ms, &runtime_probe(options))?;
    ensure_health_synced_from_listeners(control_db_path, &metadata, &mut health_map, &bindings, now_ms)?;
    Ok(metadata
        .iter()
        .map(|row| {
            let health = health_map
                .get(&row.id)
                .cloned()
                .unwrap_or_else(|| empty_session_health(&row.id, &now_iso));
            to_directory_entry(row, &health, now_ms)
        })
        .filter(|entry| options.include_ended != Some(false) || entry.status != SessionLiveStatus::Ended)
        .collect())
}

fn current_main_session_ids(
    control_db_path: &JsString,
    now_ms: i64,
    is_runtime_process_alive: &RuntimeProcessProbe,
) -> Result<HashSet<JsString>, ControlDbError> {
    Ok(
        reconcile_current_main_session_bindings(control_db_path, now_ms, is_runtime_process_alive)?
            .into_keys()
            .collect(),
    )
}

fn matches_filters(entry: &SessionDirectoryEntry, filters: Option<&BroadcastSessionFilters>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if filters
        .session_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty() && !ids.contains(&entry.session_id))
    {
        return false;
    }
    if filters.cwd.as_ref().is_some_and(|cwd| entry.cwd.as_ref() != Some(cwd)) {
        return false;
    }
    if filters
        .name
        .as_ref()
        .is_some_and(|name| entry.name.as_ref() != Some(name))
    {
        return false;
    }
    if filters
        .status
        .as_ref()
        .is_some_and(|statuses| !statuses.is_empty() && !statuses.contains(&entry.status))
    {
        return false;
    }
    true
}

/// `broadcastToSessions`.
pub fn broadcast_to_sessions(
    control_db_path: &JsString,
    input: &BroadcastSessionsInput,
    options: &SessionDirectoryOptions,
) -> Result<Vec<SessionBroadcastResult>, ControlDbError> {
    let message = input.message.trim();
    if message.is_empty() {
        return Err(ControlDbError::new("broadcast requires a non-empty message"));
    }

    let now_ms = resolve_now_ms(options);
    let stable_options = SessionDirectoryOptions {
        now_ms: Some(now_ms),
        include_ended: options.include_ended,
        is_runtime_process_alive: options.is_runtime_process_alive.clone(),
    };
    let is_runtime_process_alive = runtime_probe(options);
    let current_session_ids = current_main_session_ids(control_db_path, now_ms, &is_runtime_process_alive)?;
    let mut seen_session_ids = HashSet::new();
    let entries: Vec<SessionDirectoryEntry> = list_sessions(control_db_path, &stable_options)?
        .into_iter()
        .filter(|entry| {
            current_session_ids.contains(&entry.session_id) && seen_session_ids.insert(entry.session_id.clone())
        })
        .collect();

    let mut results = Vec::new();
    for entry in entries {
        if !matches_filters(&entry, input.filters.as_ref()) {
            results.push(SessionBroadcastResult {
                session_id: entry.session_id,
                outcome: SessionBroadcastOutcome::SkippedFilter,
                check_status: entry.check_status,
                error: None,
            });
            continue;
        }

        let sticky_dead_record = SessionHealthRecord {
            session_id: entry.session_id.clone(),
            agent_generation: entry.agent_generation,
            pid: entry.pid,
            last_active_at: entry.last_active_at.clone(),
            last_checked_at: entry.last_checked_at.clone(),
            check_status: entry.check_status,
            checked_generation: entry.checked_generation,
            check_latency_ms: entry.check_latency_ms,
            updated_at: entry
                .last_checked_at
                .clone()
                .unwrap_or_else(|| entry.started_at.clone()),
        };
        if entry.check_status == SessionCheckStatus::Dead || is_sticky_dead(&sticky_dead_record) {
            results.push(SessionBroadcastResult {
                session_id: entry.session_id,
                outcome: SessionBroadcastOutcome::SkippedDead,
                check_status: entry.check_status,
                error: None,
            });
            continue;
        }

        if entry.check_status == SessionCheckStatus::Timeout {
            results.push(SessionBroadcastResult {
                session_id: entry.session_id,
                outcome: SessionBroadcastOutcome::Timeout,
                check_status: entry.check_status,
                error: Some("session check timed out".into()),
            });
            continue;
        }

        if !entry.eligible_to_receive || entry.pid.is_none() {
            results.push(SessionBroadcastResult {
                session_id: entry.session_id,
                outcome: SessionBroadcastOutcome::CheckFailed,
                check_status: entry.check_status,
                error: Some("session is not eligible to receive messages".into()),
            });
            continue;
        }

        match send_broadcast(control_db_path, &message, input, &entry) {
            Ok(()) => results.push(SessionBroadcastResult {
                session_id: entry.session_id,
                outcome: SessionBroadcastOutcome::Sent,
                check_status: entry.check_status,
                error: None,
            }),
            Err(error) => results.push(SessionBroadcastResult {
                session_id: entry.session_id,
                outcome: SessionBroadcastOutcome::SendFailed,
                check_status: entry.check_status,
                error: Some(error.message().clone()),
            }),
        }
    }

    Ok(results)
}

fn send_broadcast(
    control_db_path: &JsString,
    message: &JsString,
    input: &BroadcastSessionsInput,
    entry: &SessionDirectoryEntry,
) -> Result<(), ControlDbError> {
    let message_id = format!("broadcast_{}", random_uuid()?);
    enqueue_stored_runtime_mailbox_message(
        control_db_path,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient_host: None,
            kind: RuntimeMailboxMessageKind::Message,
            message: mailbox_payload(&message_id, message, input.sender_agent_id.as_ref()),
            recipient: RuntimeMailboxAddress {
                agent_id: None,
                session_id: entry.session_id.clone(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: input.sender_agent_id.clone(),
                session_id: input.sender_session_id.clone(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.into(),
                session_path: input.sender_session_path.clone(),
            },
            updated_at: None,
        },
    )?;
    Ok(())
}

fn mailbox_payload(message_id: &str, body: &JsString, sender_agent_id: Option<&JsString>) -> JsValue {
    pi_js_compat::js_json!({
        "body": body,
        "fromAgentId": sender_agent_id.cloned().unwrap_or_else(|| "main".into()),
        "id": message_id,
        "kind": "message",
        "status": "pending",
        "toAgentId": "main",
    })
}
