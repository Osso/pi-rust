//! `core/detached-job-control.ts` behavioral tests.

use pi_coding_agent::core::detached_job_control::{
    DetachedJobRuntimeCommand, claim_detached_job_control_commands, claim_detached_job_runtime_commands,
    enqueue_detached_job_status_request,
};
use pi_coding_agent::core::detached_job_runner::DetachedJobOwnershipIdentity;
use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_coding_agent::core::session_control_db::{
    EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxMessageStatus,
    RuntimeMailboxStoreRef, enqueue_runtime_mailbox_message, list_runtime_mailbox_messages,
    register_runtime_mailbox_listener, upsert_multi_agent_mailbox_message,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use tempfile::TempDir;

#[path = "support/oracle.rs"]
mod oracle;

#[test]
fn cancellation_matching_ignores_incarnation_like_frozen() {
    let current = identity();
    let mut candidate = current.clone();
    candidate.process_identity.incarnation = Some("different-incarnation".into());
    let native = fixture();
    let frozen = fixture();
    for fixture in [&native, &frozen] {
        enqueue_control(
            fixture,
            "incarnation-command",
            pi_js_compat::js_json!({
                "command": "cancel", "identity": candidate, "reason": "stop",
            }),
        );
    }
    let mut oracle = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    let expected = oracle.request(&pi_js_compat::js_json!({
        "id": 28101,
        "module": "core/detached-job-control.ts",
        "operation": "detachedControlClaim",
        "db": frozen.control_db_path,
        "recipient": frozen.recipient,
        "identity": current,
    }));
    assert_eq!(expected["ok"], JsValue::Bool(true));
    assert_eq!(expected["output"].as_array().unwrap().len(), 1);
    let commands = claim_detached_job_control_commands(&native.control_db_path, &native.recipient, &current).unwrap();
    let actual = commands
        .iter()
        .map(|command| {
            pi_js_compat::js_json!({
                "command": "cancel", "identity": command.identity, "reason": command.reason,
                "mailboxRowId": command.mailbox_row_id,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(JsValue::Array(actual), expected["output"]);
}

#[test]
fn status_persistence_and_reciprocal_claims_match_frozen() {
    use pi_coding_agent::core::detached_job_control::{
        EnqueueDetachedJobStatusRequestInput, EnqueueDetachedJobStatusResponseInput,
        enqueue_detached_job_status_response,
    };
    let mut oracle = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    for response in [false, true] {
        let native = fixture();
        let frozen = fixture();
        let request = EnqueueDetachedJobStatusRequestInput {
            control_db_path: native.control_db_path.clone(),
            identity: identity(),
            requester_address: native.recipient.clone(),
            request_id: "status-raw".into(),
            runner_address: native.recipient.clone(),
            session_path: native.session_path.clone(),
        };
        let status = js_json::parse_lossless(r#"{"phase":"running","text":"raw-\ud800"}"#).unwrap();
        if response {
            enqueue_detached_job_status_response(&EnqueueDetachedJobStatusResponseInput {
                control_db_path: native.control_db_path.clone(),
                identity: identity(),
                reply_to: native.recipient.clone(),
                request_id: request.request_id.clone(),
                runner_address: native.recipient.clone(),
                session_path: native.session_path.clone(),
                status: status.clone(),
            })
            .unwrap();
        } else {
            enqueue_detached_job_status_request(&request).unwrap();
        }
        let expected = oracle.request(&pi_js_compat::js_json!({
            "id": 28102, "module":"core/detached-job-control.ts", "operation":"detachedControlEnqueue",
            "response": response,
            "input": {
                "controlDbPath": frozen.control_db_path, "identity": identity(),
                "requesterAddress": frozen.recipient, "replyTo": frozen.recipient,
                "requestId": request.request_id, "runnerAddress": frozen.recipient,
                "sessionPath": frozen.session_path, "status": status,
            },
        }));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let frozen_wire = persisted_message(&frozen);
        let native_wire = persisted_message(&native);
        assert_eq!(native_wire, frozen_wire, "response={response}");

        let frozen_commands = oracle.request(&pi_js_compat::js_json!({
            "id": 28103, "module":"core/detached-job-control.ts", "operation":"detachedControlClaim",
            "db": native.control_db_path, "recipient": native.recipient, "identity": identity(),
        }));
        assert_eq!(frozen_commands["ok"], JsValue::Bool(true));
        assert_eq!(frozen_commands["output"].as_array().unwrap().len(), 1);
        let native_commands =
            claim_detached_job_runtime_commands(&frozen.control_db_path, &frozen.recipient, &identity()).unwrap();
        assert_eq!(native_commands.len(), 1);
        match (&native_commands[0], response) {
            (DetachedJobRuntimeCommand::Status(command), false) => assert_eq!(command.request_id, request.request_id),
            (DetachedJobRuntimeCommand::Response(command), true) => assert_eq!(command.result, Some(status)),
            _ => panic!("wrong reciprocal command: {:?}", native_commands),
        }
    }
}

fn persisted_message(fixture: &Fixture) -> String {
    let db = rusqlite::Connection::open(encode_node_path(&fixture.control_db_path)).unwrap();
    let raw: String = db
        .query_row("SELECT data FROM multi_agent_mailbox_messages", [], |row| row.get(0))
        .unwrap();
    let mut data = js_json::parse_lossless(&raw).unwrap();
    assert!(data["createdAt"].as_string().is_some());
    data["createdAt"] = JsValue::String("TIMESTAMP".into());
    js_json::stringify_lossless(&data)
}

fn encode_node_path(path: &JsString) -> String {
    pi_js_compat::js_json::encode_node_utf8(path)
}

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

fn identity() -> DetachedJobOwnershipIdentity {
    DetachedJobOwnershipIdentity {
        job_id: "agent_1".into(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "supervisor-1".into(),
        },
        output_label: "Bash output".into(),
        process_identity: test_process_identity("runner-1"),
    }
}

struct Fixture {
    _directory: TempDir,
    control_db_path: JsString,
    recipient: RuntimeMailboxAddress,
    session_path: JsString,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
    let control_db_path = directory
        .path()
        .join("control.sqlite")
        .to_string_lossy()
        .into_owned()
        .into();
    let session_path = directory
        .path()
        .join("session.jsonl")
        .to_string_lossy()
        .into_owned()
        .into();
    let recipient = RuntimeMailboxAddress {
        agent_id: Some("agent_1".into()),
        session_id: "supervisor-1".into(),
    };
    register_runtime_mailbox_listener(
        &control_db_path,
        &recipient,
        std::process::id(),
        None,
        Default::default(),
    )
    .expect("listener");
    Fixture {
        _directory: directory,
        control_db_path,
        recipient,
        session_path,
    }
}

fn enqueue_control(fixture: &Fixture, message_id: &str, body: JsValue) {
    let message_id = JsString::from(message_id);
    upsert_multi_agent_mailbox_message(
        &fixture.control_db_path,
        &fixture.session_path,
        &message_id,
        &pi_js_compat::js_json!({
            "body": js_json::stringify_lossless(&body),
            "fromAgentId": "main",
            "id": message_id,
            "kind": "system",
            "status": "pending",
            "toAgentId": "agent_1",
        }),
    )
    .expect("upsert");
    enqueue_runtime_mailbox_message(
        &fixture.control_db_path,
        &EnqueueRuntimeMailboxMessageInput {
            recipient: fixture.recipient.clone(),
            recipient_host: None,
            sender: RuntimeMailboxAddress {
                agent_id: None,
                session_id: "supervisor-1".into(),
            },
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id,
                session_path: fixture.session_path.clone(),
            },
        },
    )
    .expect("enqueue");
}

#[test]
fn cancellation_identity_and_reason_keep_raw_units_through_real_mailbox_claim() {
    let fixture = fixture();
    let raw = JsString::from_utf16(vec![0xd800]);
    let mut current = identity();
    current.job_id = JsString::from("job-") + &raw;
    current.owner.session_id = JsString::from("session-") + &raw;
    current.owner.agent_id = Some(JsString::from("owner-") + &raw);
    current.output_label = JsString::from("output-") + &raw;
    current.process_identity.incarnation = Some(JsString::from("incarnation-") + &raw);
    let reason = JsString::from("cancel-") + JsString::from_utf16(vec![0xdfff]);
    enqueue_control(
        &fixture,
        "raw-control",
        pi_js_compat::js_json!({"command":"cancel","identity":current,"reason":reason}),
    );
    let commands = claim_detached_job_control_commands(&fixture.control_db_path, &fixture.recipient, &current).unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].identity, current);
    assert_eq!(commands[0].reason, Some(reason));
    assert!(
        list_runtime_mailbox_messages(&fixture.control_db_path)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn accepts_cancellation_from_the_exact_owner_process_and_consumes_the_command() {
    let fixture = fixture();
    enqueue_control(
        &fixture,
        "message_1",
        pi_js_compat::js_json!({
            "command": "cancel",
            "identity": identity(),
            "reason": "user requested",
        }),
    );
    let commands =
        claim_detached_job_control_commands(&fixture.control_db_path, &fixture.recipient, &identity()).expect("claim");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].identity, identity());
    assert_eq!(commands[0].reason.as_ref(), Some(&JsString::from("user requested")));
    assert_eq!(commands[0].mailbox_row_id, 1);
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    // Frozen session-control-db.ts:2281-2288 deletes accepted runtime mailbox rows.
    assert!(
        messages.is_empty(),
        "accepted command row must be removed: {messages:?}"
    );
}

#[test]
fn accepts_a_status_request_only_from_the_exact_owner_process() {
    let fixture = fixture();
    enqueue_detached_job_status_request(
        &pi_coding_agent::core::detached_job_control::EnqueueDetachedJobStatusRequestInput {
            control_db_path: fixture.control_db_path.clone(),
            identity: identity(),
            requester_address: RuntimeMailboxAddress {
                agent_id: None,
                session_id: "supervisor-1".into(),
            },
            request_id: "status-1".into(),
            runner_address: fixture.recipient.clone(),
            session_path: fixture.session_path.clone(),
        },
    )
    .expect("enqueue");
    let commands =
        claim_detached_job_runtime_commands(&fixture.control_db_path, &fixture.recipient, &identity()).expect("claim");
    assert_eq!(commands.len(), 1);
    let DetachedJobRuntimeCommand::Status(status) = &commands[0] else {
        panic!("expected status command");
    };
    assert_eq!(status.identity, identity());
    assert_eq!(
        status.reply_to,
        RuntimeMailboxAddress {
            agent_id: None,
            session_id: "supervisor-1".into(),
        }
    );
    assert_eq!(status.request_id, "status-1");
    assert_eq!(status.mailbox_row_id, 1);
}

#[test]
fn accepts_a_bridge_response_only_from_the_exact_owner_process() {
    let fixture = fixture();
    enqueue_control(
        &fixture,
        "message_1",
        pi_js_compat::js_json!({
            "command": "respond",
            "identity": identity(),
            "requestId": "request-1",
            "result": {"value": 42},
        }),
    );
    let commands =
        claim_detached_job_runtime_commands(&fixture.control_db_path, &fixture.recipient, &identity()).expect("claim");
    assert_eq!(commands.len(), 1);
    let DetachedJobRuntimeCommand::Response(response) = &commands[0] else {
        panic!("expected response command");
    };
    assert_eq!(response.request_id, "request-1");
    assert_eq!(response.result, Some(pi_js_compat::js_json!({"value": 42})));
    assert_eq!(response.error, None);
    assert_eq!(response.mailbox_row_id, 1);
}

#[test]
fn rejects_a_bridge_response_without_ownership_identity() {
    let fixture = fixture();
    enqueue_control(
        &fixture,
        "message_1",
        pi_js_compat::js_json!({
            "command": "respond",
            "requestId": "request-1",
        }),
    );
    let commands =
        claim_detached_job_runtime_commands(&fixture.control_db_path, &fixture.recipient, &identity()).expect("claim");
    assert!(commands.is_empty());
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages[0].id, 1);
    assert_eq!(messages[0].status, RuntimeMailboxMessageStatus::Failed);
    assert_eq!(
        messages[0].error.as_ref(),
        Some(&JsString::from("Detached job control identity mismatch"))
    );
}

#[test]
fn rejects_commands_for_a_different_runner_identity() {
    let fixture = fixture();
    let mut other = identity();
    other.process_identity.start_time_ticks += 1;
    enqueue_control(
        &fixture,
        "message_1",
        pi_js_compat::js_json!({
            "command": "cancel",
            "identity": other,
        }),
    );
    let commands =
        claim_detached_job_runtime_commands(&fixture.control_db_path, &fixture.recipient, &identity()).expect("claim");
    assert!(commands.is_empty());
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages[0].status, RuntimeMailboxMessageStatus::Failed);
    assert_eq!(
        messages[0].error.as_ref(),
        Some(&JsString::from("Detached job control identity mismatch"))
    );
}

#[test]
fn enqueues_a_status_response_for_the_requester() {
    let fixture = fixture();
    let response = pi_coding_agent::core::detached_job_control::enqueue_detached_job_status_response(
        &pi_coding_agent::core::detached_job_control::EnqueueDetachedJobStatusResponseInput {
            control_db_path: fixture.control_db_path.clone(),
            identity: identity(),
            reply_to: RuntimeMailboxAddress {
                agent_id: None,
                session_id: "supervisor-1".into(),
            },
            request_id: "status-1".into(),
            runner_address: fixture.recipient.clone(),
            session_path: fixture.session_path.clone(),
            status: pi_js_compat::js_json!({"state": "running"}),
        },
    )
    .expect("enqueue");
    assert_eq!(response, 1);
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages.len(), 1);
    assert!(messages[0].body.contains("\"command\":\"respond\""));
}
