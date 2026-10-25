#[path = "../examples/extensions/project_trust.rs"]
mod example;
use crate::shared_extension_examples_e as harness;
use serde_json::json;

#[tokio::test]
async fn all_choices_notes_cancellation_and_modes_match_complete_trust_results() {
    let mut steps = Vec::new();
    for mode in ["tui", "rpc", "json", "print"] {
        for choice in [
            None,
            Some("Trust and remember"),
            Some("Trust with note and remember"),
            Some("Trust this session"),
            Some("Do not trust this session"),
            Some("Let built-in prompt decide"),
        ] {
            steps.push(json!({"op":"trust","mode":mode,"selected":choice,"note":"λ demo note"}));
        }
    }
    for note in [None, Some("")] {
        steps.push(json!({"op":"trust","selected":"Trust with note and remember","note":note}));
    }
    steps.push(json!({"op":"trust","hasUI":false}));
    steps.push(json!({"op":"event","event":{"type":"session_start","reason":"startup"}}));
    let result = harness::compare(
        "project-trust.ts",
        harness::factory(example::project_trust_extension),
        json!({"steps":steps}),
    )
    .await;
    assert_eq!(result["cases"][1]["result"], json!({"trusted":"yes","remember":true}));
    assert_eq!(result["cases"][4]["result"], json!({"trusted":"no"}));
    assert_eq!(result["cases"][26]["result"], serde_json::Value::Null);
    assert_eq!(
        result["cases"][27]["ui"],
        json!([[
            "notify",
            "project-trust example loaded after trust resolution in <tmp>",
            "info"
        ]])
    );
}
