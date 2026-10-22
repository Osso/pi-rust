use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::{
    BashExecutionMessage, BashExecutionRole, CustomMessageContent, bash_execution_to_text, convert_to_llm,
    create_custom_message,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

use super::support::{TsOracle, expect_ok, oracle_request};

fn bash_messages() -> Vec<Value> {
    let base = json!({
        "role": "bashExecution",
        "command": "echo hi",
        "output": "hi\n",
        "exitCode": null,
        "cancelled": false,
        "truncated": false,
        "timestamp": 1
    });
    let mut fixtures = vec![base.clone()];
    let overrides = [
        json!({"command": "", "output": ""}),
        json!({"command": "printf 'x'", "output": "x", "exitCode": 0}),
        json!({"command": "false", "output": "", "exitCode": 1}),
        json!({"command": "false", "output": "", "exitCode": 17}),
        json!({"command": "false", "output": "", "exitCode": -2}),
        json!({"command": "sleep 1", "output": "partial", "cancelled": true, "exitCode": 130}),
        json!({"command": "sleep 1", "output": "", "cancelled": true}),
        json!({"command": "seq 5", "output": "1\n2\n3", "truncated": true, "fullOutputPath": "/tmp/full.log"}),
        json!({"command": "seq 5", "output": "1\n2\n3", "truncated": true}),
        json!({"command": "seq 5", "output": "", "truncated": true, "fullOutputPath": ""}),
        json!({"command": "echo ünïcode", "output": "ünïcode 😀\u{0}\r\n", "exitCode": 0}),
        json!({"command": "true", "output": "", "excludeFromContext": false}),
        json!({"command": "true", "output": "", "excludeFromContext": true}),
    ];
    for override_value in overrides {
        let mut message = base.clone();
        for (key, value) in override_value.as_object().unwrap() {
            message[key] = value.clone();
        }
        fixtures.push(message);
    }
    fixtures
}

fn bash_execution_message(value: &Value) -> BashExecutionMessage {
    BashExecutionMessage {
        role: BashExecutionRole::BashExecution,
        command: JsString::from(value["command"].as_str().expect("fixture command")),
        output: JsString::from(value["output"].as_str().expect("fixture output")),
        exit_code: value["exitCode"].as_f64(),
        cancelled: value["cancelled"].as_bool().unwrap_or(false),
        truncated: value["truncated"].as_bool().unwrap_or(false),
        full_output_path: value["fullOutputPath"].as_str().map(str::to_owned).map(JsString::from),
        timestamp: value["timestamp"].as_f64().unwrap_or_default(),
        exclude_from_context: value["excludeFromContext"].as_bool(),
    }
}

fn conversion_messages() -> Vec<Value> {
    vec![
        json!({"role": "user", "content": [{"type": "text", "text": "hello"}], "timestamp": 1}),
        json!({"role": "user", "content": "plain", "timestamp": 2}),
        json!({"role": "bashExecution", "command": "echo kept", "output": "kept\n", "exitCode": 0, "cancelled": false, "truncated": false, "timestamp": 3}),
        json!({"role": "bashExecution", "command": "echo hidden", "output": "", "exitCode": 1, "cancelled": false, "truncated": false, "timestamp": 4, "excludeFromContext": true}),
        json!({"role": "custom", "customType": "note", "content": "plain note", "display": false, "timestamp": 5}),
        json!({"role": "custom", "customType": "note", "content": [{"type": "text", "text": "block note"}], "display": true, "details": {"a": 1}, "timestamp": 6}),
        json!({"role": "branchSummary", "summary": "branch text", "fromId": "entry-1", "timestamp": 7}),
        json!({"role": "compactionSummary", "summary": "compacted", "tokensBefore": 10, "timestamp": 8}),
        json!({"role": "compactionSummary", "summary": "native", "tokensBefore": 10, "providerNative": {"provider": "openai", "api": "openai-responses", "format": "openai.responses.input", "value": {"id": "x"}}, "timestamp": 9}),
        json!({"role": "unknownRole", "timestamp": 10}),
        json!({"role": "assistant", "content": [{"type": "text", "text": "reply"}], "api": "openai-responses", "provider": "openai", "model": "test-model", "usage": {"input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 2, "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}}, "stopReason": "stop", "timestamp": 11}),
        json!({"role": "toolResult", "toolCallId": "call-1", "toolName": "tool", "content": [{"type": "text", "text": "result"}], "isError": false, "timestamp": 12}),
    ]
}

fn custom_messages() -> Vec<Value> {
    vec![
        json!({"customType": "note", "content": "plain", "display": true, "details": {"level": 2}, "timestamp": "1970-01-01T00:00:00.001Z"}),
        json!({"customType": "note", "content": [{"type": "text", "text": "hello"}], "display": false, "timestamp": "1970-01-01T00:00:00.002Z"}),
        json!({"customType": "note", "content": [{"type": "image", "data": "aGk=", "mimeType": "image/png"}], "display": true, "details": null, "timestamp": "1970-01-01T00:00:00.003Z"}),
        json!({"customType": "unicode-😀", "content": "ünïcode\u{0}\r\n", "display": true, "details": [1, "two", null], "timestamp": "2023-11-14T22:13:20.000Z"}),
    ]
}

fn custom_content(value: &Value) -> CustomMessageContent {
    if let Some(text) = value.as_str() {
        return CustomMessageContent::Text(JsString::from(text.to_owned()));
    }
    CustomMessageContent::Blocks(serde_json::from_value(value.clone()).expect("content blocks"))
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    for (index, message) in bash_messages().into_iter().enumerate() {
        let id = 14_100 + index as u64;
        let request = oracle_request(id, "bash_execution_to_text", json!({"bash": message}));
        let response = oracle.request(&request);
        let expected = expect_ok(&response);
        assert_eq!(
            json!(bash_execution_to_text(&bash_execution_message(&message))),
            *expected,
            "bash text fixture {id}: {message}"
        );
    }

    let messages = conversion_messages();
    let request = oracle_request(14_200, "convert_to_llm", json!({"messages": messages}));
    let response = oracle.request(&request);
    let expected = expect_ok(&response);
    let agent_messages: Vec<AgentMessage> = messages
        .iter()
        .map(|message| serde_json::from_value(message.clone()).expect("agent message"))
        .collect();
    let converted = convert_to_llm(&agent_messages).expect("convert fixture messages");
    let converted = serde_json::to_value(converted).expect("converted messages must serialize");
    assert_eq!(converted, *expected, "convert_to_llm fixture");

    for (index, message) in custom_messages().into_iter().enumerate() {
        let id = 14_300 + index as u64;
        let request = oracle_request(id, "create_custom_message", json!({"customMessage": message}));
        let response = oracle.request(&request);
        let expected = expect_ok(&response);
        let rust_message = create_custom_message(
            &JsString::from(message["customType"].as_str().unwrap()),
            custom_content(&message["content"]),
            message["display"].as_bool().unwrap(),
            message.get("details").cloned().map(JsValue::from),
            message["timestamp"].as_str().unwrap(),
        );
        let rust_value = serde_json::to_value(rust_message).expect("custom message must serialize");
        assert_eq!(rust_value, *expected, "create_custom_message fixture {id}");
    }
}
