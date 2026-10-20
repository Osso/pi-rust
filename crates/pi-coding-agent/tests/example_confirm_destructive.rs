#[path = "../examples/extensions/confirm_destructive.rs"]
mod example;
#[path = "support/extension_examples_a.rs"]
mod harness;
use serde_json::{Value, json};

#[tokio::test]
async fn switches_forks_ui_gating_and_dialog_errors_match_ts() {
    for mode in ["tui", "json"] {
        let actual = harness::compare("confirm-destructive", harness::factory(example::confirm_destructive_extension), json!({"mode":mode,"noUi":mode == "json","dialogs":[false,true,false,true,{"error":"dialog failed"},null,"No, stay in current session","Yes, create fork"],"steps":[
            {"op":"emit","event":{"type":"session_before_switch","reason":"new"}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"new"}},
            {"op":"emit","event":{"type":"session_before_switch","reason":"resume"}},
            {"op":"append","message":{"role":"user","content":"unsaved","timestamp":0}},
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
