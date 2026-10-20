#[path = "../examples/extensions/custom_compaction.rs"]
mod example;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;
fn event(previous: Value, aborted: bool) -> Value {
    json!({"event":"event","aborted":aborted,"value":{"type":"compaction","reason":"manual","willRetry":false,"branchEntries":[],"preparation":{"firstKeptEntryId":"keep-42","messagesToSummarize":[{"role":"user","content":"old request","timestamp":1}],"turnPrefixMessages":[{"role":"user","content":[{"type":"text","text":"prefix λ🚀"}],"timestamp":2}],"tokensBefore":12345,"previousSummary":previous,"isSplitTurn":true,"fileOps":{"read":[],"written":[],"edited":[]},"settings":{"enabled":true,"reserveTokens":16384,"keepRecentTokens":20000}}}})
}
#[tokio::test]
async fn custom_compaction_full_result_prompt_options_usage_and_failures_match() {
    for (fixture, previous, aborted) in [
        (
            json!({"content":[{"type":"text","text":"summary one"},{"type":"thinking","thinking":"hidden"},{"type":"text","text":"summary two"}]}),
            json!("prior state"),
            false,
        ),
        (json!({"content":[{"type":"text","text":"  \n"}]}), Value::Null, false),
        (json!({"content":[]}), json!(""), true),
        (json!({"error":"private provider failure"}), Value::Null, false),
        (json!({"missing":true}), Value::Null, false),
    ] {
        let factory: ExtensionFactory = Arc::new(|api| {
            let result = example::custom_compaction_extension(api);
            Box::pin(async move { result })
        });
        let actual = support::compare(
            "custom-compaction",
            factory,
            json!({"modelFixture":fixture,"cases":[event(previous,aborted)]}),
        )
        .await;
        assert_eq!(actual["errors"], json!([]));
    }
}
