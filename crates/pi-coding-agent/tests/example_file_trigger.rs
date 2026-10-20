#[path = "../examples/extensions/file_trigger.rs"]
mod example;
#[path = "support/extension_examples_a.rs"]
mod harness;
use pi_coding_agent::core::extensions::ExtensionFactory;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn private_watch_injects_clears_ignores_blank_and_preserves_failure_match_ts() {
    let _default = harness::factory(example::file_trigger_extension);
    for (no_ui, send_error, missing) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("trigger.txt");
        if !missing {
            std::fs::write(&path, "").unwrap();
        }
        let fixture = path.clone();
        let factory = move |_trace| {
            let path = fixture.clone();
            let factory: ExtensionFactory = Arc::new(move |api| {
                let result = example::file_trigger_with_path(api, path.clone());
                Box::pin(async move { result })
            });
            factory
        };
        let mut steps = vec![json!({"op":"event","event":{"type":"session_start","reason":"startup"}})];
        if !missing {
            if !send_error {
                steps.push(json!({"op":"trigger","content":" \t\n"}));
            }
            steps.push(json!({"op":"trigger","content":"\u{feff} Run tests 🚀 \u{2028}"}));
            if !send_error {
                steps.push(json!({"op":"trigger","content":"second"}));
            }
        }
        let actual = harness::compare("file-trigger",factory,json!({"fileLoader":true,"noUi":no_ui,"triggerPath":path,"sendError":if send_error {Some("delivery failed")} else {None},"steps":steps})).await;
        let trace = actual["trace"].as_array().unwrap();
        if missing {
            assert_eq!(trace[0][0], "handlerError");
        } else {
            assert!(trace.contains(&json!(["sendMessage",{"customType":"file-trigger","content":"External trigger: Run tests 🚀","display":true},{"triggerTurn":true}])));
            assert_eq!(
                trace.last().unwrap()[1],
                if send_error {
                    "\u{feff} Run tests 🚀 \u{2028}"
                } else {
                    ""
                }
            );
        }
    }
}
