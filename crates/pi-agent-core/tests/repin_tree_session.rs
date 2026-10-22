use pi_agent_core::harness::session::{
    InMemorySessionStorage, InMemorySessionStorageOptions, SessionStorage, SessionTreeEntry,
};
use pi_js_compat::js_json::{JsString, parse_lossless};

fn entry(raw: &str) -> SessionTreeEntry {
    SessionTreeEntry::from_value(parse_lossless(raw).unwrap()).unwrap()
}

#[tokio::test]
async fn duplicate_ids_remain_in_the_append_log_and_last_value_wins_the_index() {
    let storage = InMemorySessionStorage::new(InMemorySessionStorageOptions::default()).unwrap();
    let first = entry(
        r#"{"type":"custom","id":"duplicate","parentId":null,"timestamp":"2026-10-22T00:00:00.000Z","customType":"fixture","data":"first"}"#,
    );
    let second = entry(
        r#"{"type":"custom","id":"duplicate","parentId":null,"timestamp":"2026-10-22T00:00:00.000Z","customType":"fixture","data":"second"}"#,
    );
    storage.append_entry(first.clone()).await.unwrap();
    storage
        .append_entry(second.clone())
        .await
        .expect("frozen appends duplicate ids");
    assert_eq!(storage.get_entries().await.unwrap(), vec![first, second.clone()]);
    assert_eq!(
        storage.get_entry(&JsString::from("duplicate")).await.unwrap(),
        Some(second)
    );
}
