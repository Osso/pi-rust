//! Differential tests: the pinned TypeScript coding-agent compaction modules
//! versus the Rust port.

#[path = "support/oracle.rs"]
pub(crate) mod oracle;

use pi_js_compat::js_json::{JsString, JsValue};
use std::fs;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::Message;
use pi_coding_agent::core::compaction::{
    CompactionSettings, DEFAULT_COMPACTION_SETTINGS, FileOperations, calculate_context_tokens,
    collect_entries_for_branch_summary, compute_file_lists, create_file_ops, estimate_context_tokens, estimate_tokens,
    extract_file_ops_from_message, find_cut_point, find_turn_start_index, format_file_operations,
    get_last_assistant_usage, prepare_branch_entries, prepare_compaction, serialize_conversation, should_compact,
};
use pi_coding_agent::core::session_manager::{SessionEntry, SessionManager};
use serde_json::{Value, json};

struct Oracle {
    inner: oracle::TsOracle,
    next_id: u64,
}

impl Oracle {
    fn new() -> Self {
        Self {
            inner: oracle::TsOracle::spawn("coding-agent-compaction-oracle.ts", "packages/coding-agent/src"),
            next_id: 0,
        }
    }

    fn request(&mut self, module: &str, operation: &str, body: Value) -> Value {
        self.next_id += 1;
        let mut request = body;
        request["id"] = json!(self.next_id);
        request["module"] = json!(module);
        request["operation"] = json!(operation);
        let response = self.inner.request(&request);
        assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
        response["output"].clone()
    }
}

const COMPACTION: &str = "core/compaction/index.ts";
const UTILS: &str = "core/compaction/utils.ts";

/// ECMA-262 number rendering used by the TypeScript side (`1800`, not `1800.0`).
fn js_num(value: f64) -> Value {
    pi_js_compat::number::js_number_value(value)
}

fn settings_value(settings: &CompactionSettings) -> Value {
    let mut value = json!({
        "enabled": settings.enabled,
        "reserveTokens": js_num(settings.reserve_tokens),
        "keepRecentTokens": js_num(settings.keep_recent_tokens),
    });
    if let Some(percent) = settings.threshold_percent {
        value["thresholdPercent"] = js_num(percent);
    }
    value
}

fn entries(value: &Value) -> Vec<SessionEntry> {
    value
        .as_array()
        .expect("entries must be an array")
        .iter()
        .map(|entry| serde_json::from_value(entry.clone()).expect("session entry must deserialize"))
        .collect()
}

fn agent_messages(value: &Value) -> Vec<AgentMessage> {
    value
        .as_array()
        .expect("messages must be an array")
        .iter()
        .map(|message| serde_json::from_value(message.clone()).expect("agent message must deserialize"))
        .collect()
}

fn llm_messages(value: &Value) -> Vec<Message> {
    value
        .as_array()
        .expect("messages must be an array")
        .iter()
        .map(|message| serde_json::from_value(message.clone()).expect("llm message must deserialize"))
        .collect()
}

fn settings(value: &Value) -> CompactionSettings {
    serde_json::from_value(value.clone()).expect("settings must deserialize")
}

