use crate::shared_oracle as oracle;
use pi_agent_core::AgentMessage;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::session::{JsonlSessionStorage, Session, SessionStorage};
use pi_js_compat::{
    js_json as json,
    js_json::{JsObject, JsString, JsValue, parse_lossless, stringify_lossless},
    number::js_number_to_string,
};
use std::sync::Arc;

fn collect_number_tags(value: &JsValue, path: &str, tags: &mut JsObject) {
    match value {
        JsValue::Number(value) => {
            let text = if *value == 0.0 && value.is_sign_negative() {
                "-0".to_owned()
            } else {
                js_number_to_string(*value)
            };
            tags.insert(path.into(), JsValue::String(text.into()));
        }
        JsValue::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_number_tags(item, &format!("{path}[{index}]"), tags);
            }
        }
        JsValue::Object(items) => {
            for (key, item) in items {
                collect_number_tags(item, &format!("{path}.{}", key.to_utf8().unwrap()), tags);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn jsonl_typed_message_projection_preserves_nonfinite_negative_zero_and_native_text() {
    let header =
        r#"{"type":"session","version":3,"id":"fixture","timestamp":"2023-11-14T22:13:20.000Z","cwd":"/fixture"}"#;
    let user = r#"{"type":"message","id":"u","parentId":null,"timestamp":"2023-11-14T22:13:20.000Z","message":{"role":"user","content":"hi","timestamp":1e999}}"#;
    let assistant = r#"{"type":"message","id":"a","parentId":"u","timestamp":"2023-11-14T22:13:20.000Z","message":{"role":"assistant","content":[{"type":"text","text":"\ud800"}],"api":"faux","provider":"fixture","model":"fixture","usage":{"input":1e999,"output":-1e999,"cacheRead":-0,"cacheWrite":3.5,"totalTokens":1e999,"cost":{"input":1e999,"output":-1e999,"cacheRead":-0,"cacheWrite":0.5,"total":1e999}},"stopReason":"stop","timestamp":-1e999}}"#;
    let tool = r#"{"type":"message","id":"t","parentId":"a","timestamp":"2023-11-14T22:13:20.000Z","message":{"role":"toolResult","toolCallId":"call","toolName":"read","content":[{"type":"text","text":"done"}],"details":{"positive":1e999,"negative":-1e999,"zero":-0},"isError":false,"timestamp":-0}}"#;
    let custom = r#"{"type":"custom_message","id":"c","parentId":"t","timestamp":"2023-11-14T22:13:20.000Z","customType":"note","content":[{"type":"text","text":"\udfff"}],"display":true,"details":{"zero":-0,"positive":1e999}}"#;
    let compaction = r#"{"type":"compaction","id":"compact","parentId":"c","timestamp":"2023-11-14T22:13:20.000Z","summary":"summary","firstKeptEntryId":"u","tokensBefore":1e999,"durationMs":-0}"#;
    let mut oracle = oracle::TsOracle::spawn(
        "tree-session-oracle.ts",
        "packages/agent-core/src/harness/session/session.ts",
    );
    for suffix in [
        String::new(),
        format!("\n{custom}"),
        format!("\n{custom}\n{compaction}"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let file = format!("{header}\n{user}\n{assistant}\n{tool}{suffix}\n");
        let input = json!({"kind":"numericFile","mode":"jsonl","root":root.path(),"fileContent":file});
        let envelope = oracle.request(&serde_json::json!({"wire":stringify_lossless(&input)}));
        let expected = parse_lossless(envelope["wire"].as_str().unwrap()).unwrap();
        assert_eq!(
            expected["value"]["numberTags"][".entries[1].message.timestamp"],
            json!("-Infinity")
        );
        let path = root.path().join("session.jsonl");
        std::fs::write(&path, &file).unwrap();
        let fs = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
            cwd: root.path().to_str().unwrap().into(),
            ..Default::default()
        }));
        let storage = Arc::new(
            JsonlSessionStorage::open(fs, path.to_str().unwrap().into())
                .await
                .unwrap(),
        );
        let entries = storage.get_entries().await.unwrap();
        assert!(
            entries[1].as_message().is_some(),
            "typed assistant projection must not silently disappear"
        );
        let mut tags = JsObject::default();
        let native_entries = JsValue::Array(entries.iter().map(|entry| entry.value()).collect());
        collect_number_tags(&native_entries, ".entries", &mut tags);
        let context = Session::new(storage).build_context().await.unwrap();
        for message in &context.messages {
            match message {
                AgentMessage::User(message) => assert_eq!(message.timestamp, f64::INFINITY),
                AgentMessage::Assistant(message) => {
                    assert_eq!(message.timestamp, f64::NEG_INFINITY);
                    assert_eq!(message.usage.input, f64::INFINITY);
                    assert!(message.usage.cache_read.is_sign_negative());
                }
                AgentMessage::ToolResult(message) => assert!(message.timestamp.is_sign_negative()),
                AgentMessage::Custom(_) => {}
            }
        }
        let context = json!({"messages":context.messages,"thinkingLevel":context.thinking_level,"model":context.model.map(|model|json!({"provider":model.provider,"modelId":model.model_id})),"activeToolNames":context.active_tool_names});
        let mut snapshot = json!({"entries":entries,"context":context});

        assert_eq!(
            tags[&JsString::from(".entries[1].message.timestamp")],
            json!("-Infinity")
        );
        snapshot["numberTags"] = JsValue::Object(tags);
        // Observe the same JSON output boundary while tags retain the pre-serialization number domain.
        let actual = parse_lossless(JsString::from(stringify_lossless(&json!({"value":snapshot})))).unwrap();
        assert_eq!(actual, expected);
    }
}
