//! Port of `packages/session-backends/sqlite-node/test/branch-cache.test.ts`.

mod support;

use pi_agent_core::harness::session::{
    BranchQuery, Entry, EntryBody, EntryOrder, EntryQuery, ForkOptions, OptionalValue, Session, SessionErrorCode,
};
use pi_session_sqlite::{SqliteSessionCreateOptions, SqliteSessionMetadata};
use support::{Fixture, assistant_message, user_message};

fn assert_code<T: std::fmt::Debug>(
    result: Result<T, pi_agent_core::harness::session::SessionError>,
    code: SessionErrorCode,
) {
    match result {
        Ok(value) => panic!("expected {code:?}, got {value:?}"),
        Err(error) => assert_eq!(error.code(), code, "unexpected error: {error}"),
    }
}

async fn append_compaction(session: &Session<SqliteSessionMetadata>, summary: &str, tokens_before: f64) -> String {
    let entry = session
        .append_entry(
            EntryBody::Compaction {
                id: session.id_generator().next(),
                summary: summary.to_owned(),
                tokens_before,
                retained_tail: Vec::new(),
                duration_ms: None,
                details: OptionalValue::Missing,
                usage: None,
            },
            "main",
        )
        .await
        .expect("compaction");
    entry.id().to_owned()
}

async fn move_main_lane(session: &Session<SqliteSessionMetadata>, entry_id: Option<&str>) {
    session.move_lane("main", entry_id).await.expect("move lane");
}

async fn branch_entries(session: &Session<SqliteSessionMetadata>) -> Vec<Entry> {
    let Some(start) = session.get_leaf_id().await.expect("leaf") else {
        return Vec::new();
    };
    let mut newest_first = session
        .find_entries_on_branch(BranchQuery {
            bounds: pi_agent_core::harness::session::BranchBounds {
                start: Some(start),
                stop_at_type: Some(pi_agent_core::harness::session::EntryType::Compaction),
                ..Default::default()
            },
            entry: EntryQuery::default(),
        })
        .await
        .expect("branch");
    newest_first.reverse();
    newest_first
}

#[tokio::test]
async fn collects_complete_root_paths_for_branches_created_after_compaction() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let root_id = session.append_message(user_message("root")).await.expect("root");
    let kept_id = session.append_message(user_message("kept")).await.expect("kept");
    let compaction_id = append_compaction(&session, "summary", 100.0).await;
    session
        .append_message(assistant_message("first child"))
        .await
        .expect("first child");
    move_main_lane(&session, Some(&compaction_id)).await;
    let branched_id = session
        .append_message(assistant_message("branched child"))
        .await
        .expect("branched child");

    let db = fixture.open_connection();
    let branch_id: String = db
        .query_row(
            "SELECT branch_id FROM branch_entries WHERE session_id = ? AND entry_id = ?",
            ("session-1", &branched_id),
            |row| row.get(0),
        )
        .expect("branch row");
    let entry_ids: Vec<String> = db
        .prepare("SELECT entry_id FROM branch_entries WHERE session_id = ? AND branch_id = ? ORDER BY entry_seq")
        .expect("statement")
        .query_map(("session-1", &branch_id), |row| row.get(0))
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("entry ids");
    assert_eq!(entry_ids, vec![root_id, kept_id, compaction_id, branched_id]);
}

#[tokio::test]
async fn reads_only_the_compacted_branch_window_from_the_complete_cache() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let old_id = session.append_message(user_message("old")).await.expect("old");
    session.append_message(user_message("kept")).await.expect("kept");
    let compaction_id = append_compaction(&session, "summary", 100.0).await;
    let leaf_id = session.append_message(assistant_message("new")).await.expect("new");
    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET payload = ? WHERE session_id = ? AND id = ?",
        ("not json", "session-1", &old_id),
    )
    .expect("corrupt old entry");
    drop(db);

    assert_eq!(
        branch_entries(&session)
            .await
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect::<Vec<_>>(),
        vec![compaction_id, leaf_id]
    );
}

#[tokio::test]
async fn preserves_nested_compaction_boundaries_when_reading_the_cache() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    session.append_message(user_message("root")).await.expect("root");
    append_compaction(&session, "first summary", 100.0).await;
    session.append_message(user_message("middle")).await.expect("middle");
    let second_compaction_id = append_compaction(&session, "second summary", 200.0).await;
    let leaf_id = session.append_message(assistant_message("new")).await.expect("new");
    assert_eq!(
        branch_entries(&session)
            .await
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect::<Vec<_>>(),
        vec![second_compaction_id, leaf_id]
    );
}