fn usage(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Value {
    json!({
        "input": input,
        "output": output,
        "cacheRead": cache_read,
        "cacheWrite": cache_write,
        "totalTokens": input + output + cache_read + cache_write,
        "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 },
    })
}

fn user_message(text: &str) -> Value {
    json!({ "role": "user", "content": text, "timestamp": 0 })
}

fn extension_user_message(text: &str) -> Value {
    json!({ "role": "user", "content": text, "inputSource": "extension", "timestamp": 0 })
}

fn assistant_message(text: &str, usage: Value) -> Value {
    json!({
        "role": "assistant",
        "content": [{ "type": "text", "text": text }],
        "api": "anthropic-messages",
        "provider": "anthropic",
        "model": "claude-sonnet-4-5",
        "usage": usage,
        "stopReason": "stop",
        "timestamp": 0,
    })
}

fn entry(id: &str, parent: Option<&str>, timestamp: &str, body: Value) -> Value {
    let mut object = body.as_object().expect("entry body").clone();
    object.insert("id".to_owned(), json!(id));
    object.insert("parentId".to_owned(), json!(parent));
    object.insert("timestamp".to_owned(), json!(timestamp));
    Value::Object(object)
}

fn message_entry(id: &str, parent: Option<&str>, message: Value) -> Value {
    entry(
        id,
        parent,
        "2026-01-01T00:00:00.000Z",
        json!({ "type": "message", "message": message }),
    )
}

fn compaction_entry(id: &str, parent: Option<&str>, summary: &str, first_kept: &str) -> Value {
    entry(
        id,
        parent,
        "2026-01-01T00:00:00.000Z",
        json!({
            "type": "compaction",
            "summary": summary,
            "firstKeptEntryId": first_kept,
            "tokensBefore": 10_000,
        }),
    )
}

fn linear_entries() -> Value {
    let mut entries = Vec::new();
    let mut parent: Option<String> = None;
    for index in 0..6 {
        let user_id = format!("u{index}");
        let assistant_id = format!("a{index}");
        entries.push(message_entry(
            &user_id,
            parent.as_deref(),
            user_message(&format!("User {index}")),
        ));
        entries.push(message_entry(
            &assistant_id,
            Some(&user_id),
            assistant_message(&format!("Assistant {index}"), usage(0.0, 100.0, 0.0, 0.0)),
        ));
        parent = Some(assistant_id);
    }
    Value::Array(entries)
}

#[test]
fn differential_token_calculation_and_estimation() {
    let mut oracle = Oracle::new();
    let usages = json!([
        usage(1000.0, 500.0, 200.0, 100.0),
        usage(0.0, 0.0, 0.0, 0.0),
        { "input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "totalTokens": 0, "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 } },
        { "input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "totalTokens": 99, "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 } },
    ]);
    let expected = oracle.request(COMPACTION, "tokens", json!({ "usages": usages }));
    let actual: Vec<Value> = usages
        .as_array()
        .expect("usages")
        .iter()
        .map(|usage| {
            json!(js_num(calculate_context_tokens(
                &serde_json::from_value(usage.clone()).unwrap()
            )))
        })
        .collect();
    assert_eq!(actual, *expected.as_array().unwrap());

    let messages = json!([
        user_message("12345678"),
        user_message("123456789"),
        { "role": "user", "content": [{ "type": "text", "text": "abcd" }, { "type": "image", "data": "x", "mimeType": "image/png" }], "timestamp": 0 },
        assistant_message("12345678", usage(0.0, 0.0, 0.0, 0.0)),
        {
            "role": "assistant",
            "content": [
                { "type": "text", "text": "abcd" },
                { "type": "thinking", "thinking": "abcdefgh" },
                { "type": "toolCall", "id": "t", "name": "read", "arguments": { "path": "a" } }
            ],
            "api": "anthropic-messages",
            "provider": "anthropic",
            "model": "m",
            "usage": usage(0.0, 0.0, 0.0, 0.0),
            "stopReason": "stop",
            "timestamp": 0,
        },
        { "role": "toolResult", "toolCallId": "t", "toolName": "read", "content": [{ "type": "text", "text": "abcdefgh" }], "isError": false, "timestamp": 0 },
        { "role": "custom", "customType": "x", "content": "abcdefgh", "display": true, "timestamp": 0 },
        { "role": "bashExecution", "command": "abcd", "output": "abcdefgh", "timestamp": 0 },
        { "role": "branchSummary", "summary": "abcdefgh", "fromId": "x", "timestamp": 0 },
        { "role": "compactionSummary", "summary": "abcdefgh", "tokensBefore": 1, "timestamp": 0 },
    ]);
    let expected = oracle.request(COMPACTION, "estimateTokens", json!({ "messages": messages }));
    let actual: Vec<Value> = agent_messages(&messages)
        .iter()
        .map(|message| json!(js_num(estimate_tokens(message))))
        .collect();
    assert_eq!(actual, *expected.as_array().unwrap());

    let context_messages = json!([
        user_message("Hello"),
        assistant_message("Hi", usage(100.0, 50.0, 0.0, 0.0)),
        user_message("continue"),
        assistant_message("Partial", usage(0.0, 0.0, 0.0, 0.0)),
    ]);
    let expected = oracle.request(COMPACTION, "estimateContext", json!({ "messages": context_messages }));
    let estimate = estimate_context_tokens(&agent_messages(&context_messages));
    assert_eq!(
        expected,
        json!({
            "tokens": js_num(estimate.tokens),
            "usageTokens": js_num(estimate.usage_tokens),
            "trailingTokens": js_num(estimate.trailing_tokens),
            "lastUsageIndex": estimate.last_usage_index.map(js_num),
        })
    );
}

