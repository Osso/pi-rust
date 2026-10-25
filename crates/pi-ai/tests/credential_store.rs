#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_ai::auth::{
    ApiKeyCredential, AuthError, Credential, CredentialModifier, CredentialStore, InMemoryCredentialStore,
};
use serde_json::{Value, json};
use tokio::sync::Notify;

fn credential(key: &str) -> Credential {
    Credential::ApiKey(ApiKeyCredential {
        key: Some(key.into()),
        env: None,
    })
}

fn modifier(result: Result<Option<Credential>, AuthError>) -> CredentialModifier {
    Arc::new(move |_| {
        let result = result.clone();
        Box::pin(async move { result })
    })
}

fn gated_modifier(
    entered: Arc<Notify>,
    gate: Arc<Notify>,
    result: Result<Option<Credential>, AuthError>,
) -> CredentialModifier {
    Arc::new(move |_| {
        let entered = Arc::clone(&entered);
        let gate = Arc::clone(&gate);
        let result = result.clone();
        Box::pin(async move {
            entered.notify_one();
            gate.notified().await;
            result
        })
    })
}

async fn queue_scenario() -> Value {
    let store = InMemoryCredentialStore::new();
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Notify::new());
    store.modify("p", modifier(Ok(Some(credential("seed"))))).await.unwrap();
    let first = store.modify(
        "p",
        gated_modifier(Arc::clone(&entered), Arc::clone(&gate), Ok(Some(credential("first")))),
    );
    let second_input = Arc::new(Mutex::new(None));
    let recorded = Arc::clone(&second_input);
    let tail = async {
        entered.notified().await;
        let second = store.modify(
            "p",
            Arc::new(move |current| {
                *recorded.lock().unwrap() = current;
                Box::pin(async { Ok(Some(credential("second"))) })
            }),
        );
        let deletion = store.delete("p");
        let during = store.read("p").await.unwrap();
        let other = store
            .modify("q", modifier(Ok(Some(credential("other")))))
            .await
            .unwrap();
        gate.notify_one();
        let (second, deletion) = tokio::join!(second, deletion);
        deletion.unwrap();
        (during, other, second.unwrap())
    };
    let (first, (during, other, second)) = tokio::join!(first, tail);
    let second_input = second_input.lock().unwrap().clone();
    let final_credential = store.read("p").await.unwrap();
    json!({"during": during, "other": other, "results": [first.unwrap(), second],
        "secondInput": second_input, "final": final_credential})
}

async fn recovery_scenario() -> Value {
    let store = InMemoryCredentialStore::new();
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Notify::new());
    store.modify("p", modifier(Ok(Some(credential("seed"))))).await.unwrap();
    let failing = store.modify(
        "p",
        gated_modifier(
            Arc::clone(&entered),
            Arc::clone(&gate),
            Err(AuthError::failed("denied")),
        ),
    );
    let tail = async {
        entered.notified().await;
        let following = store.modify("p", modifier(Ok(None)));
        gate.notify_one();
        let unchanged = following.await.unwrap();
        let preserved = store.read("p").await.unwrap();
        let missing_noop = store.modify("q", modifier(Ok(None))).await.unwrap();
        store.delete("p").await.unwrap();
        let deleted = store.read("p").await.unwrap();
        let recreated = store
            .modify("p", modifier(Ok(Some(credential("recovered")))))
            .await
            .unwrap();
        json!({"unchanged": unchanged, "preserved": preserved, "missingNoop": missing_noop,
            "deleted": deleted, "recreated": recreated})
    };
    let (failure, mut result) = tokio::join!(failing, tail);
    result["rejected"] = json!(failure.is_err());
    result
}

fn compare_source(scenario: &str, output: Value) {
    let mut source = oracle::TsOracle::spawn("credential-store-oracle.ts", "packages/ai/src/auth/credential-store.ts");
    let expected = source.request(&json!({"id": 0, "scenario": scenario}));
    assert_eq!(json!({"id": 0, "ok": true, "output": output}), expected);
}

#[tokio::test]
async fn provider_queue_allows_reads_and_other_providers_then_orders_modify_delete() {
    let output = tokio::time::timeout(Duration::from_secs(5), queue_scenario())
        .await
        .expect("credential queue must complete");
    compare_source("queue", output);
}

#[tokio::test]
async fn queued_failure_preserves_current_and_does_not_poison_later_operations() {
    let output = tokio::time::timeout(Duration::from_secs(5), recovery_scenario())
        .await
        .expect("failure recovery must complete");
    compare_source("recovery", output);
}
