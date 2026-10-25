#[path = "../examples/extensions/widget_placement.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
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

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
