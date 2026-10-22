use pi_agent_core::harness::session::{
    EntryBody, InMemorySessionStorage, OptionalValue, SessionMetadata, SessionStorage,
};

#[tokio::test]
async fn duplicate_ids_remain_in_the_append_log_and_last_value_wins_the_index() {
    let storage = InMemorySessionStorage::new(SessionMetadata {
        id: "fixture".into(),
        created_at: 0,
        parent_session_id: None,
    });
    let make = || EntryBody::Custom {
        id: "duplicate".into(),
        custom_type: "fixture".into(),
        data: OptionalValue::Missing,
    };
    storage.append_entry(make(), "main").await.unwrap();
    // Frozen memory-storage.ts:97–100 intentionally does not reject duplicate IDs.
    storage
        .append_entry(make(), "main")
        .await
        .expect("frozen appends duplicate ids");
}
