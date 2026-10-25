//! Frozen mailbox-retention.ts and control DB creation-age predicates.
use super::ControlDbError;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use rusqlite::{Connection, OptionalExtension, params};
pub const MAILBOX_MESSAGE_RETENTION_MS: i64 = 86_400_000;
pub(crate) const MAILBOX_CREATED_AT_MS_SQL: &str = "COALESCE(CASE WHEN json_valid(data) THEN
	CASE WHEN json_type(data, '$.createdAt') = 'text' THEN
	CAST(ROUND((julianday(json_extract(data, '$.createdAt')) - 2440587.5) * 86400000) AS INTEGER)
	END END, -8640000000000000)";
pub(crate) fn mailbox_cutoff_ms() -> i64 {
    pi_js_compat::date::js_now_millis() as i64 - MAILBOX_MESSAGE_RETENTION_MS
}
pub fn is_mailbox_message_expired(created_at: Option<&JsString>, now_ms: f64) -> bool {
    created_at
        .map(|created_at| pi_js_compat::date::date_parse_utf16(created_at.units()))
        .filter(|birth| !birth.is_nan())
        .is_none_or(|birth| now_ms - birth >= MAILBOX_MESSAGE_RETENTION_MS as f64)
}
pub(crate) fn assert_mailbox_message_fresh(message: &JsObject, context: &JsString) -> Result<(), ControlDbError> {
    if is_mailbox_message_expired(
        message.get("createdAt").and_then(JsValue::as_string),
        pi_js_compat::date::js_now_millis(),
    ) {
        return Err(ControlDbError::new(js_format!(
            "Expired mailbox message at {}",
            context
        )));
    }
    Ok(())
}
pub(crate) fn cleanup_expired_messages(db: &Connection) -> Result<(), ControlDbError> {
    cleanup_expired_messages_at(db, pi_js_compat::date::js_now_millis() as i64)
}
pub(super) fn cleanup_expired_messages_at(db: &Connection, now_ms: i64) -> Result<(), ControlDbError> {
    let cutoff = now_ms - MAILBOX_MESSAGE_RETENTION_MS;
    cleanup_expired_mailbox_messages(db, cutoff)?;
    cleanup_expired_channel_messages(db, &crate::utils::iso_time::format_iso_millis(cutoff))
}

fn cleanup_expired_mailbox_messages(db: &Connection, cutoff: i64) -> Result<(), ControlDbError> {
    let due = db
        .query_row(
            &format!("SELECT 1 FROM multi_agent_mailbox_messages WHERE {MAILBOX_CREATED_AT_MS_SQL} <= ? LIMIT 1"),
            params![cutoff],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if due {
        db.execute(
            &format!("DELETE FROM multi_agent_mailbox_messages WHERE {MAILBOX_CREATED_AT_MS_SQL} <= ?"),
            params![cutoff],
        )?;
    }
    Ok(())
}

fn cleanup_expired_channel_messages(db: &Connection, cutoff: &str) -> Result<(), ControlDbError> {
    let due = db
        .query_row(
            "SELECT 1 FROM shared_channel_messages WHERE created_at <= ? LIMIT 1",
            params![cutoff],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if due {
        db.execute(
            "DELETE FROM shared_channel_messages WHERE created_at <= ?",
            params![cutoff],
        )?;
    }
    Ok(())
}
