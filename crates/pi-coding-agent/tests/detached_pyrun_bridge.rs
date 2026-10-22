//! `extensions/pyrun/src/detached-bridge.ts` round-trip through a temp control
//! database, including exact-identity response delivery and request parsing.

use pi_coding_agent::bundled_extensions::pyrun::detached_bridge::{
    EnqueueDetachedPyrunBridgeRequestInput, EnqueueDetachedPyrunBridgeResponseInput,
    enqueue_detached_pyrun_bridge_request, enqueue_detached_pyrun_bridge_response, parse_detached_pyrun_bridge_request,
};
use pi_coding_agent::core::detached_job_control::{DetachedJobRuntimeCommand, claim_detached_job_runtime_commands};
use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_coding_agent::core::session_control_db::{
    DetachedJobOwnershipIdentity, MultiAgentRuntimeOwnerIdentity, RuntimeMailboxAddress,
    claim_runtime_mailbox_messages, register_runtime_mailbox_listener,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString};
use tempfile::TempDir;

fn test_process_identity(name: &str) -> ProcessIdentity {
    let mut value: u32 = 0;
    for character in name.chars() {
        value = value.wrapping_mul(31).wrapping_add(u32::from(character));
    }
    ProcessIdentity {
        pid: 1_000_000_000 + (value % 1_000_000_000),
        start_time_ticks: u64::from(value) + 1,
        incarnation: None,
    }
}

#[tokio::test]
async fn round_trips_a_durable_request_and_exact_identity_response() {
    let directory = TempDir::new().expect("temp dir");
    let control_db_path = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let session_path = directory
        .path()
        .join("session.jsonl")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let runner_address = RuntimeMailboxAddress {
        agent_id: Some("agent_1".into()),
        session_id: "supervisor-1".into(),
    };
    let supervisor_address = RuntimeMailboxAddress {
        agent_id: None,
        session_id: "supervisor-1".into(),
    };
    register_runtime_mailbox_listener(
        &control_db_path,
        &supervisor_address,
        std::process::id(),
        None,
        Default::default(),
    )
    .expect("supervisor listener");
    register_runtime_mailbox_listener(
        &control_db_path,
        &runner_address,
        std::process::id(),
        None,
        Default::default(),
    )
    .expect("runner listener");
    let identity = DetachedJobOwnershipIdentity {
        job_id: "agent_1".into(),
        owner: MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "supervisor-1".into(),
        },
        process_identity: test_process_identity("runner-1"),
        output_label: "Pyrun output".into(),
    };

    let request_id = enqueue_detached_pyrun_bridge_request(EnqueueDetachedPyrunBridgeRequestInput {
        control_db_path: control_db_path.clone().into(),
        identity: identity.clone(),
        method: "models.scoped".into(),
        params: js_json::parse_lossless(r#"{"2":2.0,"1":1.0}"#).unwrap(),
        runner_address: runner_address.clone(),
        session_path: session_path.clone().into(),
        tool_call_id: Some("pyrun-call-1".into()),
        supervisor_address: supervisor_address.clone(),
    })
    .expect("bridge request");
    let messages = claim_runtime_mailbox_messages(&control_db_path, &supervisor_address, 10).expect("claim");
    let message = messages.first().expect("bridge request message");
    let request = parse_detached_pyrun_bridge_request(message).expect("valid bridge request");
    assert_eq!(request.method, "models.scoped");
    assert_eq!(request.request_id, request_id);
    assert_eq!(request.tool_call_id.as_ref(), Some(&JsString::from("pyrun-call-1")));
    assert_eq!(request.params, json!({"1":1,"2":2}));
    assert!(message.body.contains(r#""params":{"1":1,"2":2}"#), "{:?}", message.body);

    for tool_call_id in [json!(42), json!("")] {
        let mut body = js_json::parse_lossless(&message.body).expect("body");
        body["toolCallId"] = tool_call_id;
        let mut invalid = message.clone();
        invalid.body = js_json::stringify_lossless(&body).into();
        assert_eq!(parse_detached_pyrun_bridge_request(&invalid), None);
    }

    enqueue_detached_pyrun_bridge_response(EnqueueDetachedPyrunBridgeResponseInput {
        control_db_path: control_db_path.clone().into(),
        error: None,
        request,
        result: Some(json!([{"id": "model-1"}])),
        session_path: session_path.clone().into(),
        supervisor_address: supervisor_address.clone(),
    })
    .expect("bridge response");
    let commands = claim_detached_job_runtime_commands(&JsString::from(&control_db_path), &runner_address, &identity)
        .expect("claim commands");
    assert_eq!(commands.len(), 1);
    let DetachedJobRuntimeCommand::Response(response) = &commands[0] else {
        panic!("expected a response command: {:?}", commands[0]);
    };
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.result, Some(json!([{"id": "model-1"}])));
    assert_eq!(response.error, None);
}
