//! Port of `packages/session-backends/sqlite-node/test/log-query.test.ts`.

mod support;

use pi_agent_core::harness::session::{FactItem, LogItem, LogOptions};
use support::{Fixture, user_message};

#[tokio::test]
async fn does_not_decode_rows_beyond_the_requested_log_limit() {
    let fixture = Fixture::new();
    let session = fixture.create("session-1").await;
    let root_id = session.append_message(user_message("root")).await.expect("root");
    session.set_name(Some("name")).await.expect("name");
    let tail_id = session.append_message(user_message("tail")).await.expect("tail");

    let db = fixture.open_connection();
    db.execute(
        "UPDATE entries SET payload = ? WHERE session_id = ? AND id = ?",
        ("not json", "session-1", &tail_id),
    )
    .expect("corrupt tail");
    drop(db);

    assert_eq!(
        session
            .get_log(LogOptions {
                after_seq: None,
                limit: Some(1)
            })
            .await
            .expect("log"),
        vec![LogItem::Entry {
            seq: 1,
            entry: session.get_entry(&root_id).await.expect("entry").expect("entry")
        }]
    );
    assert_eq!(
        session
            .get_log(LogOptions {
                after_seq: Some(1),
                limit: Some(1)
            })
            .await
            .expect("log"),
        vec![LogItem::Fact {
            seq: 2,
            fact: FactItem::Name {
                name: Some("name".to_owned())
            }
        }]
    );
}
