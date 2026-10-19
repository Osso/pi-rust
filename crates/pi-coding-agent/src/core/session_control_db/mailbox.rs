//! Runtime mailbox routing, claims, delivery, and listener registration.

use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};

use super::schema::{add_runtime_mailbox_routing, assert_runtime_mailbox_routing};
use super::{
    ControlDbError, EnqueueRuntimeMailboxMessageInput, EnqueueStoredRuntimeMailboxMessageInput,
    MultiAgentRuntimeOwnershipRow, RUNTIME_MAILBOX_REGISTRATION_MAX_ATTEMPTS, RUNTIME_MAILBOX_ROUTING_MAX_ATTEMPTS,
    RuntimeMailboxAddress, RuntimeMailboxListener, RuntimeMailboxListenerRow, RuntimeMailboxMessage,
    RuntimeMailboxMessageStatus, RuntimeMailboxRegistrationOptions, RuntimeMailboxRow,
    compare_and_write_canonical_mailbox_payload, compare_and_write_serialized_canonical_mailbox_payload,
    get_runtime_process_instance_id, multi_agent_runtime_ownership_rows_equal, parse_canonical_mailbox_payload,
    parse_process_identity, parse_stored_json_object, process_identity_is_alive, read_canonical_mailbox_row_by_id,
    read_canonical_mailbox_row_by_store_ref, read_multi_agent_runtime_ownership_row, read_runtime_mailbox_listener_row,
    require_string_field, runtime_mailbox_agent_id_key, runtime_mailbox_listener_rows_equal,
    runtime_mailbox_message_from_canonical_row, session_health_rows_equal, to_runtime_mailbox_message_status,
    trusted_runtime_mailbox_session_path, validate_mailbox_payload, with_control_db, with_immediate_transaction,
};
use crate::core::runtime_process::{is_verified_pi_runtime_process, read_process_identity};
use crate::core::session_health::{SessionHealthRecord, empty_session_health, end_session_health};
use crate::utils::js_json;

const UPSERT_RUNTIME_MAILBOX_LISTENER_SQL: &str = "
	INSERT INTO runtime_mailbox_listeners (
		recipient_session_id,
		recipient_agent_id_key,
		pid,
		runtime_instance_id,
		session_path,
		session_path_asserted_at,
		updated_at
	)
	VALUES (?, ?, ?, ?, ?, ?, ?)
	ON CONFLICT(recipient_session_id, recipient_agent_id_key) DO UPDATE SET
		pid = excluded.pid,
		runtime_instance_id = excluded.runtime_instance_id,
		session_path = excluded.session_path,
		session_path_asserted_at = excluded.session_path_asserted_at,
		updated_at = excluded.updated_at
";

/// Result of `readRuntimeMailboxMessageForDelivery`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMailboxDeliveryRead {
    pub message: RuntimeMailboxMessage,
    pub payload_data: Option<String>,
    pub payload_valid: bool,
}

/// `enqueueRuntimeMailboxMessage`.
pub fn enqueue_runtime_mailbox_message(
    control_db_path: &str,
    input: &EnqueueRuntimeMailboxMessageInput,
) -> Result<i64, ControlDbError> {
    let result = with_control_db(control_db_path, |db| route_runtime_mailbox_message(db, input))?;
    notify_runtime_mailbox_listener(result.listener.as_ref());
    Ok(result.id)
}

struct RoutedMailboxMessage {
    id: i64,
    listener: Option<RuntimeMailboxListenerRow>,
}

fn route_runtime_mailbox_message(
    db: &Connection,
    input: &EnqueueRuntimeMailboxMessageInput,
) -> Result<RoutedMailboxMessage, ControlDbError> {
    for _ in 0..RUNTIME_MAILBOX_ROUTING_MAX_ATTEMPTS {
        let Some(row) = read_canonical_mailbox_row_by_store_ref(db, &input.store_ref)? else {
            return Err(ControlDbError::new(format!(
                "Runtime mailbox store reference does not exist: {}#{}",
                input.store_ref.session_path, input.store_ref.message_id
            )));
        };
        let context = format!(
            "multi_agent_mailbox_messages:{}#{}",
            input.store_ref.session_path, input.store_ref.message_id
        );
        let message = parse_stored_json_object(&row.data, &context)?;
        validate_mailbox_payload(&Value::Object(message.clone()), &context)?;
        let routed = add_runtime_mailbox_routing(&message, input)?;
        if js_json::stringify(&Value::Object(routed.clone())) == row.data {
            return Ok(RoutedMailboxMessage {
                id: row.id,
                listener: None,
            });
        }
        if !compare_and_write_canonical_mailbox_payload(db, &row, &routed, &crate::utils::iso_time::now_iso_string())? {
            continue;
        }
        return Ok(RoutedMailboxMessage {
            id: row.id,
            listener: read_runtime_mailbox_listener_row(db, &input.recipient)?,
        });
    }
    Err(ControlDbError::new(format!(
        "Runtime mailbox row changed while routing: {}#{}",
        input.store_ref.session_path, input.store_ref.message_id
    )))
}

pub(crate) struct PreparedStoredRuntimeMailboxMessage {
    pub(crate) context: String,
    pub(crate) routed: Map<String, Value>,
    pub(crate) serialized: String,
    pub(crate) updated_at: String,
}

pub(crate) fn prepare_stored_runtime_mailbox_message(
    input: &EnqueueStoredRuntimeMailboxMessageInput,
) -> Result<PreparedStoredRuntimeMailboxMessage, ControlDbError> {
    let context = format!(
        "multi_agent_mailbox_messages:{}#{}",
        input.store_ref.session_path, input.store_ref.message_id
    );
    validate_mailbox_payload(&input.message, &context)?;
    let Value::Object(incoming) = js_json::parse(&js_json::stringify(&input.message))
        .map_err(|_| ControlDbError::new(format!("Invalid persisted JSON at {context}")))?
    else {
        return Err(ControlDbError::new(format!("Invalid persisted object at {context}")));
    };
    let routed = add_runtime_mailbox_routing(
        &incoming,
        &EnqueueRuntimeMailboxMessageInput {
            recipient: input.recipient.clone(),
            sender: input.sender.clone(),
            kind: input.kind,
            store_ref: input.store_ref.clone(),
        },
    )?;
    Ok(PreparedStoredRuntimeMailboxMessage {
        context,
        serialized: js_json::stringify(&Value::Object(routed.clone())),
        routed,
        updated_at: input
            .updated_at
            .clone()
            .unwrap_or_else(crate::utils::iso_time::now_iso_string),
    })
}

