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
    compare_and_write_serialized_canonical_mailbox_payload, get_runtime_process_instance_id,
    multi_agent_runtime_ownership_rows_equal, parse_canonical_mailbox_payload, parse_process_identity,
    parse_stored_json_object, process_identity_is_alive, read_canonical_mailbox_row_by_id,
    read_canonical_mailbox_row_by_store_ref, read_multi_agent_runtime_ownership_row, read_runtime_mailbox_listener_row,
    require_string_field, runtime_mailbox_agent_id_key, runtime_mailbox_listener_rows_equal,
    runtime_mailbox_message_from_canonical_row, session_health_rows_equal, to_runtime_mailbox_message_status,
    trusted_runtime_mailbox_session_path, validate_mailbox_payload, with_control_db, with_immediate_transaction,
};
use crate::core::runtime_process::{is_verified_pi_runtime_process, read_process_identity};
use crate::core::session_health::{SessionHealthRecord, empty_session_health, end_session_health};
use pi_js_compat::js_json::{self, JsString, JsValue};

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
    if input
        .recipient_host
        .as_deref()
        .is_none_or(|host| host == gethostname::gethostname().to_string_lossy())
    {
        notify_runtime_mailbox_listener(result.listener.as_ref());
    }
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
        assert_runtime_mailbox_routing(&super::parse_canonical_mailbox_payload_lossless(&row)?, input)?;
        let routed = add_runtime_mailbox_routing(&message, input)?;
        let serialized = super::serialize_canonical_mailbox_mutation(&row, |canonical| {
            for key in [
                "recipientHost",
                "recipientSessionId",
                "recipientAgentId",
                "senderSessionId",
                "senderAgentId",
            ] {
                if let Some(value) = routed.get(key) {
                    canonical.insert(JsString::from(key), JsValue::from(value));
                }
            }
        })?;
        if serialized == row.data {
            return Ok(RoutedMailboxMessage {
                id: row.id,
                listener: None,
            });
        }
        if !compare_and_write_serialized_canonical_mailbox_payload(
            db,
            &row,
            &serialized,
            &crate::utils::iso_time::now_iso_string(),
        )? {
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
    pub(crate) defaulted_birth: bool,
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
    let mut incoming = incoming;
    let defaulted_birth = incoming.get("createdAt").is_none_or(Value::is_null);
    if defaulted_birth {
        incoming.insert(
            "createdAt".into(),
            Value::String(crate::utils::iso_time::now_iso_string()),
        );
    }
    let routed = add_runtime_mailbox_routing(
        &incoming,
        &EnqueueRuntimeMailboxMessageInput {
            recipient_host: input.recipient_host.clone(),
            recipient: input.recipient.clone(),
            sender: input.sender.clone(),
            kind: input.kind,
            store_ref: input.store_ref.clone(),
        },
    )?;
    super::retention::assert_mailbox_message_fresh(&routed, &context)?;
    Ok(PreparedStoredRuntimeMailboxMessage {
        defaulted_birth,
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
        if let Some(existing) = read_canonical_mailbox_row_by_store_ref(db, &input.store_ref)? {
            return validate_existing_stored_runtime_mailbox_message(&existing, input, &prepared);
        }
        with_immediate_transaction(db, |db| persist_stored_runtime_mailbox_message(db, input, &prepared))
    })?;
    if input
        .recipient_host
        .as_deref()
        .is_none_or(|host| host == gethostname::gethostname().to_string_lossy())
    {
        notify_runtime_mailbox_listener(result.listener.as_ref());
    }
    Ok(result.id)
}

