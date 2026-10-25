use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{
    EnqueueRuntimeMailboxMessageInput, EnqueueStoredRuntimeMailboxMessageInput, enqueue_runtime_mailbox_message,
    enqueue_stored_runtime_mailbox_message,
};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use serde_json::json;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

struct WakeFixture {
    child: Child,
    events: Receiver<String>,
}
impl WakeFixture {
    fn spawn() -> Self {
        // The unused source-entrypoint argv marks this owned signal fixture as a verified Pi runtime.
        let mut child = Command::new("node")
            .args([
                "-e",
                "process.on('SIGUSR2',()=>console.log('wake'));console.log('ready');setInterval(()=>{},1000)",
                "packages/coding-agent/src/cli.ts",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.expect("wake fixture stdout");
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let fixture = Self { child, events };
        assert_eq!(fixture.events.recv_timeout(Duration::from_secs(3)).unwrap(), "ready");
        fixture
    }
    fn wake(&self, runtime: &str) {
        assert_eq!(
            self.events
                .recv_timeout(Duration::from_secs(2))
                .unwrap_or_else(|error| panic!("{runtime} mailbox recipient wake: {error}")),
            "wake"
        );
    }
}
impl Drop for WakeFixture {
    fn drop(&mut self) {
        if let Err(error) = self.child.kill() {
            eprintln!("wake fixture kill failed: {error}");
        }
        if let Err(error) = self.child.wait() {
            eprintln!("wake fixture wait failed: {error}");
        }
    }
}

#[test]
fn an_explicit_empty_local_host_still_wakes_the_mailbox_recipient() {
    let fixture = WakeFixture::spawn();
    let root = tempfile::tempdir().unwrap();
    let db = JsString::from(root.path().join("control.sqlite").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let recipient = json!({"sessionId":"recipient","agentId":null});
    let input = |id: &str| json!({"recipientHost":"","recipient":recipient,"sender":{"sessionId":"sender","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s.jsonl","messageId":id},"message":{"id":id,"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"wake"}});
    let response=source.request(&json!({"id":1,"steps":[{"op":"call","args":["registerRuntimeMailboxListener",[db,recipient,fixture.child.id()]]},{"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db,input("source")]]}]}));
    assert_eq!(response["output"][1]["ok"], true, "{response}");
    fixture.wake("source");
    let native: EnqueueStoredRuntimeMailboxMessageInput = from_value(&JsValue::from(input("native"))).unwrap();
    enqueue_stored_runtime_mailbox_message(&db, &native).unwrap();
    fixture.wake("native stored");
    let route = input("native-route");
    let seeded=source.request(&json!({"id":2,"steps":[{"op":"call","args":["upsertMultiAgentMailboxMessage",[db,"/fixture/s.jsonl","native-route",route["message"]]]}]}));
    assert_eq!(seeded["output"][0]["ok"], true, "{seeded}");
    let native: EnqueueRuntimeMailboxMessageInput = from_value(&JsValue::from(route)).unwrap();
    enqueue_runtime_mailbox_message(&db, &native).unwrap();
    fixture.wake("native routed");
}