/// `enqueueStoredRuntimeMailboxMessage`.
pub fn enqueue_stored_runtime_mailbox_message(
    control_db_path: &str,
    input: &EnqueueStoredRuntimeMailboxMessageInput,
) -> Result<i64, ControlDbError> {
    let prepared = prepare_stored_runtime_mailbox_message(input)?;
    let result = with_control_db(control_db_path, |db| {
        persist_stored_runtime_mailbox_message(db, input, &prepared)
    })?;
    notify_runtime_mailbox_listener(result.listener.as_ref());
    Ok(result.id)
}

fn persist_stored_runtime_mailbox_message(
    db: &Connection,
    input: &EnqueueStoredRuntimeMailboxMessageInput,
    prepared: &PreparedStoredRuntimeMailboxMessage,
) -> Result<RoutedMailboxMessage, ControlDbError> {
    if let Some(existing) = read_canonical_mailbox_row_by_store_ref(db, &input.store_ref)? {
        return validate_existing_stored_runtime_mailbox_message(&existing, input, prepared);
    }
    let inserted = db.execute(
        "INSERT INTO multi_agent_mailbox_messages (session_path, message_id, data, updated_at)
			 VALUES (?, ?, ?, ?)
			 ON CONFLICT(session_path, message_id) DO NOTHING",
        params![
            input.store_ref.session_path,
            input.store_ref.message_id,
            prepared.serialized,
            prepared.updated_at
        ],
    )?;
    if inserted == 1 {
        return Ok(RoutedMailboxMessage {
            id: db.last_insert_rowid(),
            listener: read_runtime_mailbox_listener_row(db, &input.recipient)?,
        });
    }
    let Some(concurrent) = read_canonical_mailbox_row_by_store_ref(db, &input.store_ref)? else {
        return Err(ControlDbError::new(format!(
            "Runtime mailbox store reference disappeared after insert conflict: {}#{}",
            input.store_ref.session_path, input.store_ref.message_id
        )));
    };
    validate_existing_stored_runtime_mailbox_message(&concurrent, input, prepared)
}

fn validate_existing_stored_runtime_mailbox_message(
    existing: &RuntimeMailboxRow,
    input: &EnqueueStoredRuntimeMailboxMessageInput,
    prepared: &PreparedStoredRuntimeMailboxMessage,
) -> Result<RoutedMailboxMessage, ControlDbError> {
    let previous = parse_stored_json_object(&existing.data, &prepared.context)?;
    if !same_mailbox_message_identity(&previous, &prepared.routed, &input.store_ref.message_id) {
        return Err(ControlDbError::new(format!(
            "Mailbox message ID collision: {}#{}",
            input.store_ref.session_path, input.store_ref.message_id
        )));
    }
    assert_runtime_mailbox_routing(
        &previous,
        &EnqueueRuntimeMailboxMessageInput {
            recipient: input.recipient.clone(),
            sender: input.sender.clone(),
            kind: input.kind,
            store_ref: input.store_ref.clone(),
        },
    )?;
    if strip_runtime_mailbox_delivery_state(&existing.data)?
        != strip_runtime_mailbox_delivery_state(&prepared.serialized)?
    {
        return Err(ControlDbError::new(format!(
            "Mailbox message ID collision: {}#{}",
            input.store_ref.session_path, input.store_ref.message_id
        )));
    }
    Ok(RoutedMailboxMessage {
        id: existing.id,
        listener: None,
    })
}

pub(crate) fn same_mailbox_message_identity(
    previous: &Map<String, Value>,
    next: &Map<String, Value>,
    id: &str,
) -> bool {
    let required_identity = [previous, next].iter().all(|message| {
        matches!(message.get("id"), Some(Value::String(_)))
            && matches!(message.get("fromAgentId"), Some(Value::String(_)))
            && matches!(message.get("toAgentId"), Some(Value::String(_)))
            && matches!(message.get("kind"), Some(Value::String(_)))
    });
    if !required_identity {
        return false;
    }
    previous.get("id").and_then(Value::as_str) == Some(id)
        && next.get("id").and_then(Value::as_str) == Some(id)
        && previous.get("fromAgentId") == next.get("fromAgentId")
        && previous.get("toAgentId") == next.get("toAgentId")
        && previous.get("kind") == next.get("kind")
        && previous.get("threadId") == next.get("threadId")
}

fn strip_runtime_mailbox_delivery_state(serialized: &str) -> Result<String, ControlDbError> {
    let mut message = parse_stored_json_object(serialized, "runtime_mailbox_identity")?;
    for key in [
        "claimedAt",
        "claimantProcessIdentity",
        "deliveredAt",
        "error",
        "status",
        "updatedAt",
    ] {
        message.remove(key);
    }
    Ok(js_json::stringify(&Value::Object(message)))
}

/// `readRuntimeMailboxMessageForDelivery`.
pub fn read_runtime_mailbox_message_for_delivery(
    control_db_path: &str,
    id: i64,
) -> Result<Option<RuntimeMailboxDeliveryRead>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(None);
        };
        let message = parse_canonical_mailbox_payload(&row)?;
        let runtime_message = runtime_mailbox_message_from_canonical_row(&row, &message)?;
        let payload_valid = message.get("status").and_then(Value::as_str) == Some("claimed")
            && message.get("claimantProcessIdentity").and_then(Value::as_str)
                == Some(get_runtime_process_instance_id()?);
        Ok(Some(RuntimeMailboxDeliveryRead {
            message: runtime_message,
            payload_data: Some(row.data.clone()),
            payload_valid,
        }))
    })
}

/// `consumeRuntimeMailboxMessage`.
pub fn consume_runtime_mailbox_message(control_db_path: &str, id: i64) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(false);
        };
        Ok(parse_canonical_mailbox_payload(&row)?
            .get("status")
            .and_then(Value::as_str)
            == Some("delivered"))
    })
}

