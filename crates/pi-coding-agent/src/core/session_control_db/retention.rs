//! Frozen mailbox-retention.ts and control DB creation-age predicates.
use super::ControlDbError;
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
pub const MAILBOX_MESSAGE_RETENTION_MS: i64 = 86_400_000;
pub(crate) const MAILBOX_CREATED_AT_MS_SQL: &str = "COALESCE(CASE WHEN json_valid(data) THEN
	CASE WHEN json_type(data, '$.createdAt') = 'text' THEN
	CAST(ROUND((julianday(json_extract(data, '$.createdAt')) - 2440587.5) * 86400000) AS INTEGER)
	END END, -8640000000000000)";
pub(crate) fn mailbox_cutoff_ms() -> i64 {
    pi_js_compat::date::js_now_millis() as i64 - MAILBOX_MESSAGE_RETENTION_MS
}
pub fn is_mailbox_message_expired(created_at: Option<&str>, now_ms: f64) -> bool {
    created_at
        .and_then(pi_js_compat::date::date_parse_millis)
        .is_none_or(|birth| now_ms - birth as f64 >= MAILBOX_MESSAGE_RETENTION_MS as f64)
}
pub(crate) fn assert_mailbox_message_fresh(message: &Map<String, Value>, context: &str) -> Result<(), ControlDbError> {
    if is_mailbox_message_expired(
        message.get("createdAt").and_then(Value::as_str),
        pi_js_compat::date::js_now_millis(),
    ) {
        return Err(ControlDbError::new(format!("Expired mailbox message at {context}")));
    }
    Ok(())
}
pub(crate) fn cleanup_expired_messages(db: &Connection) -> Result<(), ControlDbError> {
    cleanup_expired_messages_at(db, pi_js_compat::date::js_now_millis() as i64)
}
pub(super) fn cleanup_expired_messages_at(db: &Connection, now_ms: i64) -> Result<(), ControlDbError> {
    let cutoff = now_ms - MAILBOX_MESSAGE_RETENTION_MS;
    db.execute(
        &format!("DELETE FROM multi_agent_mailbox_messages WHERE {MAILBOX_CREATED_AT_MS_SQL} <= ?"),
        params![cutoff],
    )?;
    db.execute(
        "DELETE FROM shared_channel_messages WHERE created_at <= ?",
        params![crate::utils::iso_time::format_iso_millis(cutoff)],
    )?;
    Ok(())
}
