//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/writer-leases.ts`.

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, text_param};

/// A held per-session writer claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterLease {
    pub owner_id: String,
    pub fence: i64,
    pub expires_at_ms: i64,
}

/// Attempts to claim the session; returns `None` while another writer's lease is live.
pub fn acquire_writer_lease(
    db: &rusqlite::Connection,
    session_id: &str,
    owner_id: &str,
    now: i64,
    expires_at_ms: i64,
) -> Result<Option<WriterLease>, SqliteError> {
    let row = SqlQuery::new(
        "INSERT INTO writer_leases (session_id, owner_id, fence, expires_at_ms)
		VALUES (?, ?, 1, ?)
		ON CONFLICT(session_id) DO UPDATE SET
			owner_id = excluded.owner_id,
			fence = writer_leases.fence + 1,
			expires_at_ms = excluded.expires_at_ms
		WHERE writer_leases.expires_at_ms <= ?
		RETURNING owner_id, fence, expires_at_ms",
        vec![
            text_param(session_id),
            text_param(owner_id),
            int_param(expires_at_ms),
            int_param(now),
        ],
    )
    .get(db, |row| {
        let owner_id: String = row.get(0)?;
        Ok((owner_id, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
    })?;
    Ok(row.map(|(owner_id, fence, expires_at_ms)| WriterLease {
        owner_id,
        fence,
        expires_at_ms,
    }))
}

/// Renews the lease while this owner still holds an unexpired fence.
pub fn renew_writer_lease(
    db: &rusqlite::Connection,
    session_id: &str,
    lease: &mut WriterLease,
    now: i64,
    expires_at_ms: i64,
) -> Result<bool, SqliteError> {
    let result = SqlQuery::new(
        "UPDATE writer_leases
		SET expires_at_ms = ?
		WHERE session_id = ?
			AND owner_id = ?
			AND fence = ?
			AND expires_at_ms > ?",
        vec![
            int_param(expires_at_ms),
            text_param(session_id),
            text_param(lease.owner_id.clone()),
            int_param(lease.fence),
            int_param(now),
        ],
    )
    .run(db)?;
    if result.changes == 1 {
        lease.expires_at_ms = expires_at_ms;
    }
    Ok(result.changes == 1)
}

pub fn release_writer_lease(
    db: &rusqlite::Connection,
    session_id: &str,
    lease: &WriterLease,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM writer_leases
		WHERE session_id = ? AND owner_id = ? AND fence = ?",
        vec![
            text_param(session_id),
            text_param(lease.owner_id.clone()),
            int_param(lease.fence),
        ],
    )
    .run(db)?;
    Ok(())
}

pub fn delete_writer_lease(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM writer_leases WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    Ok(())
}
