//! Frozen `host-relay-store.ts`: canonical SQLite queues and persistent relay cursors.
use pi_js_compat::js_json::{self, JsString, JsValue};
use rusqlite::{Connection, OptionalExtension, params};

use super::session_control_db::{self as control, ControlDbError, RuntimeMailboxAddress};

#[derive(Debug, Clone, PartialEq)]
pub struct RelayMailbox {
    pub seq: i64,
    pub session_path: String,
    pub message_id: String,
    pub data: JsValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayChannel {
    pub origin_host: JsString,
    pub origin_id: i64,
    pub sender_session_id: String,
    pub sender_agent_id: Option<String>,
    pub body: String,
    pub created_at: String,
}

impl RelayChannel {
    pub fn to_value(&self) -> JsValue {
        let mut object = JsValue::from(serde_json::json!({"type":"channel"}))
            .as_object()
            .unwrap()
            .clone();
        object.insert("originHost".into(), JsValue::String(self.origin_host.clone()));
        for (key, value) in [
            ("originId", serde_json::json!(self.origin_id)),
            ("senderSessionId", serde_json::json!(self.sender_session_id)),
            ("senderAgentId", serde_json::json!(self.sender_agent_id)),
            ("body", serde_json::json!(self.body)),
            ("createdAt", serde_json::json!(self.created_at)),
        ] {
            object.insert(key.into(), JsValue::from(value));
        }
        JsValue::Object(object)
    }
}

pub fn require_lossless_text(value: Option<&JsValue>, field: &str) -> Result<JsString, ControlDbError> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| !pi_js_compat::string::js_trim(&text.to_utf8_lossy()).is_empty())
        .cloned()
        .ok_or_else(|| ControlDbError::new(format!("Invalid relay {field}")))
}

pub fn require_text(value: Option<&JsValue>, field: &str) -> Result<String, ControlDbError> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| !pi_js_compat::string::js_trim(&text.to_utf8_lossy()).is_empty())
        .map(JsString::to_utf8_lossy)
        .ok_or_else(|| ControlDbError::new(format!("Invalid relay {field}")))
}
pub fn nullable_text(value: Option<&JsValue>, field: &str) -> Result<Option<String>, ControlDbError> {
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
fn read_counter(db: &Connection, key: &str) -> Result<i64, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT last_seen_id FROM shared_channel_cursors WHERE session_id = ? AND agent_id_key = ''",
            [key],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}
fn write_counter(db: &Connection, key: &str, value: i64) -> Result<(), ControlDbError> {
    db.execute("INSERT INTO shared_channel_cursors(session_id, agent_id_key, last_seen_id, updated_at) VALUES (?, '', ?, ?)
 ON CONFLICT(session_id, agent_id_key) DO UPDATE SET last_seen_id = MAX(last_seen_id, excluded.last_seen_id), updated_at = excluded.updated_at", params![key, value, crate::utils::iso_time::now_iso_string()])?;
    Ok(())
}

pub fn next_relay_mailbox(path: &str, peer: &str) -> Result<Option<RelayMailbox>, ControlDbError> {
    control::with_control_db(path, |db| {
        control::with_immediate_transaction(db, |db| {
            let row = db
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
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let Some((session_path, message_id, raw)) = row else {
                return Ok(None);
            };
            let mut data = js_json::parse_lossless(&raw).map_err(|e| ControlDbError::new(e.to_string()))?;
            let seq = match data.get("relaySeq") {
                Some(value) => positive_integer(Some(value))
                    .map_err(|_| ControlDbError::new(format!("Invalid stored relay sequence for {peer}")))?,
                None => {
                    let seq = read_counter(db, &format!("relay-out:{peer}"))? + 1;
                    if seq > 9_007_199_254_740_991 {
                        return Err(ControlDbError::new(format!("Relay sequence exhausted for {peer}")));
                    }
                    if let JsValue::Object(object) = &mut data {
                        object.insert("relaySeq".into(), JsValue::Number(seq as f64));
                    }
                    db.execute("UPDATE multi_agent_mailbox_messages SET data = ? WHERE session_path = ? AND message_id = ? AND data = ?", params![js_json::stringify_lossless(&data),session_path,message_id,raw])?;
                    write_counter(db, &format!("relay-out:{peer}"), seq)?;
                    seq
                }
            };
            Ok(Some(RelayMailbox {
                seq,
                session_path,
                message_id,
                data,
            }))
        })
    })
}
pub fn ack_relay_mailbox(path: &str, peer: &str, message: &RelayMailbox) -> Result<(), ControlDbError> {
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
pub fn import_relay_mailbox(path: &str, peer: &str, message: &RelayMailbox) -> Result<(), ControlDbError> {
    let mut data = message
        .data
        .as_object()
        .cloned()
        .ok_or_else(|| ControlDbError::new("Invalid relay mailbox data"))?;
    data.shift_remove(&JsString::from("recipientHost"));
    data.shift_remove(&JsString::from("relaySeq"));
    data.insert("senderHost".into(), JsValue::String(peer.into()));
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
            let key = format!("relay-in:{peer}");
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
                message: data.clone().into_serde_lossy(),
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
fn relay_channel_cursor(db: &Connection, peer: &str) -> Result<i64, ControlDbError> {
    let key = format!("relay:{peer}");
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
pub fn next_relay_channel(path: &str, peer: &str, host: &str) -> Result<Option<RelayChannel>, ControlDbError> {
    control::with_control_db(path, |db| {
        let cursor = relay_channel_cursor(db, peer)?;
        Ok(db
            .query_row(
                "SELECT id, sender_session_id, sender_agent_id, body, created_at FROM shared_channel_messages
   WHERE origin_host IS NULL AND id > ? ORDER BY id LIMIT 1",
                [cursor],
                |row| {
                    Ok(RelayChannel {
                        origin_host: host.into(),
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
pub fn ack_relay_channel(path: &str, peer: &str, id: i64) -> Result<(), ControlDbError> {
    control::with_control_db(path, |db| write_counter(db, &format!("relay:{peer}"), id))
}
pub fn import_relay_channel(path: &str, message: &RelayChannel) -> Result<(), ControlDbError> {
    if control::is_mailbox_message_expired(Some(&message.created_at), pi_js_compat::date::js_now_millis()) {
        return Ok(());
    }
    let recipients = control::with_control_db(path, |db| {
        let changed = db.execute("INSERT INTO shared_channel_messages(sender_session_id, sender_agent_id, body, created_at, origin_host, origin_id)
   VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING", params![message.sender_session_id, message.sender_agent_id, message.body, message.created_at, message.origin_host.to_utf8_lossy(), message.origin_id])?;
        if changed == 0 {
            return Ok(Vec::new());
        }
        let mut query =
            db.prepare("SELECT recipient_session_id FROM runtime_mailbox_listeners WHERE recipient_agent_id_key = ''")?;
        Ok(query
            .query_map([], |row| row.get::<_, String>(0))?
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