/// Caller owns the transaction and subsequent recipient notification.
pub fn enqueue_stored_runtime_mailbox_message_in_transaction(
    db: &Connection,
    input: &EnqueueStoredRuntimeMailboxMessageInput,
) -> Result<i64, ControlDbError> {
    Ok(persist_stored_runtime_mailbox_message(db, input, &prepare_stored_runtime_mailbox_message(input)?)?.id)
}
pub fn notify_runtime_mailbox_recipient(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
        notify_runtime_mailbox_listener(read_runtime_mailbox_listener_row(db, recipient)?.as_ref());
        Ok(())
    })
}
pub fn is_known_local_session(control_db_path: &str, session_id: &str) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        Ok(db.query_row("SELECT 1 FROM session_metadata WHERE id = ? UNION ALL SELECT 1 FROM session_health WHERE session_id = ? LIMIT 1",params![session_id,session_id],|row|row.get::<_,i64>(0)).optional()?.is_some())
    })
}

fn persist_stored_runtime_mailbox_message(
    db: &Connection,
    input: &EnqueueStoredRuntimeMailboxMessageInput,
    prepared: &PreparedStoredRuntimeMailboxMessage,
) -> Result<RoutedMailboxMessage, ControlDbError> {
    super::retention::assert_mailbox_message_fresh(&prepared.routed, &prepared.context)?;
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
    super::retention::assert_mailbox_message_fresh(&previous, &prepared.context)?;
    if !same_mailbox_message_identity(&previous, &prepared.routed, &input.store_ref.message_id) {
        return Err(ControlDbError::new(format!(
            "Mailbox message ID collision: {}#{}",
            input.store_ref.session_path, input.store_ref.message_id
        )));
    }
    assert_runtime_mailbox_routing(
        &super::parse_canonical_mailbox_payload_lossless(existing)?,
        &EnqueueRuntimeMailboxMessageInput {
            recipient_host: input.recipient_host.clone(),
            recipient: input.recipient.clone(),
            sender: input.sender.clone(),
            kind: input.kind,
            store_ref: input.store_ref.clone(),
        },
    )?;
    if strip_runtime_mailbox_delivery_state(&existing.data, prepared.defaulted_birth)?
        != strip_runtime_mailbox_delivery_state(&prepared.serialized, prepared.defaulted_birth)?
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

fn strip_runtime_mailbox_delivery_state(
    serialized: &str,
    omit_defaulted_birth: bool,
) -> Result<String, ControlDbError> {
    let mut message = super::parse_stored_json_object_lossless(serialized, "runtime_mailbox_identity")?;
    for key in ["claimedAt", "claimantProcessIdentity", "error", "status", "updatedAt"] {
        message.shift_remove(&JsString::from(key));
    }
    if omit_defaulted_birth {
        message.shift_remove(&JsString::from("createdAt"));
    }
    Ok(js_json::stringify_lossless(&JsValue::Object(message)))
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

/// `deliverRuntimeMailboxMessage`.
pub fn deliver_runtime_mailbox_message(
    control_db_path: &str,
    id: i64,
    expected_payload_data: Option<&str>,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(false);
        };
        let message = parse_canonical_mailbox_payload(&row)?;
        if !mailbox_host_is_local(&message)
            || message.get("status").and_then(Value::as_str) != Some("claimed")
            || message.get("claimantProcessIdentity").and_then(Value::as_str)
                != Some(get_runtime_process_instance_id()?)
            || expected_payload_data.is_some_and(|expected| expected != row.data)
        {
            return Ok(false);
        }
        with_immediate_transaction(db, |db| super::compare_and_delete_canonical_mailbox_payload(db, &row))
    })
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
        let serialized = super::serialize_canonical_mailbox_mutation(&row, |updated| {
            updated.insert(
                JsString::from("status"),
                JsValue::String(JsString::from(status_name(status))),
            );
            updated.insert(
                JsString::from("updatedAt"),
                JsValue::String(JsString::from(now.as_str())),
            );
            updated.shift_remove(&JsString::from("claimedAt"));
            updated.shift_remove(&JsString::from("claimantProcessIdentity"));
            if status == RuntimeMailboxMessageStatus::Failed {
                updated.insert(
                    JsString::from("error"),
                    error
                        .map(|text| JsValue::String(JsString::from(text)))
                        .unwrap_or(JsValue::Null),
                );
            }
        })?;
        with_immediate_transaction(db, |db| {
            compare_and_write_serialized_canonical_mailbox_payload(db, &row, &serialized, &now)
        })
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
        if !mailbox_host_is_local(&message) {
            return Ok(0);
        }
        Ok(i64::from(with_immediate_transaction(db, |db| {
            super::compare_and_delete_canonical_mailbox_payload(db, &row)
        })?))
    })
}

