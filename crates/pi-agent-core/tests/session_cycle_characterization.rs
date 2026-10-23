#![cfg(target_os = "linux")]
#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::session::{
    InMemorySessionStorage, InMemorySessionStorageOptions, SessionMetadata, SessionStorage, SessionTreeEntry,
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsValue, parse_lossless, stringify_lossless},
};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn cycle_entries(pair: bool) -> JsValue {
    if pair {
        json!([{"type":"custom","id":"a","parentId":"b","timestamp":"t","customType":"cycle"},{"type":"custom","id":"b","parentId":"a","timestamp":"t","customType":"cycle"}])
    } else {
        json!([{"type":"custom","id":"a","parentId":"a","timestamp":"t","customType":"cycle"}])
    }
}

#[tokio::test]
async fn cycle_native_child() {
    let Some(root) = std::env::var_os("PI_CORE_CYCLE_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let raw = parse_lossless(&pi_js_compat::js_json::JsString::from(
        std::fs::read_to_string(root.join("input.json")).unwrap(),
    ))
    .unwrap();
    let entries = raw
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|value| SessionTreeEntry::from_value(value).unwrap())
        .collect();
    let storage = InMemorySessionStorage::new(InMemorySessionStorageOptions {
        entries,
        metadata: Some(SessionMetadata {
            id: "fixture".into(),
            created_at: "t".into(),
        }),
    })
    .unwrap();
    std::fs::write(root.join("ready"), b"ready").unwrap();
    let _ = storage.get_path_to_root(Some(&"a".into())).await;
    std::process::exit(0);
}

#[test]
fn cyclic_paths_remain_unsettled_with_a_bounded_owned_process_probe() {
    for pair in [false, true] {
        let entries = cycle_entries(pair);
        let mut oracle = oracle::TsOracle::spawn(
            "session-cycle-oracle.ts",
            "packages/agent-core/src/harness/session/memory-storage.ts",
        );
        let ready = oracle.request(&json!({"entries":entries}));
        let pid = ready["pid"].as_u64().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let state = state.rsplit_once(") ").unwrap().1.as_bytes()[0];
        assert!(!matches!(state, b'Z' | b'X'), "frozen cycle unexpectedly settled");
        drop(oracle);

        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("input.json"), stringify_lossless(&entries)).unwrap();
        let mut child = OwnedChild(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "cycle_native_child", "--nocapture"])
                .env("PI_CORE_CYCLE_ROOT", root.path())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !std::fs::read(root.path().join("ready")).is_ok_and(|bytes| bytes == b"ready") {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "native child exited before traversal"
            );
            assert!(std::time::Instant::now() < deadline, "native child did not start");
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "native cycle unexpectedly settled"
        );
        drop(child);
    }
}
