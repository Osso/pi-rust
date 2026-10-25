//! Persisted source JSON is not covered by typed-input exclusions.
use pi_js_compat::js_json::JsString;
#[path = "support/repin_fixture_process.rs"]
mod fixture_process;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::{js_json, js_json as json};

fn source_call(source: &mut oracle::TsOracle, name: &str, arguments: &str) -> JsValue {
    let response = source.request(&json!({"id":1,"steps":[{"op":"callWire","args":[name,arguments]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    response["output"][0]["value"].clone()
}

fn persisted_raw(source: &mut oracle::TsOracle, path: &str) -> js_json::JsObject {
    let response = source.request(
        &json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT data FROM multi_agent_mailbox_messages",[]]}]}),
    );
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    let JsValue::Object(value) =
        js_json::parse_lossless(response["output"][0]["value"][0]["data"].as_string().unwrap()).unwrap()
    else {
        panic!("canonical object")
    };
    value
}

#[test]
fn canonical_status_mutations_preserve_source_utf16_body_and_distinct_unknown_keys() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","unknown":{{"\ud800":"high","\udfff":"low","�":"replacement","array":["\ud800",{{"\udfff":"\ud800"}}]}}}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    let id = source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire)
        .as_string()
        .unwrap()
        .to_utf8()
        .expect("ASCII source row ID")
        .parse::<i64>()
        .unwrap();
    let original = persisted_raw(&mut source, &path);
    let address = RuntimeMailboxAddress {
        session_id: "to".into(),
        agent_id: None,
    };
    register_runtime_mailbox_listener(&path, &address, std::process::id(), None, Default::default()).unwrap();
    for transition in ["claim", "release", "claim", "fail"] {
        match transition {
            "claim" => {
                assert_eq!(claim_runtime_mailbox_messages(&path, &address, 1).unwrap().len(), 1);
            }
            "release" => {
                release_runtime_mailbox_message_claim(&path, id).unwrap();
            }
            "fail" => {
                fail_runtime_mailbox_message(&path, id, "fixture failure").unwrap();
            }
            _ => unreachable!(),
        }
        let updated = persisted_raw(&mut source, &path);
        for key in ["body", "unknown"] {
            let key = js_json::JsString::from(key);
            assert_eq!(
                updated.get(&key),
                original.get(&key),
                "{transition} must preserve exact raw field {key:?}"
            );
        }
    }
}

#[test]
fn routing_existing_source_mailbox_preserves_raw_utf16_fields() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},"/fixture/s","m",{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","unknown":{{"\ud800":"high","�":"replacement"}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "upsertMultiAgentMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let input = EnqueueRuntimeMailboxMessageInput {
        recipient_host: None,
        recipient: RuntimeMailboxAddress {
            session_id: "to".into(),
            agent_id: None,
        },
        sender: RuntimeMailboxAddress {
            session_id: "from".into(),
            agent_id: None,
        },
        kind: RuntimeMailboxMessageKind::Message,
        store_ref: RuntimeMailboxStoreRef {
            session_path: "/fixture/s".into(),
            message_id: "m".into(),
        },
    };
    let id = enqueue_runtime_mailbox_message(&path, &input).unwrap();
    assert_eq!(enqueue_runtime_mailbox_message(&path, &input).unwrap(), id);
    let routed = persisted_raw(&mut source, &path);
    for key in ["body", "unknown"] {
        let key = js_json::JsString::from(key);
        assert_eq!(
            routed.get(&key),
            original.get(&key),
            "routing must preserve raw field {key:?}"
        );
    }
    assert_eq!(
        routed.get(&js_json::JsString::from("recipientSessionId")),
        Some(&JsValue::String(js_json::JsString::from("to")))
    );
}

#[test]
fn routing_identity_does_not_alias_raw_surrogate_with_replacement_character() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"\ud800","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"identity"}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let input = EnqueueRuntimeMailboxMessageInput {
        recipient_host: None,
        recipient: RuntimeMailboxAddress {
            session_id: "�".into(),
            agent_id: None,
        },
        sender: RuntimeMailboxAddress {
            session_id: "from".into(),
            agent_id: None,
        },
        kind: RuntimeMailboxMessageKind::Message,
        store_ref: RuntimeMailboxStoreRef {
            session_path: "/fixture/s".into(),
            message_id: "m".into(),
        },
    };
    let peer =
        source.request(&json!({"id":1,"steps":[{"op":"call","args":["enqueueRuntimeMailboxMessage",[path,input]]}]}));
    assert_eq!(peer["output"][0]["ok"], json!(false), "{peer:?}");
    let error = enqueue_runtime_mailbox_message(&path, &input).unwrap_err();
    assert_eq!(
        error.message(),
        peer["output"][0]["error"]["message"].as_string().unwrap()
    );
    assert_eq!(
        persisted_raw(&mut source, &path),
        original,
        "rejected routing may not mutate canonical identity"
    );
}

#[test]
fn idempotent_enqueue_does_not_alias_raw_surrogate_body_with_replacement() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after"}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let input: EnqueueStoredRuntimeMailboxMessageInput = js_json::from_value(&json!({"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":"m"},"message":{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before�after"}})).unwrap();
    let peer = source
        .request(&json!({"id":1,"steps":[{"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[path,input]]}]}));
    assert_eq!(peer["output"][0]["ok"], json!(false), "{peer:?}");
    let error = enqueue_stored_runtime_mailbox_message(&path, &input).unwrap_err();
    assert_eq!(
        error.message(),
        peer["output"][0]["error"]["message"].as_string().unwrap()
    );
    assert_eq!(persisted_raw(&mut source, &path), original);
}

#[test]
fn raw_writer_multi_agent_failure_preserves_source_payload() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},"/fixture/s","m",{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","unknown":{{"\ud800":"high","�":"replacement"}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "upsertMultiAgentMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    assert!(mark_multi_agent_mailbox_message_failed(&path, "/fixture/s", &JsString::from("m"), "failure").unwrap());
    let failed = persisted_raw(&mut source, &path);
    for key in ["body", "unknown"] {
        let key = js_json::JsString::from(key);
        assert_eq!(failed.get(&key), original.get(&key));
    }
    assert_eq!(
        failed.get(&js_json::JsString::from("status")),
        Some(&JsValue::String(js_json::JsString::from("failed")))
    );
}

#[test]
fn raw_legacy_artifact_migration_preserves_unrelated_source_utf16_fields() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","unknown":{{"\ud800":"high","�":"replacement"}}}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let mut historical = original.clone();
    historical.insert(
        js_json::JsString::from("legacy"),
        JsValue::from(&json!({"artifactIds":["old"],"children":[{"artifactRefs":["old"],"keep":"value"}]})),
    );
    let serialized = js_json::stringify_lossless(&JsValue::Object(historical));
    let mutation = format!(
        "DROP TRIGGER multi_agent_mailbox_messages_reject_legacy_artifact_fields_update; UPDATE multi_agent_mailbox_messages SET data='{}'; PRAGMA user_version=13;",
        serialized.replace('\'', "''")
    );
    let result = source.request(&json!({"id":1,"steps":[{"op":"fixtureExec","args":[path,mutation]}]}));
    assert_eq!(result["output"][0]["ok"], json!(true), "{result:?}");
    read_session_metadata(&path, "/missing").unwrap();
    let migrated = persisted_raw(&mut source, &path);
    for key in ["body", "unknown"] {
        let key = js_json::JsString::from(key);
        assert_eq!(
            migrated.get(&key),
            original.get(&key),
            "migration must not normalize raw field {key:?}"
        );
    }
    assert_eq!(
        migrated.get(&js_json::JsString::from("legacy")),
        Some(&JsValue::from(&json!({"children":[{"keep":"value"}]})))
    );
}

#[test]
fn raw_legacy_routing_migration_preserves_unrelated_source_utf16_fields() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","unknown":{{"\ud800":"high","�":"replacement"}}}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let mutation = "CREATE TABLE runtime_mailbox_messages AS SELECT rowid AS id,'to' AS recipient_session_id,NULL AS recipient_agent_id,'from' AS sender_session_id,NULL AS sender_agent_id,'message' AS kind,session_path AS store_session_path,message_id AS store_message_id,'claimed' AS status,json_extract(data,'$.createdAt') AS created_at,updated_at,NULL AS error FROM multi_agent_mailbox_messages; PRAGMA user_version=13;";
    let result = source.request(&json!({"id":1,"steps":[{"op":"fixtureExec","args":[path,mutation]}]}));
    assert_eq!(result["output"][0]["ok"], json!(true), "{result:?}");
    read_session_metadata(&path, "/missing").unwrap();
    let migrated = persisted_raw(&mut source, &path);
    for key in ["body", "unknown"] {
        let key = js_json::JsString::from(key);
        assert_eq!(
            migrated.get(&key),
            original.get(&key),
            "legacy routing must preserve raw field {key:?}"
        );
    }
    assert_eq!(
        migrated.get(&js_json::JsString::from("status")),
        Some(&JsValue::String(js_json::JsString::from("pending")))
    );
}

#[test]
fn source_sql_bind_keys_and_raw_receipts_have_distinct_utf16_identity_on_recovery() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/\ud800/s","messageId":"\udfff"}},"message":{{"id":"\udfff","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"raw\ud800","unknown":{{"\ud800":"high","�":"replacement"}}}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let rows = source.request(&json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT session_path,message_id FROM multi_agent_mailbox_messages",[]]}]}));
    assert_eq!(rows["output"][0]["ok"], json!(true), "{rows:?}");
    assert_eq!(
        rows["output"][0]["value"],
        json!([{"session_path":"/fixture/�/s","message_id":"�"}]),
        "actual frozen SQLite binding replaces lone surrogates only at TEXT bind boundary"
    );
    assert_eq!(
        original.get(&js_json::JsString::from("id")),
        Some(&JsValue::String(js_json::JsString::from_utf16(vec![0xdfff])))
    );
    let receipt_wire = r#"{"mailboxStoreRefs":[{"sessionPath":"/fixture/\ud800/s","messageId":"\udfff"}]}"#;
    let created = source.request(
        &json!({"id":1,"steps":[{"op":"createReceipt","args":[temporary.path().join("receipts"),receipt_wire,true]}]}),
    );
    assert_eq!(created["output"][0]["ok"], json!(true), "{created:?}");
    let transcript = created["output"][0]["value"].as_string().unwrap();
    let address = RuntimeMailboxAddress {
        session_id: "to".into(),
        agent_id: None,
    };
    let registered = source.request(&json!({"id":1,"steps":[{"op":"registerSelf","args":[path,address,transcript]}]}));
    assert_eq!(registered["output"][0]["ok"], json!(true), "{registered:?}");
    let identity =
        js_json::from_value(&js_json::parse_lossless(registered["output"][0]["value"].as_string().unwrap()).unwrap())
            .unwrap();
    source_call(
        &mut source,
        "claimRuntimeMailboxMessages",
        &serde_json::to_string(&json!([path, address, 1])).unwrap(),
    );
    drop(source);
    fixture_process::wait_for_owned_fixture_exit(&identity);
    register_runtime_mailbox_listener(
        &path,
        &address,
        std::process::id(),
        Some(&JsString::from(transcript)),
        Default::default(),
    )
    .unwrap();
    let mut observer = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let recovered = persisted_raw(&mut observer, &path);
    assert_eq!(
        recovered.get(&js_json::JsString::from("status")),
        Some(&JsValue::String(js_json::JsString::from("pending"))),
        "raw receipt does not strictly equal SQL-replaced row keys; must requeue, not delete"
    );
    for key in ["id", "body", "unknown"] {
        let key = js_json::JsString::from(key);
        assert_eq!(
            recovered.get(&key),
            original.get(&key),
            "recovery must preserve raw identity {key:?}"
        );
    }
}

