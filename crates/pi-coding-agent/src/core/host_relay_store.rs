//! Frozen `host-relay-store.ts`: canonical SQLite queues and persistent relay cursors.
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue};
use rusqlite::{Connection, OptionalExtension, params};

use super::session_control_db::{self as control, ControlDbError, RuntimeMailboxAddress};

#[derive(Debug, Clone, PartialEq)]
pub struct RelayMailbox {
    pub seq: i64,
    pub session_path: JsString,
    pub message_id: JsString,
    pub data: JsValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayChannel {
    pub origin_host: JsString,
    pub origin_id: i64,
    pub sender_session_id: JsString,
    pub sender_agent_id: Option<JsString>,
    pub body: JsString,
    pub created_at: JsString,
}

impl RelayChannel {
    pub fn to_value(&self) -> JsValue {
        let mut object = JsValue::from(serde_json::json!({"type":"channel"}))
            .as_object()
            .unwrap()
            .clone();
        object.insert("originHost".into(), JsValue::String(self.origin_host.clone()));
        for (key, value) in [
            ("originId", JsValue::Number(self.origin_id as f64)),
            ("senderSessionId", JsValue::String(self.sender_session_id.clone())),
            (
                "senderAgentId",
                self.sender_agent_id
                    .clone()
                    .map(JsValue::String)
                    .unwrap_or(JsValue::Null),
            ),
            ("body", JsValue::String(self.body.clone())),
            ("createdAt", JsValue::String(self.created_at.clone())),
        ] {
            object.insert(key.into(), value);
        }
        JsValue::Object(object)
    }
}

pub fn require_text(value: Option<&JsValue>, field: &str) -> Result<JsString, ControlDbError> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| !text.trim().is_empty())
        .cloned()
        .ok_or_else(|| ControlDbError::new(js_format!("Invalid relay {}", field)))
}

pub fn nullable_text(value: Option<&JsValue>, field: &str) -> Result<Option<JsString>, ControlDbError> {
    if value == Some(&JsValue::Null) {
        Ok(None)
    } else {
        require_text(value, field).map(Some)
    }
}

pub(crate) fn positive_integer(value: Option<&JsValue>) -> Result<i64, ControlDbError> {
    value
        .and_then(JsValue::as_f64)
        .filter(|n| n.fract() == 0.0 && *n >= 1.0 && *n <= 9_007_199_254_740_991.0)
        .map(|n| n as i64)
        .ok_or_else(|| ControlDbError::new("Invalid relay sequence/id"))
}
fn read_counter(db: &Connection, key: &JsString) -> Result<i64, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT last_seen_id FROM shared_channel_cursors WHERE session_id = ? AND agent_id_key = ''",
            [key],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}
