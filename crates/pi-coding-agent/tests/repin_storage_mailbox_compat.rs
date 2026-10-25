#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::runtime_mailbox_acceptance::{RuntimeMailboxAcceptance, RuntimeMailboxAcceptanceRef};
use pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, RuntimeMailboxRegistrationOptions, enqueue_stored_runtime_mailbox_message,
    register_runtime_mailbox_listener, take_runtime_mailbox_messages_for_delivery,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};
fn call(source: &mut oracle::TsOracle, mut request: Value) -> Value {
    request["id"] = json!(1);
    let response = source.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

#[test]
fn source_and_native_mailbox_claims_accept_into_the_same_recipient_jsonl() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite");
    let mut source = oracle::TsOracle::spawn("storage-mailbox-oracle.mts", "packages/coding-agent/src/core");
    let created = call(&mut source, json!({"op":"create","dir":tmp.path(),"db":db}));
    let path = JsString::from(created["path"].as_str().unwrap());
    let mut session = SessionManager::open(&path, None, None).unwrap();
    session
        .set_metadata_control_db_path(Some(&db.to_str().unwrap().into()), Default::default())
        .unwrap();
    let recipient = RuntimeMailboxAddress {
        session_id: session.get_session_id().clone(),
        agent_id: None,
    };
    register_runtime_mailbox_listener(
        db.to_str().unwrap(),
        &recipient,
        std::process::id(),
        Some(&path),
        RuntimeMailboxRegistrationOptions::default(),
    )
    .unwrap();
    let mut ids = Vec::new();
    let mut expected_refs = Vec::new();
    for (index, text) in [(0, "Bonjour 漢😀"), (1, "Native continuation")] {
        let input = json!({"recipient":recipient,"sender":{"sessionId":"sender","agentId":null},"kind":"message",
            "storeRef":{"sessionPath":"/fixture/sender.jsonl","messageId":format!("message-{index}")},
            "message":{"id":format!("message-{index}"),"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":text}});
        let id = if index == 0 {
            call(&mut source, json!({"op":"enqueue","db":db,"input":input}))
                .as_i64()
                .unwrap()
        } else {
            enqueue_stored_runtime_mailbox_message(
                db.to_str().unwrap(),
                &pi_js_compat::js_json::from_value(&JsValue::from(&input)).unwrap(),
            )
            .unwrap()
        };
        ids.push(id);
        expected_refs.push(input["storeRef"].clone());
        if index == 0 {
            let messages =
                take_runtime_mailbox_messages_for_delivery(db.to_str().unwrap(), &recipient, |_| Ok(true), 10).unwrap();
            assert_eq!(messages.len(), 1);
            let message = pi_js_compat::js_json::from_value(&JsValue::from(
                json!({"role":"user","content":[{"type":"text","text":text}],"inputSource":"extension","timestamp":1}),
            ))
            .unwrap();
            let mut acceptance = RuntimeMailboxAcceptance::default();
            acceptance
                .accept_messages(
                    &mut session,
                    &db.to_str().unwrap().into(),
                    &messages
                        .iter()
                        .map(RuntimeMailboxAcceptanceRef::from)
                        .collect::<Vec<_>>(),
                    Some(&message),
                )
                .unwrap();
            assert!(acceptance.consume_persisted_message(&message));
            assert!(!acceptance.consume_persisted_message(&message));
        } else {
            let accepted = call(&mut source, json!({"op":"accept","db":db,"path":path,"text":text}));
            assert_eq!(accepted["persisted"], true);
        }
    }
    let observed = call(&mut source, json!({"op":"read","db":db,"path":path,"ids":ids}));
    assert_eq!(observed["refs"], json!(expected_refs));
    assert_eq!(observed["remaining"], json!([null, null]));
    assert_eq!(observed["messages"].as_array().unwrap().len(), 2);
    let native_refs = read_accepted_runtime_mailbox_store_refs(Some(&path)).unwrap();
    assert_eq!(JsValue::Array(native_refs), JsValue::from(&observed["refs"]));
    let reopened = SessionManager::open(&path, None, None).unwrap();
    assert_eq!(
        reopened.get_entries().len(),
        2,
        "both source/native complete acceptance records survive reopen"
    );
}
