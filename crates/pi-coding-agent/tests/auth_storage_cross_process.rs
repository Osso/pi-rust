//! Cross-implementation proof for `auth.json`: the pinned TypeScript `AuthStorage` and the Rust port
//! read and write the same bytes, and both honor the shared `proper-lockfile` protocol.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use pi_ai::auth::types::{ApiKeyCredential, AuthOperationOptions, Credential, CredentialStore};
use pi_coding_agent::core::auth_storage::AuthStorage;
use serde_json::{Value, json};

#[path = "support/oracle.rs"]
mod oracle;

use oracle::TsOracle;

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-auth-cross-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn api_key(key: &str) -> Credential {
    Credential::ApiKey(ApiKeyCredential {
        key: Some(key.to_owned().into()),
        env: None,
    })
}

fn source_root() -> PathBuf {
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    PathBuf::from(lock["worktree"].as_str().unwrap())
}

fn spawn_peer(root: &Path, mode: &str, file: &Path, extra: Option<&str>) -> Child {
    let mut command = Command::new(root.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(root.join("tsconfig.json"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/lockfile-cross-process.ts"))
        .arg("unused")
        .arg(root)
        .arg(mode)
        .arg(file);
    if let Some(value) = extra {
        command.arg(value);
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("tsx must start the lock peer")
}

fn read_line(child: &mut Child) -> String {
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().expect("peer stdout"))
        .read_line(&mut line)
        .expect("peer must print a line");
    line.trim().to_owned()
}

#[test]
fn auth_json_bytes_match_and_both_runtimes_read_each_other() {
    let dir = temp_dir("bytes");
    let auth_path = dir.join("auth.json");
    fs::write(&auth_path, "{}").unwrap();
    let mut oracle = TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );

    // TypeScript writes; Rust reads the same bytes and the same credential.
    let response = oracle.request(&json!({
        "id": 1,
        "op": "modify",
        "authPath": auth_path,
        "provider": "anthropic",
        "credential": {"type": "api_key", "key": "sk-ts"},
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let ts_file = response["output"]["file"].as_str().unwrap().to_owned();
    let ts_credential = response["output"]["credential"].clone();

    let storage = AuthStorage::create(Some(&auth_path.display().to_string()));
    let read = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    assert_eq!(read, Some(api_key("sk-ts")));

    // Rust writes the same logical change into a fresh file; bytes must match exactly.
    fs::write(&auth_path, "{}").unwrap();
    let rust_storage = AuthStorage::create(Some(&auth_path.display().to_string()));
    let modified = runtime()
        .block_on(rust_storage.modify(
            "anthropic",
            Arc::new(|_current| Box::pin(async { Ok(Some(api_key("sk-ts"))) })),
            AuthOperationOptions::default(),
        ))
        .unwrap();
    assert_eq!(json!(modified), ts_credential);
    let rust_file = fs::read_to_string(&auth_path).unwrap();
    assert_eq!(rust_file, ts_file, "auth.json bytes must match");

    // TypeScript reads what Rust wrote.
    let response = oracle.request(&json!({
        "id": 2,
        "op": "read",
        "authPath": auth_path,
        "provider": "anthropic",
    }));
    assert_eq!(
        response["output"]["credential"],
        json!({"type": "api_key", "key": "sk-ts"})
    );

    // Deletes keep the remaining provider order in both runtimes and produce identical bytes.
    let response = oracle.request(&json!({
        "id": 3,
        "op": "modify",
        "authPath": auth_path,
        "provider": "openai",
        "credential": {"type": "api_key", "key": "sk-openai"},
    }));
    assert_eq!(response["ok"], true, "{response}");
    let two_providers = response["output"]["file"].as_str().unwrap().to_owned();
    fs::write(&auth_path, &two_providers).unwrap();

    runtime()
        .block_on(rust_storage.delete("anthropic", AuthOperationOptions::default()))
        .unwrap();
    let rust_file = fs::read_to_string(&auth_path).unwrap();

    fs::write(&auth_path, &two_providers).unwrap();
    let response = oracle.request(&json!({
        "id": 4,
        "op": "delete",
        "authPath": auth_path,
        "provider": "anthropic",
    }));
    let ts_file = response["output"]["file"].as_str().unwrap().to_owned();
    assert_eq!(rust_file, ts_file, "delete bytes must match");
    assert!(rust_file.contains("openai"));
    assert!(!rust_file.contains("anthropic"));
}

#[test]
fn rust_waits_for_a_typescript_holder_and_typescript_reports_rust_holders() {
    let root = source_root();
    let dir = temp_dir("locks");
    let auth_path = dir.join("auth.json");
    fs::write(&auth_path, "{\"anthropic\":{\"type\":\"api_key\",\"key\":\"held\"}}").unwrap();

    // TypeScript holds `<auth.json>.lock`; the Rust read must wait, then see the data.
    let mut holder = spawn_peer(&root, "hold", &auth_path, Some("500"));
    assert_eq!(read_line(&mut holder), "LOCKED");
    let storage = AuthStorage::create(Some(&auth_path.display().to_string()));
    let started = Instant::now();
    let read = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    let waited = started.elapsed();
    assert_eq!(read, Some(api_key("held")));
    assert!(waited >= Duration::from_millis(300), "Rust did not wait: {waited:?}");
    assert_eq!(read_line(&mut holder), "RELEASED");
    let _ = holder.wait();

    // Rust holds the lock; TypeScript observes ELOCKED.
    let holder = runtime()
        .block_on(pi_coding_agent::utils::lockfile::acquire(
            &auth_path,
            &pi_coding_agent::utils::lockfile::LockOptions {
                realpath: false,
                ..Default::default()
            },
        ))
        .expect("Rust holder");
    let mut blocked = spawn_peer(&root, "try", &auth_path, None);
    assert_eq!(read_line(&mut blocked), "ELOCKED ELOCKED");
    let _ = blocked.wait();
    holder.release().unwrap();
}