#[tokio::test]
async fn rejects_reads_and_writes_without_repairing_a_missing_private_branch_cache() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    session.append_message(user_message("root")).await.expect("root");
    session.append_message(assistant_message("child")).await.expect("child");
    let db = fixture.open_connection();
    db.execute("DELETE FROM branch_tips WHERE session_id = ?", ("session-1",))
        .expect("delete tips");
    db.execute("DELETE FROM branch_entries WHERE session_id = ?", ("session-1",))
        .expect("delete entries");
    drop(db);

    assert_code(
        session
            .find_entries_on_branch(BranchQuery {
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some(session.get_leaf_id().await.expect("leaf").expect("leaf")),
                    stop_at_type: Some(pi_agent_core::harness::session::EntryType::Compaction),
                    ..Default::default()
                },
                entry: EntryQuery::default(),
            })
            .await,
        SessionErrorCode::InvalidEntry,
    );
    let error = session
        .append_message(assistant_message("later"))
        .await
        .expect_err("append must fail");
    assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
    assert!(
        error.message().contains("has no branch containing parent entry"),
        "unexpected error: {error}"
    );

    let inspection = fixture.open_connection();
    let count: i64 = inspection
        .query_row(
            "SELECT COUNT(*) FROM branch_entries WHERE session_id = ?",
            ("session-1",),
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn repairs_the_private_branch_cache_explicitly() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let root_id = session.append_message(user_message("root")).await.expect("root");
    let child_id = session.append_message(assistant_message("child")).await.expect("child");
    let session_metadata = session.get_metadata().await.expect("metadata");
    let db = fixture.open_connection();
    db.execute("DELETE FROM branch_tips WHERE session_id = ?", ("session-1",))
        .expect("delete tips");
    db.execute("DELETE FROM branch_entries WHERE session_id = ?", ("session-1",))
        .expect("delete entries");
    drop(db);

    assert_code(
        session
            .find_entries_on_branch(BranchQuery {
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some(child_id.clone()),
                    stop_at_type: Some(pi_agent_core::harness::session::EntryType::Compaction),
                    ..Default::default()
                },
                entry: EntryQuery::default(),
            })
            .await,
        SessionErrorCode::InvalidEntry,
    );
    fixture
        .repo
        .repair_branch_cache(&session_metadata)
        .await
        .expect("repaired");
    assert_eq!(
        branch_entries(&session)
            .await
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect::<Vec<_>>(),
        vec![root_id, child_id]
    );
}

#[tokio::test]
async fn fails_when_forking_from_a_source_with_a_missing_branch_cache() {
    let fixture = Fixture::new();
    let source = fixture.create("source").await;
    let root_id = source.append_message(user_message("root")).await.expect("root");
    let child_id = source.append_message(assistant_message("child")).await.expect("child");
    let db = fixture.open_connection();
    db.execute("DELETE FROM branch_tips WHERE session_id = ?", ("source",))
        .expect("delete tips");
    db.execute("DELETE FROM branch_entries WHERE session_id = ?", ("source",))
        .expect("delete entries");
    drop(db);
    assert_ne!(root_id, child_id);
    assert_code(
        fixture
            .repo
            .fork(
                &source.get_metadata().await.expect("metadata"),
                ForkOptions::BranchAt { entry_id: child_id },
                SqliteSessionCreateOptions {
                    id: Some("fork".to_owned()),
                    cwd: fixture.cwd(),
                    ..Default::default()
                },
            )
            .await,
        SessionErrorCode::InvalidForkTarget,
    );
}

#[tokio::test]
async fn fails_when_the_private_branch_cache_is_stale() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let root_id = session.append_message(user_message("root")).await.expect("root");
    let stale_id = session.append_message(assistant_message("stale")).await.expect("stale");
    let leaf_id = session.append_message(user_message("leaf")).await.expect("leaf");
    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET parent_id = ? WHERE session_id = ? AND id = ?",
        (&root_id, "session-1", &leaf_id),
    )
    .expect("stale cache");
    drop(db);
    assert_ne!(stale_id, leaf_id);
    assert_code(
        session
            .find_entries_on_branch(BranchQuery {
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some(leaf_id),
                    ..Default::default()
                },
                entry: EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..Default::default()
                },
            })
            .await,
        SessionErrorCode::InvalidEntry,
    );
}

#[tokio::test]
async fn deletes_branch_entries_and_tips_with_the_session() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    session.append_message(user_message("root")).await.expect("root");
    let session_metadata = session.get_metadata().await.expect("metadata");
    fixture.repo.delete(&session_metadata).await.expect("delete");

    let db = fixture.open_connection();
    let entries: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM branch_entries WHERE session_id = ?",
            ("session-1",),
            |row| row.get(0),
        )
        .expect("entries");
    let tips: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM branch_tips WHERE session_id = ?",
            ("session-1",),
            |row| row.get(0),
        )
        .expect("tips");
    assert_eq!((entries, tips), (0, 0));
}
