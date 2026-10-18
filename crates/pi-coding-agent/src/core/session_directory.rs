//! Port of `packages/coding-agent/src/core/session-directory.ts`.
//!
//! Session inventory and broadcast targets backed by `control.sqlite`. Listing reconciles
//! runtime mailbox listeners with session health so every row reflects one live main-session
//! binding per process.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::core::runtime_process::is_pi_runtime_process_alive;
use crate::core::session_control_db::{
    ControlDbError, EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind,
    RuntimeMailboxStoreRef, SessionMetadata, enqueue_runtime_mailbox_message, list_active_session_metadata,
    list_runtime_mailbox_listeners, list_session_health, retire_runtime_mailbox_listener,
    upsert_multi_agent_mailbox_message, write_session_health,
};
use crate::core::session_health::{
    SESSION_CHECK_STALE_MS, SessionBroadcastOutcome, SessionBroadcastResult, SessionCheckStatus, SessionDirectoryEntry,
    SessionHealthRecord, SessionLiveStatus, derive_live_status, empty_session_health, end_session_health,
    is_session_eligible_to_receive, is_sticky_dead, parse_goal_objective,
};
use crate::utils::iso_time;
use pi_ai::utils::uuid::random_uuid;
use pi_js_compat::locale_compare;

/// Options for [`list_sessions`] and [`broadcast_to_sessions`].
#[derive(Clone, Default)]
pub struct SessionDirectoryOptions {
    /// Clock override returning Unix milliseconds.
    pub now: Option<Arc<dyn Fn() -> i64 + Send + Sync>>,
    pub include_ended: Option<bool>,
    pub is_runtime_process_alive: Option<Arc<dyn Fn(u32) -> bool + Send + Sync>>,
}

#[derive(Debug, Clone, Default)]
pub struct BroadcastSessionFilters {
    pub session_ids: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub name: Option<String>,
    pub status: Option<Vec<SessionLiveStatus>>,
}

#[derive(Debug, Clone)]
pub struct BroadcastSessionsInput {
    pub message: String,
    pub filters: Option<BroadcastSessionFilters>,
    pub sender_session_id: String,
    pub sender_session_path: String,
    pub sender_agent_id: Option<Option<String>>,
}

fn resolve_now(options: &SessionDirectoryOptions) -> i64 {
    options.now.as_ref().map_or_else(
        || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0)
        },
        |clock| clock(),
    )
}

#[derive(Debug, Clone)]
struct CurrentMainSessionBinding {
    pid: u32,
    updated_at: String,
    heartbeat_fresh: bool,
}

fn health_by_session_id(control_db_path: &str) -> Result<Vec<(String, SessionHealthRecord)>, ControlDbError> {
    Ok(list_session_health(control_db_path)?
        .into_iter()
        .map(|health| (health.session_id.clone(), health))
        .collect())
}

fn newest_metadata_by_session_id(metadata: Vec<SessionMetadata>) -> Vec<SessionMetadata> {
    let mut seen_session_ids: Vec<String> = Vec::new();
    metadata
        .into_iter()
        .filter(|row| {
            if seen_session_ids.contains(&row.id) {
                return false;
            }
            seen_session_ids.push(row.id.clone());
            true
        })
        .collect()
}

