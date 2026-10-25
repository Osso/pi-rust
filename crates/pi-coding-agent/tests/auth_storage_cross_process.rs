//! Cross-implementation proof for `auth.json`: the pinned TypeScript `AuthStorage` and the Rust port
//! read and write the same bytes, and both honor the shared `proper-lockfile` protocol.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use pi_coding_agent::core::auth_storage::{AuthStorage, AuthStorageBackend, FileAuthStorageBackend};
use pi_js_compat::js_json::{self, JsValue};
use serde_json::{Value, json};

use crate::shared_oracle as oracle;

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

fn api_key(key: &str) -> JsValue {
    pi_js_compat::js_json!({"type":"api_key", "key":key})
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

    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    let read = storage.get("anthropic");
    assert_eq!(read, Some(api_key("sk-ts")));

    // Rust writes the same logical change into a fresh file; bytes must match exactly.
    fs::write(&auth_path, "{}").unwrap();
    let rust_storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    rust_storage.set("anthropic", api_key("sk-ts")).unwrap();
    let modified = rust_storage.get("anthropic");
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

    rust_storage.remove("anthropic").unwrap();
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

    // Frozen async locking waits; its ordinary reads use a finite synchronous retry budget.
    let source_release = dir.join("source-release");
    let ready = dir.join("source-ready");
    let mut source = TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let warm: Value = source.request(&json!({"id":0,"op":"read","authPath":auth_path,"provider":"anthropic"}));
    assert_eq!(warm["ok"], true);
    let mut holder = spawn_peer(
        &root,
        "hold-until-file",
        &auth_path,
        Some(source_release.to_str().unwrap()),
    );
    assert_eq!(read_line(&mut holder), "LOCKED");
    let request = json!({"id":1,"op":"async-lock-read","authPath":auth_path,"provider":"anthropic","readyPath":ready});
    let (sender, receiver) = std::sync::mpsc::channel();
    let source_thread = std::thread::spawn(move || {
        let response: Value = source.request(&request);
        sender.send(response).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "source must enter its async lock operation");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        matches!(
            receiver.recv_timeout(Duration::from_millis(300)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ),
        "source async lock must remain pending before release"
    );
    fs::write(&source_release, b"").unwrap();
    let expected = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(expected["ok"], true, "{expected}");
    source_thread.join().unwrap();
    assert_eq!(read_line(&mut holder), "RELEASED");
    assert!(holder.wait().unwrap().success());

    let native_release = dir.join("native-release");
    let mut holder = spawn_peer(
        &root,
        "hold-until-file",
        &auth_path,
        Some(native_release.to_str().unwrap()),
    );
    assert_eq!(read_line(&mut holder), "LOCKED");
    let backend = FileAuthStorageBackend::new(&JsString::from(auth_path.to_str().unwrap())).unwrap();
    let observed = Arc::new(std::sync::Mutex::new(None));
    let captured = Arc::clone(&observed);
    let started = Instant::now();
    runtime().block_on(async {
        let operation = backend.with_lock_async(Box::new(move |current| {
            Box::pin(async move {
                *captured.lock().unwrap() = js_json::parse_lossless(current.as_deref().unwrap())
                    .unwrap()
                    .get("anthropic")
                    .cloned();
                Ok(None)
            })
        }));
        tokio::pin!(operation);
        assert!(
            tokio::time::timeout(Duration::from_millis(300), &mut operation)
                .await
                .is_err(),
            "native async lock must remain pending before release"
        );
        fs::write(&native_release, b"").unwrap();
        operation.await.unwrap()
    });
    let read = observed.lock().unwrap().clone();
    let waited = started.elapsed();
    assert_eq!(read, Some(api_key("held")));
    assert_eq!(serde_json::to_value(read).unwrap(), expected["output"]["credential"]);
    assert!(waited >= Duration::from_millis(300), "Rust did not wait: {waited:?}");
    assert_eq!(read_line(&mut holder), "RELEASED");
    assert!(holder.wait().unwrap().success());

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

#[test]
fn held_lock_sync_reads_match_frozen_best_effort_snapshots() {
    let dir = temp_dir("held-sync-read");
    let auth_path = dir.join("auth.json");
    let release = dir.join("release");
    fs::write(&auth_path, r#"{"anthropic":{"type":"api_key","key":"held"}}"#).unwrap();
    let mut source = TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let request = json!({"id":0,"op":"read","authPath":auth_path,"provider":"anthropic"});
    let initial: Value = source.request(&request);
    assert_eq!(initial["output"]["credential"], json!({"type":"api_key","key":"held"}));
    let mut holder = spawn_peer(
        &source_root(),
        "hold-until-file",
        &auth_path,
        Some(release.to_str().unwrap()),
    );
    assert_eq!(read_line(&mut holder), "LOCKED");
    let expected: Value = source.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(
        expected["output"]["credential"],
        Value::Null,
        "frozen sync retry budget must expire while lock remains held"
    );
    let storage = AuthStorage::create(Some(&JsString::from(auth_path.to_str().unwrap()))).unwrap();
    let actual = storage.get("anthropic");
    assert_eq!(serde_json::to_value(actual).unwrap(), expected["output"]["credential"]);
    fs::write(&release, b"").unwrap();
    assert_eq!(read_line(&mut holder), "RELEASED");
    assert!(holder.wait().unwrap().success());
    let expected: Value = source.request(&request);
    storage.reload();
    let actual = storage.get("anthropic");
    assert_eq!(actual, Some(api_key("held")));
    assert_eq!(serde_json::to_value(actual).unwrap(), expected["output"]["credential"]);
}