/// `deliverRuntimeMailboxMessage`.
pub fn deliver_runtime_mailbox_message(
    control_db_path: &str,
    id: i64,
    expected_payload_data: Option<&str>,
) -> Result<bool, ControlDbError> {
    update_claimed_canonical_mailbox_message(
        control_db_path,
        id,
        expected_payload_data,
        RuntimeMailboxMessageStatus::Delivered,
        None,
    )
}

fn update_claimed_canonical_mailbox_message(
    control_db_path: &str,
    id: i64,
    expected_payload_data: Option<&str>,
    status: RuntimeMailboxMessageStatus,
    error: Option<&str>,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(false);
        };
        let message = parse_canonical_mailbox_payload(&row)?;
        let current_status =
            to_runtime_mailbox_message_status(message.get("status").and_then(Value::as_str).unwrap_or("failed"));
        if current_status == status {
            return Ok(true);
        }
        if current_status != RuntimeMailboxMessageStatus::Claimed
            || message.get("claimantProcessIdentity").and_then(Value::as_str)
                != Some(get_runtime_process_instance_id()?)
        {
            return Ok(false);
        }
        if let Some(expected_payload_data) = expected_payload_data
            && row.data != expected_payload_data
        {
            return Ok(false);
        }
        let now = crate::utils::iso_time::now_iso_string();
        let mut updated = message.clone();
        updated.insert("status".to_owned(), Value::String(status_name(status).to_owned()));
        updated.insert("updatedAt".to_owned(), Value::String(now.clone()));
        updated.remove("claimedAt");
        updated.remove("claimantProcessIdentity");
        if status == RuntimeMailboxMessageStatus::Delivered {
            updated.insert("deliveredAt".to_owned(), Value::String(now.clone()));
        }
        if status == RuntimeMailboxMessageStatus::Failed {
            updated.insert(
                "error".to_owned(),
                error
                    .map(|error| Value::String(error.to_owned()))
                    .unwrap_or(Value::Null),
            );
        }
        compare_and_write_canonical_mailbox_payload(db, &row, &updated, &now)
    })
}

/// `consumeRuntimeMailboxMessageByStoreRef`.
pub fn consume_runtime_mailbox_message_by_store_ref(
    control_db_path: &str,
    store_ref: &super::RuntimeMailboxStoreRef,
) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_store_ref(db, store_ref)? else {
            return Ok(0);
        };
        let message = parse_canonical_mailbox_payload(&row)?;
        if message.get("status").and_then(Value::as_str) == Some("delivered") {
            return Ok(0);
        }
        let now = crate::utils::iso_time::now_iso_string();
        let mut delivered = message.clone();
        delivered.insert("status".to_owned(), Value::String("delivered".to_owned()));
        delivered.insert("deliveredAt".to_owned(), Value::String(now.clone()));
        delivered.insert("updatedAt".to_owned(), Value::String(now.clone()));
        delivered.remove("claimedAt");
        delivered.remove("claimantProcessIdentity");
        Ok(
            if compare_and_write_canonical_mailbox_payload(db, &row, &delivered, &now)? {
                1
            } else {
                0
            },
        )
    })
}

struct RuntimeMailboxAuthoritySnapshot {
    listener: Option<RuntimeMailboxListenerRow>,
    ownership: Option<MultiAgentRuntimeOwnershipRow>,
}

struct RuntimeMailboxCandidate {
    authority: RuntimeMailboxAuthoritySnapshot,
    data: Map<String, Value>,
    row: RuntimeMailboxRow,
}

/// `recoverDeadRuntimeMailboxClaims`.
pub fn recover_dead_runtime_mailbox_claims(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let candidates = read_recoverable_runtime_mailbox_claims(db, recipient)?;
        let now_iso = crate::utils::iso_time::now_iso_string();
        let mut recovered = 0;
        for candidate in candidates {
            if recover_runtime_mailbox_claim(db, recipient, &candidate, &now_iso)? {
                recovered += 1;
            }
        }
        Ok(recovered)
    })
}

fn read_recoverable_runtime_mailbox_claims(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
) -> Result<Vec<RuntimeMailboxCandidate>, ControlDbError> {
    let rows =
        read_canonical_mailbox_rows_for_recipient(db, recipient, RuntimeMailboxMessageStatus::Claimed, i64::MAX)?;
    let mut candidates = Vec::new();
    for row in rows {
        let authority = read_runtime_mailbox_authority_snapshot(db, recipient, &row.session_path)?;
        if !current_runtime_owns_mailbox_authority(recipient, &authority)? {
            continue;
        }
        let data = parse_canonical_mailbox_payload(&row)?;
        let claimant = require_string_field(&data, "claimantProcessIdentity", "runtime_mailbox_claim")?;
        if runtime_mailbox_claimant_is_live(authority.listener.as_ref(), &claimant) {
            continue;
        }
        candidates.push(RuntimeMailboxCandidate { authority, data, row });
    }
    Ok(candidates)
}

fn recover_runtime_mailbox_claim(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidate: &RuntimeMailboxCandidate,
    now_iso: &str,
) -> Result<bool, ControlDbError> {
    let mut pending = candidate.data.clone();
    pending.insert("status".to_owned(), Value::String("pending".to_owned()));
    pending.insert("updatedAt".to_owned(), Value::String(now_iso.to_owned()));
    pending.remove("claimedAt");
    pending.remove("claimantProcessIdentity");
    let serialized = js_json::stringify(&Value::Object(pending));
    with_immediate_transaction(db, |db| {
        let current_authority = read_runtime_mailbox_authority_snapshot(db, recipient, &candidate.row.session_path)?;
        if !runtime_mailbox_authority_snapshots_equal(&candidate.authority, &current_authority) {
            return Ok(false);
        }
        compare_and_write_serialized_canonical_mailbox_payload(db, &candidate.row, &serialized, now_iso)
    })
}

/// `takeRuntimeMailboxMessagesForDelivery`.
pub fn take_runtime_mailbox_messages_for_delivery(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    is_eligible: impl Fn(&RuntimeMailboxMessage) -> bool,
    limit: usize,
) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let candidates = read_eligible_runtime_mailbox_delivery_candidates(db, recipient, &is_eligible)?;
        deliver_runtime_mailbox_candidates(
            db,
            recipient,
            candidates,
            limit,
            &crate::utils::iso_time::now_iso_string(),
        )
    })
}

