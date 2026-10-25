#[path = "../examples/extensions/file_trigger.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn private_watch_injects_clears_ignores_blank_and_preserves_failure_match_ts() {
    for (no_ui, send_error, missing) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        pi_js_compat::process_env::set("PI_TEST_FILE_TRIGGER_ROOT", temp.path().to_str().unwrap());
        let path = temp.path().join("tmp/agent-trigger.txt");
        std::fs::create_dir(path.parent().unwrap()).unwrap();
        if !missing {
            std::fs::write(&path, "").unwrap();
        }
        let factory = harness::factory(example::file_trigger_extension);
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
    pi_js_compat::process_env::clear();
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
