#[path = "../examples/extensions/widget_placement.rs"]
mod example;
#[path = "support/extensions.rs"]
mod extension_support;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn pinned_example_matches_runtime_effects() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::widget_placement_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare(
        "widget-placement",
        factory,
        json!({"cases":[
            {"event":"session_start","hasUI":false},
            {"event":"session_start"},
            {"event":"session_start"}
        ]}),
    )
    .await;
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":[]}));
    assert_eq!(actual["cases"][0]["ui"], json!([]));
    assert_eq!(
        actual["cases"][1]["ui"],
        json!([
            ["widget","widget-above",["Above editor widget"],null],
            ["widget","widget-below",["Below editor widget"],{"placement":"belowEditor"}]
        ])
    );
    assert_eq!(actual["cases"][1], actual["cases"][2]);
}