fn read_eligible_runtime_mailbox_delivery_candidates(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    is_eligible: &impl Fn(&RuntimeMailboxMessage) -> bool,
) -> Result<Vec<RuntimeMailboxCandidate>, ControlDbError> {
    let rows =
        read_canonical_mailbox_rows_for_recipient(db, recipient, RuntimeMailboxMessageStatus::Pending, i64::MAX)?;
    let mut candidates = Vec::new();
    for row in rows {
        let authority = read_runtime_mailbox_authority_snapshot(db, recipient, &row.session_path)?;
        if !current_runtime_owns_mailbox_authority(recipient, &authority)? {
            continue;
        }
        let data = parse_canonical_mailbox_payload(&row)?;
        let message = runtime_mailbox_message_from_canonical_row(&row, &data)?;
        if is_eligible(&message) {
            candidates.push(RuntimeMailboxCandidate { authority, data, row });
        }
    }
    Ok(candidates)
}

fn deliver_runtime_mailbox_candidates(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidates: Vec<RuntimeMailboxCandidate>,
    limit: usize,
    now_iso: &str,
) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    let mut delivered_messages = Vec::new();
    for candidate in candidates {
        if let Some(delivered) = deliver_runtime_mailbox_candidate(db, recipient, &candidate, now_iso)? {
            delivered_messages.push(delivered);
        }
        if delivered_messages.len() == limit {
            break;
        }
    }
    Ok(delivered_messages)
}

fn deliver_runtime_mailbox_candidate(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidate: &RuntimeMailboxCandidate,
    now_iso: &str,
) -> Result<Option<RuntimeMailboxMessage>, ControlDbError> {
    let mut delivered = candidate.data.clone();
    delivered.insert("status".to_owned(), Value::String("delivered".to_owned()));
    delivered.insert("deliveredAt".to_owned(), Value::String(now_iso.to_owned()));
    delivered.insert("updatedAt".to_owned(), Value::String(now_iso.to_owned()));
    let serialized = js_json::stringify(&Value::Object(delivered.clone()));
    let updated = with_immediate_transaction(db, |db| {
        let current_authority = read_runtime_mailbox_authority_snapshot(db, recipient, &candidate.row.session_path)?;
        if !runtime_mailbox_authority_snapshots_equal(&candidate.authority, &current_authority) {
            return Ok(false);
        }
        compare_and_write_serialized_canonical_mailbox_payload(db, &candidate.row, &serialized, now_iso)
    })?;
    if updated {
        Ok(Some(runtime_mailbox_message_from_canonical_row(
            &candidate.row,
            &delivered,
        )?))
    } else {
        Ok(None)
    }
}

fn read_runtime_mailbox_authority_snapshot(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    session_path: &str,
) -> Result<RuntimeMailboxAuthoritySnapshot, ControlDbError> {
    Ok(RuntimeMailboxAuthoritySnapshot {
        listener: read_runtime_mailbox_listener_row(db, recipient)?,
        ownership: match &recipient.agent_id {
            Some(agent_id) => read_multi_agent_runtime_ownership_row(db, session_path, agent_id)?,
            None => None,
        },
    })
}

fn current_runtime_owns_mailbox_authority(
    recipient: &RuntimeMailboxAddress,
    authority: &RuntimeMailboxAuthoritySnapshot,
) -> Result<bool, ControlDbError> {
    if let Some(listener) = &authority.listener
        && listener.pid == std::process::id()
        && listener.runtime_instance_id.as_deref() == Some(get_runtime_process_instance_id()?)
    {
        return Ok(true);
    }
    if recipient.agent_id.is_none() {
        return Ok(false);
    }
    Ok(persisted_process_identity_is_current(
        authority
            .ownership
            .as_ref()
            .and_then(|ownership| ownership.process_identity.as_deref()),
    ))
}

fn runtime_mailbox_authority_snapshots_equal(
    expected: &RuntimeMailboxAuthoritySnapshot,
    current: &RuntimeMailboxAuthoritySnapshot,
) -> bool {
    runtime_mailbox_listener_rows_equal(expected.listener.as_ref(), current.listener.as_ref())
        && multi_agent_runtime_ownership_rows_equal(expected.ownership.as_ref(), current.ownership.as_ref())
}

/// `claimRuntimeMailboxMessages`.
pub fn claim_runtime_mailbox_messages(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    limit: i64,
) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let candidates = read_claimable_runtime_mailbox_candidates(db, recipient, limit)?;
        claim_runtime_mailbox_candidates(db, recipient, candidates, &crate::utils::iso_time::now_iso_string())
    })
}

fn read_claimable_runtime_mailbox_candidates(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    limit: i64,
) -> Result<Vec<RuntimeMailboxCandidate>, ControlDbError> {
    let rows = read_canonical_mailbox_rows_for_recipient(db, recipient, RuntimeMailboxMessageStatus::Pending, limit)?;
    let mut candidates = Vec::new();
    for row in rows {
        let authority = read_runtime_mailbox_authority_snapshot(db, recipient, &row.session_path)?;
        if !current_runtime_owns_mailbox_authority(recipient, &authority)? {
            continue;
        }
        candidates.push(RuntimeMailboxCandidate {
            authority,
            data: parse_canonical_mailbox_payload(&row)?,
            row,
        });
    }
    Ok(candidates)
}

fn claim_runtime_mailbox_candidates(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidates: Vec<RuntimeMailboxCandidate>,
    now_iso: &str,
) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    let mut claimed_messages = Vec::new();
    for candidate in candidates {
        if let Some(claimed) = claim_runtime_mailbox_candidate(db, recipient, &candidate, now_iso)? {
            claimed_messages.push(claimed);
        }
    }
    Ok(claimed_messages)
}