fn write_counter(db: &Connection, key: &JsString, value: i64) -> Result<(), ControlDbError> {
    db.execute("INSERT INTO shared_channel_cursors(session_id, agent_id_key, last_seen_id, updated_at) VALUES (?, '', ?, ?)
 ON CONFLICT(session_id, agent_id_key) DO UPDATE SET last_seen_id = MAX(last_seen_id, excluded.last_seen_id), updated_at = excluded.updated_at", params![key, value, crate::utils::iso_time::now_iso_string()])?;
    Ok(())
}

struct StoredRelayRow {
    session_path: JsString,
    message_id: JsString,
    raw: JsString,
}
fn read_next_relay_row(db: &Connection, peer: &JsString) -> Result<Option<StoredRelayRow>, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT session_path, message_id, data FROM multi_agent_mailbox_messages
   WHERE json_valid(data) AND json_extract(data, '$.recipientHost') = ? AND json_extract(data, '$.status') = 'pending'
   AND json_type(data, '$.createdAt') = 'text'
   AND CAST(ROUND((julianday(json_extract(data, '$.createdAt')) - 2440587.5) * 86400000) AS INTEGER) > ?
   ORDER BY json_extract(data, '$.relaySeq') IS NULL, json_extract(data, '$.relaySeq'), rowid LIMIT 1",
            params![
                peer,
                pi_js_compat::date::js_now_millis() as i64 - control::MAILBOX_MESSAGE_RETENTION_MS
            ],
            |row| {
                Ok(StoredRelayRow {
                    session_path: row.get(0)?,
                    message_id: row.get(1)?,
                    raw: row.get(2)?,
                })
            },
        )
        .optional()?)
}
fn assign_relay_sequence(
    db: &Connection,
    peer: &JsString,
    row: &StoredRelayRow,
    data: &mut JsValue,
) -> Result<i64, ControlDbError> {
    if data.get("relaySeq").is_none() {
        let key = js_format!("relay-out:{}", peer);
        let exhausted = || ControlDbError::new(js_format!("Relay sequence exhausted for {}", peer));
        let seq = read_counter(db, &key)?.checked_add(1).ok_or_else(exhausted)?;
        if seq > 9_007_199_254_740_991 {
            return Err(exhausted());
        }
        if let JsValue::Object(object) = data {
            object.insert("relaySeq".into(), JsValue::Number(seq as f64));
        }
        db.execute(
            "UPDATE multi_agent_mailbox_messages SET data = ? WHERE session_path = ? AND message_id = ? AND data = ?",
            params![
                js_json::stringify_lossless(data),
                row.session_path,
                row.message_id,
                row.raw
            ],
        )?;
        write_counter(db, &key, seq)?;
    }
    positive_integer(data.get("relaySeq"))
        .map_err(|_| ControlDbError::new(js_format!("Invalid stored relay sequence for {}", peer)))
}
pub fn next_relay_mailbox(path: &JsString, peer: &JsString) -> Result<Option<RelayMailbox>, ControlDbError> {
    control::with_control_db(path, |db| {
        control::with_immediate_transaction(db, |db| {
            let Some(row) = read_next_relay_row(db, peer)? else {
                return Ok(None);
            };
            let mut data = js_json::parse_lossless(&row.raw).map_err(|e| ControlDbError::new(e.to_string()))?;
            let seq = assign_relay_sequence(db, peer, &row, &mut data)?;
            Ok(Some(RelayMailbox {
                seq,
                session_path: row.session_path,
                message_id: row.message_id,
                data,
            }))
        })
    })
}
pub fn ack_relay_mailbox(path: &JsString, peer: &JsString, message: &RelayMailbox) -> Result<(), ControlDbError> {
    control::with_control_db(path, |db| {
        db.execute(
            "DELETE FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ? AND data = ?
   AND json_extract(data, '$.recipientHost') = ? AND json_extract(data, '$.relaySeq') = ?",
            params![
                message.session_path,
                message.message_id,
                js_json::stringify_lossless(&message.data),
                peer,
                message.seq
            ],
        )?;
        Ok(())
    })
}
pub fn import_relay_mailbox(path: &JsString, peer: &JsString, message: &RelayMailbox) -> Result<(), ControlDbError> {
    let mut data = message
        .data
        .as_object()
        .cloned()
        .ok_or_else(|| ControlDbError::new("Invalid relay mailbox data"))?;
    data.shift_remove(&JsString::from("recipientHost"));
    data.shift_remove(&JsString::from("relaySeq"));
    data.insert("senderHost".into(), JsValue::String(peer.clone()));
    let data = JsValue::Object(data);
    let recipient = RuntimeMailboxAddress {
        session_id: require_text(data.get("recipientSessionId"), "recipientSessionId")?,
        agent_id: nullable_text(data.get("recipientAgentId"), "recipientAgentId")?,
    };
    let sender = RuntimeMailboxAddress {
        session_id: require_text(data.get("senderSessionId"), "senderSessionId")?,
        agent_id: nullable_text(data.get("senderAgentId"), "senderAgentId")?,
    };
    let applied = control::with_control_db(path, |db| {
        control::with_immediate_transaction(db, |db| {
            let key = js_format!("relay-in:{}", peer);
            if message.seq <= read_counter(db, &key)? {
                return Ok(false);
            }
            let kind = require_text(data.get("kind"), "kind")?;
            let input = control::EnqueueStoredRuntimeMailboxMessageInput {
                recipient_host: None,
                recipient: recipient.clone(),
                sender,
                kind: control::to_runtime_mailbox_message_kind(&kind),
                store_ref: control::RuntimeMailboxStoreRef {
                    session_path: message.session_path.clone(),
                    message_id: message.message_id.clone(),
                },
                message: data.clone(),
                updated_at: None,
            };
            control::enqueue_lossless_stored_runtime_mailbox_message_in_transaction(db, &input, &data)?;
            write_counter(db, &key, message.seq)?;
            Ok(true)
        })
    })?;
    if applied {
        control::notify_runtime_mailbox_recipient(path, &recipient)?;
    }
    Ok(())
}
fn relay_channel_cursor(db: &Connection, peer: &JsString) -> Result<i64, ControlDbError> {
    let key = js_format!("relay:{}", peer);
    let existing = db
        .query_row(
            "SELECT last_seen_id FROM shared_channel_cursors WHERE session_id = ? AND agent_id_key = ''",
            [&key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(value) = existing {
        return Ok(value);
    }
    let tail = db
        .query_row("SELECT MAX(id) AS tail FROM shared_channel_messages", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?
        .unwrap_or(0);
    write_counter(db, &key, tail)?;
    Ok(tail)
}
pub fn next_relay_channel(
    path: &JsString,
    peer: &JsString,
    host: &JsString,
) -> Result<Option<RelayChannel>, ControlDbError> {
    control::with_control_db(path, |db| {
        let cursor = relay_channel_cursor(db, peer)?;
        Ok(db
            .query_row(
                "SELECT id, sender_session_id, sender_agent_id, body, created_at FROM shared_channel_messages
   WHERE origin_host IS NULL AND id > ? ORDER BY id LIMIT 1",
                [cursor],
                |row| {
                    Ok(RelayChannel {
                        origin_host: host.clone(),
                        origin_id: row.get(0)?,
                        sender_session_id: row.get(1)?,
                        sender_agent_id: row.get(2)?,
                        body: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    })
}
pub fn ack_relay_channel(path: &JsString, peer: &JsString, id: i64) -> Result<(), ControlDbError> {
    control::with_control_db(path, |db| write_counter(db, &js_format!("relay:{}", peer), id))
}
pub fn import_relay_channel(path: &JsString, message: &RelayChannel) -> Result<(), ControlDbError> {
    if control::is_mailbox_message_expired(Some(&message.created_at), pi_js_compat::date::js_now_millis()) {
        return Ok(());
    }
    let recipients = control::with_control_db(path, |db| {
        let changed = db.execute("INSERT INTO shared_channel_messages(sender_session_id, sender_agent_id, body, created_at, origin_host, origin_id)
   VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING", params![message.sender_session_id, message.sender_agent_id, message.body, message.created_at, message.origin_host, message.origin_id])?;
        if changed == 0 {
            return Ok(Vec::new());
        }
        let mut query =
            db.prepare("SELECT recipient_session_id FROM runtime_mailbox_listeners WHERE recipient_agent_id_key = ''")?;
        Ok(query
            .query_map([], |row| row.get::<_, JsString>(0))?
            .collect::<Result<Vec<_>, _>>()?)
    })?;
    for session_id in recipients {
        control::notify_runtime_mailbox_recipient(
            path,
            &RuntimeMailboxAddress {
                session_id,
                agent_id: None,
            },
        )?;
    }
    Ok(())
}
