//! Port of `packages/session-backends/sqlite-node/test/branch-query.test.ts`.

mod support;

use pi_agent_core::harness::session::{BranchBounds, BranchQuery, EntryOrder, EntryQuery, EntryType, SessionErrorCode};
use support::{Fixture, assistant_message, user_message};

fn branch_query(start: &str) -> BranchQuery {
    BranchQuery {
        bounds: BranchBounds {
            start: Some(start.to_owned()),
            ..Default::default()
        },
        entry: EntryQuery::default(),
    }
}

#[tokio::test]
async fn does_not_decode_entries_outside_bounded_branch_queries() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let root_id = session.append_message(user_message("root")).await.expect("root");
    let middle_id = session
        .append_message(assistant_message("middle"))
        .await
        .expect("middle");
    let leaf_id = session.append_message(user_message("leaf")).await.expect("leaf");

    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET payload = ? WHERE session_id = ? AND id = ?",
        ("not json", "session-1", &middle_id),
    )
    .expect("corrupt middle");
    let branch_id: String = db
        .query_row(
            "SELECT branch_id FROM branch_entries WHERE session_id = ? AND entry_id = ?",
            ("session-1", &leaf_id),
            |row| row.get(0),
        )
        .expect("branch row");
    db.execute(
        "DELETE FROM branch_entries WHERE session_id = ? AND branch_id = ? AND entry_id = ?",
        ("session-1", &branch_id, &middle_id),
    )
    .expect("drop middle from cache");
    drop(db);

    let bounded = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(leaf_id.clone()),
                stop_at_id: Some(leaf_id.clone()),
                ..Default::default()
            },
            entry: EntryQuery::default(),
        })
        .await
        .expect("bounded");
    assert_eq!(
        bounded.iter().map(|entry| entry.id().to_owned()).collect::<Vec<_>>(),
        vec![leaf_id.clone()]
    );
    let root_window = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(leaf_id.clone()),
                stop_at_id: Some(root_id.clone()),
                ..Default::default()
            },
            entry: EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                limit: Some(1),
                ..Default::default()
            },
        })
        .await
        .expect("root window");
    assert_eq!(
        root_window
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect::<Vec<_>>(),
        vec![root_id]
    );
    let error = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(leaf_id),
                ..Default::default()
            },
            entry: EntryQuery {
                limit: Some(2),
                ..Default::default()
            },
        })
        .await
        .expect_err("missing middle");
    assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
    assert!(
        error.message().contains(&format!("Entry {middle_id} not found")),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn does_not_decode_entries_excluded_by_branch_query_filters_and_limits() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    session.append_message(user_message("root")).await.expect("root");
    let custom_id = session
        .append_custom_entry("note", Some(serde_json::json!({"value": 1})))
        .await
        .expect("custom");
    let leaf_id = session.append_message(assistant_message("leaf")).await.expect("leaf");

    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET payload = ? WHERE session_id = ? AND id = ?",
        ("{}", "session-1", &custom_id),
    )
    .expect("blank custom payload");
    drop(db);

    let messages = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(leaf_id.clone()),
                ..Default::default()
            },
            entry: EntryQuery {
                entry_type: Some(EntryType::Message),
                limit: Some(1),
                ..Default::default()
            },
        })
        .await
        .expect("messages");
    assert_eq!(
        messages.iter().map(|entry| entry.id().to_owned()).collect::<Vec<_>>(),
        vec![leaf_id.clone()]
    );

    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET payload = ? WHERE session_id = ? AND id = ?",
        ("not json", "session-1", &custom_id),
    )
    .expect("corrupt custom payload");
    drop(db);
    let other_custom = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(leaf_id),
                ..Default::default()
            },
            entry: EntryQuery {
                custom_type: Some("other".to_owned()),
                ..Default::default()
            },
        })
        .await
        .expect("other custom");
    assert!(other_custom.is_empty());
}

#[tokio::test]
async fn does_not_validate_ancestors_beyond_newest_first_stop_bounds() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let root_id = session.append_message(user_message("root")).await.expect("root");
    let child_id = session.append_message(assistant_message("child")).await.expect("child");

    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET parent_id = ? WHERE session_id = ? AND id = ?",
        ("missing-parent", "session-1", &child_id),
    )
    .expect("break parent");
    drop(db);

    let stopped = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(child_id.clone()),
                stop_at_id: Some(child_id.clone()),
                ..Default::default()
            },
            entry: EntryQuery::default(),
        })
        .await
        .expect("stopped by id");
    assert_eq!(
        stopped.iter().map(|entry| entry.id().to_owned()).collect::<Vec<_>>(),
        vec![child_id.clone()]
    );
    let stopped_by_type = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(child_id.clone()),
                stop_at_type: Some(EntryType::Message),
                ..Default::default()
            },
            entry: EntryQuery::default(),
        })
        .await
        .expect("stopped by type");
    assert_eq!(
        stopped_by_type
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect::<Vec<_>>(),
        vec![child_id.clone()]
    );
    let error = session
        .find_entries_on_branch(branch_query(&child_id))
        .await
        .expect_err("missing parent");
    assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
    assert!(
        error.message().contains("Entry missing-parent not found"),
        "unexpected error: {error}"
    );

    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET parent_id = ? WHERE session_id = ? AND id = ?",
        (&root_id, "session-1", &child_id),
    )
    .expect("cycle child");
    db.execute(
        "UPDATE entries SET parent_id = ? WHERE session_id = ? AND id = ?",
        (&child_id, "session-1", &root_id),
    )
    .expect("cycle root");
    drop(db);

    let stopped = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(child_id.clone()),
                stop_at_id: Some(child_id.clone()),
                ..Default::default()
            },
            entry: EntryQuery::default(),
        })
        .await
        .expect("stopped by id");
    assert_eq!(
        stopped.iter().map(|entry| entry.id().to_owned()).collect::<Vec<_>>(),
        vec![child_id.clone()]
    );
    let stopped_by_type = session
        .find_entries_on_branch(BranchQuery {
            bounds: BranchBounds {
                start: Some(child_id.clone()),
                stop_at_type: Some(EntryType::Message),
                ..Default::default()
            },
            entry: EntryQuery::default(),
        })
        .await
        .expect("stopped by type");
    assert_eq!(
        stopped_by_type
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect::<Vec<_>>(),
        vec![child_id.clone()]
    );
    let error = session
        .find_entries_on_branch(branch_query(&child_id))
        .await
        .expect_err("cycle");
    assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
    assert!(
        error.message().contains(&format!("Entry {child_id} not found")),
        "unexpected error: {error}"
    );
}