fn claim_runtime_mailbox_candidate(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidate: &RuntimeMailboxCandidate,
    now_iso: &str,
) -> Result<Option<RuntimeMailboxMessage>, ControlDbError> {
    let mut claimed = candidate.data.clone();
    claimed.insert("claimedAt".to_owned(), Value::String(now_iso.to_owned()));
    claimed.insert(
        "claimantProcessIdentity".to_owned(),
        Value::String(get_runtime_process_instance_id()?.to_owned()),
    );
    claimed.insert("status".to_owned(), Value::String("claimed".to_owned()));
    claimed.insert("updatedAt".to_owned(), Value::String(now_iso.to_owned()));
    let serialized = js_json::stringify(&Value::Object(claimed.clone()));
    let updated = with_immediate_transaction(db, |db| {
        let current_authority = read_runtime_mailbox_authority_snapshot(db, recipient, &candidate.row.session_path)?;
        if !runtime_mailbox_authority_snapshots_equal(&candidate.authority, &current_authority) {
            return Ok(false);
        }
        compare_and_write_serialized_canonical_mailbox_payload(db, &candidate.row, &serialized, now_iso)
    })?;
    if updated {
        Ok(Some(runtime_mailbox_message_from_canonical_row(
            &candidate.row,
            &claimed,
        )?))
    } else {
        Ok(None)
    }
}

fn runtime_mailbox_claimant_is_live(
    listener: Option<&RuntimeMailboxListenerRow>,
    claimant_process_identity: &str,
) -> bool {
    if let Some(listener) = listener {
        return listener.runtime_instance_id.as_deref() == Some(claimant_process_identity);
    }
    persisted_process_identity_is_live(claimant_process_identity)
}

fn persisted_process_identity_is_current(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Ok(current) = read_process_identity(std::process::id()) else {
        return false;
    };
    let Ok(persisted) = parse_process_identity(value) else {
        return false;
    };
    current.pid == persisted.pid && current.start_time_ticks == persisted.start_time_ticks
}

fn persisted_process_identity_is_live(value: &str) -> bool {
    parse_process_identity(value).is_ok_and(|identity| process_identity_is_alive(&identity))
}

