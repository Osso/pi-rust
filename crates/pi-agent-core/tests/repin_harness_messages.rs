use crate::shared_oracle as oracle;
use pi_agent_core::{AgentMessage, harness::messages::*};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, from_value, to_value},
};
fn raw(unit: u16) -> JsString {
    JsString::from_utf16(vec![unit])
}
#[test]
fn frozen_message_builders_and_conversions_preserve_native_content_and_dates() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-messages-oracle.ts",
        "packages/agent-core/src/harness/messages.ts",
    );
    for timestamp in [
        JsString::from("2026-10-22T00:00:00.125Z"),
        "1970-01-01T00:00:00Z".into(),
        "invalid".into(),
        raw(0xd800),
    ] {
        for details in [None, Some(JsValue::Null), Some(json!({raw(0xd800):[raw(0xdc00),null]}))] {
            let options = CompactionSummaryOptions {
                duration_ms: Some(1.25),
                tokens_after: Some(4.5),
                kept_from_previous_context_tokens: Some(8.75),
                compacted_result_tokens: Some(9.25),
                provider_native: Some(
                    json!({"api":"openai-responses","provider":"openai","format":"openai.responses.input","value":{"native":raw(0xd800)}}),
                ),
            };
            let options_value = json!({"durationMs":1.25,"tokensAfter":4.5,"keptFromPreviousContextTokens":8.75,"compactedResultTokens":9.25,"providerNative":options.provider_native});
            let mut input = json!({"op":"builders","summary":raw(0xd800),"id":raw(0xdc00),"timestamp":timestamp,"tokens":12.5,"options":options_value,"customType":"custom-type","content":[{"type":"text","text":raw(0xdc00)},{"type":"image","data":"AA==","mimeType":"image/png"}],"display":true});
            if let Some(details) = &details {
                input["details"] = details.clone();
            }
            let expected: JsValue = oracle.request(&input);
            let content: CustomMessageContent = from_value(&input["content"]).unwrap();
            let created = vec![
                create_branch_summary_message(raw(0xd800), &raw(0xdc00), timestamp.clone()),
                create_compaction_summary_message(raw(0xd800), 12.5, timestamp.clone(), &options),
                create_custom_message(&"custom-type".into(), content, true, details, timestamp.clone()),
            ];
            let actual = json!({"created":to_value(&created).unwrap(),"converted":to_value(&convert_to_llm(&created).unwrap()).unwrap(),"markers":[COMPACTION_SUMMARY_PREFIX,COMPACTION_SUMMARY_SUFFIX,BRANCH_SUMMARY_PREFIX,BRANCH_SUMMARY_SUFFIX]});
            assert_eq!(actual, expected);
        }
    }
}
#[test]
fn frozen_bash_message_rendering_keeps_js_numbers_and_cancel_precedence() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-messages-oracle.ts",
        "packages/agent-core/src/harness/messages.ts",
    );
    for numeric in ["0", "-0", "1", "-2", "4.25", "NaN", "Infinity", "-Infinity"] {
        for cancelled in [false, true] {
            let code = match numeric {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => numeric.parse().unwrap(),
            };
            let message = BashExecutionMessage {
                role: BashExecutionRole::BashExecution,
                command: raw(0xd800),
                output: raw(0xdc00),
                exit_code: Some(code),
                cancelled,
                truncated: true,
                full_output_path: Some(raw(0xd800)),
                timestamp: 0.0,
                exclude_from_context: None,
            };
            let expected: JsValue = oracle.request(&json!({"op":"bash","numeric":numeric,"message":{"role":"bashExecution","command":message.command,"output":message.output,"exitCode":0,"cancelled":cancelled,"truncated":true,"fullOutputPath":message.full_output_path,"timestamp":0}}));
            assert_eq!(json!(bash_execution_to_text(&message)), expected);
        }
    }
}
#[test]
fn frozen_conversion_drops_only_excluded_bash_and_unknown_roles() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-messages-oracle.ts",
        "packages/agent-core/src/harness/messages.ts",
    );
    let messages = json!([{ "role":"user","content":raw(0xd800),"timestamp":1.25},{"role":"bashExecution","command":"cmd","output":"","exitCode":null,"cancelled":false,"truncated":false,"timestamp":2.25},{"role":"bashExecution","command":"hidden","output":"","cancelled":true,"truncated":false,"excludeFromContext":true,"timestamp":3.25},{"role":"custom","customType":"plain","content":raw(0xdc00),"display":false,"timestamp":4.25},{"role":"unknown","native":raw(0xd800),"timestamp":5.25}]);
    let expected: JsValue = oracle.request(&json!({"op":"convert","messages":messages}));
    let messages: Vec<AgentMessage> = from_value(&messages).unwrap();
    assert_eq!(to_value(&convert_to_llm(&messages).unwrap()).unwrap(), expected);
}
