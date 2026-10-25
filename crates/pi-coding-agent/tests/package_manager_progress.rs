use crate::shared_oracle as oracle;
use pi_ai::request::CallbackError;
use pi_coding_agent::core::{
    package_manager::{DefaultPackageManager, PackageManagerError, ProgressType, SettingsManagerStore},
    settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions},
};
use pi_js_compat::{
    abort::AbortReason,
    js_json as json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::{Arc, Mutex};

fn snapshot(error: PackageManagerError) -> JsValue {
    match error.into_callback().thrown_value() {
        AbortReason::Value(value) => json!({"kind":"value","value":value}),
        AbortReason::Error(error) => json!({"kind":"error","value":error.value()}),
        AbortReason::DomException(error) => json!({"kind":"dom","value":error.value()}),
    }
}
fn failures() -> Vec<(CallbackError, JsValue)> {
    let raw = JsString::from_utf16(vec![80, 0xd800, 0xdc00, 0xdc00]);
    let mut failures: Vec<_> = [
        JsValue::Bool(false),
        JsValue::Null,
        JsValue::String(raw.clone()),
        json!({"toString":0}),
    ]
    .into_iter()
    .map(|value| {
        (
            CallbackError::from_rejection(AbortReason::Value(value.clone())),
            json!({"kind":"raw","value":value}),
        )
    })
    .collect();
    let error = NativeNodeError::from_constructor(NativeErrorClass::TypeError, raw.clone())
        .with_property("code", JsValue::String("E_PROGRESS".into()))
        .with_cause(AbortReason::Value(JsValue::Bool(false)));
    failures.push((CallbackError::from(error), json!({"kind":"native","message":raw})));
    failures
}
#[tokio::test]
async fn progress_callbacks_preserve_throw_boundaries_raw_values_and_reentrant_clear() {
    let directory = tempfile::tempdir().unwrap();
    let base = JsString::from(directory.path().to_str().unwrap());
    let existing = directory.path().join("existing");
    std::fs::create_dir(&existing).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "package-manager-progress-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    for stage in ["start", "complete", "error", "clear", "none"] {
        for (failure, descriptor) in failures() {
            let settings = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
            let manager = Arc::new(
                DefaultPackageManager::new(&base, &base, Arc::new(SettingsManagerStore::new(settings))).unwrap(),
            );
            let path = if stage == "error" {
                directory.path().join("missing")
            } else {
                existing.clone()
            };
            let path = JsString::from(path.to_str().unwrap());
            let expected = source.request(&json!({"base":base,"source":path,"stage":stage,"failure":descriptor}));
            assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
            let events = Arc::new(Mutex::new(Vec::new()));
            let recorded = events.clone();
            let owner = Arc::downgrade(&manager);
            manager.set_progress_callback(Some(Arc::new(move |event| {
                recorded.lock().unwrap().push(event.clone());
                if stage == "clear" {
                    owner.upgrade().unwrap().set_progress_callback(None);
                }
                let throws = matches!(
                    (stage, event.event_type),
                    ("start", ProgressType::Start)
                        | ("complete", ProgressType::Complete)
                        | ("error", ProgressType::Error)
                );
                if throws {
                    return Err(failure.clone());
                }
                Ok(())
            })));
            let result = manager.install(&path, false).await;
            let outcome = match result {
                Ok(()) => json!({"ok":true}),
                Err(error) => json!({"ok":false,"error":snapshot(error)}),
            };
            let actual = json!({"outcome":outcome,"progress":*events.lock().unwrap()});
            assert_eq!(actual, expected["output"], "{stage}: {descriptor:?}");
        }
    }
}