fn read_canonical_mailbox_rows_for_recipient(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    status: RuntimeMailboxMessageStatus,
    limit: i64,
) -> Result<Vec<RuntimeMailboxRow>, ControlDbError> {
    let mut statement = db.prepare(
        "SELECT rowid AS id, session_path, message_id, data, updated_at
			 FROM multi_agent_mailbox_messages
			 WHERE CASE WHEN json_valid(data) THEN json_extract(data, '$.status') END = ?
			   AND CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientSessionId') END = ?
			   AND ((? IS NULL AND CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientAgentId') END IS NULL)
			        OR CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientAgentId') END = ?)
			 ORDER BY rowid ASC
			 LIMIT ?",
    )?;
    let rows = statement
        .query_map(
            params![
                status_name(status),
                recipient.session_id,
                recipient.agent_id,
                recipient.agent_id,
                limit
            ],
            |row| {
                Ok(RuntimeMailboxRow {
                    id: row.get(0)?,
                    session_path: row.get(1)?,
                    message_id: row.get(2)?,
                    data: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn status_name(status: RuntimeMailboxMessageStatus) -> &'static str {
    match status {
        RuntimeMailboxMessageStatus::Pending => "pending",
        RuntimeMailboxMessageStatus::Claimed => "claimed",
        RuntimeMailboxMessageStatus::Delivered => "delivered",
        RuntimeMailboxMessageStatus::Failed => "failed",
    }
}

struct RuntimeMailboxRegistration {
    options: RuntimeMailboxRegistrationOptions,
    pid: u32,
    recipient: RuntimeMailboxAddress,
    runtime_instance_id: String,
    session_path: Option<String>,
}

struct RuntimeMailboxRegistrationSnapshot {
    health: Option<SessionHealthRecord>,
    listener: Option<RuntimeMailboxListenerRow>,
}

struct RuntimeMailboxRegistrationPlan {
    now_iso: String,
    should_reconcile_replacement: bool,
    snapshot: RuntimeMailboxRegistrationSnapshot,
}

/// `registerRuntimeMailboxListener`.
pub fn register_runtime_mailbox_listener(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    pid: u32,
    session_path: Option<&str>,
    options: RuntimeMailboxRegistrationOptions,
) -> Result<(), ControlDbError> {
    let runtime_instance_id = match options.runtime_instance_id.clone() {
        Some(runtime_instance_id) => runtime_instance_id,
        None => get_runtime_process_instance_id()?.to_owned(),
    };
    let registration = RuntimeMailboxRegistration {
        runtime_instance_id,
        options,
        pid,
        recipient: recipient.clone(),
        session_path: session_path.map(str::to_owned),
    };
    with_control_db(control_db_path, |db| {
        for _ in 0..RUNTIME_MAILBOX_REGISTRATION_MAX_ATTEMPTS {
            let plan = prepare_runtime_mailbox_registration(db, &registration)?;
            if commit_runtime_mailbox_registration(db, &registration, &plan)? {
                return Ok(());
            }
        }
        Err(ControlDbError::new(format!(
            "Runtime mailbox listener changed repeatedly while registering session {}",
            registration.recipient.session_id
        )))
    })
}

fn prepare_runtime_mailbox_registration(
    db: &Connection,
    registration: &RuntimeMailboxRegistration,
) -> Result<RuntimeMailboxRegistrationPlan, ControlDbError> {
    let snapshot = read_runtime_mailbox_registration_snapshot(db, &registration.recipient)?;
    let runtime_owner_changed = snapshot.listener.as_ref().is_some_and(|listener| {
        listener.pid != registration.pid
            || listener.runtime_instance_id.as_deref() != Some(registration.runtime_instance_id.as_str())
    });
    let should_reconcile_replacement = registration.recipient.agent_id.is_none()
        && registration.options.reconcile_runtime_replacement != Some(false)
        && runtime_owner_changed;
    if should_reconcile_replacement {
        assert_runtime_replacement_allowed(db, registration, &snapshot)?;
    }
    Ok(RuntimeMailboxRegistrationPlan {
        now_iso: crate::utils::iso_time::now_iso_string(),
        should_reconcile_replacement,
        snapshot,
    })
}

fn commit_runtime_mailbox_registration(
    db: &Connection,
    registration: &RuntimeMailboxRegistration,
    plan: &RuntimeMailboxRegistrationPlan,
) -> Result<bool, ControlDbError> {
    with_immediate_transaction(db, |db| {
        let current = read_runtime_mailbox_registration_snapshot(db, &registration.recipient)?;
        if !runtime_mailbox_registration_snapshots_equal(&plan.snapshot, &current) {
            return Ok(false);
        }
        register_runtime_mailbox_listener_in_transaction(
            db,
            registration,
            &plan.now_iso,
            plan.should_reconcile_replacement,
        )?;
        Ok(true)
    })
}

fn read_runtime_mailbox_registration_snapshot(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
) -> Result<RuntimeMailboxRegistrationSnapshot, ControlDbError> {
    Ok(RuntimeMailboxRegistrationSnapshot {
        health: match recipient.agent_id {
            None => read_session_health_row(db, &recipient.session_id)?,
            Some(_) => None,
        },
        listener: read_runtime_mailbox_listener_row(db, recipient)?,
    })
}

fn runtime_mailbox_registration_snapshots_equal(
    expected: &RuntimeMailboxRegistrationSnapshot,
    current: &RuntimeMailboxRegistrationSnapshot,
) -> bool {
    runtime_mailbox_listener_rows_equal(expected.listener.as_ref(), current.listener.as_ref())
        && session_health_rows_equal(expected.health.as_ref(), current.health.as_ref())
}

fn register_runtime_mailbox_listener_in_transaction(
    db: &Connection,
    registration: &RuntimeMailboxRegistration,
    now_iso: &str,
    should_reconcile_replacement: bool,
) -> Result<(), ControlDbError> {
    let RuntimeMailboxRegistration {
        pid,
        recipient,
        runtime_instance_id,
        session_path,
        ..
    } = registration;
    if recipient.agent_id.is_none() {
        retire_superseded_main_runtime_mailbox_listeners(db, &recipient.session_id, *pid, now_iso)?;
    }
    db.execute(
        UPSERT_RUNTIME_MAILBOX_LISTENER_SQL,
        params![
            recipient.session_id,
            runtime_mailbox_agent_id_key(recipient.agent_id.as_deref()),
            pid,
            runtime_instance_id,
            session_path,
            session_path.as_ref().map(|_| now_iso),
            now_iso
        ],
    )?;
    if recipient.agent_id.is_none() {
        upsert_session_health_for_listener(db, &recipient.session_id, *pid, now_iso, should_reconcile_replacement)?;
    }
    Ok(())
}

fn assert_runtime_replacement_allowed(
    db: &Connection,
    registration: &RuntimeMailboxRegistration,
    snapshot: &RuntimeMailboxRegistrationSnapshot,
) -> Result<(), ControlDbError> {
    let existing_listener = snapshot.listener.as_ref();
    let existing_owner_pid = existing_listener
        .map(|listener| listener.pid)
        .or_else(|| snapshot.health.as_ref().and_then(|health| health.pid));
    let Some(existing_owner_pid) = existing_owner_pid else {
        return Ok(());
    };
    if existing_listener.and_then(|listener| listener.runtime_instance_id.as_deref())
        == Some(registration.runtime_instance_id.as_str())
    {
        return Ok(());
    }
    if let Some(runtime_instance_id) = existing_listener.and_then(|listener| listener.runtime_instance_id.as_deref()) {
        match parse_process_identity(runtime_instance_id) {
            Ok(identity) => {
                if !process_identity_is_alive(&identity) {
                    return Ok(());
                }
            }
            Err(_) => {
                if existing_owner_pid == registration.pid {
                    return Ok(());
                }
            }
        }
    }
    let is_runtime_process_alive: Arc<dyn Fn(u32) -> bool + Send + Sync> = registration
        .options
        .is_runtime_process_alive
        .clone()
        .unwrap_or_else(|| Arc::new(super::is_process_alive));
    if !is_runtime_process_alive(existing_owner_pid) {
        return Ok(());
    }
    let session_id = &registration.recipient.session_id;
    let cwd = match existing_listener {
        Some(listener) => read_verified_session_cwd(db, session_id, listener)?,
        None => None,
    };
    let process_context = match cwd {
        Some(cwd) => format!("PID {existing_owner_pid}, cwd {cwd}"),
        None => format!("PID {existing_owner_pid}"),
    };
    Err(ControlDbError::new(format!(
        "Cannot continue session {session_id} because it is open in another Pi process ({process_context}). Close that Pi session, run pi to start a new session, or use pi -r to choose another."
    )))
}

fn read_verified_session_cwd(
    db: &Connection,
    session_id: &str,
    listener: &RuntimeMailboxListenerRow,
) -> Result<Option<String>, ControlDbError> {
    let Some(session_path) = trusted_runtime_mailbox_session_path(listener) else {
        return Ok(None);
    };
    Ok(db
        .query_row(
            "SELECT cwd FROM session_metadata WHERE session_path = ? AND id = ?",
            params![session_path, session_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// `retireRuntimeMailboxListener`.
pub fn retire_runtime_mailbox_listener(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    pid: u32,
) -> Result<bool, ControlDbError> {
    let now_iso = crate::utils::iso_time::now_iso_string();
    with_control_db(control_db_path, |db| {
        with_immediate_transaction(db, |db| {
            retire_runtime_mailbox_listener_row(db, recipient, pid, &now_iso)
        })
    })
}

fn retire_runtime_mailbox_listener_row(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    pid: u32,
    now_iso: &str,
) -> Result<bool, ControlDbError> {
    let retired = db.execute(
        "
			DELETE FROM runtime_mailbox_listeners
			WHERE recipient_session_id = ?
				AND recipient_agent_id_key = ?
				AND pid = ?
			",
        params![
            recipient.session_id,
            runtime_mailbox_agent_id_key(recipient.agent_id.as_deref()),
            pid
        ],
    )? > 0;
    if retired && recipient.agent_id.is_none() {
        retire_session_health_for_listener(db, &recipient.session_id, pid, now_iso)?;
    }
    Ok(retired)
}

fn retire_superseded_main_runtime_mailbox_listeners(
    db: &Connection,
    current_session_id: &str,
    pid: u32,
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let mut statement = db.prepare(
        "
			DELETE FROM runtime_mailbox_listeners
			WHERE recipient_agent_id_key = ''
				AND pid = ?
				AND recipient_session_id <> ?
			RETURNING recipient_session_id
			",
    )?;
    let superseded: Vec<String> = statement
        .query_map(params![pid, current_session_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for session_id in superseded {
        retire_session_health_for_listener(db, &session_id, pid, now_iso)?;
    }
    Ok(())
}

fn retire_session_health_for_listener(
    db: &Connection,
    session_id: &str,
    pid: u32,
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let Some(existing) = read_session_health_row(db, session_id)? else {
        return Ok(());
    };
    if existing.pid != Some(pid) {
        return Ok(());
    }
    write_session_health_row(db, &end_session_health(&existing, now_iso))
}

/// `readRuntimeMailboxListener`.
pub fn read_runtime_mailbox_listener(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
) -> Result<Option<RuntimeMailboxListenerRecord>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_runtime_mailbox_listener_row(db, recipient)? else {
            return Ok(None);
        };
        Ok(Some(RuntimeMailboxListenerRecord {
            pid: row.pid,
            session_path: trusted_runtime_mailbox_session_path(&row),
            updated_at: row.updated_at,
        }))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxListenerRecord {
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    pub updated_at: String,
}

/// `assertMainSessionRuntimeAvailable`.
pub fn assert_main_session_runtime_available(control_db_path: &str, session_id: &str) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
        let registration = RuntimeMailboxRegistration {
            options: RuntimeMailboxRegistrationOptions::default(),
            pid: std::process::id(),
            recipient: RuntimeMailboxAddress {
                agent_id: None,
                session_id: session_id.to_owned(),
            },
            runtime_instance_id: get_runtime_process_instance_id()?.to_owned(),
            session_path: None,
        };
        let snapshot = read_runtime_mailbox_registration_snapshot(db, &registration.recipient)?;
        assert_runtime_replacement_allowed(db, &registration, &snapshot)
    })
}

/// `resolveOwnMainRuntimeCoordinationRecipient`.
pub fn resolve_own_main_runtime_coordination_recipient(
    control_db_path: &str,
) -> Result<Option<RuntimeMailboxAddress>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut statement = db.prepare(
            "
				SELECT recipient_session_id
				FROM runtime_mailbox_listeners
				WHERE recipient_agent_id_key = ''
					AND pid = ?
					AND runtime_instance_id = ?
					AND session_path IS NOT NULL
					AND session_path_asserted_at = updated_at
				",
        )?;
        let rows = statement
            .query_map(params![std::process::id(), get_runtime_process_instance_id()?], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() != 1 {
            return Ok(None);
        }
        Ok(Some(RuntimeMailboxAddress {
            agent_id: None,
            session_id: rows[0].clone(),
        }))
    })
}

/// `listRuntimeMailboxListeners`.
pub fn list_runtime_mailbox_listeners(control_db_path: &str) -> Result<Vec<RuntimeMailboxListener>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut statement = db.prepare(
            "
				SELECT
					recipient_session_id,
					recipient_agent_id_key,
					pid,
					runtime_instance_id,
					session_path,
					session_path_asserted_at,
					updated_at
				FROM runtime_mailbox_listeners
				",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(
                |(session_id, agent_id_key, pid, _, session_path, asserted_at, updated_at)| {
                    let listener_row = RuntimeMailboxListenerRow {
                        pid,
                        runtime_instance_id: None,
                        session_path,
                        session_path_asserted_at: asserted_at,
                        updated_at: updated_at.clone(),
                    };
                    RuntimeMailboxListener {
                        agent_id: if agent_id_key.is_empty() {
                            None
                        } else {
                            Some(agent_id_key)
                        },
                        session_id,
                        pid,
                        session_path: trusted_runtime_mailbox_session_path(&listener_row),
                        updated_at,
                    }
                },
            )
            .collect())
    })
}

fn notify_runtime_mailbox_listener(listener: Option<&RuntimeMailboxListenerRow>) {
    let Some(listener) = listener else {
        return;
    };
    if !is_verified_pi_runtime_process(listener.pid) {
        return;
    }
    if listener.pid == std::process::id() {
        // The Rust runtime installs no SIGUSR2 wake handler; signalling ourselves would
        // terminate the process. Polling delivers the message instead.
        return;
    }
    let Some(pid) = i32::try_from(listener.pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return;
    };
    let _ = rustix::process::kill_process(pid, rustix::process::Signal::USR2);
}

/// `markRuntimeMailboxMessageDelivered`.
pub fn mark_runtime_mailbox_message_delivered(control_db_path: &str, id: i64) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(());
        };
        let message = parse_canonical_mailbox_payload(&row)?;
        if message.get("status").and_then(Value::as_str) == Some("delivered") {
            return Ok(());
        }
        let now = crate::utils::iso_time::now_iso_string();
        let mut delivered = message.clone();
        delivered.insert("status".to_owned(), Value::String("delivered".to_owned()));
        delivered.insert("deliveredAt".to_owned(), Value::String(now.clone()));
        delivered.insert("updatedAt".to_owned(), Value::String(now.clone()));
        delivered.remove("claimedAt");
        delivered.remove("claimantProcessIdentity");
        compare_and_write_canonical_mailbox_payload(db, &row, &delivered, &now)?;
        Ok(())
    })
}

