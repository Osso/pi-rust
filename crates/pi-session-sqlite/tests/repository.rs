//! Port of `packages/session-backends/sqlite-node/test/repository.test.ts`.
//!
//! The source cases that count `open`/`close` calls through an injected `SqliteDatabaseFactory`
//! are ported at the observable boundary: a setup failure, a retained connection after a failed
//! operation, and close-once semantics. The factory capability itself has no Rust equivalent.

mod support;

use pi_agent_core::harness::session::{
    EntryBody, EntryOrder, EntryQuery, OptionalValue, RecordBody, RecordType, Session, SessionErrorCode,
};
use pi_ai::types::{Usage, UsageCost};
use pi_session_sqlite::{SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata};
use serde_json::json;
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

async fn metadata(session: &Session<SqliteSessionMetadata>) -> SqliteSessionMetadata {
    session.get_metadata().await.expect("metadata")
}

fn metadata_object(value: &str) -> serde_json::Map<String, serde_json::Value> {
    match json!({ "profile": value }) {
        serde_json::Value::Object(object) => object,
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn persists_session_metadata_through_create_list_open_and_fork() {
    let fixture = Fixture::new();
    let source = fixture
        .create_with_metadata("session-1", metadata_object("reviewer"))
        .await;
    let source_metadata = metadata(&source).await;
    assert_eq!(source_metadata.metadata, Some(metadata_object("reviewer")));
    let listed = fixture
        .repo
        .list(SqliteSessionListOptions {
            cwd: Some(fixture.cwd()),
        })
        .await
        .expect("listed");
    assert_eq!(
        listed.iter().map(|entry| entry.metadata.clone()).collect::<Vec<_>>(),
        vec![Some(metadata_object("reviewer"))]
    );
    let reopened = fixture.repo.open(&source_metadata).await.expect("reopened");
    assert_eq!(metadata(&reopened).await.metadata, Some(metadata_object("reviewer")));
    let fork = fixture
        .repo
        .fork(
            &source_metadata,
            pi_agent_core::harness::session::ForkOptions::Branch,
            SqliteSessionCreateOptions {
                id: Some("session-2".to_owned()),
                cwd: fixture.cwd(),
                ..Default::default()
            },
        )
        .await
        .expect("fork");
    assert_eq!(metadata(&fork).await.metadata, Some(metadata_object("reviewer")));
    let overridden = fixture
        .repo
        .fork(
            &source_metadata,
            pi_agent_core::harness::session::ForkOptions::Branch,
            SqliteSessionCreateOptions {
                id: Some("session-3".to_owned()),
                cwd: fixture.cwd(),
                metadata: Some(metadata_object("writer")),
                ..Default::default()
            },
        )
        .await
        .expect("fork");
    assert_eq!(metadata(&overridden).await.metadata, Some(metadata_object("writer")));
}

#[tokio::test]
async fn rolls_back_the_entire_fork_when_copying_an_entry_fails() {
    let fixture = Fixture::new();
    let source = fixture.create("source").await;
    source.append_message(user_message("one")).await.expect("one");
    source.append_message(assistant_message("two")).await.expect("two");

    let db = fixture.open_connection();
    db.execute_batch(
        "CREATE TRIGGER fail_fork_entry BEFORE INSERT ON entries
WHEN new.session_id = 'fork' AND new.seq = 2
BEGIN
  SELECT RAISE(ABORT, 'fail fork');
END;",
    )
    .expect("trigger");
    drop(db);

    let error = fixture
        .repo
        .fork(
            &metadata(&source).await,
            pi_agent_core::harness::session::ForkOptions::Branch,
            SqliteSessionCreateOptions {
                id: Some("fork".to_owned()),
                cwd: fixture.cwd(),
                ..Default::default()
            },
        )
        .await
        .expect_err("fork must fail");
    assert_eq!(error.code(), SessionErrorCode::Storage);

    let inspection = fixture.open_connection();
    let sessions: i64 = inspection
        .query_row("SELECT COUNT(*) FROM sessions WHERE id = 'fork'", [], |row| row.get(0))
        .expect("session count");
    assert_eq!(sessions, 0);
    let entries: i64 = inspection
        .query_row("SELECT COUNT(*) FROM entries WHERE session_id = 'fork'", [], |row| {
            row.get(0)
        })
        .expect("entry count");
    assert_eq!(entries, 0);
}

#[tokio::test]
async fn retains_an_opened_database_after_a_failed_operation() {
    let fixture = Fixture::new();
    fixture.create("session-0").await;
    let db = fixture.open_connection();
    db.execute_batch(
        "CREATE TRIGGER fail_session_insert BEFORE INSERT ON sessions
BEGIN
  SELECT RAISE(ABORT, 'insert failed');
END;",
    )
    .expect("trigger");
    drop(db);

    let error = fixture
        .repo
        .create(SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd: fixture.cwd(),
            ..Default::default()
        })
        .await
        .expect_err("create must fail");
    assert!(error.message().contains("insert failed"), "unexpected error: {error}");

    let db = fixture.open_connection();
    db.execute_batch("DROP TRIGGER fail_session_insert")
        .expect("drop trigger");
    drop(db);
    fixture
        .repo
        .create(SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd: fixture.cwd(),
            ..Default::default()
        })
        .await
        .expect("the retained connection is still usable");
    fixture.repo.close().await.expect("closed once");
}

