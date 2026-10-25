use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, register_runtime_mailbox_listener, take_runtime_mailbox_messages_for_delivery,
    with_control_db,
};
use pi_js_compat::{
    abort::AbortReason,
    callback_error::CallbackError,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn eligibility_rejection_preserves_original_error_and_leaves_all_rows_unclaimed() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let recipient = RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    };
    for throw_at in [1, 2] {
        let source_db = root.path().join(format!("source{throw_at}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{throw_at}.sqlite")).to_str().unwrap());
        let mut steps = vec![json!({"op":"registerSelf","args":[source_db,recipient,null]})];
        for db in [json!(source_db), json!(native_db)] {
            for index in 0..3 {
                steps.push(json!({"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db,{"recipient":recipient,"sender":{"sessionId":"sender","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/source.jsonl","messageId":format!("m{index}")},"message":{"id":format!("m{index}"),"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":format!("message {index}")}}]]}));
            }
        }
        steps.push(json!({"op":"takeThrow","args":[source_db,recipient,throw_at]}));
        let response = source.request(&json!({"id":1,"steps":steps}));
        let expected = &response["output"].as_array().unwrap().last().unwrap()["value"];
        register_runtime_mailbox_listener(&native_db, &recipient, std::process::id(), None, Default::default())
            .unwrap();
        let calls = AtomicUsize::new(0);
        let result = take_runtime_mailbox_messages_for_delivery(
            &native_db,
            &recipient,
            |_| {
                let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
                if call == throw_at {
                    Err(CallbackError::from(
                        NativeNodeError::from_constructor(NativeErrorClass::TypeError, "eligibility failed")
                            .with_property("code", JsValue::String("FIXTURE".into())),
                    ))
                } else {
                    Ok(true)
                }
            },
            20,
        );
        let error = CallbackError::from(result.unwrap_err()).thrown_value();
        let error = match error {
            AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
            AbortReason::Value(value) => value,
        };
        assert_eq!(
            JsValue::from(json!({"calls":calls.load(Ordering::SeqCst),"ok":false,"error":error})),
            JsValue::from(expected.clone())
        );
        let pending = with_control_db(&native_db, |db| {
            Ok(db.query_row(
                "SELECT COUNT(*) FROM multi_agent_mailbox_messages WHERE json_extract(data,'$.status')='pending'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .unwrap();
        assert_eq!(pending, 3, "predicate failure must not claim an earlier eligible row");
    }
}