#[test]
fn differential_last_assistant_usage_skips_errors_aborts_and_zero_usage() {
    let mut oracle = Oracle::new();
    let first = assistant_message("valid", usage(10.0, 3.0, 2.0, 1.0));
    let zero = assistant_message("zero", usage(0.0, 0.0, 0.0, 0.0));
    let mut aborted = assistant_message("aborted", usage(99.0, 99.0, 0.0, 0.0));
    aborted["stopReason"] = json!("aborted");
    let mut failed = aborted.clone();
    failed["stopReason"] = json!("error");
    let cases = [
        json!([]),
        json!([message_entry("1", None, user_message("only user"))]),
        json!([
            message_entry("1", None, first),
            message_entry("2", Some("1"), aborted),
            message_entry("3", Some("2"), failed),
            message_entry("4", Some("3"), zero)
        ]),
    ];
    for case in cases {
        let expected = oracle.request(COMPACTION, "lastAssistantUsage", json!({"entries": case}));
        assert_eq!(json!(get_last_assistant_usage(&entries(&case))), expected);
    }
}

#[test]
fn differential_should_compact() {
    let mut oracle = Oracle::new();
    let cases = json!([
        { "contextTokens": 95000, "contextWindow": 100000, "settings": { "enabled": true, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 89000, "contextWindow": 100000, "settings": { "enabled": true, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 271999, "contextWindow": 372000, "settings": { "enabled": true, "reserveTokens": 16384, "keepRecentTokens": 20000 }, "threshold": 272000 },
        { "contextTokens": 272000, "contextWindow": 372000, "settings": { "enabled": true, "reserveTokens": 16384, "keepRecentTokens": 20000 }, "threshold": 272000 },
        { "contextTokens": 95000, "contextWindow": 100000, "settings": { "enabled": false, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 49999, "contextWindow": 100000, "settings": { "thresholdPercent": 50, "enabled": true, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 50000, "contextWindow": 100000, "settings": { "thresholdPercent": 50, "enabled": true, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 50000, "contextWindow": 100000, "threshold": 90000, "settings": { "thresholdPercent": 50, "enabled": true, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 50000, "contextWindow": 100000, "threshold": 10000, "settings": { "thresholdPercent": 75, "enabled": true, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
        { "contextTokens": 100000, "contextWindow": 100000, "settings": { "thresholdPercent": 50, "enabled": false, "reserveTokens": 10000, "keepRecentTokens": 20000 } },
    ]);
    let expected = oracle.request(COMPACTION, "shouldCompact", json!({ "cases": cases }));
    let actual: Vec<Value> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            json!(
                should_compact(
                    case["contextTokens"].as_f64().unwrap(),
                    case["contextWindow"].as_f64().unwrap(),
                    &settings(&case["settings"]),
                    case.get("threshold").and_then(Value::as_f64),
                )
                .expect("valid threshold")
            )
        })
        .collect();
    assert_eq!(actual, *expected.as_array().unwrap());
}

#[test]
fn differential_percentage_validation_precedes_disabled_flag() {
    let mut oracle = Oracle::new();
    let cases: Vec<Value> = [0.0, -1.0, 101.0, 0.5, 100.0]
        .into_iter()
        .flat_map(|percent| {
            [true, false].into_iter().map(move |enabled| json!({
            "contextTokens": 100000, "contextWindow": 100000,
            "settings": {"thresholdPercent": percent, "enabled": enabled, "reserveTokens": 0, "keepRecentTokens": 1}
        }))
        })
        .collect();
    let expected = oracle.request(COMPACTION, "shouldCompactResults", json!({"cases": cases}));
    let actual: Vec<Value> = cases
        .iter()
        .map(
            |case| match should_compact(100000.0, 100000.0, &settings(&case["settings"]), None) {
                Ok(value) => json!({"value": value}),
                Err(error) => json!({"error": {"name": "Error", "message": error.message()}}),
            },
        )
        .collect();
    assert_eq!(json!(actual), expected);
}

#[test]
fn differential_find_cut_point_and_turn_start() {
    let mut oracle = Oracle::new();
    let cases = json!([
        { "entries": linear_entries(), "startIndex": 0, "endIndex": 12, "keepRecentTokens": 2500 },
        { "entries": linear_entries(), "startIndex": 0, "endIndex": 12, "keepRecentTokens": 50000 },
        {
            "entries": [
                message_entry("1", None, user_message("old user")),
                message_entry("2", Some("1"), assistant_message("old assistant", usage(0.0, 0.0, 0.0, 0.0))),
                message_entry("3", Some("2"), user_message("latest user must stay verbatim")),
                message_entry("4", Some("3"), assistant_message(&"x".repeat(40_000), usage(0.0, 0.0, 0.0, 0.0))),
            ],
            "startIndex": 0,
            "endIndex": 4,
            "keepRecentTokens": 20000,
        },
        {
            "entries": [
                message_entry("1", None, user_message("old user")),
                message_entry("2", Some("1"), assistant_message("old assistant", usage(0.0, 0.0, 0.0, 0.0))),
                entry("3", Some("2"), "2026-01-01T00:00:00.000Z", json!({ "type": "custom_message", "customType": "test", "content": "new turn", "display": true })),
                message_entry("4", Some("3"), assistant_message("part one", usage(0.0, 0.0, 0.0, 0.0))),
                message_entry("5", Some("4"), assistant_message("part two", usage(0.0, 0.0, 0.0, 0.0)))
            ], "startIndex": 0, "endIndex": 5, "keepRecentTokens": 20000
        },
        {
            "entries": [
                entry("1", None, "2026-01-01T00:00:00.000Z", json!({ "type": "model_change", "provider": "openai", "modelId": "gpt-4" })),
                message_entry("2", Some("1"), user_message("hi")),
                message_entry("3", Some("2"), assistant_message("hello", usage(0.0, 0.0, 0.0, 0.0))),
                entry("4", Some("3"), "2026-01-01T00:00:00.000Z", json!({ "type": "custom_message", "customType": "test", "content": "x".repeat(4000), "display": true })),
                message_entry("5", Some("4"), assistant_message("ok", usage(0.0, 0.0, 0.0, 0.0))),
            ],
            "startIndex": 0,
            "endIndex": 5,
            "keepRecentTokens": 1,
        },
    ]);

    for case in cases.as_array().unwrap() {
        let expected = oracle.request(
            COMPACTION,
            "findCutPoint",
            json!({
                "entries": case["entries"],
                "startIndex": case["startIndex"],
                "endIndex": case["endIndex"],
                "keepRecentTokens": case["keepRecentTokens"],
            }),
        );
        let parsed = entries(&case["entries"]);
        let result = find_cut_point(
            &parsed,
            case["startIndex"].as_u64().unwrap() as usize,
            case["endIndex"].as_u64().unwrap() as usize,
            case["keepRecentTokens"].as_f64().unwrap(),
        );
        assert_eq!(
            expected,
            json!({
                "firstKeptEntryIndex": result.first_kept_entry_index,
                "turnStartIndex": result.turn_start_index,
                "isSplitTurn": result.is_split_turn,
            }),
            "case {case}"
        );
    }

    let entries_value = linear_entries();
    let parsed = entries(&entries_value);
    for (entry_index, start_index) in [(11, 0), (0, 1), (7, 4)] {
        let expected = oracle.request(
            COMPACTION,
            "findTurnStartIndex",
            json!({ "entries": entries_value, "entryIndex": entry_index, "startIndex": start_index }),
        );
        assert_eq!(
            expected,
            json!(find_turn_start_index(&parsed, entry_index, start_index))
        );
    }
}

#[test]
fn differential_prepare_compaction() {
    let mut oracle = Oracle::new();
    let goal_reminders = json!([
        message_entry(
            "1",
            None,
            extension_user_message("Work toward this objective until it is achieved: fix compaction")
        ),
        message_entry("2", Some("1"), assistant_message("working", usage(0.0, 0.0, 0.0, 0.0))),
        message_entry(
            "3",
            Some("2"),
            extension_user_message("Extension context that must remain")
        ),
        message_entry(
            "4",
            Some("3"),
            assistant_message("still working", usage(0.0, 0.0, 0.0, 0.0))
        ),
        message_entry(
            "5",
            Some("4"),
            extension_user_message("Continue working toward this objective until it is achieved: fix compaction")
        ),
        message_entry(
            "6",
            Some("5"),
            assistant_message(&"x".repeat(40_000), usage(0.0, 0.0, 0.0, 0.0))
        ),
    ]);
    let repeated = json!([
        message_entry("1", None, user_message("user msg 1 (summarized by compaction1)")),
        message_entry(
            "2",
            Some("1"),
            assistant_message("assistant msg 1", usage(0.0, 0.0, 0.0, 0.0))
        ),
        message_entry("3", Some("2"), user_message("user msg 2 - kept by compaction1")),
        message_entry(
            "4",
            Some("3"),
            assistant_message("assistant msg 2", usage(0.0, 0.0, 0.0, 0.0))
        ),
        message_entry("5", Some("4"), user_message("user msg 3 - kept by compaction1")),
        message_entry(
            "6",
            Some("5"),
            assistant_message("assistant msg 3", usage(5000.0, 1000.0, 0.0, 0.0))
        ),
        compaction_entry("7", Some("6"), "First summary", "3"),
        message_entry("8", Some("7"), user_message("user msg 4 (new after compaction1)")),
        message_entry(
            "9",
            Some("8"),
            assistant_message("assistant msg 4", usage(8000.0, 2000.0, 0.0, 0.0))
        ),
    ]);
    let file_ops = json!([
        message_entry("1", None, user_message("start")),
        message_entry(
            "2",
            Some("1"),
            json!({
                "role": "assistant",
                "content": [
                    { "type": "toolCall", "id": "a", "name": "read", "arguments": { "path": "a.ts" } },
                    { "type": "toolCall", "id": "b", "name": "write", "arguments": { "path": "b.ts" } },
                    { "type": "toolCall", "id": "c", "name": "edit", "arguments": { "path": "c.ts" } },
                ],
                "api": "anthropic-messages",
                "provider": "anthropic",
                "model": "m",
                "usage": usage(0.0, 0.0, 0.0, 0.0),
                "stopReason": "stop",
                "timestamp": 0,
            }),
        ),
        compaction_entry("3", Some("2"), "Earlier summary", "1"),
        message_entry("4", Some("3"), user_message(&"y".repeat(40_000))),
        message_entry(
            "5",
            Some("4"),
            assistant_message(&"z".repeat(40_000), usage(0.0, 0.0, 0.0, 0.0))
        ),
    ]);

    let cases = [
        json!({ "entries": linear_entries(), "settings": DEFAULT_COMPACTION_SETTINGS }),
        json!({ "entries": linear_entries(), "settings": {"thresholdPercent": 37.5, "enabled": true, "reserveTokens": 16384, "keepRecentTokens": 20000} }),
        json!({ "entries": goal_reminders, "settings": DEFAULT_COMPACTION_SETTINGS }),
        json!({ "entries": repeated, "settings": DEFAULT_COMPACTION_SETTINGS }),
        json!({ "entries": repeated, "settings": { "enabled": true, "reserveTokens": 16384, "keepRecentTokens": 100 } }),
        json!({ "entries": file_ops, "settings": DEFAULT_COMPACTION_SETTINGS }),
        json!({ "entries": [message_entry("1", None, user_message("short request")), message_entry("2", Some("1"), assistant_message("short response", usage(0.0, 0.0, 0.0, 0.0)))], "settings": DEFAULT_COMPACTION_SETTINGS }),
    ];

    for case in cases {
        let expected = oracle.request(
            COMPACTION,
            "prepareCompaction",
            json!({ "entries": case["entries"], "settings": case["settings"] }),
        );
        let parsed = entries(&case["entries"]);
        let actual = match prepare_compaction(&parsed, settings(&case["settings"])) {
            Some(preparation) => {
                let file_ops = preparation.file_ops;
                json!({
                    "firstKeptEntryId": preparation.first_kept_entry_id,
                    "messagesToSummarize": preparation.messages_to_summarize,
                    "turnPrefixMessages": preparation.turn_prefix_messages,
                    "isSplitTurn": preparation.is_split_turn,
                    "tokensBefore": js_num(preparation.tokens_before),
                    "previousSummary": preparation.previous_summary,
                    "fileOps": {
                        "read": file_ops.read,
                        "written": file_ops.written,
                        "edited": file_ops.edited,
                    },
                    "settings": settings_value(&preparation.settings),
                })
            }
            None => Value::Null,
        };
        assert_eq!(actual, expected, "case {case}");
    }
}

#[test]
fn differential_serialize_conversation_and_file_lists() {
    let mut oracle = Oracle::new();
    let messages = json!([
        user_message("hello"),
        { "role": "user", "content": [{ "type": "text", "text": "a" }, { "type": "text", "text": "b" }], "timestamp": 0 },
        assistant_message("visible answer", usage(0.0, 0.0, 0.0, 0.0)),
        {
            "role": "assistant",
            "content": [
                { "type": "thinking", "thinking": "hidden" },
                { "type": "text", "text": "text" },
                { "type": "toolCall", "id": "t1", "name": "write", "arguments": { "path": "large.txt", "content": "x".repeat(5000), "nested": { "2": 2.0, "1": 1.0 }, "2": 2.0, "1": 1.0 } }
            ],
            "api": "anthropic-messages",
            "provider": "anthropic",
            "model": "m",
            "usage": usage(0.0, 0.0, 0.0, 0.0),
            "stopReason": "stop",
            "timestamp": 0,
        },
        {
            "role": "toolResult",
            "toolCallId": "t1",
            "toolName": "read",
            "content": [{ "type": "text", "text": format!("{}\n[Showing lines 1-5000 of 5000. Full output: /tmp/pi-full-output.log]", "x".repeat(5000)) }],
            "details": { "truncation": { "truncated": true }, "fullOutputPath": "/tmp/pi-full-output.log" },
            "isError": false,
            "timestamp": 0,
        },
        {
            "role": "toolResult",
            "toolCallId": "t2",
            "toolName": "read",
            "content": [{ "type": "text", "text": "short" }],
            "isError": false,
            "timestamp": 0,
        },
    ]);

    let expected = oracle.request(COMPACTION, "serializeConversation", json!({ "messages": messages }));
    assert_eq!(
        JsValue::String(serialize_conversation(&llm_messages(&messages))),
        JsValue::from(expected)
    );

    let agent_calls = json!([
        {
            "role": "assistant",
            "content": [
                { "type": "toolCall", "id": "a", "name": "read", "arguments": { "path": "a.ts" } },
                { "type": "toolCall", "id": "b", "name": "write", "arguments": { "path": "b.ts" } },
                { "type": "toolCall", "id": "c", "name": "edit", "arguments": { "path": "c.ts" } },
                { "type": "toolCall", "id": "d", "name": "read", "arguments": { "path": "a.ts" } },
                { "type": "toolCall", "id": "e", "name": "bash", "arguments": { "command": "ls" } },
            ],
            "api": "anthropic-messages",
            "provider": "anthropic",
            "model": "m",
            "usage": usage(0.0, 0.0, 0.0, 0.0),
            "stopReason": "stop",
            "timestamp": 0,
        },
        { "role": "user", "content": "no ops", "timestamp": 0 },
    ]);
    let expected = oracle.request(UTILS, "fileLists", json!({ "messages": agent_calls }));
    let mut file_ops = create_file_ops();
    for message in agent_messages(&agent_calls) {
        extract_file_ops_from_message(&message, &mut file_ops);
    }
    let lists = compute_file_lists(&file_ops);
    assert_eq!(
        expected,
        json!({
            "read": file_ops.read,
            "written": file_ops.written,
            "edited": file_ops.edited,
            "readFiles": lists.read_files,
            "modifiedFiles": lists.modified_files,
            "formatted": format_file_operations(&lists.read_files, &lists.modified_files),
        })
    );
}

#[test]
fn differential_prepare_branch_entries() {
    let mut oracle = Oracle::new();
    let cases = json!([
        {
            "entries": [
                message_entry("1", None, user_message("explore")),
                message_entry("2", Some("1"), json!({
                    "role": "assistant",
                    "content": [{ "type": "toolCall", "id": "call-1", "name": "read", "arguments": { "path": "src/a.ts" } }],
                    "api": "anthropic-messages",
                    "provider": "anthropic",
                    "model": "m",
                    "usage": usage(0.0, 0.0, 0.0, 0.0),
                    "stopReason": "stop",
                    "timestamp": 0,
                })),
                message_entry("3", Some("2"), json!({
                    "role": "toolResult",
                    "toolCallId": "call-1",
                    "toolName": "read",
                    "content": [{ "type": "text", "text": "contents" }],
                    "isError": false,
                    "timestamp": 0,
                })),
            ],
            "tokenBudget": 0,
        },
        {
            "entries": [
                entry("1", None, "2026-01-01T00:00:00.000Z", json!({
                    "type": "branch_summary",
                    "fromId": "from",
                    "summary": "earlier branch",
                    "details": { "readFiles": ["old-read.ts"], "modifiedFiles": ["old-edit.ts"] },
                })),
                entry("2", Some("1"), "2026-01-01T00:00:00.000Z", json!({
                    "type": "compaction",
                    "summary": "compacted",
                    "firstKeptEntryId": "1",
                    "tokensBefore": 12,
                    "durationMs": 12.5,
                })),
            ],
            "tokenBudget": 0,
        },
        {
            "entries": [
                message_entry("1", None, user_message("old user")),
                message_entry("2", Some("1"), assistant_message("old assistant", usage(0.0, 0.0, 0.0, 0.0))),
                message_entry("3", Some("2"), user_message(&"x".repeat(40_000))),
                message_entry("4", Some("3"), assistant_message(&"x".repeat(40_000), usage(0.0, 0.0, 0.0, 0.0))),
                message_entry("5", Some("4"), user_message("latest user")),
            ],
            "tokenBudget": 20_000,
        },
        {
            "entries": [
                entry("1", None, "2026-01-01T00:00:00.000Z", json!({
                    "type": "branch_summary",
                    "fromId": "from",
                    "summary": "x".repeat(400),
                })),
                message_entry("2", Some("1"), user_message("a")),
            ],
            "tokenBudget": 10,
        },
    ]);

    for case in cases.as_array().unwrap() {
        let expected = oracle.request(
            COMPACTION,
            "prepareBranchEntries",
            json!({ "entries": case["entries"], "tokenBudget": case["tokenBudget"] }),
        );
        let preparation = prepare_branch_entries(&entries(&case["entries"]), case["tokenBudget"].as_f64().unwrap());
        let file_ops: FileOperations = preparation.file_ops;
        assert_eq!(
            expected,
            json!({
                "messages": preparation.messages,
                "fileOps": {
                    "read": file_ops.read,
                    "written": file_ops.written,
                    "edited": file_ops.edited,
                },
                "totalTokens": js_num(preparation.total_tokens),
            }),
            "case {case}"
        );
    }
}

#[test]
fn differential_collect_entries_for_branch_summary() {
    let mut oracle = Oracle::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("session.jsonl");
    fs::write(
        &path,
        concat!(
            "{\"type\":\"session\",\"version\":3,\"id\":\"s\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"cwd\":\"/tmp\"}\n",
            "{\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"root\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"2\",\"parentId\":\"1\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"common\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"3\",\"parentId\":\"2\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"abandoned 1\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"4\",\"parentId\":\"3\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"abandoned 2\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"5\",\"parentId\":\"2\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"target\",\"timestamp\":0}}\n",
        ),
    )
    .expect("write session");

    let expected = oracle.request(
        COMPACTION,
        "collectEntries",
        json!({ "sessionPath": path.display().to_string(), "oldLeafId": "4", "targetId": "5", "module2": "core/session-manager.ts" }),
    );
    let session = SessionManager::open(&JsString::from(&path.display().to_string()), None, None).expect("open session");
    let result = collect_entries_for_branch_summary(&session, Some(&JsString::from("4")), &JsString::from("5"));
    assert_eq!(
        expected,
        json!({
            "entries": result.entries,
            "commonAncestorId": result.common_ancestor_id,
        })
    );
}
