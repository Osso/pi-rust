#[path = "../examples/extensions/structured_output.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;
#[tokio::test]
async fn terminating_result_schema_prompts_and_rendered_lines_match() {
    let details = json!({"headline":"Final 🚀 report","summary":"A long summary that wraps across several lines at narrow widths.","actionItems":["Ship it","Check λ and 𐀀",""]});
    let mut steps = vec![
        json!({"op":"tool","name":"structured_output","params":details}),
        json!({"op":"tool","name":"structured_output","params":{"headline":"","summary":"","actionItems":[]}}),
    ];
    for width in [12, 40, 80] {
        for result in [
            json!({"content":[],"details":details}),
            json!({"content":[{"type":"text","text":"plain fallback"}]}),
            json!({"content":[]}),
            json!({"content":[{"type":"image","data":"fixture","mimeType":"image/png"}]}),
            json!({"content":[],"details":{"headline":"","summary":"","actionItems":[]}}),
        ] {
            steps.push(json!({"op":"renderTool","name":"structured_output","result":result,"width":width}));
        }
    }
    let actual = harness::compare(
        "structured-output",
        harness::factory(example::structured_output_extension),
        json!({"metadata":true,"steps":steps}),
    )
    .await;
    assert_eq!(actual["trace"][0][1]["terminate"], true);
    assert_eq!(actual["trace"][0][1]["details"], details);
    assert_eq!(
        actual["tools"][0][4],
        "Emit a final structured answer as a terminating tool result"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
