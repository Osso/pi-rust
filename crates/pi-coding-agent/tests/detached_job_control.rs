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

fn identity() -> DetachedJobOwnershipIdentity {
    DetachedJobOwnershipIdentity {
        job_id: "agent_1".to_owned(),
        owner: pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: None,
            session_id: "supervisor-1".to_owned(),
        },
        output_label: "Bash output".to_owned(),
        process_identity: test_process_identity("runner-1"),
    }
}

struct Fixture {
    _directory: TempDir,
    control_db_path: String,
    recipient: RuntimeMailboxAddress,
    session_path: String,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
    let control_db_path = directory.path().join("control.sqlite").to_string_lossy().into_owned();
    let session_path = directory.path().join("session.jsonl").to_string_lossy().into_owned();
    let recipient = RuntimeMailboxAddress {
        agent_id: Some("agent_1".to_owned()),
        session_id: "supervisor-1".to_owned(),
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

fn enqueue_control(fixture: &Fixture, message_id: &str, body: serde_json::Value) {
    upsert_multi_agent_mailbox_message(
        &fixture.control_db_path,
        &fixture.session_path,
        message_id,
        &serde_json::json!({
            "body": serde_json::to_string(&body).expect("body"),
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
            sender: RuntimeMailboxAddress {
                agent_id: None,
                session_id: "supervisor-1".to_owned(),
            },
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.to_owned(),
                session_path: fixture.session_path.clone(),
            },
        },
    )
    .expect("enqueue");
}

#[test]
fn accepts_cancellation_from_the_exact_owner_process_and_consumes_the_command() {
    let fixture = fixture();
    enqueue_control(
        &fixture,
        "message_1",
        serde_json::json!({
            "command": "cancel",
            "identity": identity(),
            "reason": "user requested",
        }),
    );
    let commands =
        claim_detached_job_control_commands(&fixture.control_db_path, &fixture.recipient, &identity()).expect("claim");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].identity, identity());
    assert_eq!(commands[0].reason.as_deref(), Some("user requested"));
    assert_eq!(commands[0].mailbox_row_id, 1);
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages[0].id, 1);
    assert_eq!(messages[0].status, RuntimeMailboxMessageStatus::Delivered);
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
                session_id: "supervisor-1".to_owned(),
            },
            request_id: "status-1".to_owned(),
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
            session_id: "supervisor-1".to_owned(),
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
        serde_json::json!({
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
    assert_eq!(response.result, Some(serde_json::json!({"value": 42})));
    assert_eq!(response.error, None);
    assert_eq!(response.mailbox_row_id, 1);
}

#[test]
fn rejects_a_bridge_response_without_ownership_identity() {
    let fixture = fixture();
    enqueue_control(
        &fixture,
        "message_1",
        serde_json::json!({
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
        messages[0].error.as_deref(),
        Some("Detached job control identity mismatch")
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
        serde_json::json!({
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
        messages[0].error.as_deref(),
        Some("Detached job control identity mismatch")
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
                session_id: "supervisor-1".to_owned(),
            },
            request_id: "status-1".to_owned(),
            runner_address: fixture.recipient.clone(),
            session_path: fixture.session_path.clone(),
            status: serde_json::json!({"state": "running"}),
        },
    )
    .expect("enqueue");
    assert_eq!(response, 1);
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body.contains("\"command\":\"respond\""), true);
}
