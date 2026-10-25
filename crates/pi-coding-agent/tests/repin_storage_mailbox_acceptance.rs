use super::repin_storage_history as oracle;
use pi_agent_core::AgentMessage;
use pi_coding_agent::core::runtime_mailbox_acceptance::{
    RuntimeMailboxAcceptance, RuntimeMailboxAcceptanceRef, release_unaccepted_runtime_mailbox_messages,
};
use pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, enqueue_stored_runtime_mailbox_message, read_runtime_mailbox_message,
    register_runtime_mailbox_listener, take_runtime_mailbox_messages_for_delivery,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::{JsValue, from_value};
use serde_json::{Value, json};

fn source_call(source: &mut oracle::TsOracle, mut request: Value) -> Value {
    request["id"] = json!(1);
    let response = source.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

fn accepted_message(kind: &str) -> Option<Value> {
    match kind {
        "marker" => None,
        "user" => Some(
            json!({"role":"user","content":[{"type":"text","text":"Accepted 漢😀"}],"timestamp":1,"inputSource":"extension"}),
        ),
        "toolResult" => Some(json!({"role":"toolResult","toolCallId":"call","toolName":"fixture",
            "content":[{"type":"text","text":"Accepted 漢😀"}],"details":{"context":"receipt 漢😀"},"timestamp":1,"isError":false})),
        _ => unreachable!(),
    }
}

#[test]
fn invalid_acceptance_role_releases_claim_and_empty_batch_skips_validation() {
    use pi_coding_agent::core::session_control_db::{
        RuntimeMailboxMessageStatus, mark_runtime_mailbox_message_delivered,
    };
    let directory = tempfile::tempdir().unwrap();
    let db = directory.path().join("control.sqlite");
    let mut source = oracle::TsOracle::spawn("storage-mailbox-oracle.mts", "packages/coding-agent/src/core");
    let created = source_call(&mut source, json!({"op":"create","dir":directory.path(),"db":db}));
    let path = created["path"].as_str().unwrap();
    let recipient = RuntimeMailboxAddress {
        session_id: created["sessionId"].as_str().unwrap().into(),
        agent_id: None,
    };
    let input = json!({"recipient":recipient,"sender":{"sessionId":"sender","agentId":null},"kind":"message",
        "storeRef":{"sessionPath":"/fixture/sender.jsonl","messageId":"invalid-role"},
        "message":{"id":"invalid-role","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"Bonjour"}});
    let id = source_call(&mut source, json!({"op":"enqueue","db":db,"input":input}))
        .as_i64()
        .unwrap();
    let assistant = AgentMessage::from(pi_ai::providers::faux::faux_assistant_message(
        "not an accepted role",
        Default::default(),
    ));
    let expected = source.request(&json!({"id":1,"op":"acceptRaw","db":db,"path":path,"message":assistant}));
    assert_eq!(expected["ok"], false);
    assert_eq!(expected["error"], "Error: Invalid mailbox acceptance message");
    assert_eq!(
        read_runtime_mailbox_message(db.to_str().unwrap(), id)
            .unwrap()
            .unwrap()
            .status,
        RuntimeMailboxMessageStatus::Pending
    );
    register_runtime_mailbox_listener(
        db.to_str().unwrap(),
        &recipient,
        std::process::id(),
        Some(&path.into()),
        Default::default(),
    )
    .unwrap();
    let claims =
        take_runtime_mailbox_messages_for_delivery(db.to_str().unwrap(), &recipient, |_| Ok(true), 10).unwrap();
    assert_eq!(claims.len(), 1);
    let refs = claims.iter().map(RuntimeMailboxAcceptanceRef::from).collect::<Vec<_>>();
    let mut session = SessionManager::open(&path.into(), None, None).unwrap();
    let error = RuntimeMailboxAcceptance::default()
        .accept_messages(&mut session, &db.to_str().unwrap().into(), &refs, Some(&assistant))
        .unwrap_err();
    assert_eq!(error.message(), "Invalid mailbox acceptance message");
    release_unaccepted_runtime_mailbox_messages(&session, &db.to_str().unwrap().into(), &refs).unwrap();
    assert_eq!(
        read_runtime_mailbox_message(db.to_str().unwrap(), id)
            .unwrap()
            .unwrap()
            .status,
        RuntimeMailboxMessageStatus::Pending
    );
    assert!(session.get_entries().is_empty());
    mark_runtime_mailbox_message_delivered(db.to_str().unwrap(), id).unwrap();
    let expected = source_call(
        &mut source,
        json!({"op":"acceptRaw","db":db,"path":path,"message":assistant}),
    );
    assert_eq!(expected["persisted"], false);
    let mut memory = SessionManager::in_memory(None, None).unwrap();
    RuntimeMailboxAcceptance::default()
        .accept_messages(&mut memory, &db.to_str().unwrap().into(), &[], Some(&assistant))
        .unwrap();
    assert!(memory.get_entries().is_empty());
}

#[test]
fn tool_delivery_descriptor_matches_frozen_valid_and_absent_cases() {
    use pi_coding_agent::core::runtime_mailbox_acceptance::read_runtime_mailbox_tool_delivery;
    let mut source = oracle::TsOracle::spawn("storage-mailbox-oracle.mts", "packages/coding-agent/src/core");
    let delivery = json!({"controlDbPath":"/fixture/control.sqlite","messages":[{"id":17,"storeRef":{"sessionPath":"/sender.jsonl","messageId":"answer"}}]});
    let mut messages = vec![accepted_message("user").unwrap()];
    for details in [
        None,
        Some(Value::Null),
        Some(json!({"context":"ordinary"})),
        Some(json!({"runtimeMailboxDelivery":delivery})),
    ] {
        let mut message = accepted_message("toolResult").unwrap();
        message.as_object_mut().unwrap().remove("details");
        if let Some(details) = details {
            message["details"] = details;
        }
        messages.push(message);
    }
    for message in messages {
        let expected = source_call(&mut source, json!({"op":"toolDelivery","message":message}));
        let native: AgentMessage = from_value(&JsValue::from(message)).unwrap();
        let actual = read_runtime_mailbox_tool_delivery(&native).unwrap().map(|delivery|json!({
            "controlDbPath":delivery.control_db_path,"messages":delivery.messages.iter().map(|message|json!({"id":message.id,"storeRef":message.store_ref})).collect::<Vec<_>>()
        })).unwrap_or(Value::Null);
        assert_eq!(actual, expected);
    }
}

#[test]
fn reciprocal_marker_user_and_tool_result_acceptance_preserves_same_jsonl_and_claims() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-mailbox-oracle.mts", "packages/coding-agent/src/core");
    for kind in ["marker", "user", "toolResult"] {
        let directory = root.path().join(kind);
        std::fs::create_dir(&directory).unwrap();
        let db = directory.join("control.sqlite");
        let created = source_call(&mut source, json!({"op":"create","dir":directory,"db":db}));
        let path = created["path"].as_str().unwrap();
        let recipient = RuntimeMailboxAddress {
            session_id: created["sessionId"].as_str().unwrap().into(),
            agent_id: None,
        };
        let message = accepted_message(kind);
        let mut expected_refs = Vec::new();
        for source_first in [true, false] {
            let sender_id = if source_first { "from-source" } else { "from-native" };
            let input = json!({"recipient":recipient,"sender":{"sessionId":"sender","agentId":null},"kind":"message",
                "storeRef":{"sessionPath":"/fixture/sender.jsonl","messageId":sender_id},
                "message":{"id":sender_id,"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"Bonjour 漢😀"}});
            expected_refs.push(input["storeRef"].clone());
            let id = if source_first {
                source_call(&mut source, json!({"op":"enqueue","db":db,"input":input}))
                    .as_i64()
                    .unwrap()
            } else {
                enqueue_stored_runtime_mailbox_message(
                    db.to_str().unwrap(),
                    &from_value(&JsValue::from(input)).unwrap(),
                )
                .unwrap()
            };
            if source_first {
                let mut session = SessionManager::open(&path.into(), None, None).unwrap();
                register_runtime_mailbox_listener(
                    db.to_str().unwrap(),
                    &recipient,
                    std::process::id(),
                    Some(&path.into()),
                    Default::default(),
                )
                .unwrap();
                let claims =
                    take_runtime_mailbox_messages_for_delivery(db.to_str().unwrap(), &recipient, |_| Ok(true), 10)
                        .unwrap();
                assert_eq!(claims.len(), 1);
                let refs = claims.iter().map(RuntimeMailboxAcceptanceRef::from).collect::<Vec<_>>();
                let native: Option<AgentMessage> = message
                    .as_ref()
                    .map(|message| from_value(&JsValue::from(message)).unwrap());
                let mut acceptance = RuntimeMailboxAcceptance::default();
                acceptance
                    .accept_messages(&mut session, &db.to_str().unwrap().into(), &refs, native.as_ref())
                    .unwrap();
                release_unaccepted_runtime_mailbox_messages(&session, &db.to_str().unwrap().into(), &refs).unwrap();
                if let Some(message) = native {
                    assert!(acceptance.consume_persisted_message(&message));
                    assert!(!acceptance.consume_persisted_message(&message));
                }
            } else {
                let mut request = json!({"op":"acceptRaw","db":db,"path":path});
                if let Some(message) = &message {
                    request["message"] = message.clone();
                }
                let accepted = source_call(&mut source, request);
                assert_eq!(accepted["persisted"], message.is_some());
            }
            assert!(
                read_runtime_mailbox_message(db.to_str().unwrap(), id)
                    .unwrap()
                    .is_none()
            );
            let observed = source_call(&mut source, json!({"op":"read","db":db,"path":path,"ids":[id]}));
            assert_eq!(observed["refs"], json!(expected_refs));
            assert_eq!(observed["remaining"], json!([null]));
            let expected_messages = match &message {
                Some(message) => vec![message.clone(); expected_refs.len()],
                None => Vec::new(),
            };
            assert_eq!(observed["messages"], json!(expected_messages));
            let native_refs = read_accepted_runtime_mailbox_store_refs(Some(&path.into())).unwrap();
            assert_eq!(JsValue::Array(native_refs), JsValue::from(&observed["refs"]));
            let reopened = SessionManager::open_read_only(&path.into(), None).unwrap();
            assert_eq!(reopened.get_entries().len(), expected_refs.len());
        }
    }
}