fn reconcile_current_main_session_bindings(
    control_db_path: &str,
    now_ms: i64,
    is_runtime_process_alive: &dyn Fn(u32) -> bool,
) -> Result<Vec<(String, CurrentMainSessionBinding)>, ControlDbError> {
    let mut listeners: Vec<_> = list_runtime_mailbox_listeners(control_db_path)?
        .into_iter()
        .filter(|listener| listener.agent_id.is_none())
        .collect();
    listeners.sort_by(|left, right| {
        let ordering = match (
            pi_js_compat::date::date_parse_millis(&left.updated_at),
            pi_js_compat::date::date_parse_millis(&right.updated_at),
        ) {
            (Some(left_ms), Some(right_ms)) => right_ms.cmp(&left_ms),
            _ => std::cmp::Ordering::Equal,
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
        locale_compare(&left.session_id, &right.session_id)
    });

    let mut seen_pids: Vec<u32> = Vec::new();
    let mut bindings: Vec<(String, CurrentMainSessionBinding)> = Vec::new();
    for listener in listeners {
        let heartbeat_fresh = binding_heartbeat_is_fresh(&listener.updated_at, now_ms);
        let is_superseded = seen_pids.contains(&listener.pid);
        let process_unavailable = !is_runtime_process_alive(listener.pid);
        if is_superseded || process_unavailable {
            retire_runtime_mailbox_listener(
                control_db_path,
                &RuntimeMailboxAddress {
                    agent_id: None,
                    session_id: listener.session_id.clone(),
                },
                listener.pid,
            )?;
            continue;
        }
        seen_pids.push(listener.pid);
        bindings.push((
            listener.session_id.clone(),
            CurrentMainSessionBinding {
                pid: listener.pid,
                updated_at: listener.updated_at,
                heartbeat_fresh,
            },
        ));
    }
    Ok(bindings)
}

fn binding_heartbeat_is_fresh(updated_at: &str, now_ms: i64) -> bool {
    let Some(heartbeat_ms) = pi_js_compat::date::date_parse_millis(updated_at) else {
        return false;
    };
    let heartbeat_age_ms = now_ms - heartbeat_ms;
    (0..=SESSION_CHECK_STALE_MS).contains(&heartbeat_age_ms)
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
    control_db_path: &str,
    session_id: &str,
    binding: &CurrentMainSessionBinding,
    health_map: &mut Vec<(String, SessionHealthRecord)>,
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let existing = lookup_health(health_map, session_id)
        .cloned()
        .unwrap_or_else(|| empty_session_health(session_id, now_iso));
    let Some(next) = synchronized_bound_session_health(&existing, binding, now_iso) else {
        return Ok(());
    };
    write_session_health(control_db_path, &next)?;
    set_health(health_map, next);
    Ok(())
}

fn retire_unbound_session_health_if_needed(
    control_db_path: &str,
    metadata: &SessionMetadata,
    health_map: &mut Vec<(String, SessionHealthRecord)>,
    bindings: &[(String, CurrentMainSessionBinding)],
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let existing = lookup_health(health_map, &metadata.id)
        .cloned()
        .unwrap_or_else(|| empty_session_health(&metadata.id, now_iso));
    if bindings.iter().any(|(session_id, _)| session_id == &metadata.id) || existing.pid.is_none() {
        set_health(health_map, existing);
        return Ok(());
    }
    let retired = end_session_health(&existing, now_iso);
    write_session_health(control_db_path, &retired)?;
    set_health(health_map, retired);
    Ok(())
}

fn ensure_health_synced_from_listeners(
    control_db_path: &str,
    metadata: &[SessionMetadata],
    health_map: &mut Vec<(String, SessionHealthRecord)>,
    bindings: &[(String, CurrentMainSessionBinding)],
    now_iso: &str,
) -> Result<(), ControlDbError> {
    for (session_id, binding) in bindings {
        sync_bound_session_health(control_db_path, session_id, binding, health_map, now_iso)?;
    }
    for row in metadata {
        retire_unbound_session_health_if_needed(control_db_path, row, health_map, bindings, now_iso)?;
    }
    Ok(())
}

fn lookup_health<'a>(
    health_map: &'a [(String, SessionHealthRecord)],
    session_id: &str,
) -> Option<&'a SessionHealthRecord> {
    health_map
        .iter()
        .find(|(id, _)| id == session_id)
        .map(|(_, health)| health)
}

fn set_health(health_map: &mut Vec<(String, SessionHealthRecord)>, health: SessionHealthRecord) {
    match health_map.iter_mut().find(|(id, _)| id == &health.session_id) {
        Some(entry) => entry.1 = health,
        None => health_map.push((health.session_id.clone(), health)),
    }
}

