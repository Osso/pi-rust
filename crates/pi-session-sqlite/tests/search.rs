//! Port of `packages/session-backends/sqlite-node/test/search.test.ts`.

mod support;

use pi_agent_core::harness::session::{EntryOrder, EntryQuery, EntryType, SessionErrorCode};
use pi_agent_core::search::{SessionSearch, SessionSearchOptions};
use pi_session_sqlite::{
    SqliteSessionRepository, SqliteSessionRepositoryOptions, SqliteSessionSearch, SqliteSessionSearchHit,
    SqliteSessionSearchOptions, create_sqlite_session_search,
};
use serde_json::json;
use support::{Fixture, file_system, user_message};
use tempfile::TempDir;

fn search_for(fixture: &Fixture) -> SqliteSessionSearch {
    create_sqlite_session_search(SqliteSessionSearchOptions {
        env: file_system(fixture.root.path()),
        database_path: fixture.database_path.clone(),
    })
}

async fn collect(
    search: &SqliteSessionSearch,
    text: &str,
    options: SessionSearchOptions,
) -> Vec<SqliteSessionSearchHit> {
    search.search(text, options).await.expect("search")
}

fn default_options() -> SessionSearchOptions {
    SessionSearchOptions::default()
}

#[tokio::test]
async fn matches_trigrams() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let included = fixture
        .create_with_metadata(
            "included",
            json!({"name": "application-owned"})
                .as_object()
                .expect("object")
                .clone(),
        )
        .await;
    let excluded = fixture
        .repo
        .create(pi_session_sqlite::SqliteSessionCreateOptions {
            id: Some("excluded".to_owned()),
            cwd: format!("{}/other", fixture.cwd()),
            ..Default::default()
        })
        .await
        .expect("excluded");
    let entry_id = included
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    included.set_name(Some("Canonical name")).await.expect("name");
    let excluded_entry_id = excluded
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");

    let auth_hits = collect(&search, "auth", default_options()).await;
    assert_eq!(auth_hits.len(), 2);
    let included_hit = auth_hits
        .iter()
        .find(|hit| hit.session_id == "included")
        .expect("included hit");
    assert_eq!(included_hit.entry_id, entry_id);
    assert_eq!(included_hit.metadata.name.as_deref(), Some("Canonical name"));
    assert_eq!(
        included_hit.metadata.metadata,
        json!({"name": "application-owned"}).as_object().cloned()
    );
    assert!(included_hit.timestamp > 0);
    let excluded_hit = auth_hits
        .iter()
        .find(|hit| hit.session_id == "excluded")
        .expect("excluded hit");
    assert_eq!(excluded_hit.entry_id, excluded_entry_id);

    let uth_hits = collect(&search, "uth", default_options()).await;
    assert_eq!(uth_hits.len(), 2);
    assert!(uth_hits.iter().any(|hit| hit.session_id == "included"));
    assert!(uth_hits.iter().any(|hit| hit.session_id == "excluded"));
}

#[tokio::test]
async fn omits_a_cleared_session_name_from_search_metadata() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let session = fixture.create("session-1").await;
    let entry_id = session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    session.set_name(Some("Temporary")).await.expect("name");
    session.set_name(None).await.expect("clear");
    let hits = collect(&search, "auth", default_options()).await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, "session-1");
    assert_eq!(hits[0].entry_id, entry_id);
    assert_eq!(hits[0].metadata.name, None);
}

#[tokio::test]
async fn handles_quoted_search_text_without_exposing_fts_syntax() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    assert!(
        collect(&search, "missing \"phrase\"", default_options())
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn rebuilds_existing_entries_when_fts_is_first_initialized() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let session = fixture.create("session-1").await;
    let entry_id = session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    let hits = collect(&search, "auth", default_options()).await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, "session-1");
    assert_eq!(hits[0].entry_id, entry_id);
}

#[tokio::test]
async fn honors_entry_type_filters() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let session = fixture.create("session-1").await;
    let message_entry_id = session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("message");
    session
        .append_custom_entry("note", Some(json!({"text": "Find the auth custom entry"})))
        .await
        .expect("custom");
    let hits = collect(
        &search,
        "auth",
        SessionSearchOptions {
            entry_types: Some(vec![EntryType::Message]),
            ..SessionSearchOptions::default()
        },
    )
    .await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entry_id, message_entry_id);
}