fn mailbox_host_is_local(message: &Map<String, Value>) -> bool {
    message
        .get("recipientHost")
        .and_then(Value::as_str)
        .filter(|host| !host.is_empty())
        .is_none_or(|host| host == gethostname::gethostname().to_string_lossy())
}

struct RuntimeMailboxAuthoritySnapshot {
    listener: Option<RuntimeMailboxListenerRow>,
    ownership: Option<MultiAgentRuntimeOwnershipRow>,
}

pub(super) struct RuntimeMailboxCandidate {
    authority: RuntimeMailboxAuthoritySnapshot,
    pub(super) data: Map<String, Value>,
    pub(super) row: RuntimeMailboxRow,
}

pub(super) fn candidate_authority_is_current(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidate: &RuntimeMailboxCandidate,
) -> Result<bool, ControlDbError> {
    Ok(runtime_mailbox_authority_snapshots_equal(
        &candidate.authority,
        &read_runtime_mailbox_authority_snapshot(db, recipient, &candidate.row.session_path)?,
    ))
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
        if runtime_mailbox_claimant_is_live(&claimant)? {
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
    let refs = crate::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs(
        candidate
            .authority
            .listener
            .as_ref()
            .and_then(|listener| listener.session_path.as_deref()),
    )?;
    let expected = pi_js_compat::js_json::JsValue::from(
        &serde_json::json!({"sessionPath":candidate.row.session_path,"messageId":candidate.row.message_id}),
    );
    let mut accepted = false;
    for reference in refs {
        if crate::core::runtime_mailbox_transcript::mailbox_store_refs_equal(&reference, &expected)? {
            accepted = true;
            break;
        }
    }
    let serialized = super::serialize_canonical_mailbox_mutation(&candidate.row, |pending| {
        pending.insert(JsString::from("status"), JsValue::String(JsString::from("pending")));
        pending.insert(JsString::from("updatedAt"), JsValue::String(JsString::from(now_iso)));
        pending.shift_remove(&JsString::from("claimedAt"));
        pending.shift_remove(&JsString::from("claimantProcessIdentity"));
    })?;
    with_immediate_transaction(db, |db| {
        let current_authority = read_runtime_mailbox_authority_snapshot(db, recipient, &candidate.row.session_path)?;
        if !runtime_mailbox_authority_snapshots_equal(&candidate.authority, &current_authority) {
            return Ok(false);
        }
        if accepted {
            super::compare_and_delete_canonical_mailbox_payload(db, &candidate.row)
        } else {
            compare_and_write_serialized_canonical_mailbox_payload(db, &candidate.row, &serialized, now_iso)
        }
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
    if candidate.data.get("kind").and_then(Value::as_str) == Some("steer") && candidate.authority.ownership.is_some() {
        return deliver_runtime_mailbox_steering_candidate(db, recipient, candidate, now_iso);
    }
    claim_runtime_mailbox_candidate(db, recipient, candidate, now_iso)
}

fn deliver_runtime_mailbox_steering_candidate(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    candidate: &RuntimeMailboxCandidate,
    now_iso: &str,
) -> Result<Option<RuntimeMailboxMessage>, ControlDbError> {
    let Some(ownership) = &candidate.authority.ownership else {
        return Ok(None);
    };
    let (Some(process_identity), Some(owner_session_id)) = (&ownership.process_identity, &ownership.owner_session_id)
    else {
        return Ok(None);
    };
    if process_identity.is_empty() || owner_session_id.is_empty() {
        return Ok(None);
    }
    let input = super::CommitMultiAgentSteeringDeliveryInput {
        agent_id: ownership.agent_id.clone(),
        message_id: candidate.row.message_id.clone(),
        owner: super::MultiAgentRuntimeOwnerIdentity {
            agent_id: ownership.owner_agent_id.clone(),
            session_id: owner_session_id.clone(),
        },
        process_identity: parse_process_identity(process_identity)?,
        requested_lifecycle: "running".into(),
        session_path: candidate.row.session_path.clone(),
        updated_at: now_iso.to_owned(),
    };
    let Ok(plan) = super::agents::read_steering_delivery_plan(db, &input)? else {
        return Ok(None);
    };
    if plan.message_data != candidate.row.data {
        return Ok(None);
    }
    if !super::agents::persist_steering_delivery(db, &input, &plan, Some((recipient, candidate)))? {
        return Ok(None);
    }
    let mut claimed = candidate.data.clone();
    claimed.insert("status".into(), Value::String("claimed".into()));
    claimed.insert("claimedAt".into(), Value::String(now_iso.into()));
    claimed.insert("updatedAt".into(), Value::String(now_iso.into()));
    Ok(Some(runtime_mailbox_message_from_canonical_row(
        &candidate.row,
        &claimed,
    )?))
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
    let process_identity = get_runtime_process_instance_id()?;
    let mut claimed = candidate.data.clone();
    claimed.insert("claimedAt".to_owned(), Value::String(now_iso.to_owned()));
    claimed.insert(
        "claimantProcessIdentity".to_owned(),
        Value::String(process_identity.to_owned()),
    );
    claimed.insert("status".to_owned(), Value::String("claimed".to_owned()));
    claimed.insert("updatedAt".to_owned(), Value::String(now_iso.to_owned()));
    let serialized = super::serialize_canonical_mailbox_mutation(&candidate.row, |message| {
        for key in ["claimedAt", "updatedAt"] {
            message.insert(JsString::from(key), JsValue::String(JsString::from(now_iso)));
        }
        message.insert(
            JsString::from("claimantProcessIdentity"),
            JsValue::String(JsString::from(process_identity)),
        );
        message.insert(JsString::from("status"), JsValue::String(JsString::from("claimed")));
    })?;
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

fn runtime_mailbox_claimant_is_live(claimant_process_identity: &str) -> Result<bool, ControlDbError> {
    let current = get_runtime_process_instance_id()?;
    if claimant_process_identity == current {
        return Ok(true);
    }
    let claimant = parse_process_identity(claimant_process_identity)?;
    let current = parse_process_identity(current)?;
    if claimant.pid == current.pid
        && claimant.start_time_ticks == current.start_time_ticks
        && claimant.incarnation.is_some()
        && current.incarnation.is_some()
        && claimant.incarnation != current.incarnation
    {
        return Ok(false);
    }
    Ok(persisted_process_identity_is_live(claimant_process_identity))
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
    let mut statement = db.prepare(&format!(
        "SELECT rowid AS id, session_path, message_id, data, updated_at
			 FROM multi_agent_mailbox_messages
			 WHERE CASE WHEN json_valid(data) THEN json_extract(data, '$.status') END = ?
			   AND CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientSessionId') END = ?
               AND (json_extract(data,'$.recipientHost') IS NULL OR json_extract(data,'$.recipientHost') = ?)
               AND {} > ?
			   AND ((? IS NULL AND CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientAgentId') END IS NULL)
			        OR CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientAgentId') END = ?)
			 ORDER BY rowid ASC
			 LIMIT ?",
        super::retention::MAILBOX_CREATED_AT_MS_SQL
    ))?;
    let rows = statement
        .query_map(
            params![
                status_name(status),
                recipient.session_id,
                gethostname::gethostname().to_string_lossy(),
                super::retention::mailbox_cutoff_ms(),
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
    })?;
    if pid == std::process::id() && registration.runtime_instance_id == get_runtime_process_instance_id()? {
        recover_dead_runtime_mailbox_claims(control_db_path, recipient)?;
    }
    Ok(())
}

fn prepare_runtime_mailbox_registration(
    db: &Connection,
    registration: &RuntimeMailboxRegistration,
) -> Result<RuntimeMailboxRegistrationPlan, ControlDbError> {
    let snapshot = read_runtime_mailbox_registration_snapshot(db, &registration.recipient)?;
    let runtime_owner_changed = snapshot.listener.as_ref().is_none_or(|listener| {
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

/// `readLiveRuntimeMailboxSessionAgentIds`.
pub fn read_live_runtime_mailbox_session_agent_ids(
    control_db_path: &str,
    session_id: &str,
) -> Result<Vec<Option<String>>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut statement=db.prepare("SELECT recipient_agent_id_key,runtime_instance_id FROM runtime_mailbox_listeners WHERE recipient_session_id = ?")?;
        let rows = statement
            .query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut agents = Vec::new();
        for (key, identity) in rows {
            if let Some(identity) = identity.filter(|identity| !identity.is_empty())
                && runtime_mailbox_claimant_is_live(&identity)?
            {
                agents.push((!key.is_empty()).then_some(key));
            }
        }
        Ok(agents)
    })
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
    if listener.pid == std::process::id() && !runtime_mailbox_signal_keepalive_installed() {
        // Signalling ourselves before a SIGUSR2 handler exists would terminate the
        // process (OS default action). Polling delivers the message instead.
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

/// `installRuntimeMailboxSignalKeepalive`: once this process advertises itself as a
/// runtime mailbox listener, stray SIGUSR2 wakes must not terminate it.
static RUNTIME_MAILBOX_SIGNAL_KEEPALIVE_INSTALLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn mark_runtime_mailbox_signal_keepalive_installed() {
    RUNTIME_MAILBOX_SIGNAL_KEEPALIVE_INSTALLED.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn runtime_mailbox_signal_keepalive_installed() -> bool {
    RUNTIME_MAILBOX_SIGNAL_KEEPALIVE_INSTALLED.load(std::sync::atomic::Ordering::SeqCst)
}

/// `markRuntimeMailboxMessageDelivered`: caller's accepted message is fenced by its current payload.
pub fn mark_runtime_mailbox_message_delivered(control_db_path: &str, id: i64) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = read_canonical_mailbox_row_by_id(db, id)? else {
            return Ok(());
        };
        if !mailbox_host_is_local(&parse_canonical_mailbox_payload(&row)?) {
            return Ok(());
        }
        with_immediate_transaction(db, |db| super::compare_and_delete_canonical_mailbox_payload(db, &row))?;
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
    list_runtime_mailbox_messages_by_session_path(control_db_path, None)
}
pub fn list_runtime_mailbox_messages_for_session(
    control_db_path: &str,
    session_path: &str,
) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    list_runtime_mailbox_messages_by_session_path(control_db_path, Some(session_path))
}
fn list_runtime_mailbox_messages_by_session_path(
    control_db_path: &str,
    session_path: Option<&str>,
) -> Result<Vec<RuntimeMailboxMessage>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut statement = db.prepare(&format!(
            "SELECT rowid AS id, session_path, message_id, data, updated_at
				 FROM multi_agent_mailbox_messages
				 WHERE json_valid(data) AND json_type(data, '$.recipientSessionId') = 'text'
                 AND (session_path = ? OR ? IS NULL) AND {} > ?
				 ORDER BY rowid ASC",
            super::retention::MAILBOX_CREATED_AT_MS_SQL
        ))?;
        let rows = statement
            .query_map(
                params![session_path, session_path, super::retention::mailbox_cutoff_ms()],
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
