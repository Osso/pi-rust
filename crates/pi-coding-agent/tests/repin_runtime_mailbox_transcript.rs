#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs;
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::json;
#[test]
fn whole_transcript_complete_lf_raw_refs_and_error_order_match_frozen_helper() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("receipt.jsonl");
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let fixtures = [
        ("{\"type\":\"message\",\"mailboxStoreRefs\":[{}]}\n", true),
        ("null\n", false),
        (
            "{\"type\":\"message\",\"mailboxStoreRefs\":[{\"sessionPath\":\"/partial\",\"messageId\":\"last\"}]}",
            true,
        ),
        ("{broken\n", false),
        (
            "{\"type\":\"message\",\"id\":\"old\",\"mailboxStoreRefs\":[{\"sessionPath\":\"/old\",\"messageId\":\"one\"}]}\n{\"type\":\"branch_summary\",\"parentId\":null}\n{\"type\":\"custom\",\"customType\":\"runtime_mailbox_acceptance\",\"data\":{\"mailboxStoreRefs\":[{\"sessionPath\":\"/new\",\"messageId\":\"two\"}]}}\n",
            true,
        ),
    ];
    for (contents, success) in fixtures {
        std::fs::write(&path, contents).unwrap();
        let response = ts.request(&json!({"id":1,"steps":[{"op":"acceptedRefs","path":path}]}));
        let peer = &response["output"][0];
        assert_eq!(peer["ok"], success, "{response}");
        let rust =
            read_accepted_runtime_mailbox_store_refs(Some(&JsString::from(path.to_str().expect("utf-8 test path"))));
        if success {
            assert_eq!(
                JsValue::Array(rust.expect("TS accepts raw refs")),
                JsValue::from(&peer["value"])
            )
        } else {
            assert!(rust.is_err(), "TS failure must not be silently skipped: {peer}")
        }
    }
    let absent = t.path().join("absent");
    assert_eq!(
        read_accepted_runtime_mailbox_store_refs(Some(&JsString::from(absent.to_str().expect("utf-8 test path"))))
            .unwrap()
            .len(),
        0
    );
    assert_eq!(read_accepted_runtime_mailbox_store_refs(None).unwrap().len(), 0);
}

#[test]
fn transcript_read_errors_match_frozen_node_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("transcript.jsonl");
    std::fs::write(&file, "{}").unwrap();
    let paths = [tmp.path().to_path_buf(), file.join("child")];
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    for path in paths {
        let peer = ts.request(&json!({"id":1,"steps":[{"op":"acceptedRefs","path":path}]}));
        assert_eq!(peer["output"][0]["ok"], false, "{peer}");
        let error =
            read_accepted_runtime_mailbox_store_refs(Some(&JsString::from(path.to_str().unwrap()))).unwrap_err();
        assert_eq!(error.message(), peer["output"][0]["error"]["message"].as_str().unwrap());
        let native = std::error::Error::source(&error)
            .unwrap()
            .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
            .unwrap();
        assert_eq!(native.name, peer["output"][0]["error"]["name"].as_str().unwrap());
    }
}

#[test]
fn malformed_complete_transcript_retains_frozen_syntax_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("malformed.jsonl");
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    for contents in ["{broken\n", "{\"type\":\"message\",\"mailboxStoreRefs\":[}\n"] {
        std::fs::write(&path, contents).unwrap();
        let peer = ts.request(&json!({"id":1,"steps":[{"op":"acceptedRefs","path":path}]}));
        assert_eq!(peer["output"][0]["ok"], false, "{peer}");
        let error =
            read_accepted_runtime_mailbox_store_refs(Some(&JsString::from(path.to_str().unwrap()))).unwrap_err();
        assert_eq!(error.message(), peer["output"][0]["error"]["message"].as_str().unwrap());
        let native = std::error::Error::source(&error)
            .expect("JSON.parse must retain its SyntaxError")
            .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
            .unwrap();
        assert_eq!(native.name, peer["output"][0]["error"]["name"].as_str().unwrap());
    }
}