#[tokio::test]
async fn honors_result_limits() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let first = fixture.create("session-1").await;
    let second = fixture.create("session-2").await;
    first
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("first");
    second
        .append_message(user_message("Find the auth defect too"))
        .await
        .expect("second");
    assert_eq!(
        collect(
            &search,
            "auth",
            SessionSearchOptions {
                limit: Some(1),
                ..SessionSearchOptions::default()
            }
        )
        .await
        .len(),
        1
    );
    assert!(
        collect(
            &search,
            "auth",
            SessionSearchOptions {
                limit: Some(0),
                ..SessionSearchOptions::default()
            }
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn removes_deleted_session_entries_from_the_index() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let session = fixture.create("session-1").await;
    session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    assert_eq!(collect(&search, "auth", default_options()).await.len(), 1);
    fixture
        .repo
        .delete(&session.get_metadata().await.expect("metadata"))
        .await
        .expect("delete");
    assert!(collect(&search, "auth", default_options()).await.is_empty());
}

#[tokio::test]
async fn indexes_and_removes_session_entries_through_triggers_after_fts_initialization() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    assert!(collect(&search, "auth", default_options()).await.is_empty());
    let session = fixture.create("session-1").await;
    session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    assert_eq!(collect(&search, "auth", default_options()).await.len(), 1);
    fixture
        .repo
        .delete(&session.get_metadata().await.expect("metadata"))
        .await
        .expect("delete");
    assert!(collect(&search, "auth", default_options()).await.is_empty());
}

#[tokio::test]
async fn removes_deleted_entries_from_fts_through_triggers() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    let session = fixture.create("session-1").await;
    let entry_id = session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    assert_eq!(collect(&search, "auth", default_options()).await.len(), 1);
    let db = fixture.open_connection();
    db.execute(
        "DELETE FROM entries WHERE session_id = ? AND id = ?",
        ("session-1", &entry_id),
    )
    .expect("delete entry");
    drop(db);
    assert!(collect(&search, "auth", default_options()).await.is_empty());
}

#[tokio::test]
async fn does_not_initialize_fts_for_canonical_writes_or_blank_searches() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    assert!(collect(&search, "  ", default_options()).await.is_empty());
    let session = fixture.create("session-1").await;
    let db = fixture.open_connection();
    let fts_exists: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'session_search_fts'",
            [],
            |row| row.get(0),
        )
        .expect("fts lookup");
    assert_eq!(fts_exists, 0);
    drop(db);
    assert!(session.append_message(user_message("still writable")).await.is_ok());
}

#[tokio::test]
async fn rolls_back_canonical_appends_when_colocated_fts_trigger_writes_fail() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    collect(&search, "initialize", default_options()).await;
    let session = fixture.create("session-1").await;
    let db = fixture.open_connection();
    db.execute_batch("DROP TABLE session_search_fts").expect("drop fts");
    drop(db);
    assert!(session.append_message(user_message("must roll back")).await.is_err());
    let entries = session
        .find_entries(EntryQuery {
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await
        .expect("entries");
    assert!(entries.is_empty());
}

#[tokio::test]
async fn rolls_back_canonical_deletion_when_colocated_fts_cleanup_fails() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    collect(&search, "initialize", default_options()).await;
    let session = fixture.create("session-1").await;
    session
        .append_message(user_message("must remain"))
        .await
        .expect("entry");
    let session_metadata = session.get_metadata().await.expect("metadata");
    let db = fixture.open_connection();
    db.execute_batch("DROP TABLE session_search_fts").expect("drop fts");
    drop(db);
    assert!(fixture.repo.delete(&session_metadata).await.is_err());
    let reopened = fixture.repo.open(&session_metadata).await.expect("reopened");
    let entries = reopened
        .find_entries(EntryQuery {
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await
        .expect("entries");
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn fails_when_search_setup_fails() {
    let root = TempDir::new().expect("temp dir");
    let database_path = root.path().join("sessions.sqlite");
    std::fs::create_dir(&database_path).expect("directory at the database path");
    let search = create_sqlite_session_search(SqliteSessionSearchOptions {
        env: file_system(root.path()),
        database_path: database_path.to_string_lossy().into_owned(),
    });
    let error = search
        .search("auth", default_options())
        .await
        .expect_err("setup must fail");
    match error {
        pi_agent_core::search::SearchError::Session(error) => {
            assert_eq!(error.code(), SessionErrorCode::Storage);
        }
        other => panic!("unexpected error {other}"),
    }
    let _ = SqliteSessionRepository::new(SqliteSessionRepositoryOptions {
        env: file_system(root.path()),
        database_path: database_path.to_string_lossy().into_owned(),
        writer_lease: None,
    })
    .expect("valid options");
}

#[tokio::test]
async fn initializes_canonical_storage_when_searched_before_the_first_session_is_created() {
    let fixture = Fixture::new();
    let search = search_for(&fixture);
    assert!(collect(&search, "auth", default_options()).await.is_empty());
    let session = fixture.create("session-1").await;
    let entry_id = session
        .append_message(user_message("Find the auth defect"))
        .await
        .expect("entry");
    let hits = collect(&search, "auth", default_options()).await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, "session-1");
    assert_eq!(hits[0].entry_id, entry_id);
    assert_eq!(hits[0].metadata.id, "session-1");
    assert!(session.append_message(user_message("Still writable")).await.is_ok());
}