/// `releaseRuntimeMailboxMessageClaim`.
pub fn release_runtime_mailbox_message_claim(control_db_path: &str, id: i64) -> Result<(), ControlDbError> {
    update_claimed_canonical_mailbox_message(control_db_path, id, None, RuntimeMailboxMessageStatus::Pending, None)?;
    Ok(())
}

/// `failRuntimeMailboxMessage`.
pub fn fail_runtime_mailbox_message(control_db_path: &str, id: i64, error_message: &str) -> Result<(), ControlDbError> {
    update_claimed_canonical_mailbox_message(
        control_db_path,
        id,
        None,
        RuntimeMailboxMessageStatus::Failed,
        Some(error_message),
    )?;
    Ok(())
}

/// `readRuntimeMailboxMessage`.
pub fn read_runtime_mailbox_message(
    control_db_path: &str,
    id: i64,
) -> Result<Option<RuntimeMailboxMessage>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(None);
        };
        let message = parse_canonical_mailbox_payload(&row)?;
        Ok(Some(runtime_mailbox_message_from_canonical_row(&row, &message)?))
    })
}

/// `listRuntimeMailboxMessages`.
pub fn list_runtime_mailbox_messages(control_db_path: &str) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut statement = db.prepare(
            "SELECT rowid AS id, session_path, message_id, data, updated_at
				 FROM multi_agent_mailbox_messages
				 WHERE json_valid(data) AND json_type(data, '$.recipientSessionId') = 'text'
				 ORDER BY rowid ASC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(RuntimeMailboxRow {
                    id: row.get(0)?,
                    session_path: row.get(1)?,
                    message_id: row.get(2)?,
                    data: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let message = parse_canonical_mailbox_payload(&row)?;
            messages.push(runtime_mailbox_message_from_canonical_row(&row, &message)?);
        }
        Ok(messages)
    })
}

