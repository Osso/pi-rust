#[path = "../examples/extensions/pirate.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn toggle_changes_only_effective_prompt_and_preserves_exact_suffix() {
    let request = json!({"steps":[
        {"op":"before","value":"Base 🚀\n"},
        {"op":"command","name":"pirate","args":"ignored"},
        {"op":"before","value":"Base 🚀\n"},
        {"op":"before","value":""},
        {"op":"command","name":"pirate"},
        {"op":"before","value":"fresh"}
    ]});
    let actual = harness::compare("pirate", harness::factory(example::pirate_extension), request).await;
    assert_eq!(actual["trace"][0], json!(["before", null]));
    assert_eq!(
        actual["trace"][1],
        json!(["notify", "Arrr! Pirate mode enabled!", "info"])
    );
    assert!(
        actual["trace"][2][1]["systemPrompt"]
            .as_str()
            .unwrap()
            .starts_with("Base 🚀\n\n\nIMPORTANT:")
    );
    assert_eq!(actual["trace"][2][1].as_object().unwrap().len(), 1);
    assert_eq!(actual["trace"][3][1].as_object().unwrap().len(), 1);
    assert_eq!(actual["trace"][5], json!(["before", null]));
    let fresh = harness::compare(
        "pirate",
        harness::factory(example::pirate_extension),
        json!({"steps":[{"op":"before","value":"new instance"}]}),
    )
    .await;
    assert_eq!(fresh["trace"], json!([["before", null]]));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
