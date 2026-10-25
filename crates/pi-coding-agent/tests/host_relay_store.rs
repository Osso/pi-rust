//! Frozen TS ↔ Rust persistence, sequencing, collision rollback, and channel-origin behavior.
use crate::shared_oracle as oracle;
use pi_coding_agent::core::host_relay::{RelayFrame, parse_frame};
use pi_coding_agent::core::host_relay_store::*;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{self, JsValue};
use serde_json::{Value, json};
fn call(ts: &mut oracle::TsOracle, op: &str, name: &str, args: Value) -> String {
    let r = ts.request(&json!({"op":op,"fn":name,"args":args.to_string()}));
    assert_eq!(r["ok"], true, "{r}");
    r["wire"].as_str().unwrap().to_owned()
}
fn wire_call(ts: &mut oracle::TsOracle, op: &str, name: &str, args: &str) -> String {
    let r = ts.request(&json!({"op":op,"fn":name,"args":args}));
    assert_eq!(r["ok"], true, "{r}");
    r["wire"].as_str().unwrap().into()
}
fn temp() -> (tempfile::TempDir, String) {
    let t = tempfile::tempdir().unwrap();
    let path = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    (t, path)
}
fn input(id: &str) -> Value {
    json!({"kind":"message","recipientHost":"peer","recipient":{"sessionId":"receiver","agentId":null},"sender":{"sessionId":"sender","agentId":null},"storeRef":{"sessionPath":"/fixture/s","messageId":id},"message":{"id":id,"kind":"message","status":"pending","fromAgentId":"main","toAgentId":"main","body":"payload","createdAt":pi_coding_agent::utils::iso_time::now_iso_string(),"details":{"2":"index","nested":[9007199254740993u64,1e-7]}}})
}
fn counter(path: &str, key: &str) -> i64 {
    with_control_db(&JsString::from(path), |db| {
        Ok(db.query_row(
            "SELECT last_seen_id FROM shared_channel_cursors WHERE session_id = ?",
            [key],
            |row| row.get(0),
        )?)
    })
    .unwrap()
}
#[test]
fn ts_store_rust_sequence_ts_ack_and_lossless_inbound_replay() {
    let (_a, a) = temp();
    let (_b, b) = temp();
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    let raw = json!([a, input("one")])
        .to_string()
        .replace("\"payload\"", r#""payload\ud800""#)
        .replace("\"nested\"", r#""key\udc00""#);
    wire_call(&mut ts, "db", "enqueueStoredRuntimeMailboxMessage", &raw);
    for id in ["two", "three"] {
        call(
            &mut ts,
            "db",
            "enqueueStoredRuntimeMailboxMessage",
            json!([a, input(id)]),
        );
    }
    let first_ts = call(&mut ts, "store", "nextRelayMailbox", json!([a, "peer"]));
    let first = next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
        .unwrap()
        .unwrap();
    assert_eq!(RelayFrame::Mailbox(first.clone()).to_wire(), first_ts);
    assert!(first_ts.contains(r"\ud800"));
    assert!(first_ts.contains(r"\udc00"));
    assert_eq!(
        next_relay_mailbox(&JsString::from(&a), &JsString::from("peer")).unwrap(),
        Some(first.clone())
    );
    let mut changed = first.clone();
    if let JsValue::Object(data) = &mut changed.data {
        data.insert("body".into(), JsValue::String("changed".into()));
    }
    ack_relay_mailbox(&JsString::from(&a), &JsString::from("peer"), &changed).unwrap();
    assert!(
        next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
            .unwrap()
            .is_some()
    );
    import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &first).unwrap();
    let ts_in = call(&mut ts, "db", "listRuntimeMailboxMessages", json!([b]));
    assert!(ts_in.contains(r"\ud800"), "{ts_in}");
    let row = with_control_db(&JsString::from(&b), |db| {
        Ok(db.query_row("SELECT data FROM multi_agent_mailbox_messages", [], |r| {
            r.get::<_, String>(0)
        })?)
    })
    .unwrap();
    assert!(row.contains(r"\udc00"));
    assert!(!row.contains("recipientHost"));
    assert!(!row.contains("relaySeq"));
    assert!(row.contains(r#""senderHost":"origin""#));
    wire_call(
        &mut ts,
        "store",
        "ackRelayMailbox",
        &format!(
            "[{},{},{}]",
            json!(a),
            json!("peer"),
            RelayFrame::Mailbox(first.clone()).to_wire()
        ),
    );
    mark_runtime_mailbox_message_delivered(&b, 1).unwrap();
    import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &first).unwrap();
    assert!(list_runtime_mailbox_messages(&b).unwrap().is_empty());
    let second = next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
        .unwrap()
        .unwrap();
    assert_eq!(second.seq, 2);
    let second_wire = RelayFrame::Mailbox(second.clone()).to_wire();
    assert_eq!(
        call(&mut ts, "store", "nextRelayMailbox", json!([a, "peer"])),
        second_wire
    );
    wire_call(
        &mut ts,
        "store",
        "importRelayMailbox",
        &format!("[{},{},{}]", json!(b), json!("origin"), second_wire),
    );
    assert_eq!(list_runtime_mailbox_messages(&b).unwrap()[0].body, "payload");
    import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &second).unwrap();
    assert_eq!(counter(&b, "relay-in:origin"), 2);
    ack_relay_mailbox(&JsString::from(&a), &JsString::from("peer"), &second).unwrap();
    let third = next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
        .unwrap()
        .unwrap();
    assert_eq!(third.seq, 3);
    assert_eq!(third.message_id, "three");
    assert_eq!(counter(&a, "relay-out:peer"), 3);
}
#[test]
fn custom_kind_survives_frozen_queue_native_import_and_frozen_read() {
    let (_a, a) = temp();
    let (_b, b) = temp();
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    let mut queued = input("custom-kind");
    queued["kind"] = json!("custom");
    queued["message"]["kind"] = json!("custom");
    call(&mut ts, "db", "enqueueStoredRuntimeMailboxMessage", json!([a, queued]));
    let frozen = call(&mut ts, "store", "nextRelayMailbox", json!([a, "peer"]));
    let RelayFrame::Mailbox(mut message) = parse_frame(&frozen).unwrap() else {
        panic!("mailbox expected")
    };
    import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &message).unwrap();
    let read = ts.request(&json!({"op":"raw","path":b,"args":"SELECT data FROM multi_agent_mailbox_messages"}));
    assert_eq!(read["ok"], true, "{read}");
    let rows: Value = serde_json::from_str(read["wire"].as_str().unwrap()).unwrap();
    let canonical: Value = serde_json::from_str(rows[0]["data"].as_str().unwrap()).unwrap();
    assert_eq!(canonical["kind"], "custom");
    let before = with_control_db(&JsString::from(&b), |db| {
        Ok(
            db.query_row("SELECT data FROM multi_agent_mailbox_messages", [], |row| {
                row.get::<_, String>(0)
            })?,
        )
    })
    .unwrap();
    message.seq = 2;
    import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &message).unwrap();
    assert_eq!(counter(&b, "relay-in:origin"), 2);
    let after = with_control_db(&JsString::from(&b), |db| {
        Ok(
            db.query_row("SELECT data FROM multi_agent_mailbox_messages", [], |row| {
                row.get::<_, String>(0)
            })?,
        )
    })
    .unwrap();
    assert_eq!(before, after);
    wire_call(
        &mut ts,
        "store",
        "ackRelayMailbox",
        &format!("[{},{},{}]", json!(a), json!("peer"), frozen),
    );
    assert_eq!(call(&mut ts, "db", "listRuntimeMailboxMessages", json!([a])), "[]");
}

