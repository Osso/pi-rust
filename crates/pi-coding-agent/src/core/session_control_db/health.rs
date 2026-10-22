//! Session health rows (`session_health` table).

use super::mailbox::{read_session_health_row, session_health_from_row, write_session_health_row};
use super::{ControlDbError, with_control_db};
use crate::core::session_health::SessionHealthRecord;
use pi_js_compat::js_json::JsString;

/// `readSessionHealth`.
pub fn read_session_health(
    control_db_path: impl Into<JsString>,
    session_id: &JsString,
) -> Result<Option<SessionHealthRecord>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| read_session_health_row(db, session_id))
}

/// `listSessionHealth`.
pub fn list_session_health(control_db_path: impl Into<JsString>) -> Result<Vec<SessionHealthRecord>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let mut statement = db.prepare(
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
				ORDER BY updated_at DESC
				",
        )?;
        let rows = statement
            .query_map([], |row| {
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
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(session_health_from_row).collect())
    })
}

/// `writeSessionHealth`.
pub fn write_session_health(
    control_db_path: impl Into<JsString>,
    health: &SessionHealthRecord,
) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| write_session_health_row(db, health))
}
