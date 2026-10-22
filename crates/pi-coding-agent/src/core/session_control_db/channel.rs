//! Shared channel messages and cursors.

use pi_js_compat::js_json::encode_node_utf8;
use rusqlite::{Connection, OptionalExtension, params};

use super::mailbox::{has_deliverable_runtime_mailbox_message, is_lifecycle_notification_payload};
use super::{
    ControlDbError, PostSharedChannelMessageInput, RuntimeMailboxAddress, SharedChannelMessage,
    runtime_mailbox_agent_id_key, with_control_db,
};

/// `postSharedChannelMessage`.
pub fn post_shared_channel_message(
    control_db_path: &str,
    input: &PostSharedChannelMessageInput,
) -> Result<i64, ControlDbError> {
    let body = input.body.trim();
    if body.is_empty() {
        return Err(ControlDbError::new("shared channel message body must be non-empty"));
    }
    with_control_db(control_db_path, |db| {
        let now = crate::utils::iso_time::now_iso_string();
        db.execute(
            "
				INSERT INTO shared_channel_messages (sender_session_id, sender_agent_id, body, created_at)
				VALUES (?, ?, ?, ?)
				",
            params![
                encode_node_utf8(&input.sender.session_id),
                input.sender.agent_id.as_ref().map(encode_node_utf8),
                encode_node_utf8(&body),
                now
            ],
        )?;
        Ok(db.last_insert_rowid())
    })
}

/// `listSharedChannelMessagesAfter`.
pub fn list_shared_channel_messages_after(
    control_db_path: &str,
    last_seen_id: i64,
    limit: i64,
    through_id: i64,
) -> Result<Vec<SharedChannelMessage>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut statement = db.prepare(
            "
				SELECT id, sender_session_id, sender_agent_id, body, created_at, origin_host, origin_id
				FROM shared_channel_messages
				WHERE id > ? AND id <= ?
				ORDER BY id ASC
				LIMIT ?
				",
        )?;
        let rows = statement
            .query_map(params![last_seen_id, through_id, limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(
                |(id, session_id, agent_id, body, created_at, origin_host, origin_id)| SharedChannelMessage {
                    origin_host,
                    origin_id,
                    id,
                    sender: RuntimeMailboxAddress {
                        agent_id: agent_id.map(Into::into),
                        session_id: session_id.into(),
                    },
                    body,
                    created_at,
                },
            )
            .collect())
    })
}

/// `readSharedChannelCursor`.
pub fn read_shared_channel_cursor(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
) -> Result<Option<i64>, ControlDbError> {
    with_control_db(control_db_path, |db| read_shared_channel_cursor_row(db, recipient))
}

/// `hasPendingRuntimeCoordinationMessage`.
pub fn has_pending_runtime_coordination_message(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        if has_deliverable_runtime_mailbox_message(db, recipient)? {
            return Ok(true);
        }
        if recipient.agent_id.is_some() {
            return Ok(false);
        }
        let Some(cursor) = read_shared_channel_cursor_row(db, recipient)? else {
            return Ok(false);
        };
        Ok(db
            .query_row(
                "
				SELECT 1 AS present
				FROM shared_channel_messages
				WHERE id > ?
					AND sender_agent_id IS NULL
					AND (origin_host IS NOT NULL OR sender_session_id <> ?)
				LIMIT 1
				",
                params![cursor, recipient.session_id.to_utf8_lossy()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    })
}

/// `isRuntimeCoordinationMailboxMessage`.
pub fn is_runtime_coordination_mailbox_message(message: &super::RuntimeMailboxMessage) -> bool {
    let value = serde_json::to_value(message).unwrap_or(serde_json::Value::Null);
    match value {
        serde_json::Value::Object(object) => !is_lifecycle_notification_payload(&object),
        _ => true,
    }
}

/// `initializeSharedChannelCursorAtTail`.
pub fn initialize_shared_channel_cursor_at_tail(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, |db| {
        if let Some(existing) = read_shared_channel_cursor_row(db, recipient)? {
            return Ok(existing);
        }
        let tail = read_shared_channel_tail_row(db)?;
        write_shared_channel_cursor_row(db, recipient, tail)?;
        Ok(tail)
    })
}

/// `advanceSharedChannelCursor`.
pub fn advance_shared_channel_cursor(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    last_seen_id: i64,
) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
        write_shared_channel_cursor_row(db, recipient, last_seen_id)
    })
}

fn read_shared_channel_cursor_row(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
) -> Result<Option<i64>, ControlDbError> {
    Ok(db
        .query_row(
            "
			SELECT last_seen_id
			FROM shared_channel_cursors
			WHERE session_id = ? AND agent_id_key = ?
			",
            params![
                recipient.session_id.to_utf8_lossy(),
                runtime_mailbox_agent_id_key(recipient.agent_id.as_ref())
            ],
            |row| row.get(0),
        )
        .optional()?)
}

/// `readSharedChannelTail`.
pub fn read_shared_channel_tail(control_db_path: &str) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, read_shared_channel_tail_row)
}

fn read_shared_channel_tail_row(db: &Connection) -> Result<i64, ControlDbError> {
    Ok(db.query_row(
        "SELECT COALESCE(MAX(id), 0) AS tail FROM shared_channel_messages",
        [],
        |row| row.get(0),
    )?)
}

fn write_shared_channel_cursor_row(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
    last_seen_id: i64,
) -> Result<(), ControlDbError> {
    let now = crate::utils::iso_time::now_iso_string();
    db.execute(
        "
		INSERT INTO shared_channel_cursors (session_id, agent_id_key, last_seen_id, updated_at)
		VALUES (?, ?, ?, ?)
		ON CONFLICT(session_id, agent_id_key) DO UPDATE SET
			last_seen_id = MAX(shared_channel_cursors.last_seen_id, excluded.last_seen_id),
			updated_at = excluded.updated_at
		",
        params![
            recipient.session_id.to_utf8_lossy(),
            runtime_mailbox_agent_id_key(recipient.agent_id.as_ref()),
            last_seen_id,
            now
        ],
    )?;
    Ok(())
}
