//! Port of `packages/session-backends/sqlite-node/src/sqlite/branch-cache.ts`.

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};

use crate::error::SqliteError;
use crate::sqlite::sql::SqlQuery;
use crate::sqlite::storage::branch_entries::{
    copy_branch_entries_through_seq, delete_branch_entries, insert_branch_entries_for_path, insert_branch_entry,
    read_branch_containing_entry,
};
use crate::sqlite::storage::branch_tips::{
    delete_branch_tips, insert_branch_tip, read_branch_tip_branch_id, update_branch_tip,
};

pub fn delete_branch_cache(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    delete_branch_tips(db, session_id)?;
    delete_branch_entries(db, session_id)
}

pub fn rebuild_branch_cache(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    let tips = SqlQuery::new(
        "SELECT leaf.id
		FROM entries AS leaf
		WHERE leaf.session_id = ?
			AND NOT EXISTS (
				SELECT 1 FROM entries AS child WHERE child.session_id = leaf.session_id AND child.parent_id = leaf.id
			)
		ORDER BY leaf.seq",
        vec![crate::sqlite::sql::text_param(session_id)],
    )
    .all(db, |row| row.get::<_, String>(0))?;
    delete_branch_cache(db, session_id)?;
    for tip in tips {
        build_cached_branch(db, session_id, &tip)?;
    }
    Ok(())
}

pub fn build_cached_branch(db: &rusqlite::Connection, session_id: &str, leaf_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new("SAVEPOINT build_branch_cache", Vec::new()).exec(db)?;
    let branch_id = pi_ai::utils::uuid::uuidv7();
    let result = insert_branch_entries_for_path(db, session_id, &branch_id, leaf_id)
        .and_then(|()| insert_branch_tip(db, session_id, leaf_id, &branch_id))
        .and_then(|()| SqlQuery::new("RELEASE SAVEPOINT build_branch_cache", Vec::new()).exec(db));
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = SqlQuery::new("ROLLBACK TO SAVEPOINT build_branch_cache", Vec::new()).exec(db);
            let _ = SqlQuery::new("RELEASE SAVEPOINT build_branch_cache", Vec::new()).exec(db);
            match error {
                SqliteError::Session(error) => Err(SqliteError::Session(error)),
                _ => Err(SqliteError::Session(SessionError::new(
                    SessionErrorCode::Storage,
                    format!("Failed to build SQLite branch cache at entry {leaf_id}"),
                ))),
            }
        }
    }
}

struct BranchAppend<'a> {
    branch_id: &'a str,
    parent_id: &'a str,
    entry_id: &'a str,
    entry_seq: i64,
    entry_type: &'a str,
    custom_type: Option<&'a str>,
}

fn extend_branch(db: &rusqlite::Connection, session_id: &str, append: &BranchAppend<'_>) -> Result<(), SqliteError> {
    insert_branch_entry(
        db,
        session_id,
        append.branch_id,
        append.entry_id,
        append.entry_seq,
        append.entry_type,
        append.custom_type,
    )?;
    if !update_branch_tip(db, session_id, append.branch_id, append.parent_id, append.entry_id)? {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidEntry,
            format!("Branch tip {} changed during append", append.parent_id),
        )));
    }
    Ok(())
}

pub fn append_entry_to_branch_cache(
    db: &rusqlite::Connection,
    session_id: &str,
    entry_id: &str,
    entry_seq: i64,
    entry_type: &str,
    custom_type: Option<&str>,
    parent_id: Option<&str>,
) -> Result<(), SqliteError> {
    let Some(parent_id) = parent_id else {
        let branch_id = pi_ai::utils::uuid::uuidv7();
        insert_branch_entry(db, session_id, &branch_id, entry_id, entry_seq, entry_type, custom_type)?;
        insert_branch_tip(db, session_id, entry_id, &branch_id)?;
        return Ok(());
    };

    if let Some(tip_branch_id) = read_branch_tip_branch_id(db, session_id, parent_id)? {
        return extend_branch(
            db,
            session_id,
            &BranchAppend {
                branch_id: &tip_branch_id,
                parent_id,
                entry_id,
                entry_seq,
                entry_type,
                custom_type,
            },
        );
    }

    let Some((source_branch_id, source_entry_seq)) = read_branch_containing_entry(db, session_id, parent_id)? else {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidEntry,
            format!("Branch cache has no branch containing parent entry {parent_id}"),
        )));
    };

    let branch_id = pi_ai::utils::uuid::uuidv7();
    copy_branch_entries_through_seq(db, session_id, &branch_id, &source_branch_id, source_entry_seq)?;
    insert_branch_entry(db, session_id, &branch_id, entry_id, entry_seq, entry_type, custom_type)?;
    insert_branch_tip(db, session_id, entry_id, &branch_id)?;
    Ok(())
}
