use crate::shared_oracle as oracle;

use std::time::Duration;

use pi_ai::utils::event_stream::{StreamFailure, event_stream};
use serde_json::{Value, json};

fn scenarios() -> Vec<Value> {
    vec![
        json!([{"op": "push", "value": 1}, {"op": "push", "value": 2}, {"op": "push", "value": -3}, {"op": "push", "value": 4}]),
        json!([{"op": "push", "value": 1}, {"op": "end", "result": "explicit"}, {"op": "push", "value": 2}]),
        json!([{"op": "push", "value": 1}, {"op": "end", "result": null}]),
        json!([{"op": "push", "value": 1}, {"op": "fail", "message": "boom"}, {"op": "push", "value": 2}]),
        json!([{"op": "fail", "message": "first"}, {"op": "end", "result": "late"}, {"op": "push", "value": -1}]),
        json!([{"op": "push", "value": -1}, {"op": "fail", "message": "ignored"}]),
        json!([{"op": "push", "value": -5}, {"op": "end", "result": "ignored"}]),
        json!([{"op": "end", "result": null}, {"op": "fail", "message": "ignored"}]),
    ]
}

async fn observe_rust(operations: &Value) -> Value {
    let (sender, mut stream) = event_stream(|event: &i64| *event < 0, |event: &i64| format!("final:{event}"));
    for operation in operations.as_array().expect("operations array") {
        match operation["op"].as_str().expect("op") {
            "push" => sender.push(operation["value"].as_i64().expect("value")),
            "end" => sender.end(operation["result"].as_str().map(str::to_owned)),
            "fail" => sender.fail(StreamFailure::new(operation["message"].as_str().expect("message"))),
            other => panic!("unknown op {other}"),
        }
    }
    drop(sender);
    let mut events = Vec::new();
    let mut failure = Value::Null;
    // The TypeScript scripts never end some streams; bound iteration like the oracle's result probe.
    while let Ok(Some(item)) = tokio::time::timeout(Duration::from_millis(20), stream.next()).await {
        match item {
            Ok(event) => events.push(event),
            Err(error) => failure = json!(error.message()),
        }
    }
    let result = match tokio::time::timeout(Duration::from_millis(20), stream.result()).await {
        Err(_) => json!({"state": "pending", "value": null}),
        Ok(Ok(value)) => json!({"state": "resolved", "value": value}),
        Ok(Err(error)) => json!({"state": "rejected", "value": error.message()}),
    };
    json!({"events": events, "failure": failure, "result": result})
}

#[tokio::test]
async fn scripted_streams_match_the_pinned_typescript_event_stream() {
    let mut oracle = oracle::TsOracle::spawn("event-stream-oracle.ts", "packages/ai/src/utils/event-stream.ts");
    for (id, operations) in scenarios().into_iter().enumerate() {
        let mut expected = oracle.request(&json!({"id": id, "operations": operations}));
        expected.as_object_mut().expect("response object").remove("id");
        assert_eq!(observe_rust(&operations).await, expected, "scenario {id}: {operations}");
    }
}