#[test]
fn inbound_collision_rolls_back_cursor_and_sequence_exhaustion_rolls_back_row() {
    let (_a, a) = temp();
    let (_b, b) = temp();
    let input: EnqueueStoredRuntimeMailboxMessageInput = serde_json::from_value(input("one")).unwrap();
    enqueue_stored_runtime_mailbox_message(&a, &input).unwrap();
    let first = next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
        .unwrap()
        .unwrap();
    import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &first).unwrap();
    let mut conflict = first.clone();
    conflict.seq = 2;
    if let JsValue::Object(data) = &mut conflict.data {
        data.insert("body".into(), JsValue::String("conflict".into()));
    }
    assert!(
        import_relay_mailbox(&JsString::from(&b), &JsString::from("origin"), &conflict)
            .unwrap_err()
            .to_string()
            .contains("collision")
    );
    assert_eq!(counter(&b, "relay-in:origin"), 1);
    ack_relay_mailbox(&JsString::from(&a), &JsString::from("peer"), &first).unwrap();
    with_control_db(&JsString::from(&a), |db| {
        db.execute(
            "UPDATE shared_channel_cursors SET last_seen_id = 9007199254740991 WHERE session_id = 'relay-out:peer'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let mut next = input;
    next.store_ref.message_id = "two".into();
    next.message["id"] = pi_js_compat::js_json!("two");
    enqueue_stored_runtime_mailbox_message(&a, &next).unwrap();
    assert!(
        next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
            .unwrap_err()
            .to_string()
            .contains("exhausted")
    );
    let raw = with_control_db(&JsString::from(&a), |db| {
        Ok(db.query_row("SELECT data FROM multi_agent_mailbox_messages", [], |r| {
            r.get::<_, String>(0)
        })?)
    })
    .unwrap();
    assert!(!raw.contains("relaySeq"));
}
#[test]
fn channel_tail_origin_dedup_expiry_and_monotonic_cursors_match_frozen() {
    let (_a, a) = temp();
    let (_b, b) = temp();
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    let post = |body: &str| PostSharedChannelMessageInput {
        sender: RuntimeMailboxAddress {
            session_id: "sender".into(),
            agent_id: None,
        },
        body: body.into(),
    };
    post_shared_channel_message(&a, &post("old")).unwrap();
    assert!(
        next_relay_channel(&JsString::from(&a), &JsString::from("peer"), &JsString::from("origin"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        call(&mut ts, "store", "nextRelayChannel", json!([a, "peer", "origin"])),
        "null"
    );
    let id = post_shared_channel_message(&a, &post("new")).unwrap();
    let channel = next_relay_channel(&JsString::from(&a), &JsString::from("peer"), &JsString::from("origin"))
        .unwrap()
        .unwrap();
    assert_eq!(channel.origin_id, id);
    assert_eq!(
        RelayFrame::Channel(channel.clone()).to_wire(),
        call(&mut ts, "store", "nextRelayChannel", json!([a, "peer", "origin"]))
    );
    import_relay_channel(&JsString::from(&b), &channel).unwrap();
    call(
        &mut ts,
        "store",
        "importRelayChannel",
        json!([b, channel.to_value().into_serde_lossy()]),
    );
    assert_eq!(
        list_shared_channel_messages_after(&b, 0, 100, i64::MAX).unwrap().len(),
        1
    );
    // Imported messages never echo to other peers, including after cursor reset.
    ack_relay_channel(&JsString::from(&b), &JsString::from("other"), 0).unwrap();
    assert!(
        next_relay_channel(&JsString::from(&b), &JsString::from("other"), &JsString::from("peer"))
            .unwrap()
            .is_none()
    );
    call(&mut ts, "store", "ackRelayChannel", json!([a, "peer", id]));
    ack_relay_channel(&JsString::from(&a), &JsString::from("peer"), 0).unwrap();
    assert_eq!(counter(&a, "relay:peer"), id);
    assert!(
        next_relay_channel(&JsString::from(&a), &JsString::from("peer"), &JsString::from("origin"))
            .unwrap()
            .is_none()
    );
    let mut expired = channel;
    expired.origin_id += 10;
    expired.created_at = "2000-01-01T00:00:00.000Z".into();
    import_relay_channel(&JsString::from(&b), &expired).unwrap();
    assert_eq!(
        list_shared_channel_messages_after(&b, 0, 100, i64::MAX).unwrap().len(),
        1
    );
}
#[test]
fn newly_allocated_invalid_sequence_rolls_back_like_frozen() {
    let (_a, a) = temp();
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    call(
        &mut ts,
        "db",
        "enqueueStoredRuntimeMailboxMessage",
        json!([a, input("bad-counter")]),
    );
    with_control_db(&JsString::from(&a),|db|{db.execute("INSERT INTO shared_channel_cursors(session_id,agent_id_key,last_seen_id,updated_at) VALUES ('relay-out:peer','',-1,'fixture')",[])?;Ok(())}).unwrap();
    let expected = ts.request(&json!({"op":"store","fn":"nextRelayMailbox","args":json!([a,"peer"]).to_string()}));
    assert_eq!(expected["ok"], false);
    let error = next_relay_mailbox(&JsString::from(&a), &JsString::from("peer")).unwrap_err();
    assert_eq!(expected["error"], error.to_string());
    assert_eq!(counter(&a, "relay-out:peer"), -1);
}

#[test]
fn invalid_stored_sequence_and_expired_rows_match_frozen() {
    let (_a, a) = temp();
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    call(
        &mut ts,
        "db",
        "enqueueStoredRuntimeMailboxMessage",
        json!([a, input("bad")]),
    );
    for value in ["null", "0", "-1", "1.5", "9007199254740992", "\"1\""] {
        with_control_db(&JsString::from(&a), |db| {
            db.execute(
                "UPDATE multi_agent_mailbox_messages SET data = json_set(data, '$.relaySeq', json(?))",
                [value],
            )?;
            Ok(())
        })
        .unwrap();
        let rust = next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
            .unwrap_err()
            .to_string();
        let peer = ts.request(&json!({"op":"store","fn":"nextRelayMailbox","args":json!([a,"peer"]).to_string()}));
        assert_eq!(peer["ok"], false);
        assert_eq!(peer["error"], rust);
    }
    with_control_db(&JsString::from(&a), |db| {
        db.execute(
            "UPDATE multi_agent_mailbox_messages SET data = json_set(data, '$.createdAt', '2000-01-01T00:00:00.000Z')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(
        next_relay_mailbox(&JsString::from(&a), &JsString::from("peer"))
            .unwrap()
            .is_none()
    );
    assert_eq!(call(&mut ts, "store", "nextRelayMailbox", json!([a, "peer"])), "null");
}
#[test]
fn text_validation_preserves_utf16_like_frozen() {
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    for wire in [
        r#""a\ud800""#,
        r#""\udc00""#,
        r#"" plain ""#,
        r#""""#,
        r#""\ufeff\u00a0""#,
        "null",
        "1",
        "[]",
        "{}",
    ] {
        let value = js_json::parse_lossless(wire).unwrap();
        for nullable in [false, true] {
            let name = if nullable { "nullableText" } else { "requireText" };
            let actual = if nullable {
                nullable_text(Some(&value), "fixture").map(|v| v.map(JsValue::String).unwrap_or(JsValue::Null))
            } else {
                require_text(Some(&value), "fixture").map(JsValue::String)
            };
            let expected = ts.request(&json!({"op":"store","fn":name,"args":format!("[{wire},\"fixture\"]")}));
            match actual {
                Ok(value) => {
                    assert_eq!(expected["ok"], true, "{expected}");
                    assert_eq!(expected["wire"], js_json::stringify_lossless(&value));
                }
                Err(error) => {
                    assert_eq!(expected["ok"], false);
                    assert_eq!(expected["error"], error.to_string());
                }
            }
        }
    }
}

#[test]
fn parser_preserves_lossless_nested_payload_wire() {
    let wire =
        r#"{"type":"mailbox","seq":1,"sessionPath":"/s","messageId":"m","data":{"body":"\ud800","\udc00":1e400}}"#;
    let parsed = parse_frame(wire).unwrap();
    let round = parse_frame(&parsed.to_wire()).unwrap();
    // JSON.stringify maps Infinity to null, exactly like the frozen transport.
    if let RelayFrame::Mailbox(message) = round {
        assert_eq!(
            js_json::stringify_lossless(&message.data),
            r#"{"body":"\ud800","\udc00":null}"#
        );
    } else {
        panic!("mailbox expected")
    }
    assert!(matches!(parsed, RelayFrame::Mailbox(_)));
}
