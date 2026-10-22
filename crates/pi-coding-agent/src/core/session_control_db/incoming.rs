//! Incoming control messages (`incoming_messages` table).

use pi_js_compat::js_json::JsString;
use rusqlite::{OptionalExtension, params};

use super::{ControlDbError, IncomingControlMessage, with_control_db, with_immediate_transaction};

/// `claimCodexRestrictionRecovery`.
pub fn claim_codex_restriction_recovery(
    control_db_path: impl Into<JsString>,
    user_request_id: &JsString,
) -> Result<bool, ControlDbError> {
    let control_db_path = control_db_path.into();
    if user_request_id.trim().is_empty() {
        return Err(ControlDbError::new(
            "Codex restriction recovery request ID must not be empty",
        ));
    }
    with_control_db(&control_db_path, |db| {
        Ok(db.execute("INSERT INTO codex_restriction_recovery_claims(user_request_id,claimed_at) VALUES(?,?) ON CONFLICT(user_request_id) DO NOTHING",params![user_request_id,crate::utils::iso_time::now_iso_string()])?==1)
    })
}

/// `enqueueIncomingMessage`.
pub fn enqueue_incoming_message(control_db_path: impl Into<JsString>, content: &str) -> Result<i64, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        db.execute(
            "
				INSERT INTO incoming_messages (content, status, created_at)
				VALUES (?, 'pending', ?)
				",
            params![content, crate::utils::iso_time::now_iso_string()],
        )?;
        Ok(db.last_insert_rowid())
    })
}

/// `claimLatestIncomingMessage`.
pub fn claim_latest_incoming_message(
    control_db_path: impl Into<JsString>,
) -> Result<Option<IncomingControlMessage>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        if read_latest_pending_incoming_message(db)?.is_none() {
            return Ok(None);
        }
        let now_iso = crate::utils::iso_time::now_iso_string();
        with_immediate_transaction(db, |db| claim_latest_incoming_message_in_transaction(db, &now_iso))
    })
}

fn claim_latest_incoming_message_in_transaction(
    db: &rusqlite::Connection,
    now: &str,
) -> Result<Option<IncomingControlMessage>, ControlDbError> {
    let Some(row) = read_latest_pending_incoming_message(db)? else {
        return Ok(None);
    };
    db.execute(
        "
		UPDATE incoming_messages
		SET status = 'superseded', completed_at = ?
		WHERE status = 'pending' AND id <> ?
		",
        params![now, row.id],
    )?;
    db.execute(
        "
		UPDATE incoming_messages
		SET status = 'claimed', claimed_at = ?
		WHERE id = ?
		",
        params![now, row.id],
    )?;
    Ok(Some(row))
}

fn read_latest_pending_incoming_message(
    db: &rusqlite::Connection,
) -> Result<Option<IncomingControlMessage>, ControlDbError> {
    Ok(db
        .query_row(
            "
			SELECT id, content
			FROM incoming_messages
			WHERE status = 'pending'
			ORDER BY id DESC
			LIMIT 1
			",
            [],
            |row| {
                Ok(IncomingControlMessage {
                    id: row.get(0)?,
                    content: row.get(1)?,
                })
            },
        )
        .optional()?)
}

/// `completeIncomingMessage`.
pub fn complete_incoming_message(control_db_path: impl Into<JsString>, id: i64) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        db.execute(
            "
			UPDATE incoming_messages
			SET status = 'completed', completed_at = ?
			WHERE id = ?
			",
            params![crate::utils::iso_time::now_iso_string(), id],
        )?;
        Ok(())
    })
}

/// `failIncomingMessage`.
pub fn fail_incoming_message(
    control_db_path: impl Into<JsString>,
    id: i64,
    error_message: &JsString,
) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        db.execute(
            "
			UPDATE incoming_messages
			SET status = 'failed', error = ?, completed_at = ?
			WHERE id = ?
			",
            params![error_message, crate::utils::iso_time::now_iso_string(), id],
        )?;
        Ok(())
    })
}

/// `readIncomingMessageStatus`.
pub fn read_incoming_message_status(
    control_db_path: impl Into<JsString>,
    id: i64,
) -> Result<Option<String>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        Ok(db
            .query_row(
                "
				SELECT status
				FROM incoming_messages
				WHERE id = ?
				",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    })
}
