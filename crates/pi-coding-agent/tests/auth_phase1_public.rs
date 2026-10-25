//! Frozen public APIs and bidirectional files, with synthetic credentials only.
use crate::shared_oracle as oracle;
use pi_coding_agent::core::auth_storage::{AuthStorage, GetApiKeyOptions};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::{fs, time::SystemTime};
fn fixture() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "public-auth-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p.join("private/nested/auth.json")
}
#[test]
fn public_ts_rust_ts_and_reverse_preserve_oauth_extras_and_unrelated_providers() {
    let path = fixture();
    let mut oracle = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let credential = json!({"type":"oauth","refresh":"A","access":"synthetic-access","expires":4_000_000_000_000u64,"accountId":"account","email":"synthetic@example.test","sentinel":{"extra":[1,true]}});
    for (provider, cred) in [
        ("openai-codex", credential.clone()),
        (
            "api",
            json!({"type":"api_key","key":"${TOKEN}","env":{"TOKEN":"synthetic-key"},"unknown":"preserve"}),
        ),
        ("unrelated", json!({"raw":["untouched"]})),
    ] {
        let out = oracle.request(&json!({"id":1,"op":"modify","authPath":path,"provider":provider,"credential":cred}));
        assert_eq!(out["ok"], true, "{out}");
    }
    let storage = AuthStorage::create(Some(&JsString::from(path.to_str().unwrap()))).unwrap();
    assert_eq!(
        storage.get("openai-codex").map(|value| value.to_serde().unwrap()),
        Some(credential.clone())
    );
    assert_eq!(
        pi_js_compat::js_json::JsValue::Object(storage.get_all())
            .to_serde()
            .unwrap()["unrelated"],
        json!({"raw":["untouched"]})
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        rt.block_on(storage.get_api_key("api", GetApiKeyOptions::default()))
            .unwrap()
            .map(|key| key.to_utf8().unwrap())
            .as_deref(),
        Some("synthetic-key")
    );
    storage
        .set("new-provider", json!({"type":"api_key","key":"rust-key"}))
        .unwrap();
    let out = oracle.request(&json!({"id":2,"op":"list","authPath":path,"provider":"new-provider"}));
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["output"]["all"]["openai-codex"], credential);
    assert_eq!(out["output"]["all"]["api"]["unknown"], "preserve");
    let newer = json!({"type":"oauth","refresh":"B","access":"from-ts","expires":4_000_000_000_001u64,"accountId":"account","email":"next@example.test","sentinel":{"next":true}});
    let out =
        oracle.request(&json!({"id":3,"op":"modify","authPath":path,"provider":"openai-codex","credential":newer}));
    assert_eq!(out["ok"], true, "{out}");
    storage.reload();
    assert_eq!(
        storage.get("openai-codex").map(|value| value.to_serde().unwrap()),
        Some(newer)
    );
    assert_eq!(
        pi_js_compat::js_json::JsValue::Object(storage.get_all())
            .to_serde()
            .unwrap()["unrelated"],
        json!({"raw":["untouched"]})
    );
    storage.remove("new-provider").unwrap();
    let out = oracle.request(&json!({"id":4,"op":"read","authPath":path,"provider":"new-provider"}));
    assert_eq!(out["output"]["credential"], Value::Null);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
#[test]
fn source_runtime_precedence_status_errors_and_reload_recovery() {
    let path = fixture();
    let storage = AuthStorage::create(Some(&JsString::from(path.to_str().unwrap()))).unwrap();
    storage.set_runtime_api_key("synthetic", &JsString::from("runtime"));
    assert!(storage.has_auth("synthetic").unwrap());
    assert!(!storage.has("synthetic"));
    assert_eq!(storage.get_auth_status("synthetic").source.as_deref(), Some("runtime"));
    assert!(!storage.get_auth_status("synthetic").configured);
    storage
        .set("synthetic", json!({"type":"api_key","key":"stored"}))
        .unwrap();
    assert!(storage.get_auth_status("synthetic").configured);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        rt.block_on(storage.get_api_key("synthetic", GetApiKeyOptions::default()))
            .unwrap()
            .map(|key| key.to_utf8().unwrap())
            .as_deref(),
        Some("runtime")
    );
    storage.remove_runtime_api_key("synthetic");
    assert_eq!(
        rt.block_on(storage.get_api_key("synthetic", GetApiKeyOptions::default()))
            .unwrap()
            .map(|key| key.to_utf8().unwrap())
            .as_deref(),
        Some("stored")
    );
    fs::write(&path, "broken-json").unwrap();
    storage.reload();
    assert_eq!(storage.get("synthetic").unwrap().to_serde().unwrap()["key"], "stored");
    assert!(
        storage
            .set("other", json!({"type":"api_key","key":"x"}))
            .unwrap_err()
            .to_string()
            .contains("Cannot update auth storage because it could not be loaded")
    );
    assert!(!storage.drain_errors().is_empty());
    assert!(storage.drain_errors().is_empty());
    fs::write(&path, "{}").unwrap();
    storage.set("2", json!({"type":"api_key","key":"two"})).unwrap();
    storage.set("1", json!({"type":"api_key","key":"one"})).unwrap();
    assert_eq!(
        storage.list_providers(),
        ["1", "2"].map(pi_js_compat::js_json::JsString::from)
    );
    storage.logout("2").unwrap();
    assert!(!storage.has("2"));
    assert_eq!(
        rt.block_on(storage.get_api_key(
            "missing",
            GetApiKeyOptions {
                include_fallback: false,
                ..Default::default()
            }
        ))
        .unwrap(),
        None
    );
}
