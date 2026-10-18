use pi_ai::auth::AuthError;
use pi_ai::models_store::{InMemoryModelsStore, ModelsStore, ModelsStoreEntry, ModelsStoreOperationOptions};
use pi_ai::types::{Modality, Model, ModelCost, ModelCostRates};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn model(provider: &str, id: &str) -> Model {
    Model {
        id: id.to_owned(),
        name: id.to_owned(),
        api: "test-api".to_owned(),
        provider: provider.to_owned(),
        base_url: "https://example.test/v1".to_owned(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 10_000.0,
        auto_compaction_threshold: None,
        max_tokens: 1_000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

#[tokio::test]
async fn in_memory_models_store_round_trips_clones_and_deletes() {
    let store = InMemoryModelsStore::new();
    assert_eq!(
        store.read("p1", ModelsStoreOperationOptions::default()).await.unwrap(),
        None
    );

    store
        .write(
            "p1",
            ModelsStoreEntry {
                models: vec![model("p1", "m1")],
                last_modified: Some(1_700_000_000_000.0),
                checked_at: Some(1_700_000_001_000.0),
                etag: Some("\"etag\"".to_owned()),
            },
            ModelsStoreOperationOptions::default(),
        )
        .await
        .unwrap();

    let mut read = store
        .read("p1", ModelsStoreOperationOptions::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.models[0].id, "m1");
    assert_eq!(read.etag.as_deref(), Some("\"etag\""));
    read.models[0].id = "mutated".to_owned();
    let reread = store
        .read("p1", ModelsStoreOperationOptions::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reread.models[0].id, "m1");

    store
        .delete("p1", ModelsStoreOperationOptions::default())
        .await
        .unwrap();
    assert_eq!(
        store.read("p1", ModelsStoreOperationOptions::default()).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn in_memory_models_store_honors_cancellation() {
    let store = InMemoryModelsStore::new();
    let signal = CancellationToken::new();
    signal.cancel();
    let options = || ModelsStoreOperationOptions {
        signal: Some(signal.clone()),
    };
    assert_eq!(store.read("p1", options()).await, Err(AuthError::Aborted));
    assert_eq!(
        store.write("p1", ModelsStoreEntry::new(Vec::new()), options()).await,
        Err(AuthError::Aborted)
    );
    assert_eq!(store.delete("p1", options()).await, Err(AuthError::Aborted));
}

#[test]
fn models_store_entry_serializes_camel_case_timestamps() {
    let entry = ModelsStoreEntry {
        models: vec![model("p1", "m1")],
        last_modified: Some(1_700_000_000_000.0),
        checked_at: Some(1_700_000_001_000.5),
        etag: Some("v1".to_owned()),
    };
    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(value["lastModified"], json!(1_700_000_000_000_i64));
    assert_eq!(value["checkedAt"], json!(1_700_000_001_000.5));
    assert_eq!(value["etag"], json!("v1"));
    assert_eq!(value["models"][0]["provider"], json!("p1"));
}
