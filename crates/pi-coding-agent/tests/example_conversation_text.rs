#[path = "../examples/extensions/handoff.rs"]
mod handoff;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "../examples/extensions/summarize.rs"]
mod summarize;
use pi_coding_agent::core::session_manager::SessionEntry;
use pi_js_compat::js_json::{self, JsString, JsValue};

fn entries(raw: JsString, compacted: bool) -> JsValue {
    let mut entries = vec![pi_js_compat::js_json!({
        "type":"message", "id":"u", "parentId":null, "timestamp":"2026-01-01T00:00:00.000Z",
        "message":{"role":"user","content":raw,"timestamp":0}
    })];
    let mut assistant = js_json::to_value(&pi_ai::providers::faux::faux_assistant_message(
        "",
        pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
    ))
    .unwrap();
    assistant["content"] = pi_js_compat::js_json!([
        {"type":"text","text":raw},
        {"type":"toolCall","id":"tool-1","name":"read","arguments":{"2":"two","1":raw,"nested":{"text":raw}}}
    ]);
    entries.push(pi_js_compat::js_json!({
        "type":"message", "id":"a", "parentId":"u", "timestamp":"2026-01-01T00:00:00.000Z",
        "message":assistant
    }));
    if compacted {
        entries.push(pi_js_compat::js_json!({
            "type":"compaction", "id":"c", "parentId":"u", "timestamp":"2026-01-01T00:00:00.000Z",
            "summary":raw,"firstKeptEntryId":"u","tokensBefore":100
        }));
    }
    JsValue::Array(entries)
}

#[test]
fn handoff_compacted_and_uncompacted_messages_keep_raw_text_like_frozen() {
    let _factory = handoff::handoff_extension;
    let mut oracle = oracle::TsOracle::spawn(
        "example-conversation-text-oracle.ts",
        "packages/coding-agent/examples/extensions/handoff.ts",
    );
    for unit in [0xd800, 0xdc00] {
        for compacted in [false, true] {
            let entries = entries(JsString::from_utf16(vec![97, unit, 98]), compacted);
            let expected = oracle.request(&pi_js_compat::js_json!({"entries":entries}));
            let native: Vec<SessionEntry> = js_json::from_value(&entries).unwrap();
            let actual = handoff::handoff_messages(&native).unwrap();
            assert_eq!(js_json::to_value(&actual).unwrap(), expected);
        }
    }
}

#[test]
fn summary_conversation_keeps_raw_text_and_skips_compaction_like_frozen() {
    let _factory = summarize::summarize_extension;
    let mut oracle = oracle::TsOracle::spawn(
        "example-conversation-text-oracle.ts",
        "packages/coding-agent/examples/extensions/summarize.ts",
    );
    for unit in [0xd800, 0xdc00] {
        let entries = entries(JsString::from_utf16(vec![32, 97, unit, 98, 32]), true);
        let expected = oracle.request(&pi_js_compat::js_json!({"entries":entries}));
        let native: Vec<SessionEntry> = js_json::from_value(&entries).unwrap();
        let actual = summarize::conversation_text(&native).unwrap();
        assert_eq!(js_json::to_value(&actual).unwrap(), expected);
    }
}
