use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{
    claim_latest_incoming_message, enqueue_incoming_message, retain_control_db_connection, with_control_db,
    with_immediate_transaction,
};
use pi_coding_agent::core::sqlite::ControlDbError;
use pi_js_compat::js_json::JsString;
use serde_json::json;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        match self.0.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => eprintln!("retained nested child status: {error}"),
        }
        if let Err(error) = self.0.kill() {
            eprintln!("retained nested child kill: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("retained nested child wait: {error}");
        }
    }
}

#[test]
fn retained_callbacks_can_reenter_the_same_database() {
    const KEY: &str = "PI_STORAGE_REENTRANT_CHILD_DB";
    if let Some(path) = std::env::var_os(KEY) {
        let path = JsString::from(path.to_str().unwrap());
        let retained = retain_control_db_connection(&path).unwrap();
        let id = with_control_db(&path, |_| enqueue_incoming_message(&path, "nested")).unwrap();
        assert_eq!(id, 1);
        let failure = with_control_db(&path, |db| {
            with_immediate_transaction(db, |_| {
                enqueue_incoming_message(&path, "rollback")?;
                Err::<(), _>(ControlDbError::new("nested rollback"))
            })
        })
        .unwrap_err();
        assert_eq!(failure.message(), &JsString::from("nested rollback"));
        let remaining = claim_latest_incoming_message(&path).unwrap().unwrap();
        assert_eq!(remaining.id, 1);
        assert_eq!(remaining.content, JsString::from("nested"));
        assert!(claim_latest_incoming_message(&path).unwrap().is_none());
        retained.release();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let result =
        source.request(&json!({"id":1,"steps":[{"op":"retainedNested","args":[root.path().join("source.sqlite")]}]}));
    assert_eq!(result["output"][0]["ok"], true, "{result}");
    assert_eq!(result["output"][0]["value"]["id"], 1, "{result}");
    assert_eq!(result["output"][0]["value"]["remaining"]["id"], 1, "{result}");
    assert_eq!(
        result["output"][0]["value"]["remaining"]["content"], "nested",
        "{result}"
    );
    assert!(result["output"][0]["value"]["second"].is_null(), "{result}");
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "repin_storage_control_reentrant::retained_callbacks_can_reenter_the_same_database",
            "--nocapture",
        ])
        .env(KEY, root.path().join("native.sqlite"))
        .spawn()
        .unwrap();
    let mut child = OwnedChild(child);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native retained callback deadlocked; frozen returned successfully"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
