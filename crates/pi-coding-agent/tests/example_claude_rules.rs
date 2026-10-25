#[path = "../examples/extensions/claude_rules.rs"]
mod example;
use crate::shared_extension_examples_e as harness;
use serde_json::json;

#[tokio::test]
async fn rules_are_scanned_recursively_sorted_and_refreshed_only_at_session_start() {
    let result=harness::compare("claude-rules.ts",harness::factory(example::claude_rules_extension),json!({"steps":[
        {"op":"before"},
        {"op":"event","event":{"type":"session_start","reason":"startup"}},
        {"op":"before"},
        {"op":"write","files":{".claude/rules/z.md":"z",".claude/rules/a/b.md":"b",".claude/rules/A.md":"a",".claude/rules/not.MD":"ignored",".claude/rules/notes.txt":"ignored",".claude/rules/.hidden.md":"hidden"}},
        {"op":"before"},
        {"op":"event","event":{"type":"session_start","reason":"resume"}},
        {"op":"before","prompt":"base λ"},
        {"op":"write","files":{".claude/rules/z.md":null}},
        {"op":"before","prompt":"next"},
        {"op":"event","event":{"type":"session_start","reason":"startup"}},
        {"op":"before"}
    ]})).await;
    assert_eq!(result["cases"][2]["result"], serde_json::Value::Null);
    assert_eq!(
        result["cases"][5]["ui"],
        json!([["notify", "Found 4 rule(s) in .claude/rules/", "info"]])
    );
    assert!(
        result["cases"][8]["result"]["systemPrompt"]
            .as_str()
            .unwrap()
            .contains("z.md")
    );
    assert!(
        !result["cases"][10]["result"]["systemPrompt"]
            .as_str()
            .unwrap()
            .contains("z.md")
    );
}

#[tokio::test]
async fn scandir_errors_are_reported_with_node_messages() {
    let result=harness::compare("claude-rules.ts",harness::factory(example::claude_rules_extension),json!({"files":{".claude/rules":"not a directory"},"steps":[{"op":"event","event":{"type":"session_start","reason":"startup"}},{"op":"before"}]})).await;
    assert_eq!(result["errors"].as_array().unwrap().len(), 1);
}
