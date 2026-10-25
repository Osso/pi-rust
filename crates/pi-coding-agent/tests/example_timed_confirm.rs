#[path = "../examples/extensions/timed_confirm.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test(start_paused = true)]
async fn timed_dialog_options_results_abort_and_errors_match_ts() {
    let actual = harness::compare("timed-confirm", harness::factory(example::timed_confirm_extension), json!({"timers":true,"dialogs":[true,false,"Option B",null,"",true,false,"abort",{"error":"confirm failed"},{"error":"select failed"}],"steps":[
        {"op":"command","name":"timed"},{"op":"command","name":"timed"},
        {"op":"command","name":"timed-select"},{"op":"command","name":"timed-select"},{"op":"command","name":"timed-select"},
        {"op":"command","name":"timed-signal"},{"op":"command","name":"timed-signal"},{"op":"command","name":"timed-signal"},
        {"op":"command","name":"timed"},{"op":"command","name":"timed-select"}
    ]})).await;
    assert_eq!(actual["commands"].as_array().unwrap().len(), 3);
    let trace = actual["trace"].as_array().unwrap();
    assert!(trace.contains(&json!(["notify", "Dialog timed out (auto-cancelled)", "warning"])));
    assert!(trace.contains(&json!(["notify", "Cancelled by user", "info"])));
    assert!(trace.contains(&json!(["error", "confirm failed"])));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