#[test]
fn source_manager_receipts_preserve_lone_surrogate_values_keys_and_raw_items() {
    let t = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let payloads = [
        (
            r#"{"mailboxStoreRefs":[{"sessionPath":"/\ud800/source","messageId":"msg\udfff","\ud800":1,"�":2}]}"#,
            false,
        ),
        (
            r#"{"mailboxStoreRefs":[{},null,7,false,[],{"sessionPath":null,"messageId":0}]}"#,
            true,
        ),
        (r#"{"mailboxStoreRefs":"a\ud83d\udca5\ud800"}"#, true),
    ];
    for (index, (wire, custom_only)) in payloads.into_iter().enumerate() {
        let directory = t.path().join(format!("case{index}"));
        let made = ts.request(&json!({"id":1,"steps":[{"op":"createReceipt","args":[directory,wire,custom_only]}]}));
        assert_eq!(made["output"][0]["ok"], true, "{made}");
        let path = made["output"][0]["value"].as_str().unwrap();
        let peer = ts.request(&json!({"id":1,"steps":[{"op":"acceptedRefsWire","path":path}]}));
        assert_eq!(peer["output"][0]["ok"], true, "{peer}");
        let rust = read_accepted_runtime_mailbox_store_refs(Some(&JsString::from(path))).unwrap();
        let encoded = pi_js_compat::js_json::stringify_lossless(&JsValue::Array(rust));
        assert_eq!(
            encoded,
            peer["output"][0]["value"].as_str().unwrap(),
            "source manager case{index}"
        );
    }
}

#[test]
fn noniterable_ref_collection_throws_at_the_source_spread_boundary() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("bad.jsonl");
    std::fs::write(
        &path,
        "{\"type\":\"custom\",\"customType\":\"runtime_mailbox_acceptance\",\"data\":{\"mailboxStoreRefs\":{}}}\n",
    )
    .unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let peer = ts.request(&json!({"id":1,"steps":[{"op":"acceptedRefsWire","path":path}]}));
    assert_eq!(peer["output"][0]["ok"], false);
    let error =
        read_accepted_runtime_mailbox_store_refs(Some(&JsString::from(path.to_str().expect("utf-8 test path"))))
            .unwrap_err();
    assert_eq!(error.message(), peer["output"][0]["error"]["message"].as_str().unwrap());
}

#[test]
fn raw_ref_equality_uses_js_property_access_strict_equality_and_utf16() {
    use pi_coding_agent::core::runtime_mailbox_transcript::mailbox_store_refs_equal;
    use pi_js_compat::js_json::parse_lossless;
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let cases = [
        ("{}", "{}"),
        ("null", "{}"),
        ("{\"sessionPath\":\"x\"}", "null"),
        ("7", "false"),
        (
            r#"{"sessionPath":"\ud800","messageId":"\udfff"}"#,
            r#"{"sessionPath":"\ud800","messageId":"\udfff"}"#,
        ),
        (
            r#"{"sessionPath":"\ud800","messageId":"a"}"#,
            r#"{"sessionPath":"�","messageId":"a"}"#,
        ),
        (
            r#"{"sessionPath":{},"messageId":0}"#,
            r#"{"sessionPath":{},"messageId":0}"#,
        ),
    ];
    for (left, right) in cases {
        let peer = ts.request(&json!({"id":1,"steps":[{"op":"refsEqualWire","args":[left,right]}]}));
        let actual = mailbox_store_refs_equal(&parse_lossless(left).unwrap(), &parse_lossless(right).unwrap());
        if peer["output"][0]["ok"] == true {
            assert_eq!(
                actual.unwrap(),
                peer["output"][0]["value"].as_bool().unwrap(),
                "{left} / {right}"
            )
        } else {
            assert_eq!(
                actual.unwrap_err().message(),
                peer["output"][0]["error"]["message"].as_str().unwrap()
            );
        }
    }
}
