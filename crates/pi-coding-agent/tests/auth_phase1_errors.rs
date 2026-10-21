//! Filesystem failures must retain frozen public error semantics, not silently become empty data.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::auth_storage::AuthStorage;
use serde_json::json;
#[test]
fn auth_file_utf8_decoder_replaces_invalid_bytes_like_node() {
    let file = std::env::temp_dir().join(format!("auth-invalid-utf8-{}.json", std::process::id()));
    std::fs::write(&file, b"{\"synthetic\":{\"type\":\"api_key\",\"key\":\"bad-\xff\"}}").unwrap();
    let storage = AuthStorage::create(Some(file.to_str().unwrap()));
    assert_eq!(
        storage.get("synthetic").unwrap().to_serde().unwrap()["key"],
        "bad-\u{fffd}"
    );
    assert!(storage.drain_errors().is_empty());
}
#[test]
fn unrelated_raw_lone_surrogates_survive_public_provider_update_like_ts() {
    let root = std::env::temp_dir().join(format!("auth-raw-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let ts_file = root.join("ts.json");
    let rust_file = root.join("rust.json");
    let raw = r#"{"untouched":{"type":"api_key","key":"\ud800","extra":{"\udc00":"\udfff"}},"\ud800":{"raw":["\udc00"]},"safe":{"type":"api_key","key":"before"}}"#;
    std::fs::write(&ts_file, raw).unwrap();
    std::fs::write(&rust_file, raw).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let credential = json!({"type":"api_key","key":"updated"});
    let source = oracle
        .request(&json!({"id":1,"op":"modify-file","authPath":ts_file,"provider":"safe","credential":credential}));
    assert_eq!(source["ok"], true, "{source}");
    let storage = AuthStorage::create(Some(rust_file.to_str().unwrap()));
    storage.set("safe", credential).unwrap();
    assert_eq!(
        std::fs::read_to_string(&rust_file).unwrap(),
        source["output"]["file"].as_str().unwrap()
    );
}
#[test]
fn directory_auth_path_reports_same_source_error() {
    let directory = std::env::temp_dir().join(format!("auth-error-directory-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let source = oracle.request(&json!({"id":1,"op":"errors","authPath":directory}));
    assert_eq!(source["ok"], true, "{source}");
    let storage = AuthStorage::create(Some(directory.to_str().unwrap()));
    let errors: Vec<String> = storage.drain_errors().into_iter().map(|e| e.to_string()).collect();
    assert_eq!(json!(errors), source["output"]["errors"]);
}
