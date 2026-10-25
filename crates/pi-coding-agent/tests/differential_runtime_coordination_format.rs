//! Differential: `core/runtime-coordination-format.ts` formatting.

#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::runtime_coordination_format::{format_runtime_mailbox_prompt, format_shared_channel_prompt};
use pi_coding_agent::core::session_control_db::{RuntimeMailboxMessage, SharedChannelMessage};
use serde_json::{Value, json};

fn mailbox_case(name: &str, recipient: &str, message: Value) -> Value {
    json!({"name": name, "recipientSessionId": recipient, "message": message})
}

fn shared_case(name: &str, recipient: &str, messages: Vec<Value>) -> Value {
    json!({"name": name, "recipientSessionId": recipient, "messages": messages})
}

fn message(sender_session: &str, sender_agent: Option<&str>, body: &str, file_refs: Option<Value>) -> Value {
    let mut message = json!({
        "id": 1,
        "recipient": {"agentId": null, "sessionId": "recipient"},
        "sender": {"agentId": sender_agent, "sessionId": sender_session},
        "kind": "system",
        "body": body,
        "storeRef": {"messageId": "message_1", "sessionPath": "/tmp/session.jsonl"},
        "status": "claimed",
        "createdAt": "2026-07-11T22:00:00.000Z",
        "updatedAt": "2026-07-11T22:00:00.000Z"
    });
    if let Some(file_refs) = file_refs {
        message["fileRefs"] = file_refs;
    }
    message
}

fn shared(sender_session: &str, sender_agent: Option<&str>, body: &str, id: i64) -> Value {
    json!({
        "body": body,
        "createdAt": "2026-07-11T22:00:00.000Z",
        "id": id,
        "sender": {"agentId": sender_agent, "sessionId": sender_session}
    })
}

fn with_host(mut message: Value, key: &str, host: &str) -> Value {
    message[key] = json!(host);
    message
}

#[test]
fn runtime_coordination_format_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "runtime-coordination-format-oracle.ts",
        "packages/coding-agent/src/core/runtime-coordination-format.ts",
    );
    let cases = vec![
        mailbox_case(
            "same-session",
            "session-1",
            message("session-1", Some("main"), "Hello there", None),
        ),
        mailbox_case(
            "different-session",
            "session-2",
            message("session-1", Some("agent_1"), "Cross session", None),
        ),
        mailbox_case(
            "unknown-session-and-agent",
            "session-2",
            message("", None, "No sender", None),
        ),
        mailbox_case(
            "empty-body",
            "session-1",
            message("session-1", Some("main"), "   \n\t ", None),
        ),
        mailbox_case(
            "trimmed-body",
            "session-1",
            message("session-1", Some("main"), "\n  padded body \t\n", None),
        ),
        mailbox_case(
            "file-refs",
            "session-1",
            message(
                "session-1",
                Some("main"),
                "With files",
                Some(json!([
                    {"label": "Bash output", "path": "/tmp/output.log"},
                    {"path": "/tmp/plain.log"},
                    {"label": "", "path": "/tmp/empty-label.log"}
                ])),
            ),
        ),
        mailbox_case(
            "empty-file-refs",
            "session-1",
            message("session-1", Some("main"), "No files", Some(json!([]))),
        ),
        mailbox_case(
            "unicode-body",
            "session-1",
            message("session-1", Some("main"), "héllo — 世界\nsecond line", None),
        ),
        mailbox_case(
            "same-session-remote-host",
            "session-1",
            with_host(
                message("session-1", Some("main"), "  remote body  ", None),
                "senderHost",
                "agent-server",
            ),
        ),
        mailbox_case(
            "same-session-empty-host",
            "session-1",
            with_host(message("session-1", Some("main"), "local body", None), "senderHost", ""),
        ),
        mailbox_case(
            "ecmascript-whitespace-body",
            "session-1",
            message("session-1", Some("main"), "\u{feff}\u{2003}", None),
        ),
        shared_case(
            "shared-remote-and-empty-host",
            "session-1",
            vec![
                with_host(
                    shared("session-1", Some("main"), "  remote body  ", 1),
                    "originHost",
                    "agent-server",
                ),
                with_host(shared("session-1", Some("main"), "  local body  ", 2), "originHost", ""),
            ],
        ),
        shared_case("shared-empty", "session-1", vec![]),
        shared_case(
            "shared-multiple",
            "session-1",
            vec![
                shared("session-1", Some("main"), "First", 1),
                shared("session-2", Some("agent_1"), "  Second  ", 2),
                shared("", None, "", 3),
            ],
        ),
    ];
    let request = json!({"id": 1, "operation": "format", "cases": cases});
    let response = oracle.request(&request);
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().expect("results");
    for (case, result) in cases.iter().zip(results) {
        let prompt = if let Some(messages) = case.get("messages") {
            let messages: Vec<SharedChannelMessage> =
                pi_js_compat::js_json::from_value(&pi_js_compat::js_json::JsValue::from(messages))
                    .expect("shared messages");
            format_shared_channel_prompt(&messages, case["recipientSessionId"].as_str().expect("recipient"))
        } else {
            let message: RuntimeMailboxMessage =
                pi_js_compat::js_json::from_value(&pi_js_compat::js_json::JsValue::from(&case["message"]))
                    .expect("mailbox message");
            format_runtime_mailbox_prompt(&message, case["recipientSessionId"].as_str().expect("recipient")).unwrap()
        };
        assert_eq!(case["name"], result["name"]);
        assert_eq!(
            prompt,
            pi_js_compat::js_json::JsString::from(result["prompt"].as_str().unwrap()),
            "case {} diverged: rust={prompt:?} ts={}",
            case["name"],
            result["prompt"]
        );
    }
}