#[test]
fn canonical_mailbox_source_surrogate_body_survives_native_projection_and_claim() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after"}}}}]"#,
        serde_json::to_string(&db).unwrap()
    );
    let made =
        ts.request(&json!({"id":1,"steps":[{"op":"callWire","args":["enqueueStoredRuntimeMailboxMessage",wire]}]}));
    assert_eq!(made["output"][0]["ok"], json!(true), "{made:?}");
    let id = made["output"][0]["value"]
        .as_string()
        .unwrap()
        .to_utf8()
        .expect("ASCII source row ID")
        .parse::<i64>()
        .unwrap();
    let args = serde_json::to_string(&json!([db, id])).unwrap();
    let peer = ts.request(&json!({"id":1,"steps":[{"op":"callWire","args":["readRuntimeMailboxMessage",args]}]}));
    assert_eq!(peer["output"][0]["ok"], json!(true), "{peer:?}");
    let expected = js_json::parse_lossless(peer["output"][0]["value"].as_string().unwrap()).unwrap();
    let native = read_runtime_mailbox_message(&db, id).unwrap().unwrap();
    let native = js_json::to_value(&native).unwrap();
    register_runtime_mailbox_listener(
        &db,
        &RuntimeMailboxAddress {
            session_id: "to".into(),
            agent_id: None,
        },
        std::process::id(),
        None,
        Default::default(),
    )
    .unwrap();
    claim_runtime_mailbox_messages(
        &db,
        &RuntimeMailboxAddress {
            session_id: "to".into(),
            agent_id: None,
        },
        1,
    )
    .unwrap();
    let peer=ts.request(&json!({"id":1,"steps":[{"op":"callWire","args":["readRuntimeMailboxMessage",serde_json::to_string(&json!([db,id])).unwrap()]}]}));
    let after = js_json::parse_lossless(peer["output"][0]["value"].as_string().unwrap()).unwrap();
    let (JsValue::Object(expected), JsValue::Object(native), JsValue::Object(after)) = (expected, native, after) else {
        panic!("object projection")
    };
    let key = js_json::JsString::from("body");
    assert_eq!(
        (native.get(&key), after.get(&key)),
        (expected.get(&key), expected.get(&key)),
        "neither projection nor claim may replace original UTF16 code units"
    );
}
