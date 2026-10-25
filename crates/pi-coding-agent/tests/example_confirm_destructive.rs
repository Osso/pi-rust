#[path = "../examples/extensions/confirm_destructive.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::{Value, json};

#[tokio::test]
async fn switches_forks_ui_gating_and_dialog_errors_match_ts() {
    for mode in ["tui", "json"] {
        let actual = harness::compare("confirm-destructive", harness::factory(example::confirm_destructive_extension), json!({"mode":mode,"noUi":mode == "json","dialogs":[false,true,false,true,{"error":"dialog failed"},null,"No, stay in current session","Yes, create fork"],"steps":[
            {"op":"emit","event":{"type":"session_before_switch","reason":"new"}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"new"}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"resume"}},
            {"op":"append","message":{"role":"user","content":"unsaved","timestamp":0}},
            {"op":"append","message":{"role":"assistant","content":[{"type":"text","text":"already answered"}],"api":"faux","provider":"faux","model":"faux-1","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":0}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"resume"}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"resume"}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"new"}},
            {"op":"emit","event":{"type":"session_before_fork","entryId":"abcdefghijk","position":"before"}},
            {"op":"emit","event":{"type":"session_before_fork","entryId":"short","position":"at"}},
            {"op":"emit","event":{"type":"session_before_fork","entryId":"123456789","position":"before"}}
        ]})).await;
        if mode == "json" {
            assert!(
                actual["trace"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| *v == json!(["result", Value::Null]))
            );
        } else {
            assert_eq!(actual["trace"][2], json!(["result",{"cancel":true}]));
            assert!(
                actual["trace"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(["handlerError", "dialog failed"]))
            );
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
