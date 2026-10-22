//! Port of `packages/coding-agent/test/end-turn-tool.test.ts`.

#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;

use pi_coding_agent::core::extensions::ExtensionContext;
use pi_coding_agent::core::extensions::types::ContextTool;
use pi_coding_agent::core::tools::end_turn::{EndTurnTool, parameters};
use pi_js_compat::js_json;
use serde_json::json;

async fn context() -> ExtensionContext {
    let session = tool_context::in_memory_session("/repo", "session");
    tool_context::TestContextBuilder::new("/repo", session).build().await.1
}

#[test]
fn schema_requires_a_reason() {
    assert_eq!(parameters()["required"], json!(["reason"]));
    assert_eq!(parameters()["properties"]["reason"]["type"], "string");
}

#[tokio::test]
async fn rejects_a_blank_reason() {
    let error = EndTurnTool::new()
        .execute(
            "end-blank".to_owned(),
            pi_js_compat::js_json!({ "reason": "  " }),
            None,
            None,
            context().await,
            None,
        )
        .await
        .expect_err("blank reason must fail");
    assert_eq!(error.message, "end_turn reason must be a non-empty string");
}

#[tokio::test]
async fn terminates_with_its_required_reason() {
    let result = EndTurnTool::new()
        .execute(
            "end-valid".to_owned(),
            pi_js_compat::js_json!({ "reason": "Finished requested work" }),
            None,
            None,
            context().await,
            None,
        )
        .await
        .expect("valid end_turn");

    assert_eq!(result.content.len(), 1);
    assert_eq!(
        js_json::to_value(&result.content).expect("content serializes"),
        pi_js_compat::js_json!([{ "type": "text", "text": "Turn ended: Finished requested work" }])
    );
    assert_eq!(
        result.details,
        Some(pi_js_compat::js_json!({ "reason": "Finished requested work" }))
    );
    assert_eq!(result.terminate, Some(true));
    assert_eq!(result.is_error, None);
}

#[tokio::test]
async fn raw_reasons_match_the_frozen_end_turn_result_without_normalization() {
    let cases = js_json::parse_lossless(
        r#"[{"reason":" before-\ud800-after "},{"reason":"\udfff"},{"reason":"pair-\ud83d\ude00"}]"#,
    )
    .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-end-turn-oracle.ts",
        "packages/coding-agent/src/core/tools/end-turn.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"id": 1, "cases": cases}));
    assert_eq!(expected["ok"], pi_js_compat::js_json!(true));
    let cases = cases.as_array().unwrap();
    let results = expected["output"].as_array().unwrap();
    assert_eq!(cases.len(), results.len());
    for (params, expected) in cases.iter().zip(results) {
        let result = EndTurnTool::new()
            .execute("raw-reason".into(), params.clone(), None, None, context().await, None)
            .await
            .expect("native reason must execute");
        assert_eq!(js_json::to_value(&result).unwrap(), *expected);
    }
}
