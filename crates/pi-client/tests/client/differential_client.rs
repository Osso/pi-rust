#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use crate::support::*;
use pi_client::{ConnectionState, PiClient, PiClientOptions};
use pi_protocol::{ClientMessage, CommandResult, SessionPhase, ThinkingLevel};
use serde_json::{Value, json};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[tokio::test]
async fn differential_scripted_client_session() {
    let mut ts = oracle::TsOracle::spawn("client-oracle.ts", "packages/client/src/index.ts");
    let response = ts.request(&json!({"id": 1, "operation": "client_script"}));
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");

    let server = MemoryByteServer::new();
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        let ClientMessage::Request(request) = message else {
            if let ClientMessage::Hello(_) = message {
                responder.send(server_hello(base_server_snapshot()));
            }
            return;
        };
        match request.request() {
            pi_protocol::Command::List(_) => {
                responder.send(success_response(
                    request.id(),
                    CommandResult::List(pi_protocol::ListResult::new(Vec::new())),
                ));
            }
            pi_protocol::Command::Attach(_) => {
                responder.send(success_response(
                    request.id(),
                    CommandResult::Attach(session_snapshot("session-1")),
                ));
            }
            pi_protocol::Command::Prompt(_) => {
                responder.send(success_response(
                    request.id(),
                    CommandResult::Prompt(session_snapshot_with(
                        "session-1",
                        2,
                        SessionPhase::Turn,
                        ThinkingLevel::Off,
                        true,
                    )),
                ));
            }
            pi_protocol::Command::Detach(_) => {
                responder.send(success_response(request.id(), detach_result("session-1")));
            }
            _ => {}
        }
    }));
    let states = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let client = PiClient::new(PiClientOptions::new(transport_factory(&server))).unwrap();
    let state_target = states.clone();
    let _state_subscription = client
        .on_connection_state_change(Arc::new(move |change| {
            state_target.lock().unwrap().push(change.state.as_str().to_string());
        }))
        .unwrap();

    client.connect().unwrap().await.unwrap();
    let listed = client.list_sessions().unwrap().await.unwrap();
    let list_count = match listed {
        CommandResult::List(list) => list.sessions().len(),
        other => panic!("unexpected list result: {other:?}"),
    };
    let handle = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    let observed_target = observed.clone();
    let _snapshot_subscription = handle
        .subscribe(Arc::new(move |snapshot| {
            observed_target.lock().unwrap().push(snapshot.revision());
        }))
        .unwrap();
    server.send(session_snapshot_event(session_snapshot_with(
        "session-1",
        3,
        SessionPhase::Idle,
        ThinkingLevel::Off,
        true,
    )));
    let prompted = handle.prompt("hello").unwrap().await.unwrap();
    let snapshot_after_prompt = handle.snapshot().unwrap();
    handle.detach().unwrap().await.unwrap();
    let after_detach = handle.snapshot();
    client.disconnect();

    let rust = json!({
        "frames": server.sent_by_client().iter().map(|chunk| hex(chunk)).collect::<Vec<_>>(),
        "states": states.lock().unwrap().clone(),
        "observed": observed.lock().unwrap().clone(),
        "listCount": list_count,
        "promptResultRevision": prompted.revision(),
        "snapshotAfterPrompt": {
            "revision": snapshot_after_prompt.revision(),
            "phase": snapshot_after_prompt.phase().as_str(),
            "attached": snapshot_after_prompt.attached(),
        },
        "afterDetach": after_detach.map(|snapshot| json!({
            "revision": snapshot.revision(),
            "attached": snapshot.attached(),
        })),
        "connectionState": client.connection_state().as_str(),
    });
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    assert_eq!(rust, response["output"], "Rust and TypeScript client behavior diverged");
}
