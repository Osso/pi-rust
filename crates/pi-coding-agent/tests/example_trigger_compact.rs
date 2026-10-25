#[path = "../examples/extensions/trigger_compact.rs"]
mod example;
use crate::shared_extension_examples_e as harness;
use serde_json::json;

#[tokio::test]
async fn threshold_crossings_unknown_usage_commands_and_callbacks_match() {
    let turn = json!({"op":"event","event":{"type":"turn_end","turnIndex":0,"message":{"role":"user","content":"fixture","timestamp":0},"toolResults":[]}});
    for outcome in ["complete", "error"] {
        let mut steps = Vec::new();
        for tokens in [
            None,
            Some(100001.0),
            Some(100002.0),
            Some(100000.0),
            Some(100001.0),
            Some(99999.0),
            None,
            Some(100002.0),
            Some(100003.0),
            Some(0.0),
            Some(100000.0),
            Some(100001.0),
        ] {
            steps.push(json!({"op":"usage","tokens":tokens}));
            steps.push(turn.clone());
        }
        steps.extend([
            json!({"op":"command","name":"trigger-compact","args":" \u{feff}"}),
            json!({"op":"command","name":"trigger-compact","args":"  keep λ context \u{2028}"}),
            json!({"op":"command","name":"trigger-compact","args":"private","hasUI":false}),
        ]);
        let result = harness::compare(
            "trigger-compact.ts",
            harness::factory(example::trigger_compact_extension),
            json!({"compactOutcome":outcome,"steps":steps}),
        )
        .await;
        let cases = result["cases"].as_array().unwrap();
        let calls = cases
            .iter()
            .filter(|case| !case["actions"].as_array().unwrap().is_empty())
            .count();
        assert_eq!(calls, 6);
        assert_eq!(cases.last().unwrap()["ui"], json!([]));
        assert_eq!(cases[25]["actions"], json!([["compact", "keep λ context"]]));
    }
}