#[tokio::test]
async fn fails_and_stays_unopened_when_repository_setup_fails() {
    let root = tempfile::TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite");
    std::fs::create_dir(&database_path).expect("directory at the database path");
    let repo = pi_session_sqlite::SqliteSessionRepository::new(pi_session_sqlite::SqliteSessionRepositoryOptions {
        env: support::file_system(root.path()),
        database_path: database_path.to_string_lossy().into_owned(),
        writer_lease: None,
    })
    .expect("valid options");
    let error = repo
        .create(SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd: root.path().to_string_lossy().into_owned(),
            ..Default::default()
        })
        .await
        .expect_err("setup must fail");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    repo.close().await.expect("nothing to close");
    assert!(
        repo.create(SqliteSessionCreateOptions {
            id: Some("session-2".to_owned()),
            cwd: root.path().to_string_lossy().into_owned(),
            ..Default::default()
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn closes_active_sessions_when_the_repository_is_disposed() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    fixture.repo.close().await.expect("closed");
    let error = session
        .append_message(user_message("late"))
        .await
        .expect_err("closed session rejects writes");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("SQLite session session-1 is closed"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn retains_one_connection_for_repeated_session_operations() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    for index in 0..10 {
        session
            .append_message(user_message(&format!("message {index}")))
            .await
            .expect("append");
    }
    let entries = session
        .find_entries(EntryQuery {
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await
        .expect("entries");
    assert_eq!(entries.len(), 10);
    fixture.repo.close().await.expect("closed");
    fixture.repo.close().await.expect("close is idempotent");
}

#[tokio::test]
async fn rejects_a_missing_lane_leaf_when_listing_lanes_and_opening() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let session_metadata = metadata(&session).await;
    let db = fixture.open_connection();
    db.execute(
        "UPDATE lanes SET leaf_id = ? WHERE session_id = ? AND lane = ?",
        ("missing", &session_metadata.id, "main"),
    )
    .expect("corrupt lane");
    drop(db);

    let error = session.get_lanes().await.expect_err("missing leaf");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("Lane main points at missing entry missing"),
        "unexpected error: {error}"
    );
    let error = fixture.repo.open(&session_metadata).await.expect_err("open must fail");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("Lane main points at missing entry missing"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn rejects_stored_session_metadata_containing_invalid_values() {
    for (stored, message) in [
        ("not json", "metadata is not valid JSON"),
        ("[]", "metadata must be an object"),
    ] {
        let fixture = Fixture::new();
        fixture.create("session-1").await;
        let db = fixture.open_connection();
        db.execute("UPDATE sessions SET metadata = ? WHERE id = ?", (stored, "session-1"))
            .expect("corrupt metadata");
        drop(db);
        let error = fixture
            .repo
            .list(SqliteSessionListOptions::default())
            .await
            .expect_err("list must fail");
        assert_eq!(error.code(), SessionErrorCode::Storage);
        assert!(error.message().contains(message), "expected {message}, got {error}");
    }
}

#[tokio::test]
async fn rejects_stored_session_names_containing_invalid_values() {
    for (stored, message) in [("not json", "name is not valid JSON"), ("{}", "name must be a string")] {
        let fixture = Fixture::new();
        let session = fixture.create("session-1").await;
        session.set_name(Some("valid name")).await.expect("name");
        let db = fixture.open_connection();
        db.execute(
            "UPDATE facts SET value = ? WHERE session_id = ? AND kind = 'name'",
            (stored, "session-1"),
        )
        .expect("corrupt name");
        drop(db);
        let error = fixture
            .repo
            .list(SqliteSessionListOptions::default())
            .await
            .expect_err("list must fail");
        assert_eq!(error.code(), SessionErrorCode::Storage);
        assert!(error.message().contains(message), "expected {message}, got {error}");
        let error = session.get_metadata().await.expect_err("metadata must fail");
        assert_eq!(error.code(), SessionErrorCode::Storage);
        assert!(error.message().contains(message), "expected {message}, got {error}");
    }
}

#[tokio::test]
async fn omits_a_cleared_session_name() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    session.set_name(Some("Temporary")).await.expect("name");
    assert_eq!(metadata(&session).await.name.as_deref(), Some("Temporary"));
    session.set_name(None).await.expect("clear");
    assert_eq!(session.get_name().await.expect("name"), None);
    assert_eq!(metadata(&session).await.name, None);
    assert_eq!(fixture.list().await[0].name, None);
}

#[tokio::test]
async fn fails_loudly_when_a_stored_entry_cannot_be_decoded() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let entry_id = session.append_message(user_message("message")).await.expect("message");
    let session_metadata = metadata(&session).await;
    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET payload = ? WHERE session_id = ? AND id = ?",
        ("not json", &session_metadata.id, &entry_id),
    )
    .expect("corrupt payload");
    drop(db);

    let reopened = fixture.repo.open(&session_metadata).await.expect("reopened");
    assert_code(
        reopened
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidEntry,
    );
}

#[tokio::test]
async fn fails_loudly_when_a_stored_record_cannot_be_decoded() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    session
        .append_record(RecordBody::OperationFinished {
            id: "record-1".to_owned(),
            lane: "main".to_owned(),
            run_id: "run-1".to_owned(),
            outcome: pi_agent_core::harness::session::OperationOutcome::Completed,
            error: None,
        })
        .await
        .expect("record");
    let db = fixture.open_connection();
    db.execute(
        "UPDATE records SET payload = ? WHERE session_id = ? AND id = ?",
        ("not json", "session-1", "record-1"),
    )
    .expect("corrupt payload");
    drop(db);

    let error = session
        .find_records(pi_agent_core::harness::session::RecordQuery::default())
        .await
        .expect_err("records must fail");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("failed to decode payload"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn does_not_publish_connection_state_when_an_append_transaction_fails() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let db = fixture.open_connection();
    db.execute_batch(
        "CREATE TRIGGER fail_branch_tip_insert
BEFORE INSERT ON branch_tips
BEGIN
  SELECT RAISE(ABORT, 'branch insert failed');
END;",
    )
    .expect("trigger");
    drop(db);

    let error = session
        .append_message(user_message("root"))
        .await
        .expect_err("append must fail");
    assert!(
        error.message().contains("branch insert failed"),
        "unexpected error: {error}"
    );

    let inspection = fixture.open_connection();
    let leaf_id: Option<String> = inspection
        .query_row(
            "SELECT leaf_id FROM lanes WHERE session_id = ? AND lane = ?",
            ("session-1", "main"),
            |row| row.get(0),
        )
        .expect("lane row");
    assert_eq!(leaf_id, None);
    let entry_count: i64 = inspection
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE session_id = ?",
            ("session-1",),
            |row| row.get(0),
        )
        .expect("entry count");
    assert_eq!(entry_count, 0);
    inspection
        .execute_batch("DROP TRIGGER fail_branch_tip_insert")
        .expect("drop trigger");
    drop(inspection);
    assert_eq!(session.get_stats().await.expect("stats").message_count, 0);

    let entry_id = session.append_message(user_message("root")).await.expect("append");
    let entries = session
        .find_entries(EntryQuery {
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await
        .expect("entries");
    assert_eq!(
        entries.iter().map(|entry| entry.id().to_owned()).collect::<Vec<_>>(),
        vec![entry_id]
    );
    assert_eq!(session.get_stats().await.expect("stats").message_count, 1);
}

fn usage(input: f64, output: f64, cache_read: f64, cache_write: f64, total: f64, cost: [f64; 5]) -> Usage {
    Usage {
        input,
        output,
        cache_read,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: total,
        cost: UsageCost {
            input: cost[0],
            output: cost[1],
            cache_read: cost[2],
            cache_write: cost[3],
            total: cost[4],
        },
    }
}

#[tokio::test]
async fn accounts_for_assistant_compaction_and_branch_summary_usage() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let user_id = session.append_message(user_message("one")).await.expect("user");
    let assistant_usage = usage(100.0, 25.0, 40.0, 10.0, 175.0, [0.1, 0.2, 0.03, 0.04, 0.37]);
    let mut assistant = assistant_message("two");
    if let pi_agent_core::agent::AgentMessage::Assistant(message) = &mut assistant {
        message.usage = assistant_usage.clone();
    }
    let assistant_id = session.append_message(assistant).await.expect("assistant");
    session
        .append_record(RecordBody::Usage {
            id: "assistant-usage".to_owned(),
            lane: "main".to_owned(),
            cause: pi_agent_core::harness::session::UsageCause::Assistant,
            run_id: Some("run".to_owned()),
            entry_id: Some(assistant_id),
            attempt: Some(1.0),
            stop_reason: Some(pi_agent_core::harness::session::SessionStopReason::Stop),
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: assistant_usage,
        })
        .await
        .expect("assistant usage");

    let compaction_usage = usage(1.0, 2.0, 3.0, 4.0, 10.0, [0.01, 0.02, 0.03, 0.04, 0.1]);
    let compaction = session
        .append_entry(
            EntryBody::Compaction {
                id: "compaction".to_owned(),
                summary: "summary".to_owned(),
                tokens_before: 200.0,
                retained_tail: Vec::new(),
                duration_ms: None,
                details: OptionalValue::Missing,
                usage: Some(compaction_usage.clone()),
            },
            "main",
        )
        .await
        .expect("compaction");
    session
        .append_record(RecordBody::Usage {
            id: "compaction-usage".to_owned(),
            lane: "main".to_owned(),
            cause: pi_agent_core::harness::session::UsageCause::Compaction,
            run_id: Some("run".to_owned()),
            entry_id: Some(compaction.id().to_owned()),
            attempt: Some(1.0),
            stop_reason: Some(pi_agent_core::harness::session::SessionStopReason::Stop),
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: compaction_usage,
        })
        .await
        .expect("compaction usage");

    let branch_usage = usage(5.0, 6.0, 7.0, 8.0, 26.0, [0.05, 0.06, 0.07, 0.08, 0.26]);
    session.move_lane("main", Some(&user_id)).await.expect("move");
    let branch_summary = session
        .append_entry(
            EntryBody::BranchSummary {
                id: "branch-summary".to_owned(),
                from_id: user_id,
                summary: "branch summary".to_owned(),
                details: OptionalValue::Missing,
                usage: Some(branch_usage.clone()),
            },
            "main",
        )
        .await
        .expect("branch summary");
    session
        .append_record(RecordBody::Usage {
            id: "branch-summary-usage".to_owned(),
            lane: "main".to_owned(),
            cause: pi_agent_core::harness::session::UsageCause::BranchSummary,
            run_id: Some("run".to_owned()),
            entry_id: Some(branch_summary.id().to_owned()),
            attempt: Some(1.0),
            stop_reason: Some(pi_agent_core::harness::session::SessionStopReason::Stop),
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: branch_usage,
        })
        .await
        .expect("branch summary usage");

    let stats = session.get_stats().await.expect("stats");
    assert_eq!(stats.message_count, 2);
    assert_eq!(stats.cached_tokens, 50.0);
    assert_eq!(stats.uncached_tokens, 128.0);
    assert_eq!(stats.total_tokens, 211.0);
    assert!((stats.cost_total - 0.73).abs() < 1e-9, "cost {}", stats.cost_total);
    assert_eq!(
        session
            .find_records(pi_agent_core::harness::session::RecordQuery {
                record_type: Some(RecordType::Usage),
                ..Default::default()
            })
            .await
            .expect("usage records")
            .len(),
        3
    );
}
