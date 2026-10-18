//! Port of `packages/session-backends/sqlite-node/test/writer-leases.test.ts`.

mod support;

use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::harness::session::{EntryOrder, EntryQuery, SessionErrorCode};
use pi_session_sqlite::{
    SqliteSessionRepository, SqliteSessionRepositoryOptions, SqliteWriterLeaseOptions, SqliteWriterLeaseOptionsError,
};
use support::{Fixture, file_system, user_message};
use tempfile::TempDir;

fn assert_code<T: std::fmt::Debug>(
    result: Result<T, pi_agent_core::harness::session::SessionError>,
    code: SessionErrorCode,
) {
    match result {
        Ok(value) => panic!("expected {code:?}, got {value:?}"),
        Err(error) => assert_eq!(error.code(), code, "unexpected error: {error}"),
    }
}

fn repository(
    root: &TempDir,
    database_path: &str,
    writer_lease: Option<SqliteWriterLeaseOptions>,
) -> SqliteSessionRepository {
    SqliteSessionRepository::new(SqliteSessionRepositoryOptions {
        env: file_system(root.path()),
        database_path: database_path.to_owned(),
        writer_lease,
    })
    .expect("valid lease options")
}

#[tokio::test]
async fn shares_one_write_queue_across_repeated_opens_in_one_repository() {
    let fixture = Fixture::new();
    let session = fixture.create("session").await;
    let reopened = fixture
        .repo
        .open(&session.get_metadata().await.expect("metadata"))
        .await
        .expect("reopened");
    let (first, second) = tokio::join!(
        session.append_message(user_message("first")),
        reopened.append_message(user_message("second")),
    );
    let first = first.expect("first");
    let second = second.expect("second");
    let entries = session
        .find_entries(EntryQuery {
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await
        .expect("entries");
    assert_eq!(
        entries.iter().map(|entry| entry.id().to_owned()).collect::<Vec<_>>(),
        vec![first, second]
    );
}

#[test]
fn rejects_invalid_lease_timing() {
    let root = TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let error = SqliteSessionRepository::new(SqliteSessionRepositoryOptions {
        env: file_system(root.path()),
        database_path: database_path.clone(),
        writer_lease: Some(SqliteWriterLeaseOptions {
            ttl_ms: Some(0),
            heartbeat_interval_ms: Some(1),
        }),
    })
    .err()
    .expect("invalid ttl");
    assert_eq!(error, SqliteWriterLeaseOptionsError::TtlNotPositive);
    assert_eq!(error.to_string(), "writerLease.ttlMs must be positive");
    let error = SqliteSessionRepository::new(SqliteSessionRepositoryOptions {
        env: file_system(root.path()),
        database_path,
        writer_lease: Some(SqliteWriterLeaseOptions {
            ttl_ms: Some(100),
            heartbeat_interval_ms: Some(100),
        }),
    })
    .err()
    .expect("invalid heartbeat");
    assert_eq!(
        error,
        SqliteWriterLeaseOptionsError::HeartbeatNotPositiveOrNotLessThanTtl
    );
    assert_eq!(
        error.to_string(),
        "writerLease.heartbeatIntervalMs must be positive and less than ttlMs"
    );
}

#[tokio::test]
async fn lists_complete_metadata_without_acquiring_active_sessions_writer_leases() {
    let root = TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let writer_repository = repository(&root, &database_path, None);
    let reader_repository = repository(&root, &database_path, None);
    let cwd = root.path().to_string_lossy().into_owned();
    let first = writer_repository
        .create(pi_session_sqlite::SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd: cwd.clone(),
            metadata: Some(
                serde_json::json!({"profile": "reviewer"})
                    .as_object()
                    .expect("object")
                    .clone(),
            ),
            ..Default::default()
        })
        .await
        .expect("first");
    let second = writer_repository
        .create(pi_session_sqlite::SqliteSessionCreateOptions {
            id: Some("session-2".to_owned()),
            parent_session_id: Some("session-1".to_owned()),
            cwd: cwd.clone(),
            metadata: Some(
                serde_json::json!({"profile": "writer", "name": "application-owned name"})
                    .as_object()
                    .expect("object")
                    .clone(),
            ),
            ..Default::default()
        })
        .await
        .expect("second");
    first.set_name(Some("Review session")).await.expect("name");
    second.set_name(Some("Write session")).await.expect("name");
    let first_metadata = first.get_metadata().await.expect("metadata");
    let second_metadata = second.get_metadata().await.expect("metadata");
    assert_eq!(first_metadata.name.as_deref(), Some("Review session"));
    assert_eq!(second_metadata.name.as_deref(), Some("Write session"));
    assert_eq!(
        second_metadata.metadata,
        serde_json::json!({"profile": "writer", "name": "application-owned name"})
            .as_object()
            .cloned()
    );

    let leases = |path: &str| -> Vec<(String, String, i64, i64)> {
        let db = rusqlite::Connection::open(path).expect("inspection connection");
        db.prepare("SELECT session_id, owner_id, fence, expires_at_ms FROM writer_leases ORDER BY session_id")
            .expect("statement")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("leases")
    };
    let leases_before = leases(&database_path);
    let listed = reader_repository
        .list(pi_session_sqlite::SqliteSessionListOptions { cwd: Some(cwd) })
        .await
        .expect("listed");
    let mut listed_sorted: Vec<_> = listed.into_iter().collect();
    listed_sorted.sort_by(|left, right| left.id.cmp(&right.id));
    let mut expected = vec![first_metadata, second_metadata];
    expected.sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(listed_sorted, expected);
    assert_eq!(leases(&database_path), leases_before);

    let error = reader_repository.open(&expected[0]).await.expect_err("active writer");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("already has an active writer"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn rejects_a_second_writer_until_the_first_session_releases_its_claim() {
    let root = TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let first_repository = repository(&root, &database_path, None);
    let second_repository = repository(&root, &database_path, None);
    let cwd = root.path().to_string_lossy().into_owned();
    let first = first_repository
        .create(pi_session_sqlite::SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd,
            ..Default::default()
        })
        .await
        .expect("first");
    let metadata = first.get_metadata().await.expect("metadata");
    let error = second_repository.open(&metadata).await.expect_err("second writer");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("already has an active writer"),
        "unexpected error: {error}"
    );

    first_repository.close().await.expect("closed");
    let second = second_repository.open(&metadata).await.expect("opened");
    assert!(second.append_message(user_message("new owner")).await.is_ok());
}

#[tokio::test]
async fn fences_a_stale_owner_after_an_expired_lease_is_acquired_by_another_writer() {
    let root = TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let lease = Some(SqliteWriterLeaseOptions {
        ttl_ms: Some(120_000),
        heartbeat_interval_ms: Some(60_000),
    });
    let first_repository = repository(&root, &database_path, lease.clone());
    let second_repository = repository(&root, &database_path, lease);
    let cwd = root.path().to_string_lossy().into_owned();
    let first = first_repository
        .create(pi_session_sqlite::SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd,
            ..Default::default()
        })
        .await
        .expect("first");
    let metadata = first.get_metadata().await.expect("metadata");
    let db = rusqlite::Connection::open(&database_path).expect("inspection connection");
    db.execute(
        "UPDATE writer_leases SET expires_at_ms = 0 WHERE session_id = ?",
        (&metadata.id,),
    )
    .expect("expire lease");
    drop(db);

    let second = second_repository.open(&metadata).await.expect("second");
    let error = first
        .append_message(user_message("stale owner"))
        .await
        .expect_err("stale writer");
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        error.message().contains("writer lease was lost"),
        "unexpected error: {error}"
    );
    assert!(
        second
            .find_entries(EntryQuery::default())
            .await
            .expect("entries")
            .is_empty()
    );

    let db = rusqlite::Connection::open(&database_path).expect("inspection connection");
    let current: (String, i64) = db
        .query_row(
            "SELECT owner_id, fence FROM writer_leases WHERE session_id = ?",
            (&metadata.id,),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("lease");
    assert_eq!(current.1, 2);
    drop(db);

    first_repository.close().await.expect("closed");
    let db = rusqlite::Connection::open(&database_path).expect("inspection connection");
    let after_stale_close: (String, i64) = db
        .query_row(
            "SELECT owner_id, fence FROM writer_leases WHERE session_id = ?",
            (&metadata.id,),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("lease");
    assert_eq!(after_stale_close, current);
    drop(db);
    assert!(second.append_message(user_message("current owner")).await.is_ok());
}

#[tokio::test]
async fn serializes_lease_checked_writes_for_sessions_sharing_one_database_connection() {
    let fixture = Fixture::new();
    let first = fixture.create("session-1").await;
    let second = fixture.create("session-2").await;
    let (first, second) = tokio::join!(
        first.append_message(user_message("first")),
        second.append_message(user_message("second")),
    );
    assert!(first.is_ok());
    assert!(second.is_ok());
}

#[tokio::test]
async fn renews_an_idle_writer_lease_with_a_heartbeat() {
    let fixture = Fixture::with_lease(Some(SqliteWriterLeaseOptions {
        ttl_ms: Some(300),
        heartbeat_interval_ms: Some(100),
    }));
    let session = fixture.create("session-1").await;
    let session_metadata = session.get_metadata().await.expect("metadata");
    let read_expiry = |path: &str, session_id: &str| -> i64 {
        let db = rusqlite::Connection::open(path).expect("inspection connection");
        db.query_row(
            "SELECT expires_at_ms FROM writer_leases WHERE session_id = ?",
            (session_id,),
            |row| row.get(0),
        )
        .expect("lease")
    };
    let initial = read_expiry(&fixture.database_path, &session_metadata.id);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let renewed = read_expiry(&fixture.database_path, &session_metadata.id);
    assert!(
        renewed > initial,
        "expected the heartbeat to extend {initial}, got {renewed}"
    );
}

#[tokio::test]
async fn two_repositories_share_the_same_lease_row() {
    let root = TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let first_repository = Arc::new(repository(&root, &database_path, None));
    let second_repository = Arc::new(repository(&root, &database_path, None));
    let cwd = root.path().to_string_lossy().into_owned();
    let first = first_repository
        .create(pi_session_sqlite::SqliteSessionCreateOptions {
            id: Some("session-1".to_owned()),
            cwd,
            ..Default::default()
        })
        .await
        .expect("first");
    assert_code(
        second_repository
            .open(&first.get_metadata().await.expect("metadata"))
            .await,
        SessionErrorCode::Storage,
    );
    assert_code(
        second_repository
            .delete(&first.get_metadata().await.expect("metadata"))
            .await,
        SessionErrorCode::Storage,
    );
}