fn to_directory_entry(metadata: &SessionMetadata, health: &SessionHealthRecord, now_ms: i64) -> SessionDirectoryEntry {
    let last_active_at = health
        .last_active_at
        .clone()
        .or_else(|| Some(metadata.modified_at.clone()));
    SessionDirectoryEntry {
        session_id: metadata.id.clone(),
        session_path: metadata.session_path.clone(),
        pid: health.pid,
        status: derive_live_status(health, last_active_at.as_deref(), now_ms, SESSION_CHECK_STALE_MS),
        started_at: metadata.created_at.clone(),
        last_active_at,
        name: metadata.name.clone(),
        goal: parse_goal_objective(metadata.goal_json.as_deref().map(Value::from).as_ref()),
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
    control_db_path: &str,
    options: &SessionDirectoryOptions,
) -> Result<Vec<SessionDirectoryEntry>, ControlDbError> {
    let now_ms = resolve_now(options);
    let now_iso = iso_time::format_iso_millis(now_ms);
    let metadata = newest_metadata_by_session_id(
        list_active_session_metadata(control_db_path)?
            .into_iter()
            .filter(|row| row.is_subagent != Some(true))
            .collect(),
    );
    let mut health_map = health_by_session_id(control_db_path)?;
    let is_runtime_process_alive = options.is_runtime_process_alive.as_ref().map_or_else(
        || Arc::new(is_pi_runtime_process_alive) as Arc<dyn Fn(u32) -> bool + Send + Sync>,
        Arc::clone,
    );
    let bindings = reconcile_current_main_session_bindings(control_db_path, now_ms, is_runtime_process_alive.as_ref())?;
    ensure_health_synced_from_listeners(control_db_path, &metadata, &mut health_map, &bindings, &now_iso)?;
    let entries: Vec<SessionDirectoryEntry> = metadata
        .iter()
        .map(|row| {
            let health = lookup_health(&health_map, &row.id)
                .cloned()
                .unwrap_or_else(|| empty_session_health(&row.id, &now_iso));
            to_directory_entry(row, &health, now_ms)
        })
        .collect();
    Ok(entries
        .into_iter()
        .filter(|entry| options.include_ended != Some(false) || entry.status != SessionLiveStatus::Ended)
        .collect())
}

fn current_main_session_ids(
    control_db_path: &str,
    now_ms: i64,
    is_runtime_process_alive: &dyn Fn(u32) -> bool,
) -> Result<Vec<String>, ControlDbError> {
    Ok(
        reconcile_current_main_session_bindings(control_db_path, now_ms, is_runtime_process_alive)?
            .into_iter()
            .map(|(session_id, _)| session_id)
            .collect(),
    )
}

fn matches_filters(entry: &SessionDirectoryEntry, filters: Option<&BroadcastSessionFilters>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if let Some(session_ids) = &filters.session_ids
        && !session_ids.is_empty()
        && !session_ids.contains(&entry.session_id)
    {
        return false;
    }
    if let Some(cwd) = &filters.cwd
        && entry.cwd.as_deref() != Some(cwd.as_str())
    {
        return false;
    }
    if let Some(name) = &filters.name
        && entry.name.as_deref() != Some(name.as_str())
    {
        return false;
    }
    if let Some(status) = &filters.status
        && !status.is_empty()
        && !status.contains(&entry.status)
    {
        return false;
    }
    true
}

fn entry_health(entry: &SessionDirectoryEntry) -> SessionHealthRecord {
    SessionHealthRecord {
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
    }
}

fn broadcast_result(
    entry: &SessionDirectoryEntry,
    outcome: SessionBroadcastOutcome,
    error: Option<&str>,
) -> SessionBroadcastResult {
    SessionBroadcastResult {
        session_id: entry.session_id.clone(),
        outcome,
        check_status: entry.check_status,
        error: error.map(str::to_owned),
    }
}

/// `broadcastToSessions`.
pub fn broadcast_to_sessions(
    control_db_path: &str,
    input: &BroadcastSessionsInput,
    options: &SessionDirectoryOptions,
) -> Result<Vec<SessionBroadcastResult>, ControlDbError> {
    let message = input.message.trim();
    if message.is_empty() {
        return Err(ControlDbError::new("broadcast requires a non-empty message"));
    }

    let now_ms = resolve_now(options);
    let stable_options = SessionDirectoryOptions {
        now: Some(Arc::new(move || now_ms)),
        include_ended: options.include_ended,
        is_runtime_process_alive: options.is_runtime_process_alive.clone(),
    };
    let is_runtime_process_alive = options.is_runtime_process_alive.as_ref().map_or_else(
        || Arc::new(is_pi_runtime_process_alive) as Arc<dyn Fn(u32) -> bool + Send + Sync>,
        Arc::clone,
    );
    let current_session_ids = current_main_session_ids(control_db_path, now_ms, is_runtime_process_alive.as_ref())?;
    let mut seen_session_ids: Vec<String> = Vec::new();
    let entries: Vec<SessionDirectoryEntry> = list_sessions(control_db_path, &stable_options)?
        .into_iter()
        .filter(|entry| {
            if !current_session_ids.contains(&entry.session_id) || seen_session_ids.contains(&entry.session_id) {
                return false;
            }
            seen_session_ids.push(entry.session_id.clone());
            true
        })
        .collect();
    let sender_agent_id = input.sender_agent_id.clone().unwrap_or(None);
    let mut results = Vec::new();

    for entry in entries {
        if !matches_filters(&entry, input.filters.as_ref()) {
            results.push(broadcast_result(&entry, SessionBroadcastOutcome::SkippedFilter, None));
            continue;
        }
        if entry.check_status == SessionCheckStatus::Dead || is_sticky_dead(&entry_health(&entry)) {
            results.push(broadcast_result(&entry, SessionBroadcastOutcome::SkippedDead, None));
            continue;
        }
        if entry.check_status == SessionCheckStatus::Timeout {
            results.push(broadcast_result(
                &entry,
                SessionBroadcastOutcome::Timeout,
                Some("session check timed out"),
            ));
            continue;
        }
        if !entry.eligible_to_receive || entry.pid.is_none() {
            results.push(broadcast_result(
                &entry,
                SessionBroadcastOutcome::CheckFailed,
                Some("session is not eligible to receive messages"),
            ));
            continue;
        }
        match send_broadcast(control_db_path, input, message, &sender_agent_id, &entry) {
            Ok(()) => results.push(broadcast_result(&entry, SessionBroadcastOutcome::Sent, None)),
            Err(error) => results.push(broadcast_result(
                &entry,
                SessionBroadcastOutcome::SendFailed,
                Some(&error.to_string()),
            )),
        }
    }

    Ok(results)
}

fn send_broadcast(
    control_db_path: &str,
    input: &BroadcastSessionsInput,
    message: &str,
    sender_agent_id: &Option<String>,
    entry: &SessionDirectoryEntry,
) -> Result<(), ControlDbError> {
    let message_id = format!("broadcast_{}", random_uuid());
    let payload: Value = json!({
        "body": message,
        "fromAgentId": sender_agent_id.clone().unwrap_or_else(|| "main".to_owned()),
        "id": message_id,
        "kind": "message",
        "status": "pending",
        "toAgentId": "main",
    });
    upsert_multi_agent_mailbox_message(control_db_path, &input.sender_session_path, &message_id, &payload)?;
    enqueue_runtime_mailbox_message(
        control_db_path,
        &EnqueueRuntimeMailboxMessageInput {
            kind: RuntimeMailboxMessageKind::Message,
            recipient: RuntimeMailboxAddress {
                agent_id: None,
                session_id: entry.session_id.clone(),
            },
            sender: RuntimeMailboxAddress {
                agent_id: sender_agent_id.clone(),
                session_id: input.sender_session_id.clone(),
            },
            store_ref: RuntimeMailboxStoreRef {
                message_id,
                session_path: input.sender_session_path.clone(),
            },
        },
    )?;
    Ok(())
}
