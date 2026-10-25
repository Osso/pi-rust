#[path = "../examples/extensions/titlebar_spinner.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test(start_paused = true)]
async fn deterministic_frames_wrap_restart_and_shutdown_match_ts() {
    for name in [None, Some("session 🚀")] {
        let mut steps = vec![
            json!({"op":"event","event":{"type":"agent_end","messages":[]}}),
            json!({"op":"event","event":{"type":"agent_start"}}),
        ];
        for _ in 0..12 {
            steps.push(json!({"op":"advance","ms":80}));
        }
        steps.extend([
            json!({"op":"name","value":"renamed 🚀"}),
            json!({"op":"event","event":{"type":"agent_start"}}),
            json!({"op":"advance","ms":80}),
            json!({"op":"event","event":{"type":"agent_end","messages":[]}}),
            json!({"op":"advance","ms":800}),
            json!({"op":"name","value":""}),
            json!({"op":"event","event":{"type":"agent_start"}}),
            json!({"op":"advance","ms":80}),
            json!({"op":"event","event":{"type":"session_shutdown","reason":"quit"}}),
            json!({"op":"advance","ms":800}),
        ]);
        let actual = harness::compare(
            "titlebar-spinner",
            harness::factory(example::titlebar_spinner_extension),
            json!({"name":name,"timers":true,"processCwd":std::env::current_dir().unwrap(),"steps":steps}),
        )
        .await;
        assert_eq!(actual["trace"].as_array().unwrap().len(), 20);
        assert!(actual["trace"][2][1].as_str().unwrap().starts_with("⠋ π - "));
        assert_eq!(actual["trace"][2], actual["trace"][12]);
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
