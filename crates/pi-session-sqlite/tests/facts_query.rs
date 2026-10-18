//! Port of `packages/session-backends/sqlite-node/test/facts-query.test.ts`.

mod support;

use pi_session_sqlite::apply_migrations;
use pi_session_sqlite::sqlite::storage::facts::{append_fact, read_latest_fact, read_latest_label_facts};
use support::Fixture;

#[test]
fn reads_latest_facts_and_latest_non_null_labels() {
    let fixture = Fixture::new();
    let db = fixture.open_connection();
    apply_migrations(&db).expect("migrations");
    let quoted = |value: &str| serde_json::to_string(value).expect("quoted");
    append_fact(&db, "session-1", 1, "label", Some("entry-1"), Some(&quoted("old"))).expect("fact");
    append_fact(&db, "session-1", 2, "label", Some("entry-2"), Some(&quoted("kept"))).expect("fact");
    append_fact(&db, "session-1", 3, "label", Some("entry-1"), Some(&quoted("new"))).expect("fact");
    append_fact(&db, "session-1", 4, "label", Some("entry-3"), Some(&quoted("removed"))).expect("fact");
    append_fact(&db, "session-1", 5, "label", Some("entry-3"), None).expect("fact");
    append_fact(&db, "session-1", 6, "name", None, Some(&quoted("session name"))).expect("fact");
    append_fact(
        &db,
        "other-session",
        1,
        "label",
        Some("entry-1"),
        Some(&quoted("other")),
    )
    .expect("fact");

    let latest = read_latest_fact(&db, "session-1", "label", Some("entry-1"))
        .expect("latest fact")
        .expect("fact row");
    assert_eq!(latest.value.as_deref(), Some(quoted("new").as_str()));
    let name = read_latest_fact(&db, "session-1", "name", None)
        .expect("latest name")
        .expect("name row");
    assert_eq!(name.value.as_deref(), Some(quoted("session name").as_str()));
    let labels = read_latest_label_facts(&db, "session-1").expect("labels");
    assert_eq!(
        labels,
        vec![
            (Some("entry-1".to_owned()), quoted("new")),
            (Some("entry-2".to_owned()), quoted("kept")),
        ]
    );
}
