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

use crate::shared_oracle as oracle;

#[test]
fn pyrun_bridge_persisted_request_matches_frozen_bytes() {
    let directory = TempDir::new().unwrap();
    let native_db = directory.path().join("native.sqlite");
    let frozen_db = directory.path().join("frozen.sqlite");
    let identity = DetachedJobOwnershipIdentity {
        job_id: "job".into(),
        owner: MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "session".into(),
        },
        process_identity: test_process_identity("bridge"),
        output_label: "Pyrun output".into(),
    };
    let runner = RuntimeMailboxAddress {
        agent_id: Some("job".into()),
        session_id: "session".into(),
    };
    let supervisor = RuntimeMailboxAddress {
        agent_id: None,
        session_id: "session".into(),
    };
    let params = js_json::parse_lossless(r#"{"text":"\ud800","nested":[null,{"value":42}]}"#).unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "pyrun-bridge-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/detached-bridge.ts",
    );
    let expected=ts.request(&json!({"id":1,"input":{"controlDbPath":frozen_db,"sessionPath":"/private/session.jsonl","identity":identity,"method":"custom","params":params,"runnerAddress":runner,"supervisorAddress":supervisor,"toolCallId":"call-1"}}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let id = enqueue_detached_pyrun_bridge_request(EnqueueDetachedPyrunBridgeRequestInput {
        control_db_path: native_db.to_str().unwrap().into(),
        identity,
        method: "custom".into(),
        params,
        runner_address: runner,
        session_path: "/private/session.jsonl".into(),
        tool_call_id: Some("call-1".into()),
        supervisor_address: supervisor,
    })
    .unwrap();
    let db = rusqlite::Connection::open(&native_db).unwrap();
    let raw: String = db
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE message_id = ?",
            [format!("pyrun-request:job:{}", js_json::encode_node_utf8(&id))],
            |row| row.get(0),
        )
        .unwrap();
    let mut message = js_json::parse_lossless(&raw).unwrap();
    let mut body = js_json::parse_lossless(message["body"].as_string().unwrap()).unwrap();
    body["requestId"] = json!("<request>");
    message["body"] = json!(js_json::stringify_lossless(&body));
    message["id"] = json!("pyrun-request:job:<request>");
    message["createdAt"] = json!("<created-at>");
    assert_eq!(json!(js_json::stringify_lossless(&message)), expected["output"]);
    let parsed = ts.request(&json!({"id":2,"operation":"parse","body":js_json::stringify_lossless(&body)}));
    assert_eq!(parsed["ok"], json!(true));
    let native: pi_coding_agent::bundled_extensions::pyrun::detached_bridge::DetachedPyrunBridgeRequest =
        js_json::from_value(&parsed["output"]).unwrap();
    assert_eq!(js_json::to_value(&native).unwrap(), body);

    for (index, (result, error)) in [
        (Some(js_json::parse_lossless(r#"{"text":"\ud800"}"#).unwrap()), None),
        (Some(json!(null)), None),
        (None, Some(JsString::from("bridge failed"))),
    ]
    .into_iter()
    .enumerate()
    {
        let mut request = native.clone();
        request.request_id = format!("response-{index}").into();
        let supervisor = RuntimeMailboxAddress {
            agent_id: None,
            session_id: "session".into(),
        };
        let mut input = json!({"controlDbPath":frozen_db,"sessionPath":"/private/session.jsonl","request":request,"supervisorAddress":supervisor});
        if let Some(result) = &result {
            input.as_object_mut().unwrap().insert("result".into(), result.clone());
        }
        if let Some(error) = &error {
            input.as_object_mut().unwrap().insert("error".into(), json!(error));
        }
        let expected = ts.request(&json!({"id":3,"operation":"response","input":input}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        enqueue_detached_pyrun_bridge_response(EnqueueDetachedPyrunBridgeResponseInput {
            control_db_path: native_db.to_str().unwrap().into(),
            error,
            request,
            result,
            session_path: "/private/session.jsonl".into(),
            supervisor_address: supervisor,
        })
        .unwrap();
        let raw: String = db
            .query_row(
                "SELECT data FROM multi_agent_mailbox_messages WHERE message_id = ?",
                [format!("pyrun-response:job:response-{index}")],
                |row| row.get(0),
            )
            .unwrap();
        let mut value = js_json::parse_lossless(&raw).unwrap();
        value["createdAt"] = json!("<created-at>");
        assert_eq!(json!(js_json::stringify_lossless(&value)), expected["output"]);
    }
}
