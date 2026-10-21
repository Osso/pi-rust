use pi_coding_agent::core::auth_storage::{AuthStorage, InMemoryAuthStorageBackend};
use std::sync::Arc;
#[test]
fn public_getter_preserves_surrogate_values_and_distinct_keys() {
    let backend = Arc::new(InMemoryAuthStorageBackend::new());
    backend.set_value(Some(r#"{"fake":{"type":"oauth","access":"a\ud800","refresh":"r\udfff","expires":0,"\ud800":{"nested":"\udfff"}},"\ud800":{},"�":{}}"#.into()));
    let storage = AuthStorage::from_storage(backend);
    let value = storage.get("fake").unwrap();
    assert_eq!(
        pi_js_compat::js_json::stringify_lossless(&value),
        r#"{"type":"oauth","access":"a\ud800","refresh":"r\udfff","expires":0,"\ud800":{"nested":"\udfff"}}"#
    );
}
