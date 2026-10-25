#[path = "../examples/extensions/system_prompt_header.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn counts_utf16_of_current_effective_prompt_and_clears_on_shutdown() {
    let actual = harness::compare(
        "system-prompt-header",
        harness::factory(example::system_prompt_header_extension),
        json!({"steps":[
            {"op":"event","event":{"type":"agent_start"}},
            {"op":"prompt","value":"A🚀𐀀\ne\u{301}"},
            {"op":"event","event":{"type":"agent_start"}},
            {"op":"prompt","value":"changed"},
            {"op":"event","event":{"type":"agent_start"}},
            {"op":"event","event":{"type":"session_shutdown","reason":"quit"}}
        ]}),
    )
    .await;
    assert_eq!(
        actual["trace"],
        json!([
            ["status", "system-prompt", "System: 0 chars"],
            ["status", "system-prompt", "System: 8 chars"],
            ["status", "system-prompt", "System: 7 chars"],
            ["status", "system-prompt", null]
        ])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
