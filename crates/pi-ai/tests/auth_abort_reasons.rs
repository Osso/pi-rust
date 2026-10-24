use pi_ai::auth::{ApiKeyCredential, AuthOperationOptions, Credential, CredentialStore, InMemoryCredentialStore};
use pi_js_compat::abort::{AbortController, AbortReason};
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn credential_abort_retains_the_actual_native_error_message() {
    let controller = AbortController::new();
    let native = NativeNodeError::from_constructor(NativeErrorClass::TypeError, "specific auth cancellation");
    controller.abort(Some(native.clone().into()));
    let store = InMemoryCredentialStore::new();
    let error = store
        .read("p1", AuthOperationOptions::new(Some(controller.signal())))
        .await
        .unwrap_err();
    assert_eq!(error.detail(), Some(&native.message));
    assert_eq!(error.thrown_value(), controller.signal().reason().unwrap());
}

#[tokio::test]
async fn every_credential_operation_retains_default_falsey_raw_and_native_abort_reasons() {
    let native = NativeNodeError::from_constructor(NativeErrorClass::TypeError, JsString::from_utf16(vec![78, 0xd800]))
        .with_property("code", JsValue::String("E_CANCEL".into()));
    for supplied in [
        None,
        Some(AbortReason::Value(JsValue::Null)),
        Some(AbortReason::Value(JsValue::Bool(false))),
        Some(AbortReason::Value(JsValue::Number(-0.0))),
        Some(AbortReason::Value(JsValue::String(JsString::from_utf16(vec![0xd800])))),
        Some(AbortReason::Value(
            pi_js_compat::js_json!({"raw":JsString::from_utf16(vec![0xdc00])}),
        )),
        Some(native.into()),
    ] {
        let controller = AbortController::new();
        controller.abort(supplied);
        let expected = controller.signal().reason().unwrap();
        let store = InMemoryCredentialStore::new();
        let options = AuthOperationOptions::new(Some(controller.signal()));
        let errors = vec![
            store.read("p1", options.clone()).await.unwrap_err(),
            store.list(options.clone()).await.unwrap_err(),
            store.delete("p1", options.clone()).await.unwrap_err(),
            store
                .modify(
                    "p1",
                    Arc::new(|_| Box::pin(async { panic!("aborted modifier must not run") })),
                    options,
                )
                .await
                .unwrap_err(),
        ];
        for error in errors {
            let actual = error.thrown_value();
            assert_eq!(actual, expected);
            if let AbortReason::Value(JsValue::Number(value)) = actual {
                assert_eq!(value.to_bits(), (-0.0f64).to_bits());
            }
        }
    }
}

#[tokio::test]
async fn queued_modify_and_delete_abort_with_their_own_reason_without_mutation() {
    let store = Arc::new(InMemoryCredentialStore::new());
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, gate) = tokio::sync::oneshot::channel();
    let started = Arc::new(Mutex::new(Some(started)));
    let gate = Arc::new(Mutex::new(Some(gate)));
    let active = tokio::spawn({
        let store = store.clone();
        async move {
            store
                .modify(
                    "p1",
                    Arc::new(move |_| {
                        let started = started.lock().unwrap().take().unwrap();
                        let gate = gate.lock().unwrap().take().unwrap();
                        Box::pin(async move {
                            started.send(()).unwrap();
                            gate.await.unwrap();
                            Ok(Some(Credential::ApiKey(ApiKeyCredential {
                                key: Some("committed".into()),
                                env: None,
                            })))
                        })
                    }),
                    Default::default(),
                )
                .await
                .unwrap()
        }
    });
    ready.await.unwrap();
    let modify = AbortController::new();
    let delete = AbortController::new();
    let queued_modify = store.modify(
        "p1",
        Arc::new(|_| Box::pin(async { panic!("queued canceled modifier must not run") })),
        AuthOperationOptions::new(Some(modify.signal())),
    );
    let queued_delete = store.delete("p1", AuthOperationOptions::new(Some(delete.signal())));
    tokio::pin!(queued_modify, queued_delete);
    assert!(futures_util::poll!(queued_modify.as_mut()).is_pending());
    assert!(futures_util::poll!(queued_delete.as_mut()).is_pending());
    modify.abort(Some(AbortReason::Value(JsValue::Bool(false))));
    delete.abort(Some(AbortReason::Value(JsValue::String(JsString::from_utf16(vec![
        68, 0xd800,
    ])))));
    assert_eq!(
        queued_modify.await.unwrap_err().thrown_value(),
        modify.signal().reason().unwrap()
    );
    assert_eq!(
        queued_delete.await.unwrap_err().thrown_value(),
        delete.signal().reason().unwrap()
    );
    release.send(()).unwrap();
    active.await.unwrap();
    let stored = store.read("p1", Default::default()).await.unwrap().unwrap();
    assert_eq!(stored.as_api_key().unwrap().key.as_ref().unwrap(), "committed");
}