pub(crate) fn has_deliverable_runtime_mailbox_message(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
) -> Result<bool, ControlDbError> {
    let rows =
        read_canonical_mailbox_rows_for_recipient(db, recipient, RuntimeMailboxMessageStatus::Pending, i64::MAX)?;
    Ok(rows.iter().any(|row| {
        super::parse_json_object(&row.data).is_some_and(|message| !is_lifecycle_notification_payload(&message))
    }))
}

pub(crate) fn is_lifecycle_notification_payload(message: &Map<String, Value>) -> bool {
    if message.get("kind").and_then(Value::as_str) != Some("system") {
        return false;
    }
    if message
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with("terminal:"))
    {
        return true;
    }
    let body = message
        .get("body")
        .and_then(Value::as_str)
        .and_then(super::parse_json_object);
    if body.as_ref().and_then(|body| body.get("type")).and_then(Value::as_str) == Some("multi_agent_terminal") {
        return true;
    }
    message
        .get("threadId")
        .and_then(Value::as_str)
        .is_some_and(|thread_id| {
            ["agent-completed:", "agent-failed:", "agent-waiting-for-input:"]
                .iter()
                .any(|prefix| thread_id.starts_with(prefix))
        })
}

type SessionHealthRow = (
    String,
    u64,
    Option<u32>,
    Option<String>,
    Option<String>,
    String,
    Option<u64>,
    Option<i64>,
    String,
);

pub(crate) fn read_session_health_row(
    db: &Connection,
    session_id: &str,
) -> Result<Option<SessionHealthRecord>, ControlDbError> {
    let row: Option<SessionHealthRow> = db
        .query_row(
            "
			SELECT
				session_id,
				agent_generation,
				pid,
				last_active_at,
				last_checked_at,
				check_status,
				checked_generation,
				check_latency_ms,
				updated_at
			FROM session_health
			WHERE session_id = ?
			",
            params![session_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(session_health_from_row))
}

pub(crate) fn session_health_from_row(row: SessionHealthRow) -> SessionHealthRecord {
    SessionHealthRecord {
        session_id: row.0,
        agent_generation: row.1,
        pid: row.2,
        last_active_at: row.3,
        last_checked_at: row.4,
        check_status: parse_session_check_status(&row.5),
        checked_generation: row.6,
        check_latency_ms: row.7,
        updated_at: row.8,
    }
}

pub(crate) fn write_session_health_row(db: &Connection, health: &SessionHealthRecord) -> Result<(), ControlDbError> {
    db.execute(
        "
		INSERT INTO session_health (
			session_id,
			agent_generation,
			pid,
			last_active_at,
			last_checked_at,
			check_status,
			checked_generation,
			check_latency_ms,
			updated_at
		)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(session_id) DO UPDATE SET
			agent_generation = excluded.agent_generation,
			pid = excluded.pid,
			last_active_at = excluded.last_active_at,
			last_checked_at = excluded.last_checked_at,
			check_status = excluded.check_status,
			checked_generation = excluded.checked_generation,
			check_latency_ms = excluded.check_latency_ms,
			updated_at = excluded.updated_at
		",
        params![
            health.session_id,
            health.agent_generation,
            health.pid,
            health.last_active_at,
            health.last_checked_at,
            session_check_status_name(health.check_status),
            health.checked_generation,
            health.check_latency_ms,
            health.updated_at
        ],
    )?;
    Ok(())
}

fn parse_session_check_status(value: &str) -> crate::core::session_health::SessionCheckStatus {
    match value {
        "ok" => crate::core::session_health::SessionCheckStatus::Ok,
        "dead" => crate::core::session_health::SessionCheckStatus::Dead,
        "timeout" => crate::core::session_health::SessionCheckStatus::Timeout,
        _ => crate::core::session_health::SessionCheckStatus::Never,
    }
}

fn session_check_status_name(status: crate::core::session_health::SessionCheckStatus) -> &'static str {
    match status {
        crate::core::session_health::SessionCheckStatus::Ok => "ok",
        crate::core::session_health::SessionCheckStatus::Dead => "dead",
        crate::core::session_health::SessionCheckStatus::Timeout => "timeout",
        crate::core::session_health::SessionCheckStatus::Never => "never",
    }
}

fn upsert_session_health_for_listener(
    db: &Connection,
    session_id: &str,
    pid: u32,
    now_iso: &str,
    force_new_generation: bool,
) -> Result<(), ControlDbError> {
    let existing =
        read_session_health_row(db, session_id)?.unwrap_or_else(|| empty_session_health(session_id, now_iso));
    let same_generation = !force_new_generation && existing.pid == Some(pid) && existing.agent_generation > 0;
    let agent_generation = if same_generation {
        existing.agent_generation
    } else {
        existing.agent_generation + 1
    };
    write_session_health_row(
        db,
        &SessionHealthRecord {
            pid: Some(pid),
            agent_generation,
            last_active_at: Some(now_iso.to_owned()),
            last_checked_at: Some(now_iso.to_owned()),
            check_status: crate::core::session_health::SessionCheckStatus::Ok,
            checked_generation: Some(agent_generation),
            check_latency_ms: Some(0),
            updated_at: now_iso.to_owned(),
            ..existing
        },
    )
}
